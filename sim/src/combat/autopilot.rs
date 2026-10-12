//! An automatic driver for tests and tools (not part of the original game).
//!
//! It plans a route over the map on a 2-unit grid that keeps clear of scenery (A* with a
//! penalty for squeezing past things), then steers along it with the joystick the way a
//! player would: push toward a point a little way ahead, ease off for sharp turns, back up
//! and re-plan after a crash. With `shoot` set it fires the selected weapon whenever an
//! enemy is on its line of fire.
use super::map::{MapView, TILE_H, TILE_W};
use super::motion::{heading_diff, step_vector, MOUNT_OFFSET};
use super::{Battle, BattleKind, DriveInput};
use crate::data::GameData;
use crate::game::Game;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Grid cell size in world units.
const CELL: i32 = 2;
/// Clearance (cells) below which a cell costs extra.
const COMFORT: i32 = 6;
/// Path points to look ahead when steering.
const LOOK: usize = 6;

/// Where to go: a box of half-size (rx, ry) around (x, y) (exclusive, like the exits).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Goal {
    pub x: i32,
    pub y: i32,
    pub rx: i32,
    pub ry: i32,
}

impl Goal {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        (x - self.x).abs() < self.rx && (y - self.y).abs() < self.ry
    }
}

pub struct Autopilot {
    gw: i32,
    gh: i32,
    /// distance in cells to the nearest blocked cell (0 = blocked), capped at 15
    clear: Vec<u8>,
    path: Vec<(i32, i32)>,
    idx: usize,
    goal: Option<Goal>,
    planned_for: Option<Goal>,
    last_pos: (i32, i32),
    still: u32,
    backing: u32,
    /// crashes since the last good progress (widens the berth given to wrecks)
    crashes: u32,
    /// where we crashed lately (planned around)
    bumps: Vec<(i32, i32)>,
    /// frames left of pushing toward `wiggle_dir` to get unstuck
    wiggle: u32,
    wiggle_dir: u8,
    last_frame: u32,
    cached: DriveInput,
    /// fire the selected weapon when an enemy is on its line of fire
    pub shoot: bool,
    /// cruising speed level
    pub cruise: u8,
    /// on foot: walk to this point (e.g. a wreck to search) before going back to the car
    pub walk_to: Option<(i32, i32)>,
    /// the driving goal while walking somewhere else
    goal_saved: Option<Goal>,
}

