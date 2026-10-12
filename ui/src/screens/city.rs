//! Walking (or driving) around a city (the original CITY module).
//!
//! Walker coordinates are the original's signed units: screen x = x_table[x], screen y =
//! 64 + 2*y. Collision uses the original 10x10 probe against the city's street mask; each
//! axis is tried separately so you slide along walls. Standing exactly on a door enters the
//! building; reaching an exit area in your car leaves town by that road.
//!
//! Doors are single cells at the end of narrow passages, invisible in the art, and walking
//! into the wall one row off did nothing (play-test). Pushing straight into a wall now
//! slides you toward an opening up to [`NUDGE`] units to the side (so you find the door row
//! by pushing toward it), and every door is marked with a short bar on its wall.
use crate::app::{Cx, Screen, Trans};
use crate::fb::{col, Frame};
use crate::input::{Input, Key};
use crate::screens::{buildings, death, drive, hotkeys};
use crate::text;
use cardul_sim::world::Building;
use cardul_sim::Sfx;

/// How you arrived in the city (decides where the walker appears).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrival {
    /// by road segment
    Road(u8),
    /// leaving a building
    Building(Building),
    /// game continue / bus: at the Truck Stop
    TruckStop,
    /// clone revival: at the Gold Cross
    GoldCross,
    /// a new driver: at the Arena (Amateur Night is the usual first step)
    Start,
}

/// Walker directions 0..7 = N NE E SE S SW W NW.
const DIRS: [(i32, i32); 8] = [(0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1)];
/// City car sprite per direction (non-modshaps 69-72 = N E S W, 104-107 = NE SE SW NW).
const CAR_SHAPES: [usize; 8] = [69, 104, 70, 105, 71, 106, 72, 107];
/// Walk cycles (frame numbers; shape = 16 + frame).
const CYCLE_N: [usize; 4] = [0, 4, 0, 2];
const CYCLE_S: [usize; 4] = [0, 2, 0, 4];
const CYCLE_E: [usize; 4] = [7, 8, 9, 8];
const CYCLE_W: [usize; 4] = [10, 5, 6, 5];

/// Ticks (60 Hz) per walking step.
const STEP_TICKS: u32 = 3;
/// Ticks a message stays in the strip when another one is waiting.
const MSG_TICKS: u32 = 240;
/// How far (walker units) pushing into a wall slides you sideways toward an opening.
pub const NUDGE: i32 = 3;
/// Door marker colour.
const DOOR_COL: u32 = col::YELLOW;

pub struct CityScreen {
    city: u8,
    x: i32,
    y: i32,
    dir: usize,
    anim: usize,
    moving: bool,
    timer: u32,
    /// messages for the two-line strip, shown one after the other (`msg_timer` ticks each)
    msgs: Vec<String>,
    msg_timer: u32,
    /// the exit we warned about (so the message isn't repeated every step)
    warned_exit: bool,
    /// steps walked (a footstep sounds every few)
    steps: u32,
}

fn dir_toward(dx: i32, dy: i32) -> usize {
    DIRS.iter().position(|d| *d == (dx.signum(), dy.signum())).unwrap_or(4)
}

