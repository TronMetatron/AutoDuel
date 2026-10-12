//! Joe's Bar (`di.prg`): drinks, rumors and the black market.
use super::talk::{self, Talk, LONG, MEDIUM, SHORT};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::screens::city::after_death_in_town;
use crate::term::{Answer, Ask, Asker, Term};
use crate::text;
use cardul_sim::shops::bar::{self, Drink, Sale, Visit};
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Bar::new(cx))
}

const GREETING: &str = "Hi! What can I do ya for?";
const ANYTHING_ELSE: &str = "Anything else?";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Menu(&'static str),
    Sell,
    Leave,
    Death,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Menu,
    Sell,
    /// "O.K. I'll give ya $X for it, O.K.?" for this cargo slot
    Offer(usize),
}

struct Bar {
    term: Term,
    asker: Asker,
    talk: Talk,
    then: Option<Step>,
    mode: Mode,
    visit: Visit,
}

impl Bar {
    fn new(cx: &mut Cx) -> Bar {
        let (term, _) = enter(cx, Building::Bar);
        let mut s = Bar {
            term,
            asker: Asker::default(),
            talk: Talk::default(),
            then: None,
            mode: Mode::Menu,
            visit: Visit::default(),
        };
        s.draw_menu();
        // carrying the brain tape: shot at and thrown out, no menu
        if let Some((_, died)) = bar::tape_ambush(cx.g()) {
            cx.save();
            s.talk.say(
                "As you enter the bar, things don't\nseem right. Most people stare at you\nwith very unsavory expressions.",
                LONG,
            );
            s.talk.say(
                "You hear someone mumble \"dirty\ndouble-crosser\" just before you hear\nthe sound of a gunshot.",
                LONG,
            );
            cx.sfx(Sfx::PlayerHit);
            if died {
                s.talk.say(
                    "The shot hits you, and as you try to\nmake it to the door, you realize that\nyou're not going to make it.",
                    LONG,
                );
                s.then = Some(Step::Death);
            } else {
                s.talk.say("The shot hits you, and you stumble\nout the door.", LONG);
                s.then = Some(Step::Leave);
            }
            return s;
        }
        s.then = Some(Step::Menu(GREETING));
        s
    }

    fn draw_menu(&mut self) {
        self.term.clear_main();
        talk::menu(
            &mut self.term,
            6,
            5,
            &["Buy a drink $5", "Try to sell courier tasks", "Listen for rumors", "Leave the bar"],
        );
    }

    fn draw_cargo(&mut self, cx: &Cx) {
        self.term.clear_main();
        talk::center(&mut self.term, 3, "Your cargo:");
        let car = cx.game().car_here();
        for i in 0..3 {
            let name = car.and_then(|c| c.cargo[i].as_ref()).map(|c| c.name.as_str()).unwrap_or("");
            self.term.at(2, 6 + 2 * i, &format!("{} - {}", i + 1, name));
        }
        talk::center(&mut self.term, 16, "Press Esc for none");
    }

    fn run(&mut self, cx: &mut Cx, step: Step) -> Trans {
        match step {
            Step::Menu(msg) => {
                self.draw_menu();
                self.term.prompt(msg);
                self.mode = Mode::Menu;
                self.asker.set(Ask::Choice(talk::digits(4)));
            }
            Step::Sell => {
                self.draw_cargo(cx);
                self.term.prompt("Whaddayawanna sell?");
                self.mode = Mode::Sell;
                self.asker.set(Ask::Choice(talk::digits(3)));
            }
            Step::Leave => return leave(cx, Building::Bar),
            Step::Death => return after_death_in_town(cx),
        }
        Trans::None
    }

    fn menu_choice(&mut self, cx: &mut Cx, c: char) -> Trans {
        match c {
            '1' => {
                let d = bar::buy_drink(cx.g(), &mut self.visit);
                cx.sfx(if d == Drink::Served { Sfx::Cash } else { Sfx::Beep });
                let m = match d {
                    Drink::TooMany => "If I give you another drink, you'll\nbe in no shape to drive. Sorry.",
                    Drink::Broke => "Ya bum! You can't pay for it.",
                    Drink::Served => "Here ya go.  Enjoy it.",
                };
                self.talk.say(m, MEDIUM);
                self.then = Some(Step::Menu(GREETING));
            }
            '2' => self.then = Some(Step::Sell),
            '3' => {
                let r = bar::rumor(cx.g(), &mut self.visit);
                self.talk.say(r, LONG);
                self.then = Some(Step::Menu(ANYTHING_ELSE));
            }
            _ => return self.run(cx, Step::Leave),
        }
        cx.save();
        Trans::None
    }

    /// A cargo slot was picked: make an offer.
    fn offer(&mut self, cx: &mut Cx, slot: usize) {
        let g = cx.game();
        let Some(c) = g.car_here().and_then(|car| car.cargo[slot].as_ref()) else {
            cx.sfx(Sfx::Beep);
            self.talk.say("There's nothing there! You lookin'\nfor trouble, buddy?", MEDIUM);
            self.then = Some(Step::Menu(ANYTHING_ELSE));
            return;
        };
        let price = bar::offer(c, g.driver.date);
        let m = if price == 0 {
            "I'll take it off your hands for\nfree, O.K.?".to_string()
        } else {
            format!("O.K. I'll give ya {}\nfor it, O.K.?", text::money(price))
        };
        self.term.msg(&m);
        self.mode = Mode::Offer(slot);
        self.asker.set(Ask::YesNo);
    }

    fn sell(&mut self, cx: &mut Cx, slot: usize) {
        self.talk.say("Great! I'll have someone take care\nof it.", MEDIUM);
        let sale = bar::sell(cx.g(), slot);
        if matches!(sale, Some(Sale::Sold { .. })) {
            cx.sfx(Sfx::Cash);
        }
        if let Some(Sale::Stung { fine }) = sale {
            cx.sfx(Sfx::Beep);
            self.talk.say(
                format!(
                    "As they start to unload the cargo,\nan undercover agent steps up, takes\nthe goods, and fines you {}.",
                    text::money(fine)
                ),
                LONG,
            );
        }
        cx.save();
        self.then = Some(Step::Menu(ANYTHING_ELSE));
    }
}

impl Screen for Bar {
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
            (Mode::Menu, Answer::Choice(c)) => return self.menu_choice(cx, c),
            (Mode::Menu, _) => return self.run(cx, Step::Leave),
            (Mode::Sell, Answer::Choice(c)) => self.offer(cx, (c as u8 - b'1') as usize),
            (Mode::Sell, _) => {
                self.talk.say("Suit yourself!", SHORT);
                self.then = Some(Step::Menu(ANYTHING_ELSE));
            }
            (Mode::Offer(slot), Answer::Yes) => self.sell(cx, slot),
            (Mode::Offer(_), _) => {
                self.talk.say("That's fine, but I guarantee you\nwon't do better!", MEDIUM);
                self.then = Some(Step::Menu(ANYTHING_ELSE));
            }
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
    }
}