impl Autopilot {
    /// Build the clearance grid of a map (the area the player may drive in).
    pub fn new(view: &MapView) -> Autopilot {
        let gw = ((view.w - 1) * TILE_W + CELL - 1) / CELL;
        let gh = ((view.h - 1) * TILE_H + CELL - 1) / CELL;
        let n = (gw * gh) as usize;
        let mut blocked = vec![false; n];
        for row in -1..=view.h {
            for col in -1..=view.w {
                for o in view.tile_objects(col, row) {
                    let (ox, oy) = (col * TILE_W + o.x, row * TILE_H + o.y);
                    let (xr, yr) = view.radius(o.shape);
                    let (x0, x1) = ((ox - xr).max(0), (ox + xr).min(gw * CELL - 1));
                    let (y0, y1) = ((oy - yr).max(0), (oy + yr).min(gh * CELL - 1));
                    if x0 > x1 || y0 > y1 {
                        continue;
                    }
                    for gy in (y0 + CELL - 1) / CELL..=y1 / CELL {
                        for gx in (x0 + CELL - 1) / CELL..=x1 / CELL {
                            blocked[(gy * gw + gx) as usize] = true;
                        }
                    }
                }
            }
        }
        // clearance: breadth-first distance (8-neighbour) from the blocked cells
        let mut clear = vec![15u8; n];
        let mut queue = std::collections::VecDeque::new();
        for (i, b) in blocked.iter().enumerate() {
            if *b {
                clear[i] = 0;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let (x, y) = ((i as i32) % gw, (i as i32) / gw);
            let d = clear[i];
            if d >= 14 {
                continue;
            }
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= gw || ny >= gh {
                        continue;
                    }
                    let j = (ny * gw + nx) as usize;
                    if clear[j] > d + 1 {
                        clear[j] = d + 1;
                        queue.push_back(j);
                    }
                }
            }
        }
        Autopilot {
            gw,
            gh,
            clear,
            path: vec![],
            idx: 0,
            goal: None,
            planned_for: None,
            last_pos: (i32::MIN, 0),
            still: 0,
            backing: 0,
            crashes: 0,
            bumps: vec![],
            wiggle: 0,
            wiggle_dir: 0,
            last_frame: u32::MAX,
            cached: DriveInput::default(),
            shoot: false,
            cruise: 2,
            walk_to: None,
            goal_saved: None,
        }
    }

    /// An autopilot heading for the far end of the road being driven (the exit you did not
    /// come in by), or for the arena gate you started at.
    pub fn for_battle(b: &Battle, data: &GameData) -> Autopilot {
        let mut a = Autopilot::new(&b.map(data));
        a.goal = Some(match b.kind {
            BattleKind::Road { road, from_city } => {
                let r = &data.roads[road as usize];
                let e = r.exits.iter().find(|e| e.city != from_city).unwrap_or(&r.exits[0]);
                Goal { x: e.trigger_x, y: e.trigger_y, rx: 16, ry: 16 }
            }
            BattleKind::Arena { .. } => arena_gate(b, data),
        });
        a
    }

    pub fn goal(&self) -> Option<Goal> {
        self.goal
    }
    pub fn set_goal(&mut self, g: Goal) {
        self.goal = Some(g);
    }
    /// A one-line summary for debugging tools.
    pub fn debug_state(&self) -> String {
        let next: Vec<_> = self.path.iter().skip(self.idx).take(8).collect();
        format!("idx {}/{} next {:?} crashes {} bumps {:?} backing {}", self.idx, self.path.len(), next, self.crashes, self.bumps, self.backing)
    }
    /// The planned route (world points), for drawing or checking.
    pub fn path(&self) -> &[(i32, i32)] {
        &self.path
    }

    fn cell(&self, x: i32, y: i32) -> Option<usize> {
        let (gx, gy) = ((x + CELL / 2).div_euclid(CELL), (y + CELL / 2).div_euclid(CELL));
        if gx < 0 || gy < 0 || gx >= self.gw || gy >= self.gh {
            None
        } else {
            Some((gy * self.gw + gx) as usize)
        }
    }
    /// Clearance (cells) at a world point; 0 outside the map.
    pub fn clearance(&self, x: i32, y: i32) -> u8 {
        self.cell(x, y).map(|i| self.clear[i]).unwrap_or(0)
    }

    /// Plan a route from `from` to the goal, keeping clear of the boxes `(x, y, half-size)`
    /// in `avoid` (wrecks: a car stops within 5 units of one; spots we crashed at). Returns
    /// false if there is none.
    pub fn plan(&mut self, from: (i32, i32), avoid: &[(i32, i32, i32)]) -> bool {
        self.plan_from(from, avoid, false)
    }

    /// As [`Autopilot::plan`]; `escape` lets the first few cells pass through scenery, to get
    /// out of a pocket the car has been wedged into.
    fn plan_from(&mut self, from: (i32, i32), avoid: &[(i32, i32, i32)], escape: bool) -> bool {
        self.path.clear();
        self.idx = 0;
        self.planned_for = self.goal;
        let Some(goal) = self.goal else { return false };
        let Some(start) = self.cell(from.0.clamp(0, self.gw * CELL - 1), from.1.clamp(0, self.gh * CELL - 1)) else {
            return false;
        };
        let gw = self.gw;
        let n = (self.gw * self.gh) as usize;
        let mut cost = vec![u32::MAX; n];
        let mut prev = vec![u32::MAX; n];
        let mut heap = BinaryHeap::new();
        let h = |i: usize| {
            let (x, y) = ((i as i32 % gw) * CELL, (i as i32 / gw) * CELL);
            let dx = ((x - goal.x).abs() - goal.rx).max(0) / CELL;
            let dy = ((y - goal.y).abs() - goal.ry).max(0) / CELL;
            (10 * dx.max(dy) + 4 * dx.min(dy)) as u32
        };
        cost[start] = 0;
        heap.push(Reverse((h(start), start)));
        let mut found = None;
        while let Some(Reverse((_, i))) = heap.pop() {
            let (x, y) = (i as i32 % gw, i as i32 / gw);
            if goal.contains(x * CELL, y * CELL) {
                found = Some(i);
                break;
            }
            let c = cost[i];
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= gw || ny >= self.gh {
                    continue;
                }
                let j = (ny * gw + nx) as usize;
                let near_start = escape && (nx * CELL - from.0).abs() <= 6 && (ny * CELL - from.1).abs() <= 6;
                if self.clear[j] == 0 && !near_start {
                    continue;
                }
                let (wx, wy) = (nx * CELL, ny * CELL);
                if avoid.iter().any(|&(ax, ay, r)| (ax - wx).abs() < r && (ay - wy).abs() < r) {
                    continue;
                }
                if dx != 0 && dy != 0 {
                    // no cutting corners
                    let a = (y * gw + nx) as usize;
                    let b = (ny * gw + x) as usize;
                    if (self.clear[a] == 0 || self.clear[b] == 0) && !near_start {
                        continue;
                    }
                }
                let step = if dx != 0 && dy != 0 { 14 } else { 10 };
                let tight = (COMFORT - self.clear[j] as i32).max(0) as u32;
                let nc = c + step + tight * tight * 6;
                if nc < cost[j] {
                    cost[j] = nc;
                    prev[j] = i as u32;
                    heap.push(Reverse((nc + h(j), j)));
                }
            }
        }
        let Some(mut i) = found else { return false };
        let mut pts = vec![];
        loop {
            pts.push(((i as i32 % gw) * CELL, (i as i32 / gw) * CELL));
            if i == start || prev[i] == u32::MAX {
                break;
            }
            i = prev[i] as usize;
        }
        pts.reverse();
        // finish inside the goal: run on through its centre
        pts.push((goal.x, goal.y));
        self.path = pts;
        true
    }

    /// Input for this frame of battle `b` (call once per simulation step; repeated calls on
    /// the same frame return the same input).
    pub fn input(&mut self, b: &Battle, game: &Game) -> DriveInput {
        if b.frame == self.last_frame {
            return self.cached;
        }
        self.last_frame = b.frame;
        let inp = self.decide(b, game);
        self.cached = inp;
        inp
    }

    fn decide(&mut self, b: &Battle, game: &Game) -> DriveInput {
        let p = &b.player;
        let me = (p.x, p.y);
        let mut out = DriveInput::default();
        if p.on_foot {
            if let Some(t) = self.walk_to {
                if (t.0 - p.x).abs() < 4 && (t.1 - p.y).abs() < 4 {
                    self.walk_to = None;
                    self.path.clear();
                } else {
                    let goal = Goal { x: t.0, y: t.1, rx: 4, ry: 4 };
                    if self.planned_for != Some(goal) || self.path.is_empty() || self.off_path(me) {
                        self.goal_saved = self.goal.or(self.goal_saved);
                        self.goal = Some(goal);
                        if !self.plan(me, &[]) {
                            self.walk_to = None;
                            return out;
                        }
                    }
                    let end = (self.idx + 40).min(self.path.len());
                    if let Some(k) = (self.idx..end).min_by_key(|&k| (self.path[k].0 - me.0).abs() + (self.path[k].1 - me.1).abs()) {
                        self.idx = k;
                    }
                    let t = self.path[(self.idx + 2).min(self.path.len() - 1)];
                    out.dx = (t.0 - me.0).signum() as i8;
                    out.dy = (t.1 - me.1).signum() as i8;
                    return out;
                }
            }
            if let Some(g) = self.goal_saved.take() {
                self.goal = Some(g);
                self.path.clear();
            }
            // get back in if the car is near, else walk to it
            if p.has_car {
                if (p.car_x - p.x).abs() < 6 && (p.car_y - p.y).abs() < 6 {
                    out.toggle_foot = true;
                } else {
                    self.walk_to = Some((p.car_x, p.car_y));
                    return self.decide(b, game);
                }
            }
            return out;
        }
        if self.shoot && self.on_target(b, game) {
            out.fire = true;
        }
        // stuck against something: back up for a moment, then plan again
        if me == self.last_pos && p.speed == 0 {
            self.still += 1;
        } else {
            self.still = 0;
            if b.frame.is_multiple_of(128) {
                self.crashes = self.crashes.saturating_sub(1);
            }
        }
        self.last_pos = me;
        if self.wiggle > 0 {
            self.wiggle -= 1;
            if self.wiggle == 0 {
                self.path.clear();
            }
            let (sx, sy) = stick_of(self.wiggle_dir);
            out.dx = sx;
            out.dy = sy;
            return out;
        }
        if self.backing > 0 {
            self.backing -= 1;
            if self.backing == 0 {
                self.path.clear();
            }
            let (sx, sy) = stick_of((p.heading + 8) & 15);
            out.dx = sx;
            out.dy = sy;
            return out;
        }
        if self.still > 6 {
            self.still = 0;
            self.backing = 10;
            self.crashes += 1;
            let (sx, sy) = step_vector(p.heading, 2);
            let bump = (me.0 + sx, me.1 + sy);
            if !self.bumps.iter().any(|b| (b.0 - bump.0).abs() <= 2 && (b.1 - bump.1).abs() <= 2) {
                if self.bumps.len() >= 16 {
                    self.bumps.remove(0);
                }
                self.bumps.push(bump);
            }
            if self.crashes % 4 == 3 {
                // wedged: wiggle off in some direction instead of backing straight up
                self.wiggle = 8;
                self.wiggle_dir = ((self.crashes * 6 + 2) & 14) as u8;
                self.backing = 0;
            }
            return out;
        }
        if self.path.is_empty() || self.planned_for != self.goal || self.off_path(me) {
            let wrecks = b.enemies.iter().flatten().filter(|e| e.is_wreck()).map(|e| (e.x, e.y));
            let berth = 8 + 4 * self.crashes.min(4) as i32;
            let bumps = self.bumps.iter().map(|&(x, y)| (x, y, 5));
            let mut avoid: Vec<(i32, i32, i32)> = wrecks.clone().map(|(x, y)| (x, y, berth)).chain(bumps).collect();
            if !self.plan(me, &avoid) {
                avoid = wrecks.map(|(x, y)| (x, y, 8)).collect();
                if !self.plan(me, &avoid) && !self.plan_from(me, &avoid, true) && !self.plan_from(me, &[], true) {
                    return out;
                }
            }
        }
        // closest point a little way along the route
        let end = (self.idx + 40).min(self.path.len());
        let mut best = (i64::MAX, self.idx);
        for k in self.idx..end {
            let (x, y) = self.path[k];
            let d = ((x - me.0) as i64).pow(2) + ((y - me.1) as i64).pow(2);
            if d < best.0 {
                best = (d, k);
            }
        }
        self.idx = best.1;
        let tight_here = self.clearance(me.0, me.1) < 3;
        let ahead = if tight_here { 3 } else { LOOK + p.speed as usize * 2 };
        let look = (self.idx + ahead).min(self.path.len() - 1);
        let t = self.path[look];
        let want = heading_to(me, t);
        let err = heading_diff(p.heading, want).abs();
        let tight = tight_here || self.clearance(t.0, t.1) < 3;
        let target_speed = if err >= 3 || tight { 1 } else { self.cruise };
        let stick = (want + 1) & 14; // nearest even (8-way) direction
        let too_fast = p.speed > target_speed || (p.speed == target_speed && p.sub >= 24);
        if too_fast && (err == 0 || b.frame % 2 == 1) {
            return out; // coast
        }
        let (sx, sy) = stick_of(stick);
        out.dx = sx;
        out.dy = sy;
        out
    }

    /// Wandered far from the route (pushed, skidded): re-plan.
    fn off_path(&self, me: (i32, i32)) -> bool {
        let end = (self.idx + 40).min(self.path.len());
        !(self.idx..end).any(|k| (self.path[k].0 - me.0).abs() <= 12 && (self.path[k].1 - me.1).abs() <= 12)
    }

    /// Is a live enemy on the selected weapon's line of fire (within the visible range)?
    pub fn on_target(&self, b: &Battle, game: &Game) -> bool {
        let Some(slot) = b.selected else { return false };
        let Some(wp) = game.car().and_then(|c| c.weapons[slot as usize]) else { return false };
        let dir = (b.player.heading + MOUNT_OFFSET[wp.mount as usize & 3]) & 15;
        let (sx, sy) = step_vector(dir, 1);
        let (mut x, mut y) = (b.player.x, b.player.y);
        for _ in 0..40 {
            x += sx;
            y += sy;
            if b.enemies.iter().flatten().any(|e| !e.is_wreck() && (e.x - x).abs() < 3 && (e.y - y).abs() < 3) {
                return true;
            }
        }
        false
    }
}

