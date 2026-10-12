//! Gold Cross Health Care (`go.prg`): clones, braintapes, treatment; the clone-heart quest
//! password in New York. Rules live in [`cardul_sim::shops::goldcross`].
use super::shopkit::{draw_line, Wait};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::{Input, Key};
use crate::term::{Answer, Ask, Asker, Term};
use crate::text;
use cardul_sim::shops::goldcross::{self as rules, CloneError, HandOver, Treatment, TreatmentError};
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(GoldCross::new(cx))
}

const HELLO: &str = "Good afternoon. What can I do for you?";
const WHAT_ELSE: &str = "What else can I help you with?";
const NO_MONEY: &str = "I hate to be the one to bring this to your attention, but you don't have enough money.";
const SIGN_HERE: &str = "No problem, just sign here.";

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    /// a reply is up; see `wait`
    Busy,
    Menu,
    /// "What's the password?" (New York)
    Password,
    /// "You already have a clone! Would you like to update its braintape?"
    UpdateOffer,
    /// "You would need N weeks to heal. Is that O.K.?"
    Treat(Treatment),
}

#[derive(Clone, Copy)]
enum Next {
    /// after the delivery messages: the password or the menu
    Password,
    Menu(&'static str),
    /// "Thank you." after the treatment
    Thanks,
    Leave,
}

pub struct GoldCross {
    term: Term,
    asker: Asker,
    stage: Stage,
    wait: Wait<Next>,
}

impl GoldCross {
    pub fn new(cx: &mut Cx) -> GoldCross {
        let (term, rep) = enter(cx, Building::GoldCross);
        let mut s = GoldCross { term, asker: Asker::default(), stage: Stage::Busy, wait: Wait::default() };
        s.draw_menu();
        if rep.lines.is_empty() {
            let _ = s.goto(cx, Next::Password);
        } else {
            s.wait.after(&rep.lines.join(" "), Next::Password);
        }
        s
    }

    fn draw_menu(&mut self) {
        self.term.clear_main();
        self.term.at(4, 5, "1 - Create a clone $5,000");
        self.term.at(4, 8, "2 - Update a braintape $3,000");
        self.term.at(4, 11, "3 - Undergo treatment $100-200");
        self.term.at(4, 14, "4 - Leave the center");
    }

    fn show_menu(&mut self, greeting: &str) {
        self.draw_menu();
        self.stage = Stage::Menu;
        self.asker.clear();
        self.term.prompt(greeting);
    }

    /// A refusal: a beep and the reply.
    fn refuse(&mut self, cx: &mut Cx, msg: &str, next: Next) {
        cx.sfx(Sfx::Beep);
        self.reply(msg, next);
    }

    fn reply(&mut self, msg: &str, next: Next) {
        self.stage = Stage::Busy;
        self.asker.clear();
        self.term.reply(msg);
        self.wait.after(msg, next);
    }

    fn goto(&mut self, cx: &mut Cx, next: Next) -> Option<Trans> {
        match next {
            Next::Password => {
                if rules::wants_password(cx.game()) {
                    self.stage = Stage::Password;
                    self.term.msg("What's the password? ");
                    self.asker.set(Ask::Text(20));
                } else {
                    self.show_menu(HELLO);
                }
            }
            Next::Menu(g) => self.show_menu(g),
            Next::Thanks => self.reply("Thank you.", Next::Menu(WHAT_ELSE)),
            Next::Leave => return Some(leave(cx, Building::GoldCross)),
        }
        None
    }

    fn update_tape(&mut self, cx: &mut Cx) {
        match rules::update_braintape(cx.g()) {
            Ok(()) => {
                cx.sfx(Sfx::Cash);
                cx.save();
                self.reply(SIGN_HERE, Next::Menu(WHAT_ELSE));
            }
            Err(CloneError::NoClone) => {
                self.refuse(cx, "I'm sorry, but our records don't show that you have a clone to update.", Next::Menu(WHAT_ELSE))
            }
            Err(_) => self.refuse(cx, NO_MONEY, Next::Menu(WHAT_ELSE)),
        }
    }