impl CityScreen {
    /// Enter the city: places the walker, runs the IRS check and saves the game.
    pub fn arrive(cx: &mut Cx, how: Arrival) -> CityScreen {
        let city = cx.game().driver.city;
        let c = cx.data.city(city).clone();
        let leave_of = |k: Building| c.buildings.iter().find(|b| b.kind == k as u8).map(|b| (b.leave_x, b.leave_y, b.door_x, b.door_y));
        let ts = leave_of(Building::TruckStop).unwrap_or((0, 0, 0, 0));
        let (x, y, dir) = match how {
            Arrival::Road(r) => match c.exits.iter().find(|e| e.road == r) {
                Some(e) => {
                    // face into town
                    let d = match e.dir.as_str() {
                        "N" => 4,
                        "S" => 0,
                        "E" => 6,
                        _ => 2,
                    };
                    (e.arrive_x, e.arrive_y, d)
                }
                None => (ts.0, ts.1, 4),
            },
            Arrival::Building(k) => {
                let (lx, ly, dx, dy) = leave_of(k).unwrap_or(ts);
                (lx, ly, dir_toward(lx - dx, ly - dy))
            }
            Arrival::TruckStop => (ts.0, ts.1, dir_toward(ts.0 - ts.2, ts.1 - ts.3)),
            Arrival::GoldCross => {
                let (lx, ly, dx, dy) = leave_of(Building::GoldCross).unwrap_or(ts);
                (lx, ly, dir_toward(lx - dx, ly - dy))
            }
            Arrival::Start => {
                let (lx, ly, dx, dy) = leave_of(Building::Arena).unwrap_or(ts);
                (lx, ly, dir_toward(lx - dx, ly - dy))
            }
        };
        let mut msgs = vec![];
        {
            let g = cx.g();
            g.driver.last_city = city;
            if g.irs_check() {
                msgs.push("The IRS finally catches up with you and collects $100,000 in back taxes.".to_string());
                msgs.push("Unfortunately, the fines come to an additional $250,000!".to_string());
            }
        }
        cx.save();
        if matches!(how, Arrival::Road(_) | Arrival::GoldCross) {
            cx.sfx(Sfx::Arrive);
        }
        CityScreen { city, x, y, dir, anim: 0, moving: false, timer: 0, msgs, msg_timer: 0, warned_exit: false, steps: 0 }
    }

    fn say(&mut self, s: &str) {
        self.msgs = vec![s.to_string()];
        self.msg_timer = 0;
    }
    /// A refusal: the message and a beep.
    fn refuse(&mut self, cx: &mut Cx, s: &str) {
        cx.sfx(Sfx::Beep);
        self.say(s);
    }

    /// City and walker position (tools/tests).
    pub fn pos(&self) -> (u8, i32, i32) {
        (self.city, self.x, self.y)
    }
    /// The message strip (tools/tests).
    pub fn messages(&self) -> &[String] {
        &self.msgs
    }

    /// Is the walker position free of walls (probe against the street mask)?
    pub fn free(cx: &Cx, city: u8, x: i32, y: i32) -> bool {
        let p = &cx.data.probe;
        let x0 = cx.data.x_table[((x - p.hx) & 255) as usize];
        let y0 = 64 + 2 * y - 2 * p.hy;
        if x0 < 0 || y0 < 0 || x0 + p.w > 320 || y0 + p.h > 168 {
            return false;
        }
        let m = &cx.assets.city_masks[city as usize];
        for yy in y0..y0 + p.h {
            for xx in x0..x0 + p.w {
                if m.px[(yy * m.w + xx) as usize] & 0xFF == 0 {
                    return false;
                }
            }
        }
        true
    }

    /// Pushing straight (one axis) into a wall at (x, y): the first step sideways toward the
    /// nearest opening within [`NUDGE`] units that the push would go through, if any.
    pub fn nudge(cx: &Cx, city: u8, x: i32, y: i32, dx: i32, dy: i32) -> Option<(i32, i32)> {
        if (dx == 0) == (dy == 0) {
            return None;
        }
        let (sx, sy) = if dx != 0 { (0, 1) } else { (1, 0) };
        for k in 1..=NUDGE {
            for s in [1, -1] {
                let side = |i: i32| (x + s * sx * i, y + s * sy * i);
                let clear = (1..=k).all(|i| {
                    let (px, py) = side(i);
                    CityScreen::free(cx, city, px, py)
                });
                let (ex, ey) = side(k);
                if clear && CityScreen::free(cx, city, ex + dx, ey + dy) {
                    return Some(side(1));
                }
            }
        }
        None
    }

    /// Mark the doors: a short bar on the wall beyond each door cell (the door faces from
    /// the spot you come out at toward the door).
    fn draw_doors(&self, cx: &Cx, fb: &mut Frame) {
        let p = &cx.data.probe;
        for b in &cx.data.city(self.city).buildings {
            let x0 = cx.data.x_table[((b.door_x - p.hx) & 255) as usize];
            let y0 = 64 + 2 * b.door_y - 2 * p.hy;
            let (ddx, ddy) = (b.door_x - b.leave_x, b.door_y - b.leave_y);
            let (x, y, w, h) = if ddx.abs() >= ddy.abs() {
                let x = if ddx < 0 { x0 - 2 } else { x0 + p.w };
                (x, y0 + 1, 2, p.h - 2)
            } else {
                let y = if ddy < 0 { y0 - 2 } else { y0 + p.h };
                (x0 + 1, y, p.w - 2, 2)
            };
            fb.fill(x, y, w, h, DOOR_COL);
        }
    }

