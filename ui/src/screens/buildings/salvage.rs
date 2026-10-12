//! The Salvage Yard (`sa.prg`): sell your car, its weapons or salvaged parts; the
//! fixed-autoduels quest (clue in Buffalo, password in Washington).
//! Rules live in [`cardul_sim::shops::salvage`].
use super::shopkit::{draw_line, num, status_line, Wait};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::{Input, Key};
use crate::term::{Answer, Ask, Asker, Term};
use cardul_sim::shops::garage::WEAPON_NAMES10;
use cardul_sim::shops::goldcross::HandOver;
use cardul_sim::shops::salvage as rules;
use cardul_sim::tables::LOCATION_NAMES;
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(SalvageYard::new(cx))
}

const HELLO: &str = "Wadda ya want?";
const NOW_WHAT: &str = "Now wadda ya want?";
const NO_CAR: &str = "Where's th' car?";
const NOTHING: &str = "You ain't got nothin' to sell!";
const MINE: &str = "Awright, it's mine now!";
const REFUSED: &str = "Fine wid me, it's th' best offer you'll git!";
const WHICH: &str = "O.K. Which one d'ya wanna git rid uv?";

#[derive(Clone, PartialEq)]
enum Stage {
    /// a reply is up; see `wait`
    Busy,
    Menu,
    /// "What's the password?" (Washington)
    Password,
    SellCar,
    /// the weapon list; `from_menu` = nothing sold yet in this visit to the list
    Weapons { from_menu: bool },
    WeaponOffer { slot: usize },
    Goods,
}

#[derive(Clone, Copy)]
enum Next {
    /// after the delivery messages and the Buffalo clue: the password or the menu
    Password,
    Menu(&'static str),
    Weapons,
    Leave,
}

pub struct SalvageYard {
    term: Term,
    asker: Asker,
    stage: Stage,
    wait: Wait<Next>,
    /// sounds for the next update (most replies are made without the context at hand)
    sfx: Vec<Sfx>,
}

impl SalvageYard {
    pub fn new(cx: &mut Cx) -> SalvageYard {
        let (term, rep) = enter(cx, Building::Salvage);
        let mut s = SalvageYard { term, asker: Asker::default(), stage: Stage::Busy, wait: Wait::default(), sfx: vec![] };
        s.draw_menu();
        let mut text = rep.lines.join(" ");
        if let Some(c) = rules::big_clue(cx.game()) {
            s.term.msg_add(c);
            text.push(' ');
            text.push_str(c);
        }
        if text.is_empty() {
            let _ = s.goto(cx, Next::Password);
        } else {
            s.wait.after(&text, Next::Password);
        }
        s
    }

    fn draw_menu(&mut self) {
        self.term.clear_main();
        self.term.at(5, 5, "1 - Sell a car");
        self.term.at(5, 8, "2 - Sell weapons off your car");
        self.term.at(5, 11, "3 - Sell salvaged goods");
        self.term.at(5, 14, "4 - Leave the yard");
    }

    fn show_menu(&mut self, greeting: &str) {
        self.draw_menu();
        self.stage = Stage::Menu;
        self.term.prompt(greeting);
        self.asker.set(Ask::Choice(vec!['1', '2', '3', '4']));
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
            Next::Weapons => self.show_weapons(cx, false, WHICH),
            Next::Leave => return Some(leave(cx, Building::Salvage)),
        }
        None
    }

    /// "2 - Sell weapons off your car": the ten weapon slots with what the yard pays.
    fn show_weapons(&mut self, cx: &Cx, from_menu: bool, msg: &str) {
        let Some(car) = cx.game().car_here() else {
            return self.refuse(NO_CAR, Next::Menu(NOW_WHAT));
        };
        let lines: Vec<Option<(String, &str, u32)>> = car
            .weapons
            .iter()
            .map(|w| w.map(|w| (WEAPON_NAMES10[w.kind as usize].to_string(), LOCATION_NAMES[w.mount as usize], rules::weapon_offer(&w))))
            .collect();
        let t = &mut self.term;
        t.clear_main();
        t.at(13, 1, "Weapon");
        t.at(24, 1, "Location");
        t.at(33, 1, "Offer");
        for (i, l) in lines.iter().enumerate() {
            let row = 4 + i;
            t.at(8, row, &((i + 1) % 10).to_string());
            match l {
                Some((name, loc, offer)) => {
                    t.at(11, row, name);
                    t.at(27, row, loc);
                    t.at(32, row, &format!("{:>6}", format!("${}", offer)));
                }
                None => t.at(11, row, "..........      ..."),
            }
        }
        t.at(10, 16, "Press Esc for none");
        self.stage = Stage::Weapons { from_menu };
        self.asker.clear();
        self.term.msg(msg);
    }

    /// A refusal: a beep and the reply.
    fn refuse(&mut self, msg: &str, next: Next) {
        self.sfx.push(Sfx::Beep);
        self.reply(msg, next);
    }

    fn offer(&mut self, msg: String, stage: Stage) {
        self.term.msg(&msg);
        self.asker.set(Ask::YesNo);
        self.stage = stage;
    }

