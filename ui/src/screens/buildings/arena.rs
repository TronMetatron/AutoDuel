//! The Arena building (`ar.prg`): practice, the schedule of events and sign-up, results, and
//! the Harrisburg password for the decoy prize.
use super::talk::{self, Talk, LONG, MEDIUM, SHORT};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::screens::drive;
use crate::term::{Answer, Ask, Asker, Term};
use crate::text;
use cardul_sim::date::MONTHS;
use cardul_sim::shops::arena::{self, Entry, Outcome, PRACTICE};
use cardul_sim::shops::rumors;
use cardul_sim::story::password_matches;
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Arena::new(cx))
}

const GREETING: &str = "Hi-ya Mac! What would'ja like to do?";
const WHAT_ELSE: &str = "What else are you interested in?";

/// Weekday names as on the schedule (right-aligned, `ar $AB82`).
const DAY_NAMES: [&str; 7] = ["   Sunday", "   Monday", "  Tuesday", "Wednesday", " Thursday", "   Friday", " Saturday"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Menu(&'static str),
    Password,
    /// run event 0..6 or practice (7) in the driving engine
    Run(u8),
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Menu,
    Password,
    /// schedule shown, nothing to sign up for
    Schedule,
    /// "'ya wanna sign up?" for this event
    SignUp(u8),
}

struct Arena {
    term: Term,
    asker: Asker,
    talk: Talk,
    then: Option<Step>,
    mode: Mode,
    /// the event running in the driving engine, whose result we wait for
    running: Option<u8>,
}

impl Arena {
    fn new(cx: &mut Cx) -> Arena {
        let (term, rep) = enter(cx, Building::Arena);
        let mut s =
            Arena { term, asker: Asker::default(), talk: Talk::default(), then: None, mode: Mode::Menu, running: None };
        s.draw_menu();
        talk::deliveries(cx, &mut s.talk, &rep);
        s.then = Some(if arena::decoy_password_due(cx.game()) { Step::Password } else { Step::Menu(GREETING) });
        s
    }

    fn draw_menu(&mut self) {
        self.term.clear_main();
        talk::menu(
            &mut self.term,
            5,
            5,
            &["Practice by yourself $20", "Get a schedule of events", "Leave the arena"],
        );
    }

    /// "Schedule of Events": Sunday..Friday and the next City Championship.
    fn draw_schedule(&mut self, cx: &Cx) {
        let g = cx.game();
        let (city, today) = (g.driver.city, g.driver.date);
        self.term.clear_main();
        talk::center(&mut self.term, 0, "Schedule of Events");
        for wd in 0..6u8 {
            let e = arena::weekday_event(city, wd);
            self.term.at(4, 2 + 2 * wd as usize, &format!("{} - {}", DAY_NAMES[wd as usize], arena::event_name(e)));
        }
        let c = arena::championship_date(city, today);
        talk::center(&mut self.term, 15, "Next City Championship:");
        talk::center(&mut self.term, 16, &format!("Sat, {} {}", MONTHS[c.month as usize - 1], c.day));
    }

    fn run(&mut self, cx: &mut Cx, step: Step) -> Trans {
        match step {
            Step::Menu(msg) => {
                self.draw_menu();
                self.term.prompt(msg);
                self.mode = Mode::Menu;
                self.asker.set(Ask::Choice(talk::digits(3)));
            }
            Step::Password => {
                self.term.msg(talk::PASSWORD);
                self.mode = Mode::Password;
                self.asker.set(Ask::Text(talk::PASSWORD_LEN));
            }
            Step::Run(event) => {
                self.running = Some(event);
                cx.save();
                return Trans::Push(drive::arena(cx, event));
            }
            Step::Leave => return leave(cx, Building::Arena),
        }
        Trans::None
    }

    fn password(&mut self, cx: &mut Cx, typed: &str) {
        self.then = Some(Step::Menu(GREETING));
        if !password_matches(typed, 0) {
            return;
        }
        let h = rumors::hand_over(cx.g(), 0);
        cx.save();
        self.talk.say(format!("{} {}", arena::DECOY_GIVE, h.text()), LONG);
    }

    fn practice(&mut self, cx: &mut Cx) {
        match arena::start_practice(cx.g()) {
            Ok(()) => {
                cx.sfx(Sfx::Cash);
                self.talk.say("I'll just need $20 and the arena\nwill be yours.", MEDIUM);
                self.then = Some(Step::Run(PRACTICE));
            }
            Err(r) => {
                cx.sfx(Sfx::Beep);
                self.talk.say(r.message(), LONG);
                self.then = Some(Step::Menu(WHAT_ELSE));
            }
        }
    }

    fn schedule(&mut self, cx: &mut Cx) {
        self.draw_schedule(cx);
        let g = cx.game();
        if g.driver.day_used {
            self.term.msg("Press any key when done.");
            self.mode = Mode::Schedule;
            self.asker.set(Ask::Key);
            return;
        }
        let e = arena::todays_event(g.driver.city, g.driver.date);
        self.term.msg(&format!(
            "Today's event is the {}\ncompetition, 'ya wanna sign up?",
            arena::event_name(e).to_lowercase()
        ));
        self.mode = Mode::SignUp(e);
        self.asker.set(Ask::YesNo);
    }

    fn sign_up(&mut self, cx: &mut Cx, event: u8) {
        match arena::check_entry(cx.game(), event) {
            Err(r) => {
                cx.sfx(Sfx::Beep);
                self.talk.say(r.message(), LONG);
                self.then = Some(Step::Menu(WHAT_ELSE));
            }
            Ok(entry) => {
                if entry == Entry::DownAndOut {
                    self.talk.say(
                        "You're no amateur, but you look down\nand out, so we'll let you compete\nanyway.",
                        LONG,
                    );
                }
                arena::start_event(cx.g(), event);
                self.talk.say("Good luck!", MEDIUM);
                self.then = Some(Step::Run(event));
            }
        }
    }

    /// Back from the driving engine: apply the result.
    fn results(&mut self, cx: &mut Cx, event: u8) {
        self.draw_menu();
        let result = cx.arena_result.take();
        if event == PRACTICE {
            if let Some(r) = result {
                let out = arena::finish_practice(cx.g(), r);
                self.talk.say(out.message(), LONG);
            }
        } else {
            match result {
                Some(r) => {
                    let out = arena::finish_event(cx.g(), event, r);
                    self.talk.say(out.message(), LONG);
                    if let Outcome::Won { prize } = out {
                        cx.sfx(Sfx::Fanfare);
                        self.talk.say(format!("Your prize is {}!", text::money(prize)), LONG);
                        self.talk.say("The arena is closing now, so you\nshould probably leave.", LONG);
                    }
                }
                None => arena::return_loaner(cx.g()),
            }
        }
        cx.save();
        self.then = Some(Step::Menu(WHAT_ELSE));
    }
}

impl Screen for Arena {
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
            (Mode::Menu, Answer::Choice('1')) => self.practice(cx),
            (Mode::Menu, Answer::Choice('2')) => self.schedule(cx),
            (Mode::Menu, _) => {
                self.talk.say("See ya later!", SHORT);
                self.then = Some(Step::Leave);
            }
            (Mode::Password, Answer::Text(t)) => self.password(cx, &t),
            (Mode::Password, _) => self.then = Some(Step::Menu(GREETING)),
            (Mode::Schedule, _) => self.then = Some(Step::Menu(WHAT_ELSE)),
            (Mode::SignUp(e), Answer::Yes) => self.sign_up(cx, e),
            (Mode::SignUp(_), _) => {
                self.talk.say("Well, maybe next time.", MEDIUM);
                self.then = Some(Step::Menu(WHAT_ELSE));
            }
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
    }
    fn resume(&mut self, cx: &mut Cx) {
        if let Some(event) = self.running.take() {
            self.results(cx, event);
        }
    }
}
