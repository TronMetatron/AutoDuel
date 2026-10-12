//! AADA chapter office (`aa.prg`): Road Status Bulletin and courier jobs.
use super::talk::{self, Talk, LONG, MEDIUM, SHORT};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::term::{Answer, Ask, Asker, Term};
use crate::text;
use cardul_sim::shops::aada::{self, JobView};
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Aada::new(cx))
}

const GREETING: &str = "Good afternoon! What can I get for\nyou?";
const ANYTHING_ELSE: &str = "Is there anything else?";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Menu(&'static str),
    Jobs(&'static str),
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Menu,
    Report,
    Jobs,
}

struct Aada {
    term: Term,
    asker: Asker,
    talk: Talk,
    then: Option<Step>,
    mode: Mode,
}

impl Aada {
    fn new(cx: &mut Cx) -> Aada {
        let (term, rep) = enter(cx, Building::Aada);
        let mut s = Aada { term, asker: Asker::default(), talk: Talk::default(), then: None, mode: Mode::Menu };
        s.draw_menu();
        talk::deliveries(cx, &mut s.talk, &rep);
        if let Some(clue) = aada::heart_clue(cx.game()) {
            s.talk.say(clue, LONG);
        }
        s.then = Some(Step::Menu(GREETING));
        s
    }

    fn draw_menu(&mut self) {
        self.term.clear_main();
        talk::center(&mut self.term, 1, "American Autoduel Association");
        talk::center(&mut self.term, 2, "Chapter Office");
        talk::menu(&mut self.term, 9, 6, &["Road information", "Courier Tasks", "Leave the office"]);
    }

    fn run(&mut self, cx: &mut Cx, step: Step) -> Trans {
        match step {
            Step::Menu(msg) => {
                self.draw_menu();
                self.term.prompt(msg);
                self.mode = Mode::Menu;
                self.asker.set(Ask::Choice(talk::digits(3)));
            }
            Step::Jobs(msg) => {
                self.draw_jobs(cx);
                self.term.prompt(msg);
                self.mode = Mode::Jobs;
                self.asker.set(Ask::Choice(talk::digits(aada::JOBS_PER_DAY)));
            }
            Step::Leave => return leave(cx, Building::Aada),
        }
        Trans::None
    }

    /// "Road Status Bulletin": each road out of town and how many outlaws are on it.
    fn draw_report(&mut self, cx: &mut Cx) {
        self.term.clear_main();
        talk::center(&mut self.term, 0, "Road Status Bulletin");
        let city = cx.game().driver.city;
        let lines = aada::road_bulletin(cx.game(), &cx.data, city);
        for (i, (dest, word)) in lines.iter().enumerate() {
            let r = 3 + 3 * i;
            self.term.at(3, r, &format!("The road to {} is", cx.data.city_name(*dest)));
            self.term.at(3, r + 1, &format!("{} trafficked by outlaws.", word));
        }
        talk::center(&mut self.term, 17, "Press any key when done");
        self.term.msg("There's the report for today.");
        self.mode = Mode::Report;
        self.asker.set(Ask::Key);
    }

    /// "Delivery Jobs Available": today's three offers.
    fn draw_jobs(&mut self, cx: &mut Cx) {
        {
            let (data, game) = cx.dg();
            aada::refresh_board(game, data);
        }
        let g = cx.game();
        let (here, today) = (g.driver.city, g.driver.date);
        self.term.clear_main();
        talk::center(&mut self.term, 0, "Delivery Jobs Available");
        for (i, o) in g.aada.offers.iter().enumerate() {
            let Some(o) = o else { continue };
            let j = JobView::new(o, &cx.data, here, today);
            let r = 2 + 5 * i;
            self.term.at(1, r, &format!("{}: {}", i + 1, j.name));
            self.term.at(3, r + 1, &format!("Dest: {} in {}", j.dest.name(), cx.data.city_name(j.dest_city)));
            self.term.at(3, r + 2, &format!("Weight:{:<4} Spaces:{:<2} Due:{}", j.weight, j.spaces, j.due.mdy()));
            self.term.at(3, r + 3, &format!("Value:{}", text::money(j.value)));
            self.term.at(22, r + 3, &format!("Pay:{}", text::money(j.pay)));
        }
        talk::center(&mut self.term, 17, "Press Esc for none");
    }

    fn take_job(&mut self, cx: &mut Cx, i: usize) {
        let res = {
            let (data, game) = cx.dg();
            aada::accept(game, data, i)
        };
        match res {
            Ok(_) => {
                cx.sfx(Sfx::Click);
                cx.save();
                self.talk.say("Very good. I'll have it loaded\nimmediately.", MEDIUM);
                self.then = Some(Step::Menu(ANYTHING_ELSE));
            }
            Err(r) => {
                cx.sfx(Sfx::Beep);
                self.talk.say(r.message(), LONG);
                self.then = Some(if r.back_to_menu() {
                    Step::Menu(ANYTHING_ELSE)
                } else {
                    Step::Jobs("Is there anything else you would\nlike to deliver?")
                });
            }
        }
    }
}

impl Screen for Aada {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if self.talk.update(&mut self.term, inp) {
            return Trans::None;
        }
        if let Some(step) = self.then.take() {
            let t = self.run(cx, step);
            // the key that skipped the last message also answers the new prompt
            if !matches!(t, Trans::None) || self.then.is_some() || !self.talk.type_ahead(inp, &self.asker) {
                return t;
            }
        }
        let Some(ans) = self.asker.feed(inp) else { return Trans::None };
        match (self.mode, ans) {
            (Mode::Menu, Answer::Choice('1')) => self.draw_report(cx),
            (Mode::Menu, Answer::Choice('2')) => {
                self.talk.say("Well, let me see...", SHORT);
                self.then = Some(Step::Jobs("Here are the jobs we have. Which\none would you like to take?"));
            }
            (Mode::Menu, Answer::Choice('3') | Answer::Cancel) => {
                self.talk.say("Good luck and drive offensively!", MEDIUM);
                self.then = Some(Step::Leave);
            }
            (Mode::Report, _) => return self.run(cx, Step::Menu(ANYTHING_ELSE)),
            (Mode::Jobs, Answer::Choice(c)) => self.take_job(cx, (c as u8 - b'1') as usize),
            (Mode::Jobs, Answer::Cancel) => {
                self.talk.say("Come back later, we may have a\nbetter selection then.", MEDIUM);
                self.then = Some(Step::Menu(ANYTHING_ELSE));
            }
            _ => {}
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
    }
}
