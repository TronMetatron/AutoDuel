//! Enemy cars: spawning (`r2 $7E6B` road_init / `a2 $7DD0` arena_init), AI
//! (`r2 $8782` enemy_ai), movement (`r3 $1701`) and firing decisions (`r2 $88C2`).
//!
//! An enemy has one pool of hit points, 8 headings (even values of the 16-step compass),
//! no brakes and no handling model. It only thinks and moves inside the active window
//! around the player's tile. States, advanced 0 -> 5 -> 0 with p = 8/256 per frame:
//! 0 shadow (steer at the player, match his speed), 1/4/5 flank (aim 24 units to his side),
//! 2 intercept (aim 64 units ahead of him), 3 attack (drive straight at him). Everyone
//! attacks while the player has stood still for 4+ frames. 7 = wreck, 8 = salvaged wreck.
use super::map::{in_active_window, MapView};
use super::motion::{enemy_frame, grid_bearing, step_vector, vehicle_velocity};
use super::{Battle, BattleKind, Sfx, ENEMY_SLOTS};
use crate::game::Game;
use crate::rng::Rng;
use crate::tables::{w, EnemyType, ARENA_ENEMIES, ROAD_ENEMIES};

pub const ATTACK: u8 = 3;
/// A destroyed car: coasts to a stop, may burn, can be salvaged once.
pub const WRECK: u8 = 7;
/// A wreck that has been searched.
pub const SALVAGED: u8 = 8;

/// `r2 $8904`: (bearing to the player - heading) / 2 -> the mount that faces him.
const REL_TO_MOUNT: [u8; 8] = [0, 0, 2, 1, 1, 1, 3, 0];

#[derive(Clone, Debug)]
pub struct Enemy {
    /// type 0..15 (road or arena table)
    pub kind: u8,
    pub weapon: u8,
    /// mount the weapon fires from (re-chosen at each shot to face the player)
    pub mount: u8,
    pub accel: u8,
    pub max: u8,
    pub ai_period: u8,
    /// sprite set 0..4 (shapes 27 + 8 * body)
    pub body: u8,
    pub x: i32,
    pub y: i32,
    /// 16-step heading, always even
    pub heading: u8,
    pub speed: u8,
    pub sub: u8,
    pub hp: i32,
    /// AI state 0..5, [`WRECK`] or [`SALVAGED`]
    pub state: u8,
    ai_timer: u8,
    /// what stopped the last move: $80 obstacle/window edge, $40 another car, $20 the player
    blocked: u8,
    /// flanking side, 4 or 12 (90 degrees left or right of the bearing)
    flank_side: u8,
    /// oil: random turning, no AI, no firing (`$0D40` bit 7)
    pub spin: bool,
    /// paint (or smoke for a moment): no firing (`$0D40` bit 6)
    pub blind: bool,
    pub effect_timer: u8,
    /// paint countdown (+2 to rolls while > 0), smoke and oil marks for the next roll
    pub paint: u8,
    pub smoke: bool,
    pub oil: bool,
    /// a wreck that caught fire: nothing to salvage
    pub burning: bool,
    /// weapon cooldown, ready when negative (`$0D60`)
    pub cooldown: i8,
    /// velocity chosen by the AI this frame
    vel: (i32, i32),
}

impl Enemy {
    pub fn new(t: &EnemyType, kind: u8, slot: usize, x: i32, y: i32) -> Enemy {
        Enemy {
            kind,
            weapon: t.weapon,
            mount: t.mount,
            accel: t.accel,
            max: t.max,
            ai_period: t.ai_period,
            body: t.body,
            x,
            y,
            heading: 0,
            speed: 0,
            sub: 0,
            hp: t.hp as i32,
            state: (slot & 3) as u8,
            ai_timer: 0,
            blocked: 0,
            flank_side: 4,
            spin: false,
            blind: false,
            effect_timer: 0,
            paint: 0,
            smoke: false,
            oil: false,
            burning: false,
            cooldown: -1,
            vel: (0, 0),
        }
    }
    pub fn is_wreck(&self) -> bool {
        self.state >= WRECK
    }
    /// Shape to draw (non-modshaps 27 + 8 * body + frame).
    pub fn shape(&self) -> usize {
        27 + 8 * self.body as usize + enemy_frame(self.heading)
    }

    /// Per-frame timers (`r2 $7D32`).
    pub(super) fn tick(&mut self) {
        if self.cooldown >= 0 {
            self.cooldown -= 1;
        }
        if self.paint > 0 {
            self.paint -= 1;
        }
        if self.effect_timer > 0 {
            self.effect_timer -= 1;
            if self.effect_timer == 0 {
                self.spin = false;
                self.blind = false;
            }
        }
    }

