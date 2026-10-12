//! Firing and projectiles for both sides (`r2 $6F00` fire, `r2 $723D` / `r3 $19CB` updates).
//!
//! Each side has 16 projectile slots. A new projectile takes a free slot, otherwise the
//! oldest one. Delivery depends on the weapon:
//! * traced (MG, flamethrower, recoilless rifle, anti-tank gun, laser): an instant line from
//!   the shooter in the firing direction, stepping (cos16, -sin16) per step, that ends when
//!   its tracer leaves the visible playfield (range = the screen) or after a hit. MG and
//!   flamethrower repeat the trace for 4 frames; scenery never blocks a shot.
//! * rockets (RL, HR): 3 x (cos16, -sin16) per frame for 48 frames.
//! * droppers (MD, SD): a mine or spikes 6 units out from the mount, until the slot is reused.
//! * smokescreen, paint spray, oil jet: an emitter that drops one cloud per frame for 4
//!   frames; smoke and paint clouds last 64 frames, oil slicks forever.
use super::map::{in_active_window, MapView};
use super::motion::{step_vector, DIR16_TO_FRAME8, MOUNT_OFFSET};
use super::{Battle, Sfx};
use crate::game::Game;
use crate::tables::{w, WEAPONS};

/// Projectile kinds: the weapon types 0..11 plus the three cloud objects.
pub mod pk {
    pub use crate::tables::w::*;
    pub const SMOKE: u8 = 12;
    pub const PAINT: u8 = 13;
    pub const OIL: u8 = 14;
}

/// Shape numbers (non-modshaps) of the effects.
pub mod shape {
    pub const SPIKES: usize = 67;
    pub const MINE: usize = 68;
    pub const BULLET: usize = 73;
    pub const SHELL: usize = 74;
    pub const ROCKET: usize = 75;
    pub const SMOKE: usize = 83;
    pub const PAINT: usize = 87;
    pub const OIL: usize = 91;
    pub const FLAME: usize = 92;
    pub const FIRE: usize = 100;
}

/// How long (frames) each kind lives once fired; 0 = until its slot is reused.
fn lifetime(kind: u8) -> u32 {
    match kind {
        pk::MG | pk::FT | pk::SS | pk::PS | pk::OJ => 4,
        pk::RL | pk::HR => 48,
        pk::SMOKE | pk::PAINT => 64,
        _ => 0,
    }
}

#[derive(Clone, Debug)]
pub struct Proj {
    /// see [`pk`]
    pub kind: u8,
    pub x: i32,
    pub y: i32,
    /// flight heading of a rocket
    pub dir: u8,
    /// mount it was fired from (0 F, 1 B, 2 L, 3 R)
    pub mount: u8,
    /// frames left, or 0 for "until the slot is reused"
    pub life: u32,
    /// cloud animation frame 0..3
    pub anim: u8,
    /// enemy slot that fired it (enemy projectiles)
    pub owner: Option<usize>,
    /// frames since it was fired (`$1180`; the oldest is replaced first)
    pub age: u32,
}

impl Proj {
    /// Shape to draw for a projectile that is an object on the map (traces are drawn from
    /// [`Battle::traces`] instead; emitters are invisible).
    pub fn shape(&self) -> Option<usize> {
        Some(match self.kind {
            pk::RL | pk::HR => shape::ROCKET + DIR16_TO_FRAME8[self.dir as usize & 15] as usize,
            pk::MD => shape::MINE,
            pk::SD => shape::SPIKES,
            pk::SMOKE => shape::SMOKE + self.anim as usize,
            pk::PAINT => shape::PAINT + self.anim as usize,
            pk::OIL => shape::OIL,
            _ => return None,
        })
    }
    /// Lies flat on the ground (drawn under the cars).
    pub fn on_ground(&self) -> bool {
        matches!(self.kind, pk::MD | pk::SD | pk::OIL)
    }
}

/// One side's 16 projectile slots.
#[derive(Clone, Debug)]
pub struct ProjSlots {
    pub slots: Vec<Option<Proj>>,
    next: usize,
    /// the slot being updated right now (never handed out by `alloc`)
    busy: Option<usize>,
}

