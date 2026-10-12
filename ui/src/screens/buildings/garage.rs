//! The Garage (`ga.prg`): battery, repairs, storage, mechanic lessons.
//! Rules live in [`cardul_sim::shops::garage`].
use super::shopkit::{draw_line, status_line, Wait};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::{Input, Key};
use crate::term::{Answer, Ask, Asker, Term};
use cardul_sim::shops::garage::{self as rules, GarageError, Part, RepairLine, LIST_PAGE};
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Garage::new(cx))
}

const HELLO: &str = "Hi! What would you like?";
const NO_CAR: &str = "I'm sorry, but you don't have a car with you.";
const NO_MONEY: &str = "I'm afraid you don't have enough money.";
const WHAT_FIXED: &str = "What do you want fixed?";

#[derive(Clone, PartialEq)]
enum Stage {
    /// a reply is up; see `wait`
    Busy,
    Menu,
    /// the list of damaged components
    Repair,
    /// "How many points would you like restored?"
    Armor { facing: usize },
    /// "Which car do you want?" (car slots)
    Stored(Vec<usize>),
}

#[derive(Clone, Copy)]
enum Next {
    Menu,
    Repair,
}

pub struct Garage {
    term: Term,
    asker: Asker,
    stage: Stage,
    wait: Wait<Next>,
    /// first line shown of the repair list
    scroll: usize,
}

impl Garage {
    pub fn new(cx: &mut Cx) -> Garage {
        let (term, rep) = enter(cx, Building::Garage);
        let mut g = Garage { term, asker: Asker::default(), stage: Stage::Busy, wait: Wait::default(), scroll: 0 };
        g.draw_menu();
        if rep.lines.is_empty() {
            g.show_menu();
        } else {
            g.wait.after(&rep.lines.join(" "), Next::Menu);
        }
        g
    }

    fn draw_menu(&mut self) {
        self.term.clear_main();
        let items = [
            "1 - Have power supply charged $50",
            "2 - Have car repaired",
            "3 - Store your car",
            "4 - Take a car out of storage $50",
            "5 - Take mechanic lessons $500",
            "6 - Exit the garage",
        ];
        for (i, s) in items.iter().enumerate() {
            self.term.at(2, 4 + 2 * i, s);
        }
    }

    fn show_menu(&mut self) {
        self.draw_menu();
        self.stage = Stage::Menu;
        self.term.prompt(HELLO);
        self.asker.set(Ask::Choice(vec!['1', '2', '3', '4', '5', '6']));
    }

    /// Show a reply, then continue with `next` after a moment.
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

    fn goto(&mut self, cx: &Cx, next: Next) {
        match next {
            Next::Menu => self.show_menu(),
            Next::Repair => self.show_repair(cx),
        }
    }

    fn repair_lines(cx: &Cx) -> Vec<RepairLine> {
        cx.game().car_here().map(rules::repair_list).unwrap_or_default()
    }

    /// The repair list: "Item, Loc, Damaged Pts, Max Pts, Max Cost to Repair" (`ga $A1C6`).
    fn show_repair(&mut self, cx: &Cx) {
        let list = Garage::repair_lines(cx);
        if list.is_empty() {
            return self.show_menu();
        }
        self.scroll = self.scroll.min(list.len().saturating_sub(LIST_PAGE));
        let t = &mut self.term;
        t.clear_main();
        t.at(5, 1, "Damaged");
        t.at(26, 1, "Max. Cost to");
        t.at(6, 2, "Item");
        t.at(16, 2, "Loc. Pts. Pts. Repair");
        for (n, l) in list.iter().skip(self.scroll).take(LIST_PAGE).enumerate() {
            let row = 4 + n;
            t.at(1, row, &((n + 1) % 10).to_string());
            t.at(3, row, &l.name);
            t.at(16, row, l.loc);
            t.at(22, row, &format!("{:>2}", l.cur));
            t.at(27, row, &format!("{:>2}", l.max));
            t.at(32, row, &format!("${}", l.cost));
        }
        // FIX (remake): everything at once
        let total = cx.game().car_here().map(|c| c.full_repair_cost()).unwrap_or(0);
        t.at(1, 15, "A - Repair everything");
        t.at(32, 15, &format!("${}", total));
        t.at(10, 16, "Press Esc for none");
        self.stage = Stage::Repair;
        self.asker.clear();
        self.term.msg(WHAT_FIXED);
        if list.len() > LIST_PAGE {
            self.term.msg_add("Use arrows to scroll list.");
        }
    }

