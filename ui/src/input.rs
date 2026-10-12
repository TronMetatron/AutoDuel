//! Input, as delivered by the front end once per 60 Hz tick.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Backspace,
    Space,
    Tab,
    /// F1..F12
    F(u8),
    /// letter / digit / punctuation, lower-case for letters
    Char(char),
}

/// Held state of the joystick-like controls (arrows/WASD/gamepad + fire).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Held {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
    pub fire: bool,
}

impl Held {
    pub fn dx(&self) -> i8 {
        (self.right as i8) - (self.left as i8)
    }
    pub fn dy(&self) -> i8 {
        (self.down as i8) - (self.up as i8)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Input {
    /// keys pressed this tick (edge-triggered, with key repeat from the OS)
    pub keys: Vec<Key>,
    /// characters typed this tick (for text entry; letters keep their case)
    pub chars: Vec<char>,
    pub held: Held,
}

impl Input {
    pub fn pressed(&self, k: Key) -> bool {
        self.keys.contains(&k)
    }
    pub fn any_key(&self) -> bool {
        !self.keys.is_empty()
    }
    /// First pressed character key (lower-case letters, digits).
    pub fn char_key(&self) -> Option<char> {
        self.keys.iter().find_map(|k| if let Key::Char(c) = k { Some(*c) } else { None })
    }
}
