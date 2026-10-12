//! The Baltimore Hotel (`ho.prg`): $200 a night, one day, one point of health.
use super::talk::{self, Talk, MEDIUM};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::term::{Answer, Ask, Asker, Term};
use cardul_sim::shops::hotel;
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Hotel::new(cx))
}

const WELCOME: &str = "What would you care to do, sir?";
const MORNING: &str = "Good morning! What do you wish to do\ntoday, sir?";

struct Hotel {
    term: Term,
    asker: Asker,
    talk: Talk,
    /// message for the menu once the pages are done
    then: Option<&'static str>,
}

impl Hotel {
    fn new(cx: &mut Cx) -> Hotel {
        let (mut term, rep) = enter(cx, Building::Hotel);
        talk::center(&mut term, 0, "The Baltimore Hotel");
        talk::menu(&mut term, 7, 6, &["Spend the night $200", "Leave the hotel"]);
        let mut s = Hotel { term, asker: Asker::default(), talk: Talk::default(), then: Some(WELCOME) };
        talk::deliveries(cx, &mut s.talk, &rep);
        s
    }
}

impl Screen for Hotel {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if self.talk.update(&mut self.term, inp) {
            return Trans::None;
        }
        if let Some(m) = self.then.take() {
            self.term.prompt(m);
            self.asker.set(Ask::Choice(talk::digits(2)));
            if !self.talk.type_ahead(inp, &self.asker) {
                return Trans::None;
            }
        }
        match self.asker.feed(inp) {
            Some(Answer::Choice('1')) => {
                if hotel::spend_night(cx.g()) {
                    cx.sfx(Sfx::Cash);
                    cx.save();
                    self.talk.say("Have a pleasant evening!", MEDIUM);
                    self.then = Some(MORNING);
                } else {
                    cx.sfx(Sfx::Beep);
                    self.talk.say("I'm sorry sir, but you haven't\nenough money.", MEDIUM);
                    self.then = Some(WELCOME);
                }
            }
            Some(_) => return leave(cx, Building::Hotel),
            None => {}
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, None);
    }
}
