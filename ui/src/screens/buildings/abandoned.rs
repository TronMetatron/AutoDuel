//! Abandoned building (`ab.prg`): muggers, or nothing — and one clue in Albany.
use super::talk::{self, Talk, LONG};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::term::{Ask, Asker, Term};
use crate::text;
use cardul_sim::shops::abandoned::{self, Event};
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Abandoned::new(cx))
}

const PRESS_KEY: &str = "Press any key to leave the building.";

struct Abandoned {
    term: Term,
    asker: Asker,
    talk: Talk,
    ready: bool,
    /// the clue fills the message area, so "Press any key" replaces it
    clue: bool,
}

impl Abandoned {
    fn new(cx: &mut Cx) -> Abandoned {
        let (mut term, _) = enter(cx, Building::Abandoned);
        talk::center(&mut term, 8, "An abandoned building");
        let mut talk = Talk::default();
        let event = abandoned::visit(cx.g());
        match event {
            Event::Clue(c) => talk.say(c, LONG),
            Event::Empty => term.msg("The building is abandoned."),
            Event::Mugged(n) => {
                cx.sfx(Sfx::Beep);
                term.msg(&format!("You were just mugged. They took\n{}.", text::money(n)));
            }
        }
        cx.save();
        Abandoned { term, asker: Asker::default(), talk, ready: false, clue: matches!(event, Event::Clue(_)) }
    }
}

impl Screen for Abandoned {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if self.talk.update(&mut self.term, inp) {
            return Trans::None;
        }
        if !self.ready {
            self.ready = true;
            if self.clue {
                self.term.msg(PRESS_KEY);
            } else {
                self.term.msg_add(PRESS_KEY);
            }
            self.asker.set(Ask::Key);
            return Trans::None;
        }
        if self.asker.feed(inp).is_some() {
            return leave(cx, Building::Abandoned);
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, None);
    }
}
