//! The Weapon Shop (`we.prg`): buy weapons and ammunition; the decoy-quest clues.
//! Rules live in [`cardul_sim::shops::weapons`].
use super::shopkit::{draw_line, status_line, Wait};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::term::{Answer, Ask, Asker, Term};
use cardul_sim::shops::weapons::{self as rules, Shortfall, WeaponError};
use cardul_sim::tables::{LOCATION_NAMES, MOUNT_NAMES, WEAPONS};
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(WeaponShop::new(cx))
}

const HELLO: &str = "What can I interest you in, sir?";
const ANYTHING_ELSE: &str = "Anything else?";
const NO_CAR: &str = "You don't have a car for me to put anything on, sir!";
const CHANGED_MIND: &str = "Changed your mind, I see.";

#[derive(Clone, PartialEq)]
enum Stage {
    /// a reply is up; see `wait`
    Busy,
    Menu,
    /// weapons A-L with prices
    BuyList,
    /// "And where would you like it installed?"
    Mount { kind: u8, mounts: Vec<u8> },
    /// weapons that take ammunition (car slots)
    AmmoList(Vec<usize>),
    /// "How many rounds will you take?"
    AmmoCount { slot: usize },
    /// "That will be $X, O.K.?"
    AmmoConfirm { slot: usize, rounds: u8 },
}

#[derive(Clone, Copy)]
enum Next {
    /// back to the menu with this greeting
    Menu(&'static str),
    BuyList,
    AmmoCount(usize),
    Leave,
}

pub struct WeaponShop {
    term: Term,
    asker: Asker,
    stage: Stage,
    wait: Wait<Next>,
    /// sounds for the next update (most replies are made without the context at hand)
    sfx: Vec<Sfx>,
}

impl WeaponShop {
    pub fn new(cx: &mut Cx) -> WeaponShop {
        let (term, rep) = enter(cx, Building::Weapons);
        let mut s = WeaponShop { term, asker: Asker::default(), stage: Stage::Busy, wait: Wait::default(), sfx: vec![] };
        s.draw_menu();
        let clue = rules::clue(cx.game());
        let mut text = rep.lines.join(" ");
        if let Some(c) = clue {
            // the clue follows any delivery messages (`we $A14D`)
            s.term.msg_add(c);
            text.push(' ');
            text.push_str(c);
        }
        if text.is_empty() {
            s.show_menu(HELLO);
        } else {
            s.wait.after(&text, Next::Menu(HELLO));
        }
        s
    }

    fn draw_menu(&mut self) {
        self.term.clear_main();
        self.term.at(10, 6, "1 - Buy weapons");
        self.term.at(10, 10, "2 - Buy ammunition");
        self.term.at(10, 14, "3 - Leave the shop");
    }

    fn show_menu(&mut self, greeting: &str) {
        self.draw_menu();
        self.stage = Stage::Menu;
        self.term.prompt(greeting);
        self.asker.set(Ask::Choice(vec!['1', '2', '3']));
    }

    fn reply(&mut self, msg: &str, next: Next) {
        self.stage = Stage::Busy;
        self.asker.clear();
        self.term.reply(msg);
        self.wait.after(msg, next);
    }

    fn goto(&mut self, cx: &mut Cx, next: Next) -> Option<Trans> {
        match next {
            Next::Menu(g) => self.show_menu(g),
            Next::BuyList => self.show_buy_list(),
            Next::AmmoCount(slot) => self.ask_rounds(cx, slot),
            Next::Leave => return Some(leave(cx, Building::Weapons)),
        }
        None
    }

    /// "1 - Buy weapons": the price list, A-L.
    fn show_buy_list(&mut self) {
        let t = &mut self.term;
        t.clear_main();
        t.at(12, 1, "Weapon");
        t.at(28, 1, "Cost");
        for (i, w) in WEAPONS.iter().enumerate() {
            let row = 3 + i;
            t.at(5, row, &((b'A' + i as u8) as char).to_string());
            t.at(7, row, w.name);
            t.at(27, row, &format!("{:>5}", format!("${}", w.price)));
        }
        t.at(10, 16, "Press Esc for none");
        self.stage = Stage::BuyList;
        self.term.msg("What weapon would you like?");
        self.asker.set(Ask::Choice(('a'..='l').collect()));
    }

