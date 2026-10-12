//! "You are now dead!" — clone revival or the end of the driver.
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::screens::city::{Arrival, CityScreen};
use crate::term::Term;
use cardul_sim::game::DeathOutcome;

pub struct DeathScreen {
    term: Term,
    out: DeathOutcome,
    t: u32,
}

impl DeathScreen {
    pub fn new(out: DeathOutcome) -> DeathScreen {
        let mut term = Term::new("");
        term.at(12, 6, "You are now dead!");
        match out {
            DeathOutcome::Cloned { .. } => {
                term.at(4, 9, "Your clone is now being activated.");
            }
            DeathOutcome::GameOver => {
                term.at(2, 9, "Since you don't have a clone,");
                term.at(2, 10, "the game is now over.");
            }
        }
        term.msg("Press any key.");
        DeathScreen { term, out, t: 0 }
    }
}

impl Screen for DeathScreen {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        self.t += 1;
        if self.t == 1 {
            cx.sfx(cardul_sim::Sfx::Death);
        }
        if self.t < 30 || !inp.any_key() {
            return Trans::None;
        }
        match self.out {
            DeathOutcome::Cloned { .. } => Trans::Reset(Box::new(CityScreen::arrive(cx, Arrival::GoldCross))),
            DeathOutcome::GameOver => {
                cx.save();
                cx.game = None;
                cx.save_file = None;
                Trans::Reset(Box::new(crate::screens::setup::MainMenu::new()))
            }
        }
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, None);
    }
}