    /// After a successful repair: back to the list, or "It's all fixed now."
    fn repaired(&mut self, cx: &mut Cx) {
        cx.sfx(Sfx::Cash);
        cx.save();
        if Garage::repair_lines(cx).is_empty() {
            self.reply("It's all fixed now.", Next::Menu);
        } else {
            self.reply("I'll fix it right up.", Next::Repair);
        }
    }

    fn repair_error(&mut self, cx: &mut Cx, e: GarageError) {
        match e {
            GarageError::Money => self.refuse(cx, NO_MONEY, Next::Repair),
            GarageError::NoCar => self.refuse(cx, NO_CAR, Next::Menu),
            _ => self.show_repair(cx),
        }
    }

    fn pick_repair(&mut self, cx: &mut Cx, line: &RepairLine) {
        match line.part {
            Part::Armor(f) => {
                let max = cx.game().car_here().map(|c| rules::armor_missing(c, f)).unwrap_or(0);
                self.stage = Stage::Armor { facing: f };
                self.term.msg(&format!("How many points would you like restored ({} max)? ", max));
                self.asker.set(Ask::Digits(2));
            }
            part => match rules::repair(cx.g(), part) {
                Ok(_) => self.repaired(cx),
                Err(e) => self.repair_error(cx, e),
            },
        }
    }

