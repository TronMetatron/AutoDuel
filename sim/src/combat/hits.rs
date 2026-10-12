//! To-hit rolls and damage, both ways (`r2 $75D2` to-hit, `r2 $765A` damage to the player,
//! `r3 $1C91` hits on enemies).
use super::motion::grid_bearing;
use super::weapons::{pk, Shooter};
use super::{Battle, Sfx, ORIGINAL_HIT_SIDE, WRECK};
use crate::car::{B, F, L, R, U};
use crate::game::Game;
use crate::rng::Rng;
use crate::tables::WEAPONS;

/// Frames a smoke cloud blinds an enemy (the original's 1 never reached its AI).
pub const SMOKE_BLIND_FRAMES: u8 = 2;

/// Base to-hit target of a weapon kind (`r2 $750E`; rockets 8, heavy rocket 9).
pub fn base_to_hit(kind: u8) -> i32 {
    match kind {
        pk::RL => 8,
        pk::HR => 9,
        k => WEAPONS.get(k as usize).and_then(|w| w.to_hit).unwrap_or(7) as i32,
    }
}

/// Damage die of a weapon kind: the damage is `1..=die+1`.
pub fn damage_die(kind: u8) -> u32 {
    WEAPONS.get(kind as usize).map(|w| w.dmg).unwrap_or(0)
}

/// Conditions that change the to-hit target.
#[derive(Clone, Copy, Debug, Default)]
pub struct HitMods {
    /// the enemy involved is painted, or the player is in paint this frame: +2
    pub paint: bool,
    /// smoke, the same way: +2
    pub smoke: bool,
    /// oil: +1
    pub oil: bool,
    /// the player stands still: -1
    pub player_stopped: bool,
    /// the enemy car stands still: -1
    pub other_stopped: bool,
}

/// The target number: base, made harder by paint/smoke/oil, easier for stopped cars, at
/// least 3.
pub fn to_hit_target(base: i32, m: HitMods) -> i32 {
    let t = base + 2 * m.paint as i32 + 2 * m.smoke as i32 + m.oil as i32 - m.player_stopped as i32 - m.other_stopped as i32;
    t.max(3)
}

/// Roll 1..13 against the target. The original compares the roll as BCD, so 10..13
/// ($10..$13) beat any target of 10 or more.
pub fn roll_hits(rng: &mut Rng, target: i32) -> bool {
    let roll = rng.upto(12) as i32 + 1;
    let bcd = if roll >= 10 { roll + 6 } else { roll };
    bcd >= target
}

/// Where a hit came from, to work out the side of the car it lands on.
#[derive(Clone, Copy, Debug)]
pub(super) struct HitFrom {
    /// the projectile's position when the hit is applied (end of a trace, the rocket)
    pub pos: (i32, i32),
    /// its direction of flight
    pub dir: u8,
}

impl Battle {
    /// Player's aimed shot reached enemy `i` (`hite_gun`/`hite_rocket`): roll to hit, then
    /// damage + the tens digit of marksmanship. Returns true on a hit.
    pub(super) fn player_shot(&mut self, game: &mut Game, i: usize, kind: u8) -> bool {
        let p = &self.player;
        let Some(e) = self.enemies[i].as_mut() else { return false };
        let mods = HitMods {
            paint: e.paint > 0 || p.in_paint,
            smoke: e.smoke || p.in_smoke,
            oil: e.oil || p.in_oil,
            player_stopped: p.speed0 == 0,
            other_stopped: e.speed == 0,
        };
        // the car's smoke and oil marks are used up by the roll
        e.smoke = false;
        e.oil = false;
        if !roll_hits(&mut game.rng, to_hit_target(base_to_hit(kind), mods)) {
            return false;
        }
        // every 5th aimed hit: marksmanship +1
        self.marks_counter = self.marks_counter.saturating_sub(1);
        if self.marks_counter == 0 {
            self.marks_counter = 5;
            crate::driver::Driver::raise(&mut game.driver.marksman, 1);
        }
        let dmg = game.rng.upto(damage_die(kind)) + 1 + (game.driver.marksman / 10) as u32;
        self.sfx.push(Sfx::Hit);
        self.say("He's hit!");
        self.damage_enemy(i, dmg);
        true
    }

