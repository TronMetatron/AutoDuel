//! The Assembly Line (`as.prg`): name a new car, design it on the live car sheet, buy it.
//!
//! Keys on the design sheet (as in the original): down or "/" = next field, up or Enter =
//! previous field, left/right = change the field (wrapping; held keys repeat), "D" = done,
//! Esc or F1 = abort. The remake adds "=" (or "+") / "-" to change armor and ammunition by 10.
//! Rules live in [`cardul_sim::shops::assembly`].
use super::shopkit::Wait;
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::assets::Assets;
use crate::fb::{col, Frame};
use crate::input::{Input, Key};
use crate::term::{Answer, Ask, Asker, Term};
use crate::text;
use cardul_sim::car::Car;
use cardul_sim::shops::assembly::{self as rules, BuyError, Entry, Field, FIELDS};
use cardul_sim::tables::{self, BODIES, CHASSIS_NAMES, SUSPENSION_NAMES, TIRES};
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(AssemblyLine::new(cx))
}

const HELP: &str = "Please design your car using the arrow keys; - and = change by 10. Press \"D\" when done or Esc to abort.";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// a message is up; see `wait`
    Busy,
    /// "What would you like to name your car?"
    Name,
    /// the cursor is on the car sheet
    Design,
}

#[derive(Clone, Copy)]
enum Next {
    Name,
    Leave,
}

pub struct AssemblyLine {
    term: Term,
    asker: Asker,
    stage: Stage,
    wait: Wait<Next>,
    /// the design (the original's $9E00 buffer)
    car: Car,
    /// cursor field 0..40
    cursor: usize,
    /// the car has been bought (the sheet then shows the money actually left)
    paid: bool,
}

impl AssemblyLine {
    pub fn new(cx: &mut Cx) -> AssemblyLine {
        let (mut term, rep) = enter(cx, Building::Assembly);
        // the car sheet fills the screen; no title line
        term.title.clear();
        let car = rules::new_design(cx.game(), "");
        let mut s = AssemblyLine { term, asker: Asker::default(), stage: Stage::Busy, wait: Wait::default(), car, cursor: 0, paid: false };
        let entry = rules::enter(cx.g());
        cx.save();
        match entry {
            Entry::NoRoom => {
                let m = "Unfortunately, you don't have enough room for any more cars.";
                s.term.msg_add(m);
                s.term.msg_add("Sorry we couldn't help ya.");
                s.wait.after(m, Next::Leave);
            }
            Entry::Design { stored: Some(name) } => {
                let m = format!("We'll just put {} in storage at the garage.", name);
                s.term.msg_add(&m);
                s.wait.after(&m, Next::Name);
            }
            Entry::Design { stored: None } if !rep.lines.is_empty() => {
                let m = rep.lines.join(" ");
                s.wait.after(&m, Next::Name);
            }
            Entry::Design { stored: None } => s.ask_name(),
        }
        s
    }

    fn ask_name(&mut self) {
        self.stage = Stage::Name;
        self.term.msg("What would you like to name your car?");
        self.asker.set(Ask::Text(rules::NAME_LEN));
    }

    /// Message area while designing: the problems of the design (FIX: the original only showed
    /// "??" in the failing fields), or the key help when it is fine.
    fn show_design_msgs(&mut self, cx: &Cx) {
        let problems = rules::explain(&self.car, cx.game().driver.money);
        if problems.is_empty() {
            self.term.msg(HELP);
        } else {
            self.term.msg_clear();
            if problems.len() < 3 {
                self.term.msg_add("This design won't work:");
            }
            for p in problems {
                self.term.msg_add(&p);
            }
        }
    }

    fn abort(&mut self) {
        self.stage = Stage::Busy;
        self.asker.clear();
        let m = "Sorry we couldn't help ya.";
        self.term.msg(m);
        self.wait.after(m, Next::Leave);
    }

    fn finish(&mut self, cx: &mut Cx) {
        match rules::buy(cx.g(), self.car.clone()) {
            Ok(_) => {
                cx.sfx(Sfx::Cash);
                cx.save();
                self.paid = true;
                self.stage = Stage::Busy;
                let m = "A very good choice. We'll have 'er right out momentarily.";
                self.term.msg(m);
                self.wait.after(m, Next::Leave);
            }
            Err(BuyError::Design(_)) | Err(BuyError::NoRoom) => {
                cx.sfx(Sfx::Beep);
                self.term.msg("I'm sorry, but that design just won't work!");
                for p in rules::explain(&self.car, cx.game().driver.money).into_iter().take(1) {
                    self.term.msg_add(&p);
                }
            }
        }
    }