impl ProjSlots {
    pub fn new() -> ProjSlots {
        ProjSlots { slots: vec![None; 16], next: 0, busy: None }
    }
    /// A free slot (searching on from the last one used), else the oldest (`r2 $71E0`).
    fn alloc(&mut self) -> usize {
        let n = self.slots.len();
        let busy = self.busy;
        let i = (0..n)
            .map(|k| (self.next + k) % n)
            .find(|&i| self.slots[i].is_none() && Some(i) != busy)
            .unwrap_or_else(|| {
                (0..n)
                    .filter(|&i| Some(i) != busy)
                    .max_by_key(|&i| self.slots[i].as_ref().map(|p| p.age).unwrap_or(0))
                    .unwrap_or(0)
            });
        self.next = (i + 1) % n;
        i
    }
    fn put(&mut self, p: Proj) -> usize {
        let i = self.alloc();
        self.slots[i] = Some(p);
        i
    }
    pub fn iter(&self) -> impl Iterator<Item = &Proj> {
        self.slots.iter().flatten()
    }
}

impl Default for ProjSlots {
    fn default() -> Self {
        ProjSlots::new()
    }
}

/// A tracer dot or flame burst drawn this frame at a world position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraceDot {
    pub x: i32,
    pub y: i32,
    pub shape: usize,
}

/// Who fired a trace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shooter {
    Player,
    Enemy(usize),
}

/// What the tracer of a traced weapon looks like: (shape, a dot every n steps).
fn trace_look(kind: u8, dir: u8) -> (usize, u32) {
    match kind {
        pk::FT => (shape::FLAME + DIR16_TO_FRAME8[dir as usize & 15] as usize, 4),
        pk::RR | pk::AT => (shape::SHELL, 3),
        pk::LA => (shape::BULLET, 1),
        _ => (shape::BULLET, 3),
    }
}

/// Is a world point inside the visible playfield around the camera? (240 x 128 screen
/// pixels = +-60 x +-32 units; the original stops a trace at the first tracer dot that
/// draws nothing.)
pub fn on_screen(cam: (i32, i32), x: i32, y: i32) -> bool {
    let dx = x - cam.0;
    let dy = y - cam.1;
    (-60..60).contains(&dx) && (-32..32).contains(&dy)
}

impl Battle {
    /// Fire the selected weapon (`r2 $6F00`). `origin` is the car's position at the start
    /// of the frame.
    pub(super) fn player_fire(&mut self, game: &mut Game, view: &MapView, origin: (i32, i32)) {
        let Some(slot) = self.selected.map(|s| s as usize) else {
            self.click(Sfx::Beep);
            return;
        };
        if self.cooldown[slot] >= 0 {
            return;
        }
        let Some(car) = game.car_mut() else { return };
        let Some(wp) = car.weapons[slot] else {
            self.click(Sfx::Beep);
            self.say("No weapon!");
            return;
        };
        match wp.kind {
            w::LA => {
                if car.battery == 0 {
                    self.click(Sfx::Click);
                    self.say("Out of ammo!");
                    return;
                }
                car.battery -= 1;
            }
            // one shot: the launcher goes with it (weight, spaces and value come back)
            w::HR => car.weapons[slot] = None,
            _ => {
                if wp.ammo == 0 {
                    self.click(Sfx::Click);
                    self.say("Out of ammo!");
                    return;
                }
                // the round's weight comes off automatically (Car::weight counts ammo)
                car.weapons[slot].as_mut().unwrap().ammo -= 1;
            }
        }
        self.cooldown[slot] = WEAPONS[wp.kind as usize].cooldown as i8;
        self.launch(game, view, Shooter::Player, wp.kind, origin, wp.mount);
    }

    /// Put a newly fired projectile in play for either side. Instant traces run at once.
    fn launch(&mut self, game: &mut Game, view: &MapView, who: Shooter, kind: u8, origin: (i32, i32), mount: u8) {
        let heading = match who {
            Shooter::Player => self.player.new_heading,
            Shooter::Enemy(i) => self.enemies[i].as_ref().map(|e| e.heading).unwrap_or(0),
        };
        let dir = (heading + MOUNT_OFFSET[mount as usize & 3]) & 15;
        let owner = match who {
            Shooter::Player => None,
            Shooter::Enemy(i) => Some(i),
        };
        let visible = on_screen((self.player.x, self.player.y), origin.0, origin.1);
        let mut p = Proj { kind, x: origin.0, y: origin.1, dir, mount, life: lifetime(kind), anim: 0, owner, age: 0 };
        match kind {
            pk::RR | pk::AT | pk::LA => {
                if who == Shooter::Player || visible {
                    self.sfx.push(if kind == pk::LA { Sfx::Laser } else { Sfx::Shot });
                }
                self.trace(game, view, who, kind, origin, dir);
                return;
            }
            pk::RL | pk::HR => {
                if who == Shooter::Player || visible {
                    self.sfx.push(Sfx::Rocket);
                }
            }
            pk::MD | pk::SD | pk::SMOKE | pk::PAINT | pk::OIL => {
                // dropped 6 units out from the mount (`r2 $711E` launch_drop)
                let (sx, sy) = step_vector(dir, 3);
                if view.object_x_ok(p.x + sx) {
                    p.x += sx;
                }
                if view.object_y_ok(p.y + sy) {
                    p.y += sy;
                }
                if matches!(kind, pk::MD | pk::SD) && (who == Shooter::Player || visible) {
                    self.sfx.push(Sfx::Drop);
                }
            }
            pk::SS | pk::PS | pk::OJ if who == Shooter::Player || visible => {
                self.sfx.push(Sfx::Drop);
            }
            _ => {}
        }
        match who {
            Shooter::Player => self.pproj.put(p),
            Shooter::Enemy(_) => self.eproj.put(p),
        };
    }

