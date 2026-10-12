//! Keyboard and gamepad -> `cardul_ui::Input`.
//!
//! Presses are collected every frame (in `PreUpdate`, right after Bevy's input systems) into
//! [`PendingInput`] and handed out by the 60 Hz tick: each press goes to exactly one tick, in
//! order, one per tick, so a press that changes screen is never seen by the next screen too.
//! Held controls are latched between ticks so a tap shorter than a tick still counts.
use bevy::input::gamepad::GamepadButtonStateChangedEvent;
use bevy::input::keyboard::{Key as LogicalKey, KeyboardInput};
use bevy::input::{ButtonState, InputSystems};
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};
use cardul_ui::input::{Held, Input, Key};
use std::collections::VecDeque;

/// Stick deflection that counts as a direction.
const STICK: f32 = 0.5;
/// Queue limit; OS key repeats are dropped once a couple of presses are waiting.
const MAX_QUEUED: usize = 64;
const MAX_QUEUED_REPEATS: usize = 2;

pub struct ControlsPlugin;

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingInput>().add_systems(PreUpdate, collect.after(InputSystems));
    }
}

/// One press: the key it produced and the text it typed.
#[derive(Clone, Debug, Default, PartialEq)]
struct Press {
    key: Option<Key>,
    chars: Vec<char>,
}

#[derive(Resource, Default)]
pub struct PendingInput {
    queue: VecDeque<Press>,
    /// controls held right now
    held_now: Held,
    /// controls held at any time since the last tick
    held_seen: Held,
}

impl PendingInput {
    fn push(&mut self, p: Press, repeat: bool) {
        let limit = if repeat { MAX_QUEUED_REPEATS } else { MAX_QUEUED };
        if (p.key.is_some() || !p.chars.is_empty()) && self.queue.len() < limit {
            self.queue.push_back(p);
        }
    }

    fn set_held(&mut self, now: Held, seen: Held) {
        self.held_now = now;
        self.held_seen = or(self.held_seen, seen);
    }

    /// The input for the next tick (takes the oldest waiting press).
    pub fn next_tick(&mut self) -> Input {
        let mut inp = Input { held: self.held_seen, ..default() };
        self.held_seen = self.held_now;
        if let Some(p) = self.queue.pop_front() {
            inp.keys.extend(p.key);
            inp.chars = p.chars;
        }
        inp
    }
}

fn or(a: Held, b: Held) -> Held {
    Held {
        up: a.up || b.up,
        down: a.down || b.down,
        left: a.left || b.left,
        right: a.right || b.right,
        fire: a.fire || b.fire,
    }
}

fn collect(
    mut keyboard: MessageReader<KeyboardInput>,
    mut pad_buttons: MessageReader<GamepadButtonStateChangedEvent>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut pending: ResMut<PendingInput>,
    mut prev_pad_dirs: Local<Held>,
) {
    // Ctrl+letter is not text (but AltGr, reported as Ctrl+Alt, is)
    let ctrl_only = keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && !keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]);
    for ev in keyboard.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        if ev.key_code == KeyCode::F11
            && !ev.repeat
            && let Ok(mut w) = windows.single_mut()
        {
            w.mode = match w.mode {
                WindowMode::Windowed => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
                _ => WindowMode::Windowed,
            };
        }
        let chars = if ctrl_only { vec![] } else { typed(&ev.logical_key) };
        pending.push(Press { key: map_key(ev.key_code, &ev.logical_key), chars }, ev.repeat);
    }

    for ev in pad_buttons.read() {
        if ev.state != ButtonState::Pressed {
            continue;
        }
        let key = match ev.button {
            GamepadButton::South | GamepadButton::Start => Key::Enter,
            GamepadButton::East => Key::Space,
            GamepadButton::Select => Key::Esc,
            GamepadButton::North => Key::Char('g'),
            _ => continue,
        };
        pending.push(Press { key: Some(key), chars: vec![] }, false);
    }

    // held controls; the pad's stick and d-pad also press the arrow keys (for menus)
    let pad = pads.iter().fold(Held::default(), |h, p| or(h, pad_held(p)));
    let prev = std::mem::replace(&mut *prev_pad_dirs, pad);
    for (now, before, key) in [
        (pad.up, prev.up, Key::Up),
        (pad.down, prev.down, Key::Down),
        (pad.left, prev.left, Key::Left),
        (pad.right, prev.right, Key::Right),
    ] {
        if now && !before {
            pending.push(Press { key: Some(key), chars: vec![] }, false);
        }
    }
    let now = or(key_held(|k| keys.pressed(k)), pad);
    let seen = or(key_held(|k| keys.pressed(k) || keys.just_pressed(k)), pad);
    pending.set_held(now, seen);
}