    fn not_enough(&mut self, s: Shortfall, what: &str, next: Next) {
        let m = format!("I hate to tell you this, but you don't have enough {} for that {}.", s.word(), what);
        self.refuse(&m, next);
    }
    /// A refusal: a beep and the reply.
    fn refuse(&mut self, msg: &str, next: Next) {
        self.sfx.push(Sfx::Beep);
        self.reply(msg, next);
    }

    fn pick_weapon(&mut self, cx: &Cx, kind: u8) {
        let g = cx.game();
        let Some(car) = g.car_here() else {
            return self.refuse(NO_CAR, Next::Menu(ANYTHING_ELSE));
        };
        if let Err(s) = rules::check_weapon(car, g.driver.money, kind) {
            return self.not_enough(s, "weapon", Next::BuyList);
        }
        let mounts = rules::allowed_mounts(kind);
        let t = &mut self.term;
        t.clear_main();
        t.at(13, 1, WEAPONS[kind as usize].name);
        for (i, m) in mounts.iter().enumerate() {
            t.at(13, 4 + 3 * i, &format!("{} - {}", i + 1, MOUNT_NAMES[*m as usize]));
        }
        t.at(10, 16, "Press Esc for none");
        self.term.msg("An excellent choice. And where would you like it installed?");
        let keys = (1..=mounts.len()).map(|n| char::from_digit(n as u32, 10).unwrap()).collect();
        self.asker.set(Ask::Choice(keys));
        self.stage = Stage::Mount { kind, mounts };
    }

    /// "2 - Buy ammunition": the weapons that take ammunition.
    fn show_ammo_list(&mut self, cx: &Cx) {
        let Some(car) = cx.game().car_here() else {
            return self.refuse(NO_CAR, Next::Menu(ANYTHING_ELSE));
        };
        let slots = rules::ammo_slots(car);
        if slots.is_empty() {
            return self.refuse("Excuse me, but you don't have a weapon that takes ammunition.", Next::Menu(ANYTHING_ELSE));
        }
        // weapon, mount and rounds loaded (the original showed only the name)
        let lines: Vec<(String, u8)> = slots
            .iter()
            .filter_map(|s| car.weapons[*s])
            .map(|w| (format!("{} {}", w.info().name, LOCATION_NAMES[w.mount as usize]), w.ammo))
            .collect();
        let t = &mut self.term;
        t.clear_main();
        t.at(28, 1, "Rounds");
        for (n, (name, ammo)) in lines.iter().enumerate() {
            let row = 3 + n;
            t.at(6, row, &((n + 1) % 10).to_string());
            t.at(9, row, name);
            t.at(30, row, &format!("{:>2}", ammo));
        }
        t.at(10, 16, "Press Esc for none");
        self.term.msg("What weapon do you want ammunition for, sir?");
        let keys = (1..=slots.len()).map(|n| char::from_digit((n % 10) as u32, 10).unwrap()).collect();
        self.asker.set(Ask::Choice(keys));
        self.stage = Stage::AmmoList(slots);
    }

    /// "You have room for N rounds in the magazine. They are $X each."
    fn ask_rounds(&mut self, cx: &Cx, slot: usize) {
        let Some(car) = cx.game().car_here() else {
            return self.show_menu(HELLO);
        };
        let room = rules::magazine_room(car, slot);
        if room == 0 {
            return self.refuse("You have room for no rounds in the magazine.", Next::Menu(ANYTHING_ELSE));
        }
        let price = rules::round_price(car, slot);
        self.term.msg(&format!(
            "You have room for {} rounds in the magazine. They are ${} each.\nHow many rounds will you take? ",
            room, price
        ));
        self.asker.set(Ask::Digits(2));
        self.stage = Stage::AmmoCount { slot };
    }