    /// An instant trace (`r2 $72A1` player / `r3 $1C15` enemy). The player's trace rolls
    /// to hit the first enemy it passes within 2 units of and stops on a hit; an enemy's
    /// trace ignores other enemies and hits the player once if it passed him.
    pub(super) fn trace(&mut self, game: &mut Game, view: &MapView, who: Shooter, kind: u8, origin: (i32, i32), dir: u8) {
        let (sx, sy) = step_vector(dir, 1);
        let (dot, every) = trace_look(kind, dir);
        let cam = (self.player.x, self.player.y);
        let (mut x, mut y) = origin;
        let mut counter = every;
        let mut contacted = false;
        for _ in 0..512 {
            let mut dead = false;
            if view.object_x_ok(x + sx) {
                x += sx;
            } else {
                dead = true;
            }
            if view.object_y_ok(y + sy) {
                y += sy;
            } else {
                dead = true;
            }
            match who {
                Shooter::Player => {
                    if !contacted
                        && let Some(i) = self.enemy_near(x, y, 3, false) {
                        contacted = true;
                        if self.player_shot(game, i, kind) {
                            return;
                        }
                    }
                }
                Shooter::Enemy(_) => {
                    // other cars shield the player at that step (`r2 $7D9F` checks them first)
                    if self.enemy_near(x, y, 3, false).is_none()
                        && (x - self.player.x).abs() < 3
                        && (y - self.player.y).abs() < 3
                    {
                        contacted = true;
                    }
                }
            }
            let mut visible = true;
            if counter == 1 {
                visible = on_screen(cam, x, y);
                if visible {
                    self.traces.push(TraceDot { x, y, shape: dot });
                }
            }
            counter -= 1;
            if counter == 0 {
                counter = every;
            }
            if dead || !visible {
                break;
            }
        }
        if let (Shooter::Enemy(owner), true) = (who, contacted) {
            self.enemy_shot(game, owner, kind, (x, y), dir);
        }
    }

    /// Update the player's projectiles (`r2 $723D`). The player's cloud contacts of the
    /// previous frame are forgotten first.
    pub(super) fn update_player_projectiles(&mut self, game: &mut Game, view: &MapView) {
        self.player.in_smoke = false;
        self.player.in_paint = false;
        self.player.in_oil = false;
        for s in 0..self.pproj.slots.len() {
            let Some(mut p) = self.pproj.slots[s].take() else { continue };
            p.age = (p.age + 1).min(255);
            self.pproj.busy = Some(s);
            let keep = self.update_projectile(game, view, &mut p, None);
            self.pproj.busy = None;
            if keep {
                self.pproj.slots[s] = Some(p);
            }
        }
    }

    /// Update the enemies' projectiles (`r3 $19CB`). A projectile whose owner is gone
    /// goes too.
    pub(super) fn update_enemy_projectiles(&mut self, game: &mut Game, view: &MapView) {
        for s in 0..self.eproj.slots.len() {
            let Some(mut p) = self.eproj.slots[s].take() else { continue };
            let Some(owner) = p.owner.filter(|&o| self.enemies.get(o).map(|e| e.is_some()).unwrap_or(false)) else {
                continue;
            };
            p.age = (p.age + 1).min(255);
            self.eproj.busy = Some(s);
            let keep = self.update_projectile(game, view, &mut p, Some(owner));
            self.eproj.busy = None;
            if keep {
                self.eproj.slots[s] = Some(p);
            }
        }
    }