fn key_held(down: impl Fn(KeyCode) -> bool) -> Held {
    use KeyCode as C;
    Held {
        up: down(C::ArrowUp) || down(C::KeyW),
        down: down(C::ArrowDown) || down(C::KeyS),
        left: down(C::ArrowLeft) || down(C::KeyA),
        right: down(C::ArrowRight) || down(C::KeyD),
        fire: down(C::ControlLeft) || down(C::ControlRight) || down(C::KeyZ),
    }
}

fn pad_held(p: &Gamepad) -> Held {
    let s = p.left_stick();
    Held {
        up: s.y > STICK || p.pressed(GamepadButton::DPadUp),
        down: s.y < -STICK || p.pressed(GamepadButton::DPadDown),
        left: s.x < -STICK || p.pressed(GamepadButton::DPadLeft),
        right: s.x > STICK || p.pressed(GamepadButton::DPadRight),
        fire: p.pressed(GamepadButton::South),
    }
}

/// Text typed by a key press (letters keep their case).
fn typed(k: &LogicalKey) -> Vec<char> {
    match k {
        LogicalKey::Character(s) => s.chars().filter(|c| !c.is_control()).collect(),
        LogicalKey::Space => vec![' '],
        _ => vec![],
    }
}

const LETTERS: [(KeyCode, char); 26] = {
    use KeyCode as C;
    [
        (C::KeyA, 'a'), (C::KeyB, 'b'), (C::KeyC, 'c'), (C::KeyD, 'd'), (C::KeyE, 'e'), (C::KeyF, 'f'),
        (C::KeyG, 'g'), (C::KeyH, 'h'), (C::KeyI, 'i'), (C::KeyJ, 'j'), (C::KeyK, 'k'), (C::KeyL, 'l'),
        (C::KeyM, 'm'), (C::KeyN, 'n'), (C::KeyO, 'o'), (C::KeyP, 'p'), (C::KeyQ, 'q'), (C::KeyR, 'r'),
        (C::KeyS, 's'), (C::KeyT, 't'), (C::KeyU, 'u'), (C::KeyV, 'v'), (C::KeyW, 'w'), (C::KeyX, 'x'),
        (C::KeyY, 'y'), (C::KeyZ, 'z'),
    ]
};

/// The game key for a physical key. Special keys and digits go by position (so 1..0 work on
/// any layout); letters and punctuation go by what the layout prints, falling back to the
/// US position for non-Latin layouts.
pub fn map_key(code: KeyCode, logical: &LogicalKey) -> Option<Key> {
    use KeyCode as C;
    let k = match code {
        C::ArrowUp => Key::Up,
        C::ArrowDown => Key::Down,
        C::ArrowLeft => Key::Left,
        C::ArrowRight => Key::Right,
        C::Enter | C::NumpadEnter => Key::Enter,
        C::Escape => Key::Esc,
        C::Backspace => Key::Backspace,
        C::Space => Key::Space,
        C::Tab => Key::Tab,
        C::F1 => Key::F(1),
        C::F2 => Key::F(2),
        C::F3 => Key::F(3),
        C::F4 => Key::F(4),
        C::F5 => Key::F(5),
        C::F6 => Key::F(6),
        C::F7 => Key::F(7),
        C::F8 => Key::F(8),
        C::F9 => Key::F(9),
        C::F10 => Key::F(10),
        C::F11 => Key::F(11),
        C::F12 => Key::F(12),
        C::Digit0 | C::Numpad0 => Key::Char('0'),
        C::Digit1 | C::Numpad1 => Key::Char('1'),
        C::Digit2 | C::Numpad2 => Key::Char('2'),
        C::Digit3 | C::Numpad3 => Key::Char('3'),
        C::Digit4 | C::Numpad4 => Key::Char('4'),
        C::Digit5 | C::Numpad5 => Key::Char('5'),
        C::Digit6 | C::Numpad6 => Key::Char('6'),
        C::Digit7 | C::Numpad7 => Key::Char('7'),
        C::Digit8 | C::Numpad8 => Key::Char('8'),
        C::Digit9 | C::Numpad9 => Key::Char('9'),
        _ => return char_key(code, logical).map(Key::Char),
    };
    Some(k)
}