    /// A mine or spikes under enemy `i` (`hite_mine` 1..7, `hite_spikes` 1..13). The
    /// player's drops add his marksmanship bonus.
    /// FIX: an enemy's own drop now hurts the car that drives over it; the original
    /// damaged the car that dropped it (owner lookup through the projectile table).
    pub(super) fn drop_hits_enemy(&mut self, game: &mut Game, i: usize, kind: u8, who: Shooter) {
        let die = if kind == pk::MD { 6 } else { 12 };
        let bonus = if who == Shooter::Player { (game.driver.marksman / 10) as u32 } else { 0 };
        let dmg = game.rng.upto(die) + 1 + bonus;
        self.sfx.push(Sfx::Hit);
        self.damage_enemy(i, dmg);
    }

    /// Smoke, paint or oil on enemy `i` (`r3 $1D11-$1D2F`):
    /// smoke blinds it and makes the next roll +2; paint stops it firing for 32 frames and
    /// gives +2 for 15 frames; oil spins it for 16 frames and makes the next roll +1.
    /// FIX: smoke's blindness lasted 1 frame and was cleared before the enemy's next AI
    /// pass, so it never stopped a shot; it now lasts through that pass (a car inside the
    /// cloud keeps being blinded).
    /// FIX: an enemy's own clouds act on the car that touches them (see `drop_hits_enemy`).
    pub(super) fn cloud_hits_enemy(&mut self, _game: &mut Game, i: usize, kind: u8) {
        let Some(e) = self.enemies[i].as_mut() else { return };
        match kind {
            pk::SMOKE => {
                e.blind = true;
                e.spin = false;
                e.effect_timer = SMOKE_BLIND_FRAMES;
                e.smoke = true;
            }
            pk::PAINT => {
                e.blind = true;
                e.spin = false;
                e.effect_timer = 32;
                e.paint = 15;
            }
            _ => {
                e.spin = true;
                e.blind = false;
                e.effect_timer = 16;
                e.oil = true;
                self.sfx.push(Sfx::Click);
            }
        }
    }

    /// The player drove into a cloud this frame: +2/+2/+1 on rolls, oil -20 handling.
    pub(super) fn cloud_hits_player(&mut self, _game: &mut Game, kind: u8) {
        match kind {
            pk::SMOKE => self.player.in_smoke = true,
            pk::PAINT => self.player.in_paint = true,
            _ => {
                if !self.player.in_oil {
                    self.sfx.push(Sfx::Click);
                }
                self.player.in_oil = true;
            }
        }
    }

    /// Enemy `owner`'s shot reached the player: roll to hit (the shooter's paint, smoke
    /// and oil count), then damage. Returns true on a hit.
    pub(super) fn enemy_shot(&mut self, game: &mut Game, owner: usize, kind: u8, pos: (i32, i32), dir: u8) -> bool {
        let p = &self.player;
        let Some(e) = self.enemies[owner].as_mut() else { return false };
        let mods = HitMods {
            paint: e.paint > 0 || p.in_paint,
            smoke: e.smoke || p.in_smoke,
            oil: e.oil || p.in_oil,
            player_stopped: p.speed0 == 0,
            other_stopped: e.speed == 0,
        };
        e.smoke = false;
        e.oil = false;
        if !roll_hits(&mut game.rng, to_hit_target(base_to_hit(kind), mods)) {
            return false;
        }
        self.damage_player(game, damage_die(kind), HitFrom { pos, dir });
        true
    }

    /// Side of the car a hit lands on (`r2 $7967`): F, B, L or R.
    pub(super) fn hit_side(&self, from: HitFrom) -> usize {
        let heading = self.player.new_heading as i32;
        if ORIGINAL_HIT_SIDE {
            // bearing from the player to the projectile on a 3 x 3 grid (+-2, retried
            // with +-0 when centred), mapped rotated by 180 degrees
            let me = (self.player.x, self.player.y);
            let b = grid_bearing(me, from.pos, 2).or_else(|| grid_bearing(me, from.pos, 0)).unwrap_or(16) as i32;
            match ((heading - b) & 15) >> 1 {
                0 => B,
                1..=3 => L,
                4 => F,
                _ => R,
            }
        } else {
            // the side facing where the shot came from; diagonals fall on the sides
            let incoming = (from.dir as i32 + 8) & 15;
            match (incoming - heading) & 15 {
                15 | 0 | 1 => F,
                2..=6 => L,
                7..=9 => B,
                _ => R,
            }
        }
    }