    fn menu_choice(&mut self, cx: &Cx, c: char) {
        match c {
            '1' => {
                if cx.game().car_here().is_none() {
                    self.refuse(NO_CAR, Next::Menu(ANYTHING_ELSE));
                } else {
                    self.show_buy_list();
                }
            }
            '2' => self.show_ammo_list(cx),
            _ => self.reply("Best of luck!", Next::Leave),
        }
    }
}

impl Screen for WeaponShop {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        for s in self.sfx.drain(..) {
            cx.sfx(s);
        }
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
        match self.stage.clone() {
            Stage::Busy => {}
            Stage::Menu => match self.asker.feed(inp) {
                Some(Answer::Choice(c)) => self.menu_choice(cx, c),
                Some(Answer::Cancel) => self.menu_choice(cx, '3'),
                _ => {}
            },
            Stage::BuyList => match self.asker.feed(inp) {
                Some(Answer::Choice(c)) => self.pick_weapon(cx, c as u8 - b'a'),
                Some(Answer::Cancel) => self.reply("Maybe some other time, then.", Next::Menu("Can I get you anything at all?")),
                _ => {}
            },
            Stage::Mount { kind, mounts } => match self.asker.feed(inp) {
                Some(Answer::Choice(c)) => {
                    let mount = mounts[c.to_digit(10).unwrap() as usize - 1];
                    match rules::buy_weapon(cx.g(), kind, mount) {
                        Ok(_) => {
                            cx.sfx(Sfx::Cash);
                            cx.save();
                            self.reply("I'll get my men right on it!", Next::Menu(HELLO));
                        }
                        Err(WeaponError::Short(s)) => self.not_enough(s, "weapon", Next::BuyList),
                        Err(_) => self.refuse(NO_CAR, Next::Menu(ANYTHING_ELSE)),
                    }
                }
                Some(Answer::Cancel) => self.reply(CHANGED_MIND, Next::Menu(HELLO)),
                _ => {}
            },
            Stage::AmmoList(slots) => match self.asker.feed(inp) {
                Some(Answer::Choice(c)) => {
                    let n = c.to_digit(10).unwrap() as usize;
                    let i = if n == 0 { 9 } else { n - 1 };
                    self.ask_rounds(cx, slots[i]);
                }
                Some(Answer::Cancel) => self.reply(CHANGED_MIND, Next::Menu(HELLO)),
                _ => {}
            },
            Stage::AmmoCount { slot } => match self.asker.feed(inp) {
                Some(Answer::Number(0)) | Some(Answer::Cancel) => self.reply(CHANGED_MIND, Next::Menu(HELLO)),
                Some(Answer::Number(n)) => {
                    let g = cx.game();
                    let rounds = n.min(99) as u8;
                    match g.car_here().map(|c| rules::check_ammo(c, g.driver.money, slot, rounds)) {
                        Some(Ok(price)) => {
                            self.term.msg(&format!("That will be ${}, O.K.?", price));
                            self.asker.set(Ask::YesNo);
                            self.stage = Stage::AmmoConfirm { slot, rounds };
                        }
                        Some(Err(s)) => self.not_enough(s, "much ammunition", Next::AmmoCount(slot)),
                        None => self.show_menu(HELLO),
                    }
                }
                _ => {}
            },
            Stage::AmmoConfirm { slot, rounds } => match self.asker.feed(inp) {
                Some(Answer::Yes) => match rules::buy_ammo(cx.g(), slot, rounds) {
                    Ok(_) => {
                        cx.sfx(Sfx::Cash);
                        cx.save();
                        self.reply("Thank you", Next::Menu(ANYTHING_ELSE));
                    }
                    Err(WeaponError::Short(s)) => self.not_enough(s, "much ammunition", Next::AmmoCount(slot)),
                    Err(_) => self.show_menu(HELLO),
                },
                Some(Answer::No) => self.ask_rounds(cx, slot),
                _ => {}
            },
        }
        Trans::None
    }

    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
        draw_line(fb, cx, 17, &status_line(cx.game()));
    }
}