/// The gate the arena run started from (bottom of the arena).
pub fn arena_gate(b: &Battle, data: &GameData) -> Goal {
    let ex = b.exits(data);
    let e = &ex[1.min(ex.len() - 1)];
    Goal { x: e.trigger_x, y: e.trigger_y, rx: 16, ry: 4 }
}

/// Can a shot along one of the 16 headings from `from` pass within 2 units of `to`?
pub fn in_line(from: (i32, i32), to: (i32, i32)) -> bool {
    let h = heading_to(from, to);
    let (sx, sy) = step_vector(h, 1);
    let (mut x, mut y) = from;
    for _ in 0..64 {
        x += sx;
        y += sy;
        if (x - to.0).abs() < 3 && (y - to.1).abs() < 3 {
            return true;
        }
    }
    false
}

/// Joystick deflection for an (even) heading.
pub fn stick_of(h: u8) -> (i8, i8) {
    let (x, y) = step_vector(h & 14, 1);
    (x.signum() as i8, y.signum() as i8)
}

/// 16-step heading from one point toward another.
pub fn heading_to(from: (i32, i32), to: (i32, i32)) -> u8 {
    let dx = (to.0 - from.0) as f64;
    let dy = (from.1 - to.1) as f64; // north positive
    let a = dy.atan2(dx); // 0 = east, counter-clockwise
    (((a / (std::f64::consts::PI / 8.0)).round() as i32) & 15) as u8
}