    /// Damage to the player from a weapon (`r2 $765A`): `1..=die+1`. About 12% of hits
    /// strike a tire instead (fresh 1..7). Otherwise armor on that side absorbs first, then
    /// the first weapon mounted there, then: Front -> power plant -> driver; Back -> a
    /// random cargo slot -> driver; Left/Right -> one third each to driver, cargo or plant.
    pub(super) fn damage_player(&mut self, game: &mut Game, die: u32, from: HitFrom) {
        let dmg = game.rng.upto(die) + 1;
        self.sfx.push(Sfx::PlayerHit);
        self.hit_taken = true;
        let side = self.hit_side(from);
        if self.player.on_foot || game.car().is_none() {
            // FIX: on foot every point goes to the driver (the original could still roll
            // a tire hit on the parked car)
            game.driver.hurt(dmg);
            return;
        }
        if game.rng.upto(100) < 10 {
            let t = if ORIGINAL_HIT_SIDE {
                side
            } else {
                let c = game.rng.coin() as usize;
                match side {
                    F => c,          // front-left / front-right
                    B => 2 + c,      // back-left / back-right
                    L => 2 * c,      // front-left / back-left
                    _ => 1 + 2 * c,  // front-right / back-right
                }
            };
            self.damage_tire(game, t);
            return;
        }
        let mut d = self.damage_armor(game, side, dmg);
        d = self.damage_weapon(game, side, d);
        match side {
            F => {
                d = self.damage_plant(game, d);
                game.driver.hurt(d);
            }
            B => {
                let slot = game.rng.upto(2) as usize;
                d = self.damage_cargo(game, slot, d);
                game.driver.hurt(d);
            }
            _ => self.damage_driver_cargo_or_plant(game, d),
        }
    }

    /// A mine or spikes under the player (`r2 $751B`): each tire 67% for 1..7 (spikes
    /// skip solid tires), then a mine's 1..13 to the underbody armor and on to driver,
    /// cargo or plant. A pedestrian takes 1..7.
    pub(super) fn drop_hits_player(&mut self, game: &mut Game, kind: u8) {
        self.sfx.push(Sfx::PlayerHit);
        self.hit_taken = true;
        if self.player.on_foot || game.car().is_none() {
            let d = game.rng.upto(6) + 1;
            game.driver.hurt(d);
            return;
        }
        for t in 0..4 {
            if game.rng.upto(5) < 4 {
                let solid = game.car().map(|c| c.tires[t].kind == 3).unwrap_or(false);
                if kind == pk::SD && solid {
                    continue;
                }
                self.damage_tire(game, t);
            }
        }
        if kind == pk::MD {
            let d = game.rng.upto(12) + 1;
            let d = self.damage_armor(game, U, d);
            self.damage_driver_cargo_or_plant(game, d);
        }
    }

    /// A tire takes a fresh 1..7. Going flat costs 50 handling (now and as the base).
    /// FIX: a tire already flat is not "blown" again.
    fn damage_tire(&mut self, game: &mut Game, t: usize) {
        let d = game.rng.upto(6) as u8 + 1;
        let Some(car) = game.car_mut() else { return };
        let tire = &mut car.tires[t.min(3)];
        if tire.dp == 0 {
            return;
        }
        if d >= tire.dp {
            tire.dp = 0;
            self.player.tire_blown();
        } else {
            tire.dp -= d;
        }
    }

    /// Armor on `side` absorbs what it can; returns the rest.
    fn damage_armor(&mut self, game: &mut Game, side: usize, d: u32) -> u32 {
        let Some(car) = game.car_mut() else { return d };
        let a = &mut car.armor[side];
        let taken = (a.pts as u32).min(d);
        a.pts -= taken as u8;
        d - taken
    }

    /// The first weapon mounted on `side` absorbs the rest with its DP; a destroyed weapon
    /// is removed with its ammunition. Returns the rest.
    fn damage_weapon(&mut self, game: &mut Game, side: usize, d: u32) -> u32 {
        if d == 0 || side > R {
            return d;
        }
        let Some(car) = game.car_mut() else { return d };
        let Some(slot) = car.weapons.iter().position(|w| matches!(w, Some(w) if w.mount as usize == side)) else {
            return d;
        };
        let wp = car.weapons[slot].as_mut().unwrap();
        if (wp.dp as u32) > d {
            wp.dp -= d as u8;
            0
        } else {
            let rest = d - wp.dp as u32;
            car.weapons[slot] = None;
            rest
        }
    }