    fn step(&mut self, cx: &mut Cx, dx: i32, dy: i32) -> Option<Trans> {
        let (ox, oy) = (self.x, self.y);
        if dx != 0 && CityScreen::free(cx, self.city, self.x + dx, self.y) {
            self.x += dx;
        }
        if dy != 0 && CityScreen::free(cx, self.city, self.x, self.y + dy) {
            self.y += dy;
        }
        if (self.x, self.y) == (ox, oy)
            && let Some(p) = CityScreen::nudge(cx, self.city, self.x, self.y, dx, dy) {
            (self.x, self.y) = p;
        }
        if (self.x, self.y) == (ox, oy) {
            return None;
        }
        self.anim = (self.anim + 1) % 4;
        // a footstep every eighth step on foot (about two a second); in the car, the engine
        self.steps += 1;
        if self.steps.is_multiple_of(8) && !(cx.game().driver.car_with_you && cx.game().car().is_some()) {
            cx.sfx(Sfx::Step);
        }
        // doors
        let c = cx.data.city(self.city).clone();
        if let Some(b) = c.buildings.iter().find(|b| (b.door_x, b.door_y) == (self.x, self.y)) {
            let kind = Building::from_u8(b.kind).unwrap();
            if kind.closes_after_business() && cx.game().driver.day_used {
                self.refuse(cx, "It's closed. Try again tomorrow.");
                return None;
            }
            cx.sfx(Sfx::Door);
            cx.save();
            let s = buildings::open(cx, kind);
            return Some(Trans::Replace(s));
        }
        // road exits
        if let Some(e) = c.exits.iter().find(|e| e.contains(self.x, self.y)) {
            let g = cx.game();
            if !g.driver.car_with_you || g.car().is_none() {
                if !self.warned_exit {
                    self.refuse(cx, "You can't walk to the next city. Take a bus from the Truck Stop.");
                    self.warned_exit = true;
                }
                return None;
            }
            let car = g.car().unwrap();
            if car.battery == 0 {
                self.refuse(cx, "Your car's batteries need charging.");
                return None;
            }
            // FIX: a car with a dead power plant can't move on the road either (it would
            // strand you at the city limits); send it to the Garage first
            if car.plant.dp == 0 {
                self.refuse(cx, "Your car's power plant is wrecked. Have it repaired at a Garage.");
                return None;
            }
            if car.accel_class() >= 3 {
                self.refuse(cx, "Your car is too heavy to move!");
                return None;
            }
            let road = e.road;
            let city = self.city;
            cx.save();
            return Some(Trans::Replace(drive::road(cx, road, city)));
        } else {
            self.warned_exit = false;
        }
        None
    }
}