    fn repair_keys(&mut self, cx: &mut Cx, inp: &Input) -> Option<Trans> {
        let list = Garage::repair_lines(cx);
        for k in &inp.keys {
            match k {
                Key::Esc | Key::F(1) => {
                    self.show_menu();
                    return None;
                }
                Key::Up if self.scroll > 0 => {
                    self.scroll -= 1;
                    self.show_repair(cx);
                }
                Key::Down if self.scroll + LIST_PAGE < list.len() => {
                    self.scroll += 1;
                    self.show_repair(cx);
                }
                Key::Char('a') => {
                    match rules::repair_all(cx.g()) {
                        Ok(_) => {
                            cx.sfx(Sfx::Cash);
                            cx.save();
                            self.reply("It's all fixed now.", Next::Menu);
                        }
                        Err(e) => self.repair_error(cx, e),
                    }
                    return None;
                }
                Key::Char(c) if c.is_ascii_digit() => {
                    let d = c.to_digit(10).unwrap() as usize;
                    let i = if d == 0 { 9 } else { d - 1 };
                    if let Some(line) = list.get(self.scroll + i).filter(|_| i < LIST_PAGE) {
                        let line = line.clone();
                        self.pick_repair(cx, &line);
                        return None;
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn menu_choice(&mut self, cx: &mut Cx, c: char) -> Option<Trans> {
        match c {
            '1' => match rules::charge_battery(cx.g()) {
                Ok(()) => {
                    cx.sfx(Sfx::Cash);
                    cx.save();
                    self.reply("I'll fill'er to capacity!", Next::Menu);
                }
                Err(GarageError::NoCar) => self.refuse(cx, NO_CAR, Next::Menu),
                Err(GarageError::BatteryFull) => self.refuse(cx, "The batteries don't need charging.", Next::Menu),
                Err(_) => self.refuse(cx, NO_MONEY, Next::Menu),
            },
            '2' => {
                if cx.game().car_here().is_none() {
                    self.refuse(cx, NO_CAR, Next::Menu);
                } else if Garage::repair_lines(cx).is_empty() {
                    self.reply("I don't understand. There's nothing wrong with your car.", Next::Menu);
                } else {
                    self.scroll = 0;
                    self.show_repair(cx);
                }
            }
            '3' => match rules::store_car(cx.g()) {
                Ok(name) => {
                    cx.sfx(Sfx::Click);
                    cx.save();
                    self.reply(&format!("I'll take good care of {}.", name), Next::Menu);
                }
                Err(_) => self.refuse(cx, NO_CAR, Next::Menu),
            },
            '4' => {
                let list = rules::stored_here(cx.game());
                if list.is_empty() {
                    self.refuse(cx, "I'm sorry, but none of your cars are here.", Next::Menu);
                } else {
                    self.term.clear_main();
                    for (n, slot) in list.iter().enumerate() {
                        let name = cx.game().cars[*slot].as_ref().map(|c| c.name.clone()).unwrap_or_default();
                        self.term.at(9, 1 + n, &format!("{} - {}", n + 1, name));
                    }
                                self.term.msg("Which car do you want? Press Esc for none.");
                    let keys = (1..=list.len()).map(|n| char::from_digit(n as u32, 10).unwrap()).collect();
                    self.asker.set(Ask::Choice(keys));
                    self.stage = Stage::Stored(list);
                }
            }
            '5' => match rules::take_lessons(cx.g()) {
                Ok(m) => {
                    cx.sfx(Sfx::Cash);
                    cx.save();
                    self.reply(&format!("Very good... After hard work your mechanic skill is {}.", m), Next::Menu);
                }
                Err(_) => self.refuse(cx, NO_MONEY, Next::Menu),
            },
            _ => {
                // 6 - Exit
                if !rules::can_leave(cx.game()) {
                    self.refuse(cx, "You can't leave in that car! Its batteries need charging.", Next::Menu);
                } else {
                    return Some(leave(cx, Building::Garage));
                }
            }
        }
        None
    }
}

impl Screen for Garage {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if let Some((next, pass)) = self.wait.tick(inp) {
            self.goto(cx, next);
            if !pass {
                return Trans::None;
            }
        }
        match self.stage.clone() {
            Stage::Busy => {}
            Stage::Menu => match self.asker.feed(inp) {
                Some(Answer::Choice(c)) => {
                    if let Some(t) = self.menu_choice(cx, c) {
                        return t;
                    }
                }
                Some(Answer::Cancel) => {
                    if let Some(t) = self.menu_choice(cx, '6') {
                        return t;
                    }
                }
                _ => {}
            },
            Stage::Repair => {
                if let Some(t) = self.repair_keys(cx, inp) {
                    return t;
                }
            }
            Stage::Armor { facing } => match self.asker.feed(inp) {
                Some(Answer::Number(0)) | Some(Answer::Cancel) => self.show_repair(cx),
                Some(Answer::Number(n)) => match rules::repair_armor(cx.g(), facing, n.min(99) as u8) {
                    Ok(_) => self.repaired(cx),
                    Err(GarageError::TooMany) => {
                        let max = cx.game().car_here().map(|c| rules::armor_missing(c, facing)).unwrap_or(0);
                        self.term.msg(&format!("How many points would you like restored ({} max)? ", max));
                        self.asker.set(Ask::Digits(2));
                    }
                    Err(e) => self.repair_error(cx, e),
                },
                _ => {}
            },
            Stage::Stored(list) => match self.asker.feed(inp) {
                Some(Answer::Choice(c)) => {
                    let slot = list[c.to_digit(10).unwrap() as usize - 1];
                    match rules::take_out(cx.g(), slot) {
                        Ok(()) => {
                            cx.sfx(Sfx::Cash);
                            cx.save();
                            self.show_menu();
                        }
                        Err(GarageError::Money) => self.refuse(cx, NO_MONEY, Next::Menu),
                        Err(_) => self.show_menu(),
                    }
                }
                Some(Answer::Cancel) => self.show_menu(),
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
