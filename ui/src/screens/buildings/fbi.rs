//! The F.B.I. office in New York (`fb.prg`): the Mr. Big briefing, and the end of the story when
//! the brain tape is delivered.
use super::talk::{self, Talk, LONG, MEDIUM};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::screens::ending::EndingScreen;
use crate::term::{Ask, Asker, Term};
use cardul_sim::shops::fbi;
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Fbi::new(cx))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// show the newspaper
    Ending,
    /// "Press any key to exit."
    Exit,
}

struct Fbi {
    term: Term,
    asker: Asker,
    talk: Talk,
    then: Option<Step>,
}

impl Fbi {
    fn new(cx: &mut Cx) -> Fbi {
        let (mut term, rep) = enter(cx, Building::Fbi);
        talk::center(&mut term, 8, "Federal Bureau of Investigation");
        let mut s = Fbi { term, asker: Asker::default(), talk: Talk::default(), then: Some(Step::Exit) };
        // FIX: the delivery thanks come first (`SR_cargo` runs on entry), then the briefing,
        // which a delivered quest item may just have made due
        talk::deliveries(cx, &mut s.talk, &rep);
        if fbi::briefing_due(cx.game()) {
            fbi::brief(cx.g());
            for (i, page) in fbi::BRIEFING.iter().enumerate() {
                s.talk.say(*page, if i + 1 == fbi::BRIEFING.len() { MEDIUM } else { LONG });
            }
            cx.save();
        }
        if rep.won {
            for page in fbi::WIN {
                s.talk.say(page, LONG);
            }
            s.then = Some(Step::Ending);
        }
        s
    }
}

impl Screen for Fbi {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if self.talk.update(&mut self.term, inp) {
            return Trans::None;
        }
        match self.then.take() {
            Some(Step::Ending) => {
                cx.sfx(Sfx::Fanfare);
                return Trans::Push(Box::new(EndingScreen::new(cx)));
            }
            Some(Step::Exit) => {
                self.term.prompt("Press any key to exit.");
                self.asker.set(Ask::Key);
                return Trans::None;
            }
            None => {}
        }
        if self.asker.feed(inp).is_some() {
            return leave(cx, Building::Fbi);
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, None);
    }
    fn resume(&mut self, _cx: &mut Cx) {
        // back from the newspaper (or an information screen)
        if !self.asker.waiting() && self.then.is_none() {
            self.then = Some(Step::Exit);
        }
    }
}