impl Screen for CityScreen {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = hotkeys(cx, inp, true) {
            return t;
        }
        if inp.pressed(Key::Esc) {
            return Trans::Push(Box::new(PauseMenu::new()));
        }
        // FIX: the strip has two lines; several messages (the IRS) are shown one after the
        // other instead of only the last two lines
        if self.msgs.len() > 1 {
            self.msg_timer += 1;
            if self.msg_timer >= MSG_TICKS {
                self.msgs.remove(0);
                self.msg_timer = 0;
            }
        }
        let (dx, dy) = (inp.held.dx() as i32, inp.held.dy() as i32);
        self.moving = dx != 0 || dy != 0;
        if self.moving {
            self.dir = dir_toward(dx, dy);
            self.timer += 1;
            if self.timer >= STEP_TICKS {
                self.timer = 0;
                if let Some(t) = self.step(cx, dx, dy) {
                    return t;
                }
            }
        } else {
            self.timer = STEP_TICKS - 1;
        }
        Trans::None
    }

    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        let a = &cx.assets;
        let g = cx.game();
        fb.clear(col::BLACK);
        fb.blit(&a.cities[self.city as usize], 0, 0);
        self.draw_doors(cx, fb);
        // walker or car
        let px = cx.data.x_table[(self.x & 255) as usize];
        let py = 64 + 2 * self.y;
        if g.driver.car_with_you && g.car().is_some() {
            let s = CAR_SHAPES[self.dir];
            let p = &cx.data.probe;
            let x0 = cx.data.x_table[((self.x - p.hx) & 255) as usize];
            let m = &a.shape_meta[s];
            fb.blit(&a.shapes[s], x0 + (p.w - m.w) / 2, py - 2 * p.hy + (p.h - m.h) / 2);
        } else {
            let frame = match self.dir {
                2 => if self.moving { CYCLE_E[self.anim] } else { 7 },
                6 => if self.moving { CYCLE_W[self.anim] } else { 10 },
                0 | 1 | 7 => if self.moving { CYCLE_N[self.anim] } else { 0 },
                _ => if self.moving { CYCLE_S[self.anim] } else { 0 },
            };
            a.draw_shape(fb, 16 + frame, px, py);
        }
        // message strip (rows 21-24)
        for (i, m) in self.msgs.iter().take(1).flat_map(|m| text::wrap(m, 39)).take(2).enumerate() {
            text::at(fb, a, 0, 21 + i as i32, &m, col::WHITE);
        }
        let d = &g.driver;
        let place = cx.data.city_name(self.city).to_string();
        text::at(fb, a, 0, 23, &place, col::WHITE);
        let date = d.date.long();
        text::at(fb, a, 40 - date.len() as i32, 23, &date, col::WHITE);
        text::at(fb, a, 0, 24, &text::money(d.money), col::WHITE);
        let hint = if d.car_with_you && g.car().is_some() { "in your car  F1 help" } else { "on foot  F1 help" };
        text::at(fb, a, 40 - hint.len() as i32, 24, hint, col::LIGHT_GREY);
    }

    fn resume(&mut self, cx: &mut Cx) {
        // a death while on an info screen can't happen; nothing to do
        let _ = cx;
    }
    fn as_any(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn key_clicks(&self) -> bool {
        false
    }
    /// In your car the engine idles, and revs a little while you drive about.
    fn engine(&self, cx: &Cx) -> Option<f32> {
        let g = cx.game.as_ref()?;
        let car = g.car().filter(|_| g.driver.car_with_you)?;
        if car.plant.dp == 0 || car.battery == 0 {
            return None;
        }
        Some(if self.moving { 0.3 } else { 0.05 })
    }
}

/// Esc in the city: save and quit to the title, or carry on.
pub struct PauseMenu {
    term: crate::term::Term,
    asker: crate::term::Asker,
}

impl Default for PauseMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl PauseMenu {
    pub fn new() -> PauseMenu {
        let mut term = crate::term::Term::new("Paused");
        term.at(4, 3, "1 - Continue");
        term.at(4, 5, "2 - Save and return to the title");
        term.at(4, 7, "3 - Save and quit Cardul");
        let mut asker = crate::term::Asker::default();
        asker.set(crate::term::Ask::Choice(vec!['1', '2', '3']));
        PauseMenu { term, asker }
    }
}

impl Screen for PauseMenu {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        use crate::term::Answer;
        match self.asker.feed(inp) {
            Some(Answer::Choice('1')) | Some(Answer::Cancel) => Trans::Pop,
            Some(Answer::Choice('2')) => {
                cx.save();
                cx.game = None;
                cx.save_file = None;
                Trans::Reset(Box::new(crate::screens::setup::MainMenu::new()))
            }
            Some(Answer::Choice('3')) => {
                cx.save();
                Trans::Quit
            }
            _ => Trans::None,
        }
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, None);
    }
}

/// After the driver dies on the road or in the arena: clone revival or game over.
pub fn after_death(cx: &mut Cx) -> Trans {
    let out = cx.g().kill_driver();
    cx.save();
    Trans::Reset(Box::new(death::DeathScreen::new(out)))
}

/// After the driver dies in a building (his car, if parked outside, stays in town).
pub fn after_death_in_town(cx: &mut Cx) -> Trans {
    let out = cx.g().kill_driver_in_town();
    cx.save();
    Trans::Reset(Box::new(death::DeathScreen::new(out)))
}
