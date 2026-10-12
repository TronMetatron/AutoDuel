//! Real-time road and arena combat (the R1-R3 / A1-A3 engines).
//!
//! The front end calls [`Battle::new`] once, then [`Battle::step`] once per simulation frame
//! with the player's input, and draws from the public state (`player`, `enemies`, the two
//! projectile tables, `traces`, `message` and the game's car).
//!
//! One step is one frame of the original main loop (`r1 $C083`), in the same order:
//! keys, player movement and firing, enemy AI (with enemy fire and movement), the player's
//! projectiles, the enemies' projectiles, handling, power use / odometer / trip timer,
//! salvage, exits, timers. All randomness comes from `game.rng`.
//!
//! Repairs of broken original behaviour are marked `FIX:`; see also [`ORIGINAL_HIT_SIDE`].
pub mod autopilot;
pub mod map;
pub mod motion;
mod enemy;
mod hits;
mod player;
mod salvage;
mod weapons;
#[cfg(test)]
mod tests;

pub use enemy::{Enemy, SALVAGED, WRECK};
pub use player::{DriveParams, Player};
pub use weapons::{on_screen, pk, shape, Proj, ProjSlots, TraceDot};

use crate::cargo::CargoKind;
use crate::data::GameData;
use crate::game::Game;
use map::MapView;
use serde::{Deserialize, Serialize};

/// FIX switch for the side of the car a hit lands on. The original maps the bearing of the
/// projectile rotated by 180 degrees (`r2 $7967`); with `false` the side faces the shot
/// (a hit from ahead lands on Front).
pub const ORIGINAL_HIT_SIDE: bool = false;

/// Frames a HUD message stays up (the original cleared it after 8 of its fast frames).
pub const MESSAGE_FRAMES: u32 = 30;
/// Frames on a road before the day passes (`r1 $CCD9` trip timer).
pub const TRIP_DAY_FRAMES: u32 = 192;
/// Frames "You are now dead!" is shown before the battle ends.
pub const DEATH_FRAMES: u32 = 24;
/// Enemy slots per road (and arena roster).
pub const ENEMY_SLOTS: usize = 16;

/// Sound cues for the front end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sfx {
    /// refused, not enough money, no weapon
    Beep,
    /// a small confirmation
    Click,
    Shot,
    Rocket,
    Laser,
    Flame,
    Hit,
    PlayerHit,
    Crash,
    Explode,
    /// a delivery, a win
    Fanfare,
    Drop,
    /// a key pressed on a menu or text screen
    Key,
    /// a footstep
    Step,
    /// going into a building
    Door,
    /// money changes hands
    Cash,
    /// reaching a city by road (or as a clone)
    Arrive,
    /// the driver dies
    Death,
    /// the title tune
    Title,
    /// the bus leaves
    Horn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BattleKind {
    /// driving a road segment, entered from `from_city`
    Road { road: u8, from_city: u8 },
    /// arena event 0..6, or practice (event 7)
    Arena { event: u8 },
}

/// One frame of player input.
#[derive(Clone, Copy, Debug, Default)]
pub struct DriveInput {
    /// joystick: -1 left / 0 / +1 right
    pub dx: i8,
    /// joystick: -1 up / 0 / +1 down
    pub dy: i8,
    /// fire button held
    pub fire: bool,
    /// SPACE: select the next weapon
    pub next_weapon: bool,
    /// keys 1..0: select weapon slot 0..9
    pub select: Option<u8>,
    /// G: get out of / back into the car
    pub toggle_foot: bool,
}