    fn design_keys(&mut self, cx: &mut Cx, inp: &Input) -> Option<Trans> {
        let mut changed = false;
        for k in &inp.keys {
            let field = Field::at(self.cursor);
            match k {
                Key::Esc | Key::F(1) => {
                    self.abort();
                    return None;
                }
                Key::Down | Key::Char('/') => self.cursor = (self.cursor + 1) % FIELDS,
                Key::Up | Key::Enter => self.cursor = (self.cursor + FIELDS - 1) % FIELDS,
                Key::Left => {
                    rules::adjust(&mut self.car, field, -1);
                    changed = true;
                }
                Key::Right => {
                    rules::adjust(&mut self.car, field, 1);
                    changed = true;
                }
                Key::Char('+') | Key::Char('=') => {
                    rules::adjust(&mut self.car, field, 10);
                    changed = true;
                }
                Key::Char('-') => {
                    rules::adjust(&mut self.car, field, -10);
                    changed = true;
                }
                Key::Char('d') => {
                    self.finish(cx);
                    return None;
                }
                _ => {}
            }
        }
        if changed {
            self.show_design_msgs(cx);
        }
        None
    }

    /// The car sheet (`SR_car_screen` layout, subrout $45CD) with the design's live values.
    fn sheet(&self, cx: &Cx) -> Sheet {
        let g = cx.game();
        let car = &self.car;
        let mut p = Sheet::new();
        p.put(1, 1, "Drvr:");
        p.put(6, 1, &g.driver.name);
        p.put(2, 2, "Car:");
        p.put(6, 2, &car.name);
        p.put(1, 3, "Money:$");
        let left = if self.paid { g.driver.money as i64 } else { rules::money_left(car, g.driver.money) };
        p.put(8, 3, &if left < 0 { "??????".to_string() } else { left.to_string() });
        p.put(2, 4, "Cost:$");
        p.put(8, 4, &car.value().to_string());
        p.put(2, 6, "Body:");
        p.put(7, 6, BODIES[car.body as usize].name);
        p.put(2, 7, "Chas:");
        p.put(7, 7, CHASSIS_NAMES[car.chassis as usize]);
        p.put(2, 8, "Susp:");
        p.put(7, 8, SUSPENSION_NAMES[car.suspension as usize]);
        p.put(4, 10, "Max Wt:");
        p.put(11, 10, &car.max_load().to_string());
        p.put(4, 11, "Max Sp:");
        p.put(11, 11, &car.spaces().to_string());
        p.put(6, 12, "Top:");
        p.put(10, 12, &car.top_mph().to_string());
        p.put(6, 13, "Hcl:");
        p.put(10, 13, &car.handling_class().to_string());
        p.put(3, 15, "Wt Left:");
        let wl = car.weight_left();
        p.put(11, 15, &if wl < 0 { "????".to_string() } else { wl.to_string() });
        p.put(3, 16, "Sp Left:");
        let sl = car.spaces_left();
        p.put(11, 16, &if sl < 0 { "??".to_string() } else { sl.to_string() });
        p.put(6, 17, "Acc:");
        p.put(10, 17, &car.accel_mph().map(|v| v.to_string()).unwrap_or_else(|| "??".into()));
        p.put(6, 18, "Bat:");
        p.put(10, 18, &car.battery.to_string());
        p.put(1, 19, "D done   Esc abort");
        p.put(17, 11, "Armr");
        for (i, l) in ["F:", "B:", "L:", "R:", "U:"].iter().enumerate() {
            p.put(17, 12 + i, l);
            p.put(19, 12 + i, &format!("{:>2}", car.armor[i].max));
        }
        p.put(24, 1, "Item");
        p.put(30, 1, "Pt");
        p.put(33, 1, "Loc");
        p.put(27, 2, "Plant");
        p.put(23, 3, &format!("{:<6} {:>2} Ctr", car.plant_info().name, car.plant.max_dp));
        p.put(27, 4, "Tires");
        for (i, t) in car.tires.iter().enumerate() {
            p.put(23, 5 + i, &format!("{:<6} {:>2} {}", TIRES[t.kind as usize].short, t.max_dp, tables::LOCATION_NAMES[6 + i]));
        }
        p.put(26, 9, "Weapons");
        p.put(37, 9, "Am");
        for (i, w) in car.weapons.iter().enumerate() {
            let line = match w {
                Some(w) => {
                    let ammo = if w.uses_ammo() { format!("{:>2}", w.ammo) } else { "--".to_string() };
                    format!("{:<6} {:>2} {} {}", w.info().short, w.max_dp, tables::LOCATION_NAMES[w.mount as usize], ammo)
                }
                None => "...... .. ... ..".to_string(),
            };
            p.put(23, 10 + i, &line);
        }
        p
    }
}