fn char_key(code: KeyCode, logical: &LogicalKey) -> Option<char> {
    use KeyCode as C;
    if let LogicalKey::Character(s) = logical {
        let mut it = s.chars();
        if let (Some(c), None) = (it.next(), it.next())
            && c.is_ascii_graphic()
        {
            return Some(c.to_ascii_lowercase());
        }
    }
    if let Some((_, c)) = LETTERS.iter().find(|(k, _)| *k == code) {
        return Some(*c);
    }
    Some(match code {
        C::Slash | C::NumpadDivide => '/',
        C::Minus | C::NumpadSubtract => '-',
        C::NumpadAdd => '+',
        C::NumpadMultiply => '*',
        C::Period | C::NumpadDecimal => '.',
        C::Comma => ',',
        C::Equal => '=',
        C::Semicolon => ';',
        C::Quote => '\'',
        C::BracketLeft => '[',
        C::BracketRight => ']',
        C::Backslash => '\\',
        C::Backquote => '`',
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(s: &str) -> LogicalKey {
        LogicalKey::Character(s.into())
    }

    #[test]
    fn keys() {
        assert_eq!(map_key(KeyCode::KeyA, &ch("A")), Some(Key::Char('a')));
        // AZERTY: the Q position prints 'a'; digits go by position
        assert_eq!(map_key(KeyCode::KeyQ, &ch("a")), Some(Key::Char('a')));
        assert_eq!(map_key(KeyCode::Digit1, &ch("&")), Some(Key::Char('1')));
        // non-Latin layout falls back to the US position
        assert_eq!(map_key(KeyCode::KeyQ, &ch("й")), Some(Key::Char('q')));
        assert_eq!(map_key(KeyCode::NumpadDivide, &ch("/")), Some(Key::Char('/')));
        assert_eq!(map_key(KeyCode::NumpadEnter, &LogicalKey::Enter), Some(Key::Enter));
        assert_eq!(map_key(KeyCode::F11, &LogicalKey::F11), Some(Key::F(11)));
        assert_eq!(map_key(KeyCode::ShiftLeft, &LogicalKey::Shift), None);
        assert_eq!(typed(&ch("Q")), vec!['Q']);
        assert_eq!(typed(&LogicalKey::Space), vec![' ']);
    }

    #[test]
    fn one_press_per_tick_and_latched_held() {
        let mut p = PendingInput::default();
        p.push(Press { key: Some(Key::Enter), chars: vec![] }, false);
        p.push(Press { key: Some(Key::Char('b')), chars: vec!['B'] }, false);
        p.push(Press { key: Some(Key::Char('b')), chars: vec!['B'] }, true); // repeat, queue busy
        let tap = Held { fire: true, ..default() };
        p.set_held(Held::default(), tap);
        let a = p.next_tick();
        assert_eq!(a.keys, vec![Key::Enter]);
        assert!(a.held.fire);
        let b = p.next_tick();
        assert_eq!((b.keys, b.chars), (vec![Key::Char('b')], vec!['B']));
        assert!(!b.held.fire);
        assert!(p.next_tick().keys.is_empty());
    }
}