/// An arena fighter: drives toward the nearest opponent; once close it stops, turns to face
/// him and fires whenever he is on the line of fire; leaves by the gate once none is left.
pub struct ArenaBot {
    pub pilot: Autopilot,
    /// distance (units, both axes) at which it stops to duel
    pub duel_range: i32,
}

impl ArenaBot {
    pub fn new(b: &Battle, data: &GameData) -> ArenaBot {
        let mut pilot = Autopilot::for_battle(b, data);
        pilot.shoot = true;
        ArenaBot { pilot, duel_range: 40 }
    }
    pub fn input(&mut self, b: &Battle, game: &Game, data: &GameData) -> DriveInput {
        let me = (b.player.x, b.player.y);
        let target = b
            .enemies
            .iter()
            .flatten()
            .filter(|e| !e.is_wreck())
            .min_by_key(|e| (e.x - me.0).abs() + (e.y - me.1).abs())
            .map(|e| (e.x, e.y));
        let Some((tx, ty)) = target else {
            self.pilot.set_goal(arena_gate(b, data));
            return self.pilot.input(b, game);
        };
        let p = &b.player;
        let close = (tx - me.0).abs() <= self.duel_range && (ty - me.1).abs() <= self.duel_range;
        if !p.on_foot && close && in_line(me, (tx, ty)) {
            // duel: stand still, face him, fire when he is on the line
            let mut out = DriveInput { fire: self.pilot.on_target(b, game), ..Default::default() };
            let err = heading_diff(p.heading, heading_to(me, (tx, ty)));
            if err != 0 && b.frame.is_multiple_of(2) {
                let stick = ((p.heading as i32 + 2 * err.signum()) & 14) as u8;
                let (sx, sy) = stick_of(stick);
                out.dx = sx;
                out.dy = sy;
            }
            return out;
        }
        let goal = match self.pilot.goal() {
            // re-aim when the opponent has moved away from the planned spot
            Some(g) if g.rx == 6 && (g.x - tx).abs() < 24 && (g.y - ty).abs() < 24 => g,
            _ => Goal { x: tx, y: ty, rx: 6, ry: 6 },
        };
        self.pilot.set_goal(goal);
        self.pilot.input(b, game)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings() {
        assert_eq!(heading_to((0, 0), (10, 0)), 0);
        assert_eq!(heading_to((0, 0), (0, -10)), 4);
        assert_eq!(heading_to((0, 0), (-10, 0)), 8);
        assert_eq!(heading_to((0, 0), (0, 10)), 12);
        assert_eq!(heading_to((0, 0), (10, -10)), 2);
        assert_eq!(stick_of(4), (0, -1));
        assert_eq!(stick_of(14), (1, 1));
    }
}