/// Screen cells (col, rows) the original inverts for a field (`field_col_start` etc., as $AD6F).
fn field_cells(f: Field) -> (usize, usize, usize, usize) {
    // (first col, end col exclusive, first row, last row)
    match f {
        Field::Body => (7, 17, 6, 6),
        Field::Chassis => (7, 17, 7, 7),
        Field::Suspension => (7, 15, 8, 8),
        Field::Armor(i) => (19, 21, 12 + i, 12 + i),
        Field::Plant => (23, 29, 3, 3),
        Field::Tires => (23, 29, 5, 8),
        Field::WeaponType(s) => (23, 29, 10 + s, 10 + s),
        Field::WeaponMount(s) => (33, 36, 10 + s, 10 + s),
        Field::Ammo(s) => (37, 39, 10 + s, 10 + s),
    }
}

impl Screen for AssemblyLine {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        // F1 aborts while designing (original key); everywhere else it is the help screen
        if !(self.stage == Stage::Design && inp.pressed(Key::F(1)))
            && let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if let Some((next, _)) = self.wait.tick(inp) {
            match next {
                Next::Name => self.ask_name(),
                Next::Leave => return leave(cx, Building::Assembly),
            }
            return Trans::None;
        }
        match self.stage {
            Stage::Busy => {}
            Stage::Name => match self.asker.feed(inp) {
                Some(Answer::Text(name)) if !name.is_empty() => {
                    self.car = rules::new_design(cx.game(), &name);
                    self.stage = Stage::Design;
                    self.cursor = 0;
                    self.show_design_msgs(cx);
                }
                Some(Answer::Text(_)) => self.asker.set(Ask::Text(rules::NAME_LEN)),
                Some(Answer::Cancel) => self.abort(),
                _ => {}
            },
            Stage::Design => {
                if let Some(t) = self.design_keys(cx, inp) {
                    return t;
                }
            }
        }
        Trans::None
    }

    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
        let hl = (self.stage == Stage::Design).then(|| field_cells(Field::at(self.cursor)));
        self.sheet(cx).draw(fb, &cx.assets, hl);
    }
}

/// A 40 x 21 grid of characters (rows 0-20; the message area below is the [`Term`]'s).
struct Sheet {
    rows: Vec<Vec<char>>,
}

impl Sheet {
    fn new() -> Sheet {
        Sheet { rows: vec![vec![' '; 40]; 21] }
    }
    fn put(&mut self, col: usize, row: usize, s: &str) {
        if row >= self.rows.len() {
            return;
        }
        if col + s.trim_end().chars().count() > 39 {
            text::report_overflow(s);
        }
        for (i, ch) in s.chars().enumerate() {
            if col + i < 39 {
                self.rows[row][col + i] = ch;
            }
        }
    }
    /// Draw, inverting the cells of `hl` (the field under the cursor).
    fn draw(&self, fb: &mut Frame, a: &Assets, hl: Option<(usize, usize, usize, usize)>) {
        for (r, line) in self.rows.iter().enumerate() {
            for (c, ch) in line.iter().enumerate() {
                let inv = matches!(hl, Some((c0, c1, r0, r1)) if c >= c0 && c < c1 && r >= r0 && r <= r1);
                let (x, y) = (c as i32 * 8, r as i32 * 8);
                if inv {
                    fb.fill(x, y, 8, 8, col::WHITE);
                    text::draw(fb, a, x, y, &ch.to_string(), col::BLACK);
                } else if *ch != ' ' {
                    text::draw(fb, a, x, y, &ch.to_string(), col::WHITE);
                }
            }
        }
    }
}