    fn menu_choice(&mut self, cx: &Cx, c: char) {
        let g = cx.game();
        let car = if g.driver.loaner { None } else { g.car_here() };
        match c {
            '1' => match car {
                None => self.refuse(NO_CAR, Next::Menu(NOW_WHAT)),
                Some(car) => {
                    let o = rules::car_offer(car);
                    self.offer(format!("I'll give ya {} bucks fer it, O.K.?", num(o)), Stage::SellCar);
                }
            },
            '2' => match car {
                None => self.refuse(NO_CAR, Next::Menu(NOW_WHAT)),
                Some(car) if car.weapons.iter().all(|w| w.is_none()) => self.refuse(NOTHING, Next::Menu(NOW_WHAT)),
                Some(_) => self.show_weapons(cx, true, WHICH),
            },
            '3' => match car.map(rules::salvage_value) {
                None => self.refuse(NO_CAR, Next::Menu(NOW_WHAT)),
                Some(None) => self.refuse(NOTHING, Next::Menu(NOW_WHAT)),
                Some(Some(v)) => self.offer(
                    format!("I'll give ya ${} bucks fer all the salvaged stuff in your cargo area, O.K.?", num(v)),
                    Stage::Goods,
                ),
            },
            _ => self.reply("Good, I didn't like ya anyway!", Next::Leave),
        }
    }

    fn weapon_keys(&mut self, cx: &mut Cx, inp: &Input, from_menu: bool) {
        for k in &inp.keys {
            match k {
                Key::Esc | Key::F(1) => {
                    if from_menu {
                        self.reply("Stop wastin' my time!", Next::Menu(NOW_WHAT));
                    } else {
                        self.show_menu(NOW_WHAT);
                    }
                    return;
                }
                Key::Char(c) if c.is_ascii_digit() => {
                    let d = c.to_digit(10).unwrap() as usize;
                    let slot = if d == 0 { 9 } else { d - 1 };
                    match cx.game().car_here().and_then(|car| car.weapons[slot]) {
                        None => self.refuse("There's nothin' there!", Next::Weapons),
                        Some(w) => {
                            let o = rules::weapon_offer(&w);
                            self.offer(format!("I'll give ya {} bucks fer it, O.K.?", num(o)), Stage::WeaponOffer { slot });
                        }
                    }
                    return;
                }
                _ => {}
            }
        }
    }
}

impl Screen for SalvageYard {
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
                Some(Answer::Cancel) => self.menu_choice(cx, '4'),
                _ => {}
            },
            Stage::Password => match self.asker.feed(inp) {
                Some(Answer::Text(t)) => match rules::give_password(cx.g(), &t) {
                    None => self.show_menu(HELLO),
                    Some(h) => {
                        cx.save();
                        let end = match h {
                            HandOver::NoCar => "but you don't have a car. Come back later.",
                            HandOver::NoRoom => "but you don't have room for them. Come back later.",
                            HandOver::Loaded => "someone will load them immediately.",
                        };
                        let m = format!("Get these autoduel fixes to the FBI and heads will roll, {}", end);
                        self.reply(&m, Next::Menu(HELLO));
                    }
                },
                Some(Answer::Cancel) => self.show_menu(HELLO),
                _ => {}
            },
            Stage::SellCar => match self.asker.feed(inp) {
                Some(Answer::Yes) => match rules::sell_car(cx.g()) {
                    Ok(_) => {
                        cx.sfx(Sfx::Cash);
                        // the original saves the driver at once ("CR")
                        cx.save();
                        self.reply(MINE, Next::Menu(NOW_WHAT));
                    }
                    Err(_) => self.refuse(NO_CAR, Next::Menu(NOW_WHAT)),
                },
                Some(_) => self.reply(REFUSED, Next::Menu(NOW_WHAT)),
                None => {}
            },
            Stage::Weapons { from_menu } => self.weapon_keys(cx, inp, from_menu),
            Stage::WeaponOffer { slot } => match self.asker.feed(inp) {
                Some(Answer::Yes) => {
                    let _ = rules::sell_weapon(cx.g(), slot);
                    cx.sfx(Sfx::Cash);
                    cx.save();
                    let left = cx.game().car_here().map(|c| c.weapons.iter().any(|w| w.is_some())).unwrap_or(false);
                    if left {
                        self.show_weapons(cx, false, "Awright, anythin' else?");
                    } else {
                        self.reply(MINE, Next::Menu(NOW_WHAT));
                    }
                }
                Some(_) => self.reply(REFUSED, Next::Menu(NOW_WHAT)),
                None => {}
            },
            Stage::Goods => match self.asker.feed(inp) {
                Some(Answer::Yes) => {
                    let _ = rules::sell_salvage(cx.g());
                    cx.sfx(Sfx::Cash);
                    cx.save();
                    self.reply(MINE, Next::Menu(NOW_WHAT));
                }
                Some(_) => self.reply(REFUSED, Next::Menu(NOW_WHAT)),
                None => {}
            },
        }
        Trans::None
    }

    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
        draw_line(fb, cx, 17, &status_line(cx.game()));
    }
}
