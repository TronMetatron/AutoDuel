//! Helpers for the world buildings: timed message pages and common layout.
//!
//! The original prints a message in the three-line message area and then waits a moment or
//! until a key is pressed (`SR_wait_key`). [`Talk`] queues such pages; a building screen runs
//! it first in `update` and continues with its next step once the queue is empty:
//!
//! ```ignore
//! if self.talk.update(&mut self.term, inp) { return Trans::None; }
//! if let Some(step) = self.then.take() {
//!     let t = self.run(cx, step);
//!     // the key that skipped the last page also answers the new prompt (see `type_ahead`)
//!     if !matches!(t, Trans::None) || self.then.is_some() || !self.talk.type_ahead(inp, &self.asker) {
//!         return t;
//!     }
//! }
//! ```
//!
//! Pages are shown with [`Term::reply`], so a building that asks its next question with
//! [`Term::prompt`] keeps the answer to the last request on screen above it.
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::{Input, Key};
use crate::term::{Ask, Asker, Term};
use cardul_sim::game::DeliveryReport;
use cardul_sim::world::Building;
use cardul_sim::Sfx;
use std::collections::VecDeque;

/// About a fifth of a second (`SR_wait_key(3)`).
pub const SHORT: u32 = 30;
/// About a second.
pub const MEDIUM: u32 = 60;
/// About two seconds (`SR_wait_key(30)`), the usual pause after a story message.
pub const LONG: u32 = 150;
/// Wait for a key.
pub const KEY: u32 = u32::MAX;

struct Page {
    text: String,
    ticks: u32,
}

/// A queue of message-area pages; any key skips to the next page.
#[derive(Default)]
pub struct Talk {
    queue: VecDeque<Page>,
    timer: u32,
    showing: bool,
    /// a key cut the last page short this tick
    skipped: bool,
}

impl Talk {
    /// Queue a page that replaces the message area for `ticks` (or until a key).
    pub fn say(&mut self, text: impl Into<String>, ticks: u32) {
        self.queue.push_back(Page { text: text.into(), ticks: ticks.max(1) });
    }
    pub fn busy(&self) -> bool {
        self.showing || !self.queue.is_empty()
    }
    /// Run one tick; returns true while pages are still showing. The F-keys (sound, speed)
    /// don't skip a page.
    pub fn update(&mut self, term: &mut Term, inp: &Input) -> bool {
        self.skipped = false;
        if self.showing {
            if self.timer != KEY {
                self.timer = self.timer.saturating_sub(1);
            }
            let key = inp.keys.iter().any(|k| !matches!(k, Key::F(_)));
            if self.timer > 0 && !key {
                return true;
            }
            self.showing = false;
            self.skipped = key;
        }
        match self.queue.pop_front() {
            Some(p) => {
                term.reply(&p.text);
                self.timer = p.ticks;
                self.showing = true;
                self.skipped = false;
                true
            }
            None => false,
        }
    }
    /// After the last page was skipped with a key and the next prompt is up: should that key
    /// also answer it? Then "6" pressed over "You should always wear body armor!" leaves the
    /// Truck Stop instead of only clearing the message (the shops' `Wait` does the same).
    /// Space, Enter, Esc and the F-keys only skip; a "press any key" prompt never takes the
    /// key (it would be gone before it was seen).
    pub fn type_ahead(&self, inp: &Input, asker: &Asker) -> bool {
        self.skipped
            && !self.busy()
            && matches!(&asker.ask, Some(a) if *a != Ask::Key)
            && inp.keys.iter().any(|k| !matches!(k, Key::Space | Key::Enter | Key::Esc | Key::F(_)))
    }
}

/// Queue the automatic delivery messages from [`super::enter`], one page each.
pub fn deliveries(cx: &mut Cx, talk: &mut Talk, rep: &DeliveryReport) {
    if !rep.lines.is_empty() {
        cx.sfx(Sfx::Fanfare);
    }
    for l in &rep.lines {
        talk.say(l.clone(), LONG);
    }
}

/// Print lines centred in the main area (row = main-area row).
pub fn center(term: &mut Term, row: usize, s: &str) {
    let w = crate::term::COLS;
    let n = s.chars().count().min(w);
    term.at((w - n) / 2, row, s);
}

/// A numbered menu at column `col`, starting at `row`, one blank line between items.
pub fn menu(term: &mut Term, col: usize, row: usize, items: &[&str]) {
    for (i, it) in items.iter().enumerate() {
        term.at(col, row + 2 * i, &format!("{} - {}", i + 1, it));
    }
}

/// Menu keys '1'..='n'.
pub fn digits(n: usize) -> Vec<char> {
    (1..=n.min(9)).map(|i| char::from(b'0' + i as u8)).collect()
}

/// The "What's the password?" prompt (the answer is typed after it).
pub const PASSWORD: &str = "What's the password? ";
/// Longest password (16 letters, "SAN ANTONIO ROSE" / "SHE SELLS SHELLS").
pub const PASSWORD_LEN: usize = 17;

/// A building that only shows a text and takes deliveries (Pet Store, Origin Systems):
/// "Press any key to exit."
pub struct Notice {
    kind: Building,
    term: Term,
    asker: Asker,
    talk: Talk,
    ready: bool,
}

impl Notice {
    /// `lines` are printed centred from main-area row `row`.
    pub fn new(cx: &mut Cx, kind: Building, row: usize, lines: &[&str]) -> Notice {
        let (mut term, rep) = super::enter(cx, kind);
        for (i, l) in lines.iter().enumerate() {
            center(&mut term, row + i, l);
        }
        let mut talk = Talk::default();
        deliveries(cx, &mut talk, &rep);
        Notice { kind, term, asker: Asker::default(), talk, ready: false }
    }
}

impl Screen for Notice {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if self.talk.update(&mut self.term, inp) {
            return Trans::None;
        }
        if !self.ready {
            self.ready = true;
            self.term.prompt("Press any key to exit.");
            self.asker.set(Ask::Key);
            return Trans::None;
        }
        if self.asker.feed(inp).is_some() {
            return super::leave(cx, self.kind);
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, None);
    }
}