    fn damage_plant(&mut self, game: &mut Game, d: u32) -> u32 {
        let Some(car) = game.car_mut() else { return d };
        let taken = (car.plant.dp as u32).min(d);
        car.plant.dp -= taken as u8;
        d - taken
    }

    /// Cargo slot `slot` absorbs with its DP; destroyed cargo is removed (quest items
    /// are lost). An empty slot absorbs nothing.
    fn damage_cargo(&mut self, game: &mut Game, slot: usize, d: u32) -> u32 {
        if d == 0 {
            return 0;
        }
        let present = game.car().map(|c| c.cargo[slot].is_some()).unwrap_or(false);
        if !present {
            return d;
        }
        let dp = self.cargo_dp[slot].max(0) as u32;
        if dp >= d {
            self.cargo_dp[slot] -= d as i32;
            0
        } else {
            self.cargo_dp[slot] = 0;
            game.remove_cargo(slot);
            // FIX: the road engine set this message without its "show" bit
            self.say("Lost some cargo!");
            d - dp
        }
    }

    /// Left/right hits and mines: one third each to driver, cargo or power plant; what
    /// is left over on this path is lost.
    fn damage_driver_cargo_or_plant(&mut self, game: &mut Game, d: u32) {
        if d == 0 {
            return;
        }
        match game.rng.upto(2) {
            0 => {
                game.driver.hurt(d);
            }
            1 => {
                let slot = game.rng.upto(2) as usize;
                self.damage_cargo(game, slot, d);
            }
            _ => {
                self.damage_plant(game, d);
            }
        }
    }

    /// Damage to enemy `i`'s single pool of hit points. At 0 it is a wreck.
    pub(super) fn damage_enemy(&mut self, i: usize, dmg: u32) {
        let arena = self.is_arena();
        let Some(e) = self.enemies[i].as_mut() else { return };
        if e.is_wreck() {
            return;
        }
        e.hp -= dmg as i32;
        if e.hp > 0 {
            return;
        }
        e.hp = 0;
        e.state = WRECK;
        e.spin = false;
        self.sfx.push(Sfx::Explode);
        if arena {
            self.opponents_left = self.opponents_left.saturating_sub(1);
            if self.opponents_left == 0 {
                self.say("He was the last opponent!");
                self.sfx.push(Sfx::Fanfare);
            } else {
                self.say("He's out of commission!");
            }
        } else {
            self.say("He won't give you any more trouble!");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_and_modifiers() {
        let none = HitMods::default();
        assert_eq!(to_hit_target(7, none), 7);
        assert_eq!(to_hit_target(7, HitMods { paint: true, smoke: true, oil: true, ..none }), 12);
        assert_eq!(to_hit_target(6, HitMods { player_stopped: true, other_stopped: true, ..none }), 4);
        assert_eq!(to_hit_target(3, HitMods { player_stopped: true, other_stopped: true, ..none }), 3);
        assert_eq!(base_to_hit(pk::MG), 7);
        assert_eq!(base_to_hit(pk::LA), 6);
        assert_eq!(base_to_hit(pk::RL), 8);
        assert_eq!(base_to_hit(pk::HR), 9);
    }

    /// Monte Carlo: P(hit) = (14 - T) / 13 for T <= 9 and 4/13 for every T >= 10 (BCD).
    #[test]
    fn to_hit_probabilities() {
        let mut rng = Rng::new(12345);
        let n = 200_000;
        for t in 3..=14 {
            let hits = (0..n).filter(|_| roll_hits(&mut rng, t)).count();
            let p = hits as f64 / n as f64;
            let want = if t >= 10 { 4.0 / 13.0 } else { (14 - t) as f64 / 13.0 };
            assert!((p - want).abs() < 0.006, "target {}: {} vs {}", t, p, want);
        }
    }

    #[test]
    fn damage_ranges() {
        let mut rng = Rng::new(99);
        for (kind, lo, hi) in [(pk::MG, 1, 3), (pk::FT, 1, 3), (pk::RL, 1, 19), (pk::RR, 1, 13), (pk::AT, 1, 19), (pk::LA, 1, 19), (pk::HR, 1, 31)] {
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..20_000 {
                seen.insert(rng.upto(damage_die(kind)) + 1);
            }
            assert_eq!(*seen.iter().next().unwrap(), lo, "kind {}", kind);
            assert_eq!(*seen.iter().last().unwrap(), hi, "kind {}", kind);
            assert_eq!(seen.len() as u32, hi - lo + 1);
        }
    }
}