    /// One frame of one projectile. Returns false when it is used up.
    fn update_projectile(&mut self, game: &mut Game, view: &MapView, p: &mut Proj, owner: Option<usize>) -> bool {
        let who = match owner {
            None => Shooter::Player,
            Some(o) => Shooter::Enemy(o),
        };
        // the firing car: its position and heading now
        let (cx, cy, heading) = match who {
            Shooter::Player => (self.player.frame_origin.0, self.player.frame_origin.1, self.player.new_heading),
            Shooter::Enemy(o) => match &self.enemies[o] {
                Some(e) => (e.x, e.y, e.heading),
                None => return false,
            },
        };
        let mount_dir = (heading + MOUNT_OFFSET[p.mount as usize & 3]) & 15;
        let visible = on_screen((self.player.x, self.player.y), cx, cy);
        match p.kind {
            pk::MG | pk::FT => {
                // a burst traces again every frame from where the car is now
                if who == Shooter::Player || visible {
                    self.sfx.push(if p.kind == pk::FT { Sfx::Flame } else { Sfx::Shot });
                }
                self.trace(game, view, who, p.kind, (cx, cy), mount_dir);
                if p.kind == pk::FT {
                    self.launch(game, view, who, pk::SMOKE, (cx, cy), p.mount);
                }
                tick_life(p)
            }
            pk::RL | pk::HR => {
                let (sx, sy) = step_vector(p.dir, 3);
                let mut alive = true;
                if view.object_x_ok(p.x + sx) {
                    p.x += sx;
                } else {
                    alive = false;
                }
                if view.object_y_ok(p.y + sy) {
                    p.y += sy;
                } else {
                    alive = false;
                }
                match who {
                    Shooter::Player => {
                        if let Some(i) = self.enemy_near(p.x, p.y, 3, false) {
                            // a missed roll flies on
                            if self.player_shot(game, i, p.kind) {
                                return false;
                            }
                        }
                    }
                    Shooter::Enemy(o) => {
                        if self.enemy_near(p.x, p.y, 3, false).is_none() && self.touches_player(p.x, p.y) {
                            // FIX: an enemy rocket that hits is used up (the original let it
                            // fly on and hit again on the next frame)
                            if self.enemy_shot(game, o, p.kind, (p.x, p.y), p.dir) {
                                return false;
                            }
                        }
                    }
                }
                alive && tick_life(p)
            }
            pk::MD | pk::SD => {
                if let Some(i) = self.enemy_near(p.x, p.y, 3, false) {
                    self.drop_hits_enemy(game, i, p.kind, who);
                    return false;
                }
                if self.touches_player(p.x, p.y) {
                    self.drop_hits_player(game, p.kind);
                    return false;
                }
                true
            }
            pk::SS | pk::PS | pk::OJ => {
                let cloud = match p.kind {
                    pk::SS => pk::SMOKE,
                    pk::PS => pk::PAINT,
                    _ => pk::OIL,
                };
                let alive = tick_life(p);
                self.launch(game, view, who, cloud, (cx, cy), p.mount);
                alive
            }
            pk::SMOKE | pk::PAINT => {
                p.anim = (p.anim + 1 + game.rng.upto(2) as u8) % 4;
                if let Some(i) = self.enemy_near(p.x, p.y, 3, false) {
                    self.cloud_hits_enemy(game, i, p.kind);
                } else if self.touches_player(p.x, p.y) {
                    self.cloud_hits_player(game, p.kind);
                }
                tick_life(p)
            }
            pk::OIL => {
                if let Some(i) = self.enemy_near(p.x, p.y, 3, false) {
                    let inside = self.enemies[i].as_ref().map(|e| in_active_window((self.player.x, self.player.y), e.x, e.y)).unwrap_or(false);
                    if inside {
                        self.cloud_hits_enemy(game, i, p.kind);
                    }
                } else if self.touches_player(p.x, p.y) {
                    self.cloud_hits_player(game, p.kind);
                }
                true
            }
            _ => false,
        }
    }

    fn touches_player(&self, x: i32, y: i32) -> bool {
        (x - self.player.x).abs() < 3 && (y - self.player.y).abs() < 3
    }

    /// An enemy fires its weapon from `mount` (`r3 $1822`). Needs its cooldown ready.
    pub(super) fn enemy_fire(&mut self, game: &mut Game, view: &MapView, i: usize) {
        let Some(e) = self.enemies[i].as_mut() else { return };
        if e.cooldown >= 0 {
            return;
        }
        e.cooldown = WEAPONS[e.weapon as usize].cooldown as i8;
        let (kind, origin, mount) = (e.weapon, (e.x, e.y), e.mount);
        self.launch(game, view, Shooter::Enemy(i), kind, origin, mount);
    }
}

fn tick_life(p: &mut Proj) -> bool {
    if p.life == 0 {
        return true;
    }
    p.life -= 1;
    p.life > 0
}
