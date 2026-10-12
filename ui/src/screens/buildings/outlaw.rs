//! The Outlaw Headquarters in Watertown (`ou.prg`): an "abandoned building" until you have the
//! prestige to be their courier; the brain tape is picked up here.
use super::talk::{self, Talk, LONG};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::screens::city::after_death_in_town;
use crate::term::{Answer, Ask, Asker, Term};
use cardul_sim::shops::outlaw;
use cardul_sim::shops::rumors::{self, Handover};
use cardul_sim::story::{password_matches, BRAIN_TAPE};
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Outlaw::new(cx))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Password,
    /// "Press any key to exit."
    Exit,
    Leave,
    Death,
}

struct Outlaw {
    term: Term,
    asker: Asker,
    talk: Talk,
    then: Option<Step>,
    /// the tape was handed over on this visit: they see through you as you leave
    tape_given: bool,
}

impl Outlaw {
    fn new(cx: &mut Cx) -> Outlaw {
        let (mut term, _) = enter(cx, Building::OutlawHq);
        // don't give the place away in the title
        let name = outlaw::title(cx.game());
        term.title = format!("{} - {}", name, cx.data.city_name(cx.game().driver.city));
        talk::center(&mut term, 8, name);
        let mut s = Outlaw { term, asker: Asker::default(), talk: Talk::default(), then: Some(Step::Exit), tape_given: false };
        if outlaw::double_cross(cx.game()) {
            cx.sfx(Sfx::Beep);
            for page in outlaw::DOUBLE_CROSS {
                s.talk.say(page, LONG);
            }
            s.then = Some(Step::Death);
        } else if outlaw::password_due(cx.game()) {
            s.then = Some(Step::Password);
        }
        s
    }

    fn password(&mut self, cx: &mut Cx, typed: &str) {
        self.then = Some(Step::Exit);
        if !password_matches(typed, BRAIN_TAPE as usize) {
            return;
        }
        let h = rumors::hand_over(cx.g(), BRAIN_TAPE as usize);
        self.talk.say(format!("{} {}", outlaw::GIVE, h.text()), LONG);
        if h == Handover::Loaded {
            self.talk.say(outlaw::NO_CLONE, LONG);
            self.tape_given = true;
            cx.save();
        }
    }
}

impl Screen for Outlaw {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if self.talk.update(&mut self.term, inp) {
            return Trans::None;
        }
        match self.then.take() {
            Some(Step::Password) => {
                self.term.msg(talk::PASSWORD);
                self.asker.set(Ask::Text(talk::PASSWORD_LEN));
                if !self.talk.type_ahead(inp, &self.asker) {
                    return Trans::None;
                }
            }
            Some(Step::Exit) => {
                self.term.prompt("Press any key to exit.");
                self.asker.set(Ask::Key);
                return Trans::None;
            }
            Some(Step::Leave) => return leave(cx, Building::OutlawHq),
            Some(Step::Death) => return after_death_in_town(cx),
            None => {}
        }
        match self.asker.feed(inp) {
            Some(Answer::Text(t)) => self.password(cx, &t),
            Some(Answer::Cancel) => self.then = Some(Step::Exit),
            Some(_) => {
                if self.tape_given {
                    self.tape_given = false;
                    cx.sfx(Sfx::Beep);
                    self.talk.say(outlaw::ESCAPE, LONG);
                    self.then = Some(Step::Leave);
                } else {
                    return leave(cx, Building::OutlawHq);
                }
            }
            None => {}
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
    }
}