    /// `r3 $1400`: one 45-degree step toward `target`, which counts as straight ahead when
    /// within `dz` units on an axis. Returns the new heading and the bearing (None when
    /// the target is centred).
    fn steer(&self, target: (i32, i32), dz: i32) -> (u8, Option<u8>) {
        let b = grid_bearing((self.x, self.y), target, dz);
        let h = match b {
            Some(b) if b != self.heading => {
                if (b as i32 - self.heading as i32) & 15 >= 8 {
                    (self.heading + 14) & 15
                } else {
                    (self.heading + 2) & 15
                }
            }
            _ => self.heading,
        };
        (h, b)
    }

    fn accelerate(&mut self) {
        let sub = self.sub + self.accel;
        if sub < 32 {
            self.sub = sub;
            return;
        }
        self.sub = sub & 31;
        self.speed = (self.speed + 1).min(self.max);
    }

    fn random_turn(&mut self, rng: &mut Rng) {
        self.heading = if rng.coin() { (self.heading + 2) & 15 } else { (self.heading + 14) & 15 };
    }

    /// Count the AI period; true on a decision frame.
    fn decide(&mut self) -> bool {
        self.ai_timer += 1;
        if self.ai_timer >= self.ai_period.max(1) {
            self.ai_timer = 0;
            true
        } else {
            false
        }
    }
}

/// What the AI needs to know about the player.
#[derive(Clone, Copy)]
struct Target {
    pos: (i32, i32),
    heading: u8,
    speed: u8,
}

impl Battle {
    /// Place the enemies at battle start. Roads: the road's persistent types always appear;
    /// each empty slot spawns a random type 0..15 with p ~ 1/5, or always while the brain
    /// tape is aboard. The arena uses the event's fixed roster.
    pub(super) fn spawn_enemies(&mut self, game: &mut Game, data: &crate::data::GameData) {
        match self.kind {
            BattleKind::Road { road, .. } => {
                let tape = game.driver.quest.carrying_tape();
                let r = &data.roads[road as usize];
                for i in 0..ENEMY_SLOTS.min(r.slots.len()) {
                    let preset = game.roads.get(road as usize).and_then(|s| s.slots.get(i).copied()).flatten();
                    let kind = match preset {
                        Some(k) => Some(k),
                        None if tape || game.rng.upto(4) == 0 => Some(game.rng.upto(15) as u8),
                        None => None,
                    };
                    if let Some(k) = kind {
                        let t = &ROAD_ENEMIES[k as usize & 15];
                        self.enemies[i] = Some(Enemy::new(t, k & 15, i, r.slots[i].x, r.slots[i].y));
                    }
                }
            }
            BattleKind::Arena { event } => {
                let ev = &data.arena.events[event.min(7) as usize];
                for (i, s) in ev.slots.iter().enumerate().take(ENEMY_SLOTS) {
                    if let Some(k) = s.kind {
                        let t = &ARENA_ENEMIES[k as usize & 15];
                        self.enemies[i] = Some(Enemy::new(t, k & 15, i, s.x, s.y));
                        self.opponents_left += 1;
                    }
                }
            }
        }
    }

    /// AI, movement and firing for every enemy inside the active window.
    pub(super) fn enemies_step(&mut self, game: &mut Game, view: &MapView) {
        let target = Target {
            pos: (self.player.x, self.player.y),
            heading: self.player.new_heading,
            speed: if self.player.on_foot { 0 } else { self.player.speed },
        };
        for i in 0..ENEMY_SLOTS {
            let Some(mut e) = self.enemies[i].take() else { continue };
            if !in_active_window(target.pos, e.x, e.y) {
                self.enemies[i] = Some(e);
                continue;
            }
            let state = e.state;
            if e.spin {
                // oil: random +-45 degrees, no AI
                e.random_turn(&mut game.rng);
                e.vel = vehicle_velocity(e.heading, e.speed);
            } else {
                ai_state(&mut e, target, &mut game.rng);
            }
            self.enemy_move(game, view, &mut e);
            // next state (`r2 $8898`)
            if state < WRECK {
                e.state = if self.player.stopped_frames >= 4 {
                    ATTACK
                } else {
                    let s = if game.rng.byte() < 8 { state + 1 } else { state };
                    if s >= 6 { 0 } else { s }
                };
            }
            self.enemies[i] = Some(e);
            self.enemy_ai_fire(game, view, i);
        }
    }