    fn menu_key(&mut self, cx: &mut Cx, c: char) {
        match c {
            '1' => match rules::create_clone(cx.g()) {
                Ok(()) => {
                    cx.sfx(Sfx::Cash);
                    cx.save();
                    self.reply("Very good, every autoduellist needs a clone.", Next::Menu(WHAT_ELSE));
                }
                Err(CloneError::Refused) => {
                    self.refuse(cx, "I'm sorry, but I've been told not to make you a clone.", Next::Menu(WHAT_ELSE))
                }
                Err(CloneError::AlreadyHave) => {
                    self.term.msg("You already have a clone! Would you like to update its braintape?");
                    self.asker.set(Ask::YesNo);
                    self.stage = Stage::UpdateOffer;
                }
                Err(_) => self.refuse(cx, NO_MONEY, Next::Menu(WHAT_ELSE)),
            },
            '2' => self.update_tape(cx),
            '3' => match rules::treatment_quote(cx.game()) {
                Ok(t) => {
                    let s = if t.weeks == 1 { "" } else { "s" };
                    self.term.msg(&format!("You would need {} week{} to heal.\nIs that O.K.?", t.weeks, s));
                    self.asker.set(Ask::YesNo);
                    self.stage = Stage::Treat(t);
                }
                Err(TreatmentError::Fit) => {
                    self.reply("Are you kidding me? You're as fit as a fiddle!", Next::Menu(WHAT_ELSE))
                }
                Err(TreatmentError::Money) => self.refuse(cx, NO_MONEY, Next::Menu(WHAT_ELSE)),
            },
            '4' => self.reply("Take care of yourself!", Next::Leave),
            _ => self.reply("I'm sorry, but I don't understand!", Next::Menu(WHAT_ELSE)),
        }
    }
}

impl Screen for GoldCross {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if let Some((next, pass)) = self.wait.tick(inp) {
            if let Some(t) = self.goto(cx, next) {
                return t;
            }
            if !pass {
                return Trans::None;
            }
        }
        match self.stage {
            Stage::Busy => {}
            // any letter or digit is an answer here; unknown ones get "I don't understand!"
            Stage::Menu => {
                if inp.pressed(Key::Esc) {
                    self.menu_key(cx, '4');
                } else if let Some(c) = inp.char_key() {
                    self.menu_key(cx, c);
                }
            }
            Stage::Password => match self.asker.feed(inp) {
                Some(Answer::Text(t)) => match rules::give_password(cx.g(), &t) {
                    None => self.show_menu(HELLO),
                    Some(h) => {
                        cx.save();
                        let end = match h {
                            HandOver::NoCar => "but you don't have a car. Come back later.",
                            HandOver::NoRoom => "but you don't have room for it. Come back later.",
                            HandOver::Loaded => "someone will load it immediately.",
                        };
                        let m = format!("We need you to take a clone heart to the Boston Gold Cross, {}", end);
                        self.reply(&m, Next::Menu(HELLO));
                    }
                },
                Some(Answer::Cancel) => self.show_menu(HELLO),
                _ => {}
            },
            Stage::UpdateOffer => match self.asker.feed(inp) {
                Some(Answer::Yes) => self.update_tape(cx),
                Some(_) => self.reply("'like living dangerously, huh?", Next::Menu(WHAT_ELSE)),
                None => {}
            },
            Stage::Treat(_) => match self.asker.feed(inp) {
                Some(Answer::Yes) => match rules::treatment(cx.g()) {
                    Ok(t) => {
                        cx.sfx(Sfx::Cash);
                        cx.save();
                        self.reply(&format!("That will be ${} please.", t.price), Next::Thanks);
                    }
                    Err(TreatmentError::Fit) => self.show_menu(WHAT_ELSE),
                    Err(TreatmentError::Money) => self.refuse(cx, NO_MONEY, Next::Menu(WHAT_ELSE)),
                },
                Some(_) => self.reply("You really should take care of those injuries.", Next::Menu(WHAT_ELSE)),
                None => {}
            },
        }
        Trans::None
    }

    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
        // cash, health and where the clone would wake up
        let d = &cx.game().driver;
        draw_line(fb, cx, 16, &format!("Cash: {}   Health: {}", text::money(d.money), d.health));
        let clone = d.clone.as_ref().map(|t| cx.data.city_name(t.city)).unwrap_or("none");
        draw_line(fb, cx, 17, &format!("Clone: {}", clone));
    }
}