/// Arena outcome handed back to the Arena building (`$68`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArenaResult {
    /// all opponents destroyed, still in your car (Amateur Night: even on foot)
    Won,
    /// left with opponents remaining
    Fled,
    /// won, but on foot / car not usable
    WonOnFoot,
    /// won on foot after running out of power: car towed to the garage
    CarTowed,
    /// practice run finished; `hit` = took a hit, `on_foot` = car towed
    Practice { hit: bool, on_foot: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BattleEnd {
    /// reached the city at the end of the road
    Arrived { city: u8, road: u8, on_foot: bool },
    /// the driver was killed (the caller runs `Game::kill_driver`)
    Died,
    Arena(ArenaResult),
}

/// The running battle: everything the engine keeps between frames. The car itself (armor,
/// weapons, ammo, battery, cargo) is the game's current car and is changed in place.
pub struct Battle {
    pub kind: BattleKind,
    pub frame: u32,
    /// sound cues raised this frame (the front end drains them)
    pub sfx: Vec<Sfx>,
    /// message shown in the HUD message area
    pub message: Option<String>,
    message_timer: u32,
    pub player: Player,
    /// the 16 enemy slots (road slot = index)
    pub enemies: Vec<Option<Enemy>>,
    /// player projectiles (`$1000+$20..$2F`) and enemy projectiles (`$1000+$00..$0F`)
    pub pproj: ProjSlots,
    pub eproj: ProjSlots,
    /// tracer dots and flame bursts drawn this frame
    pub traces: Vec<TraceDot>,
    /// selected weapon slot 0..9 (`$7F83`)
    pub selected: Option<u8>,
    /// weapon cooldowns per slot; ready when negative (`$0DA0`)
    cooldown: [i8; 10],
    /// rolling odometer: position 0..79 of each of the 4 digit drums (`$0DAA`) and which
    /// drums are turning (`$0DB1`)
    pub odometer: [u8; 4],
    odo_rolling: [bool; 4],
    odo_acc: i32,
    /// distance counter for battery drain
    power_acc: u32,
    trip_timer: u32,
    /// aimed hits left until marksmanship +1 (`$1DBC`)
    marks_counter: u8,
    /// road: index of the exit you came in by, and whether you have left its area
    from_exit: usize,
    origin_armed: bool,
    /// arena: opponents still in action, a hit was taken, the battery ran flat
    pub opponents_left: u32,
    hit_taken: bool,
    out_of_power: bool,
    /// frames left of the death message
    dying: u32,
    /// damage points of the three cargo slots (cargo record byte 4: 3 for jobs, 1 for quest
    /// items, +2 per salvage); cargo has no DP of its own in the save, so it lives here
    cargo_dp: [i32; 3],
    /// frame of the last "click"/"beep" (stops the out-of-ammo click from buzzing)
    last_click: u32,
    ended: Option<BattleEnd>,
}

impl Battle {
    pub fn new(game: &mut Game, data: &GameData, kind: BattleKind) -> Battle {
        let (start, heading, from_exit) = match kind {
            BattleKind::Road { road, from_city } => {
                let r = &data.roads[road as usize];
                let k = r.exits.iter().position(|e| e.city == from_city).unwrap_or(0);
                let e = &r.exits[k];
                // face away from the city, into the road
                let (dx, dy) = (e.start_x - e.trigger_x, e.start_y - e.trigger_y);
                let h = if dy.abs() >= dx.abs() {
                    if dy > 0 { 12 } else { 4 }
                } else if dx > 0 {
                    0
                } else {
                    8
                };
                ((e.start_x, e.start_y), h, k)
            }
            BattleKind::Arena { event } => {
                let ev = &data.arena.events[event.min(7) as usize];
                let e = &ev.exits[1.min(ev.exits.len() - 1)];
                ((e.start_x, e.start_y), 4, 1)
            }
        };
        let driving = game.driver.driving;
        let (h0, has_car) = match game.car() {
            Some(c) => (c.handling_base(driving), true),
            None => (0, false),
        };
        let mut b = Battle {
            kind,
            frame: 0,
            sfx: vec![],
            message: None,
            message_timer: 0,
            player: Player::new(start.0, start.1, heading, h0, has_car),
            enemies: vec![None; ENEMY_SLOTS],
            pproj: ProjSlots::new(),
            eproj: ProjSlots::new(),
            traces: vec![],
            selected: Some(0),
            cooldown: [-1; 10],
            odometer: [0; 4],
            odo_rolling: [false; 4],
            odo_acc: 0,
            power_acc: 0,
            trip_timer: TRIP_DAY_FRAMES,
            marks_counter: 5,
            from_exit,
            origin_armed: false,
            opponents_left: 0,
            hit_taken: false,
            out_of_power: false,
            dying: 0,
            cargo_dp: [0; 3],
            last_click: 0,
            ended: None,
        };
        if let Some(car) = game.car() {
            for (i, c) in car.cargo.iter().enumerate() {
                b.cargo_dp[i] = match c.as_ref().map(|c| c.kind) {
                    Some(CargoKind::Job(_)) => 3,
                    Some(CargoKind::Quest(_)) => 1,
                    Some(CargoKind::Salvage) => 2,
                    None => 0,
                };
            }
        }
        b.spawn_enemies(game, data);
        b
    }

    pub fn is_arena(&self) -> bool {
        matches!(self.kind, BattleKind::Arena { .. })
    }
    /// Arena event number (None on a road).
    pub fn event(&self) -> Option<u8> {
        match self.kind {
            BattleKind::Arena { event } => Some(event),
            _ => None,
        }
    }
    /// The map being driven.
    pub fn map<'a>(&self, data: &'a GameData) -> MapView<'a> {
        match self.kind {
            BattleKind::Road { road, .. } => MapView::road(data, road),
            BattleKind::Arena { .. } => MapView::arena(data),
        }
    }
    /// The road's exits (or the arena gates).
    pub fn exits<'a>(&self, data: &'a GameData) -> &'a [crate::data::RoadExit] {
        match self.kind {
            BattleKind::Road { road, .. } => &data.roads[road as usize].exits,
            BattleKind::Arena { event } => &data.arena.events[event.min(7) as usize].exits,
        }
    }
    /// The death message is showing; the battle ends with `Died` shortly.
    pub fn is_dying(&self) -> bool {
        self.dying > 0
    }

    /// Show a message in the HUD message area.
    pub fn say(&mut self, s: &str) {
        self.message = Some(s.to_string());
        self.message_timer = MESSAGE_FRAMES;
    }
    fn click(&mut self, s: Sfx) {
        if self.frame >= self.last_click + 4 || self.last_click == 0 {
            self.sfx.push(s);
            self.last_click = self.frame;
        }
    }

    /// Advance one simulation frame.
    pub fn step(&mut self, game: &mut Game, data: &GameData, input: &DriveInput) -> Option<BattleEnd> {
        if let Some(e) = self.ended {
            return Some(e);
        }
        self.frame += 1;
        self.traces.clear();
        if self.dying > 0 {
            self.dying -= 1;
            if self.dying == 0 {
                self.write_back(game);
                self.ended = Some(BattleEnd::Died);
            }
            return self.ended;
        }
        let view = self.map(data);

        // keys (`r1 $C115`): G, then 1..0 (in the car only)
        if input.toggle_foot {
            self.toggle_foot(game);
        }
        if let Some(s) = input.select
            && !self.player.on_foot && s < 10 {
            self.selected = Some(s);
        }
        let stick = motion::stick_heading(input.dx, input.dy);
        // no power plant or no battery: the car can neither accelerate nor steer
        let powered = game.car().map(|c| c.plant.dp > 0 && c.battery > 0).unwrap_or(false);
        let drive_stick = if self.player.on_foot || powered { stick } else { None };

        self.player_move(game, &view, drive_stick, input);
        self.enemies_step(game, &view);
        self.update_player_projectiles(game, &view);
        self.update_enemy_projectiles(game, &view);
        self.player.update_handling(drive_stick.is_some());
        self.hud_update(game);
        self.check_salvage(game);
        // FIX: a driver killed on the frame he reaches an exit is dead, not arriving with
        // 0 health
        if game.driver.health == 0 {
            self.say("You are now dead!");
            self.message_timer = DEATH_FRAMES + 1;
            self.dying = DEATH_FRAMES;
            return None;
        }
        if let Some(end) = self.check_exits(game, data) {
            self.ended = Some(end);
            return self.ended;
        }
        self.tick_timers();
        if self.message_timer > 0 {
            self.message_timer -= 1;
            if self.message_timer == 0 {
                self.message = None;
            }
        }
        None
    }

    /// G: get out, or back in when the car is within 5 units.
    fn toggle_foot(&mut self, game: &Game) {
        if !self.player.on_foot {
            self.player.get_out();
        } else if game.car().is_some() {
            self.player.try_get_in();
        }
    }

    /// Movement, collisions, firing and weapon cycling (`r1 $C494`).
    fn player_move(&mut self, game: &mut Game, view: &MapView, stick: Option<u8>, input: &DriveInput) {
        let p = &mut self.player;
        let origin = (p.x, p.y);
        p.frame_origin = origin;
        p.speed0 = p.speed;
        let (vx, vy) = if p.on_foot {
            p.walk(stick)
        } else {
            let params = game.car().map(DriveParams::for_car).unwrap_or(DriveParams { accel: 0, top: 0 });
            p.drive(stick, params, &mut game.rng)
        };
        // map bounds cancel the offending axis
        let mut nx = p.x + vx;
        let mut ny = p.y + vy;
        p.vx = vx;
        p.vy = vy;
        if !view.player_x_ok(nx) {
            nx = p.x;
            p.vx = 0;
        }
        if !view.player_y_ok(ny) {
            ny = p.y;
            p.vy = 0;
        }
        let mut crashed = view.blocked(nx, ny);
        if !crashed && !p.on_foot {
            // another car within 5 units: both stop
            if let Some(i) = self.enemy_near(nx, ny, 6, true) {
                if let Some(e) = self.enemies[i].as_mut() {
                    e.speed = 0;
                }
                crashed = true;
            }
        }
        let p = &mut self.player;
        if crashed {
            // nothing was driven: no power use, the odometer stands still
            p.vx = 0;
            p.vy = 0;
            let mut armor_left = None;
            if !p.on_foot {
                self.sfx.push(Sfx::Crash);
                if p.speed0 >= 3
                    && let Some(car) = game.car_mut()
                    && car.armor[crate::car::F].pts > 0 {
                    car.armor[crate::car::F].pts -= 1;
                    armor_left = Some(car.armor[crate::car::F].pts);
                }
            }
            p.crash();
            // the original took the point silently; say so, as hits do
            if let Some(n) = armor_left {
                self.say(&format!("Crash! Front armor down to {}.", n));
            }
        } else {
            // footsteps on the road
            if p.on_foot && (nx, ny) != (p.x, p.y) && self.frame.is_multiple_of(4) {
                self.sfx.push(Sfx::Step);
            }
            p.x = nx;
            p.y = ny;
        }
        if !self.player.on_foot {
            if input.fire {
                self.player_fire(game, view, origin);
            }
            if input.next_weapon {
                self.cycle_weapon(game);
            }
        }
        let p = &mut self.player;
        if p.speed == 0 {
            p.handling = p.handling_base;
            p.stopped_frames += 1;
        } else {
            p.stopped_frames = 0;
        }
    }

    /// SPACE: the next slot that holds a weapon (`r1 $C81C`).
    fn cycle_weapon(&mut self, game: &Game) {
        let cur = self.selected.map(|s| s as usize).unwrap_or(usize::MAX);
        let start = if cur == usize::MAX { 0 } else { (cur + 1) % 10 };
        let has = |i: usize| game.car().map(|c| c.weapons[i].is_some()).unwrap_or(false);
        let next = (0..10).map(|k| (start + k) % 10).find(|&i| has(i)).unwrap_or(start);
        self.selected = Some(next as u8);
    }

    /// Power use, odometer, trip timer (`r1 $CCB1`).
    fn hud_update(&mut self, game: &mut Game) {
        // a day on the road (counts on foot too)
        if !self.is_arena() && self.trip_timer > 0 {
            self.trip_timer -= 1;
            if self.trip_timer == 0 {
                game.next_day();
                let d = &mut game.driver;
                crate::driver::Driver::raise(&mut d.driving, 1);
                d.add_prestige(1);
            }
        }
        if self.player.on_foot {
            return;
        }
        let (vx, vy) = (self.player.vx.abs(), self.player.vy.abs());
        // odometer: one drum step in each frame where the running count of |dx|, then
        // |dy|, reaches 2 (`r1 $CE78`)
        let mut roll = false;
        for v in [vx, vy] {
            self.odo_acc += v;
            if self.odo_acc >= 2 {
                self.odo_acc &= 1;
                roll = true;
            }
        }
        if roll {
            self.roll_odometer();
        }
        // battery: road 1 unit per 256 units driven, arena 1 per 16384
        let per = if self.is_arena() { 16384 } else { 256 };
        self.power_acc += (vx + vy) as u32;
        if self.power_acc >= per {
            self.power_acc -= per;
            if let Some(car) = game.car_mut() {
                if car.battery > 0 {
                    car.battery -= 1;
                }
                if car.battery == 0 {
                    self.out_of_power = true;
                }
            }
        }
    }

    /// Mechanical odometer (`r1 $CE3E`): the last drum always turns; a drum passing 9
    /// starts the next one, which turns until it reaches its next digit.
    fn roll_odometer(&mut self) {
        self.odo_rolling[3] = true;
        for x in (0..4).rev() {
            if !self.odo_rolling[x] {
                break;
            }
            self.odometer[x] = (self.odometer[x] + 1) % 80;
            let pos = self.odometer[x];
            if pos == 73 && x > 0 {
                self.odo_rolling[x - 1] = true;
            }
            if pos.is_multiple_of(8) {
                self.odo_rolling[x] = false;
            }
        }
    }

    /// Road exits (`r2 $7F84`) and arena gates (`a2 $7EC6`).
    fn check_exits(&mut self, game: &mut Game, data: &GameData) -> Option<BattleEnd> {
        let (px, py) = (self.player.x, self.player.y);
        let exits = self.exits(data);
        match self.kind {
            BattleKind::Road { road, .. } => {
                // FIX: the exit you came in by only works once you have left its area
                if let Some(e) = exits.get(self.from_exit)
                    && ((px - e.trigger_x).abs() > 64 || (py - e.trigger_y).abs() > 64) {
                    self.origin_armed = true;
                }
                for (k, e) in exits.iter().enumerate() {
                    if k == self.from_exit && !self.origin_armed {
                        continue;
                    }
                    if (px - e.trigger_x).abs() < 16 && (py - e.trigger_y).abs() < 16 {
                        let on_foot = self.player.on_foot;
                        self.write_back(game);
                        if on_foot {
                            game.driver.add_prestige(-2);
                            game.lose_current_car();
                        }
                        return Some(BattleEnd::Arrived { city: e.city, road, on_foot });
                    }
                }
                None
            }
            BattleKind::Arena { event } => {
                let hit = exits.iter().any(|e| (px - e.trigger_x).abs() < 16 && (py - e.trigger_y).abs() < 4);
                if !hit {
                    return None;
                }
                Some(BattleEnd::Arena(self.leave_arena(game, event)))
            }
        }
    }

    /// Leaving the arena (`a2 $7F26`): 1 battery unit, the result, and a car abandoned
    /// with opponents left is lost.
    fn leave_arena(&mut self, game: &mut Game, event: u8) -> ArenaResult {
        if let Some(car) = game.car_mut() {
            car.battery = car.battery.saturating_sub(1);
        }
        let on_foot = self.player.on_foot;
        let left = self.opponents_left > 0;
        let result = if event >= 7 {
            ArenaResult::Practice { hit: self.hit_taken, on_foot }
        } else if left {
            ArenaResult::Fled
        } else if event == 0 || !on_foot {
            ArenaResult::Won
        } else if self.out_of_power {
            ArenaResult::CarTowed
        } else {
            ArenaResult::WonOnFoot
        };
        if on_foot && left && event != 0 && event < 7 {
            game.lose_current_car();
        }
        result
    }

    /// On leaving a road, survivors are written back to it; wrecks leave empty slots
    /// (`r2 $7FE4`). The arena keeps nothing.
    fn write_back(&mut self, game: &mut Game) {
        if let BattleKind::Road { road, .. } = self.kind
            && let Some(rs) = game.roads.get_mut(road as usize) {
            for (i, e) in self.enemies.iter().enumerate() {
                if i < rs.slots.len() {
                    rs.slots[i] = e.as_ref().filter(|e| !e.is_wreck()).map(|e| e.kind);
                }
            }
        }
    }

    /// Once per frame (`r2 $7D32`): weapon cooldowns, paint, spin and blind timers.
    fn tick_timers(&mut self) {
        for c in self.cooldown.iter_mut() {
            if *c >= 0 {
                *c -= 1;
            }
        }
        for e in self.enemies.iter_mut().flatten() {
            e.tick();
        }
    }

    /// Enemy (incl. wrecks when `wrecks`) whose centre is within `r - 1` units of (x, y)
    /// on both axes, searched from the last slot down like the original.
    fn enemy_near(&self, x: i32, y: i32, r: i32, wrecks: bool) -> Option<usize> {
        (0..ENEMY_SLOTS).rev().find(|&i| {
            matches!(&self.enemies[i], Some(e) if (wrecks || !e.is_wreck()) && (e.x - x).abs() < r && (e.y - y).abs() < r)
        })
    }

    /// Radar is shown while the power plant has at least 5 DP and you are in the car.
    pub fn radar_on(&self, game: &Game) -> bool {
        !self.player.on_foot && game.car().map(|c| c.plant.dp >= 5).unwrap_or(false)
    }
    /// The road day has not passed yet (frames left).
    pub fn trip_frames_left(&self) -> u32 {
        self.trip_timer
    }

    /// Tools/tests: wreck every enemy still in action, as if shot (arena opponents count down).
    pub fn debug_wreck_all(&mut self) {
        for i in 0..ENEMY_SLOTS {
            if matches!(&self.enemies[i], Some(e) if !e.is_wreck()) {
                self.damage_enemy(i, 1000);
            }
        }
    }
}