    /// Move one enemy (`r3 $1701`): blocked by scenery or the window edge it stops dead;
    /// touching another car or the player's car it slows by one level. Touching a
    /// pedestrian kills him.
    fn enemy_move(&mut self, game: &mut Game, view: &MapView, e: &mut Enemy) {
        e.blocked = 0;
        if e.speed == 0 && e.state < WRECK && e.state != ATTACK {
            // unstick (`r2 $8AD9`): crawl off in a random direction
            e.speed = 1;
            e.random_turn(&mut game.rng);
            e.vel = vehicle_velocity(e.heading, e.speed);
        }
        let (vx, vy) = e.vel;
        let mut nx = e.x + vx;
        let mut ny = e.y + vy;
        if !view.object_x_ok(nx) {
            nx = e.x;
        }
        if !view.object_y_ok(ny) {
            ny = e.y;
        }
        let p = &self.player;
        if !in_active_window((p.x, p.y), nx, ny) || view.blocked(nx, ny) {
            e.blocked = 0x80;
            e.speed = 0;
            e.sub = 31;
            return;
        }
        let other = self.enemies.iter().flatten().any(|o| (o.x - nx).abs() < 6 && (o.y - ny).abs() < 6);
        if other {
            e.blocked = 0x40;
            e.speed = e.speed.saturating_sub(1);
            e.sub = 0;
            return;
        }
        if p.on_foot && e.state < WRECK && (nx - p.x).abs() < 2 && (ny - p.y).abs() < 2 {
            // run over
            game.driver.health = 0;
            game.driver.body_armor = 0;
            self.sfx.push(Sfx::Crash);
        }
        let car = if p.on_foot { p.has_car.then_some((p.car_x, p.car_y)) } else { Some((p.x, p.y)) };
        if let Some((cx, cy)) = car
            && (nx - cx).abs() < 6 && (ny - cy).abs() < 6 {
            e.blocked = 0x20;
            e.speed = e.speed.saturating_sub(1);
            e.sub = 0;
            return;
        }
        e.x = nx;
        e.y = ny;
    }

    /// Fire decision (`r2 $88C2`, arena `a2 $88CA`): not blinded, spinning or wrecked,
    /// then a per-frame chance (road guns 32/256, arena 24/256, droppers 12/256). The
    /// mount is the side facing the player (droppers and arena type 0 keep theirs). The
    /// shot still flies along heading + mount. Arena cars never shoot a pedestrian.
    fn enemy_ai_fire(&mut self, game: &mut Game, view: &MapView, i: usize) {
        let arena = self.is_arena();
        let player = (self.player.x, self.player.y);
        if arena && self.player.on_foot {
            return;
        }
        let Some(e) = self.enemies[i].as_mut() else { return };
        if e.spin || e.blind || e.is_wreck() {
            return;
        }
        let gun = e.weapon <= w::LA;
        let chance = if !gun { 12 } else if arena { 24 } else { 32 };
        if game.rng.byte() as u32 >= chance {
            return;
        }
        if e.weapon != w::MD && !(arena && e.kind == 0) {
            let (_, b) = e.steer(player, 1);
            let b = b.unwrap_or(16) as i32;
            e.mount = REL_TO_MOUNT[(((b - e.heading as i32) & 15) >> 1) as usize];
        }
        self.enemy_fire(game, view, i);
    }
}

/// The state handlers (`r2 $892E`).
fn ai_state(e: &mut Enemy, t: Target, rng: &mut Rng) {
    match e.state {
        // shadow: every AI period steer at the player (dead zone 24); match his speed
        // when at least as fast and he is moving, else speed up
        0 => {
            if e.decide() {
                e.heading = e.steer(t.pos, 24).0;
                if e.speed >= t.speed && t.speed != 0 {
                    e.speed = t.speed;
                } else {
                    e.accelerate();
                }
            }
        }
        // flank: aim 24 units to the player's side; switch sides after hitting something
        1 | 4 | 5 => {
            if e.blocked & 0x80 != 0 {
                e.flank_side ^= 8;
            }
            if e.decide() {
                let (_, b) = e.steer(t.pos, 1);
                // 90 degrees off the bearing (a centred bearing counts as 16, as in the original)
                let dir = (b.unwrap_or(16) + e.flank_side) & 15;
                let (ox, oy) = step_vector(dir, 12);
                let dz = if e.state == 1 { 2 } else { 5 };
                e.heading = e.steer((t.pos.0 + ox, t.pos.1 + oy), dz).0;
            }
            e.accelerate();
        }
        // intercept: aim 64 units ahead of the player
        2 => {
            if e.decide() {
                let (ox, oy) = step_vector(t.heading, 32);
                e.heading = e.steer((t.pos.0 + ox, t.pos.1 + oy), 1).0;
            }
            e.accelerate();
        }
        // attack: straight at the player, every frame
        ATTACK => {
            e.ai_timer = 0;
            e.heading = e.steer(t.pos, 1).0;
            e.accelerate();
        }
        // wreck: coast down at its acceleration rate, wobbling; 50% it burns when it stops
        _ => {
            if e.speed > 0 {
                let sub = e.sub + e.accel;
                if sub >= 32 {
                    e.sub = sub & 31;
                    e.speed -= 1;
                    if e.speed == 0 && rng.coin() {
                        e.burning = true;
                    }
                } else {
                    e.sub = sub;
                }
            }
            e.vel = vehicle_velocity(e.heading, e.speed);
            if e.speed > 0 {
                e.random_turn(rng);
            }
            return;
        }
    }
    e.vel = vehicle_velocity(e.heading, e.speed);
}
