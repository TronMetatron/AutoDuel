//! Small helpers shared by the vehicle shops (Assembly Line, Garage, Weapon Shop, Salvage
//! Yard, Gold Cross).
use crate::app::Cx;
use crate::fb::{col, Frame};
use crate::input::{Input, Key};
use crate::text;
use cardul_sim::Game;

/// A reply stays on screen for a moment, then the shop moves on to its next prompt — the
/// original's `SR_wait_key` ("wait a few ticks or until a key") between messages.
///
/// The time grows with the length of the text. Any key ends the wait early; keys other than
/// Space, Enter and Esc are also passed on to the next prompt, so typing ahead works.
pub struct Wait<S> {
    pending: Option<(u32, S)>,
}

impl<S> Default for Wait<S> {
    fn default() -> Self {
        Wait { pending: None }
    }
}

impl<S> Wait<S> {
    /// Wait long enough to read `msg`, then continue with `next`.
    pub fn after(&mut self, msg: &str, next: S) {
        let ticks = (40 + 2 * msg.chars().count() as u32).min(240);
        self.pending = Some((ticks, next));
    }
    /// One tick. When the wait is over returns the next stage, and whether this tick's keys
    /// should also be handled by it.
    pub fn tick(&mut self, inp: &Input) -> Option<(S, bool)> {
        let (t, _) = self.pending.as_mut()?;
        let skip = inp.keys.iter().any(|k| !matches!(k, Key::F(_)));
        if skip {
            let pass = inp.keys.iter().any(|k| !matches!(k, Key::Space | Key::Enter | Key::Esc | Key::F(_)));
            return self.pending.take().map(|(_, s)| (s, pass));
        }
        *t = t.saturating_sub(1);
        if *t == 0 {
            return self.pending.take().map(|(_, s)| (s, false));
        }
        None
    }
}

/// Bottom line of a shop screen: your cash and the car you have with you.
pub fn status_line(g: &Game) -> String {
    let mut s = format!("Cash: {}", text::money(g.driver.money));
    if let Some(c) = g.car_here() {
        s.push_str(&format!("   Car: {}", c.name));
    }
    s.chars().take(crate::term::COLS).collect()
}

/// Draw a line of text on main-area row `row` (0-17) of a [`crate::term::Term`] screen, over
/// whatever the term drew. Used for the status lines, which must always show current values.
pub fn draw_line(fb: &mut Frame, cx: &Cx, row: usize, s: &str) {
    text::at(fb, &cx.assets, 1, 2 + row as i32, s, col::WHITE);
}

/// A number with thousands separators but no dollar sign ("I'll give ya 1,705 bucks").
pub fn num(v: u32) -> String {
    text::money(v)[1..].to_string()
}
