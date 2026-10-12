//! Salvaging wrecks on foot (`r1 $C846` check_salvage, `r1 $C8A7` salvage_wreck).
//!
//! Walk within 5 units of a stopped wreck: it is searched once. A burnt wreck, or one
//! armed with a laser or heavy rocket, has nothing. Otherwise each of your weapons of the
//! wreck's type with room in its magazine has a 50% chance of 4..13 rounds (what fits the
//! weight and keeps the car able to move); failing that you may find "Salvaged Parts":
//! success if random 0..100 < mechanic + 50, 1..6 x 100 lb and 1..3 spaces that must fit,
//! worth 5 x (mechanic tens) + 1..6 hundred dollars (arena: at most 25), and a 50% chance
//! of mechanic +1.
use super::{Battle, Sfx, ENEMY_SLOTS, SALVAGED, WRECK};
use crate::cargo::{Cargo, CargoKind};
use crate::driver::Driver;
use crate::game::Game;
use crate::tables::{w, MAGAZINE_MAX, WEAPONS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    Nothing,
    Ammo,
    Parts,
}

impl Battle {
    pub(super) fn check_salvage(&mut self, game: &mut Game) {
        if !self.player.on_foot || self.event() == Some(0) {
            return;
        }
        let (px, py) = (self.player.x, self.player.y);
        let found = (0..ENEMY_SLOTS).rev().find(|&i| {
            matches!(&self.enemies[i], Some(e) if e.state == WRECK && e.speed == 0 && (e.x - px).abs() < 6 && (e.y - py).abs() < 6)
        });
        if let Some(i) = found {
            let what = self.salvage_wreck(game, i);
            self.sfx.push(Sfx::Beep);
            self.say(match what {
                Found::Nothing => "You found nothing.",
                Found::Ammo => "You found some ammo.",
                Found::Parts => "You found some parts.",
            });
        }
    }

    fn salvage_wreck(&mut self, game: &mut Game, i: usize) -> Found {
        let Some(e) = self.enemies[i].as_mut() else { return Found::Nothing };
        e.state = SALVAGED;
        let (burnt, weapon) = (e.burning, e.weapon);
        if burnt || weapon == w::LA || weapon == w::HR || game.car().is_none() {
            return Found::Nothing;
        }
        // ammunition for a weapon of the same type
        for slot in 0..10 {
            let fits = matches!(game.car().and_then(|c| c.weapons[slot]), Some(wp) if wp.kind == weapon && wp.ammo < MAGAZINE_MAX);
            if !fits || !game.rng.coin() {
                continue;
            }
            let car = game.car_mut().unwrap();
            let have = car.weapons[slot].unwrap().ammo as u32;
            let mut n = (game.rng.upto(9) + 4).min(MAGAZINE_MAX as u32 - have);
            let per = WEAPONS[weapon as usize].ammo_weight;
            let car = game.car_mut().unwrap();
            while n > 0 && !car.can_carry(n * per) {
                n -= 1;
            }
            if n == 0 {
                return Found::Nothing;
            }
            car.weapons[slot].as_mut().unwrap().ammo += n as u8;
            return Found::Ammo;
        }
        // salvaged parts: an existing parts slot, else a free one
        let car = game.car().unwrap();
        let slot = car
            .cargo
            .iter()
            .position(|c| matches!(c, Some(c) if c.kind == CargoKind::Salvage))
            .or_else(|| car.free_cargo_slot());
        let Some(slot) = slot else { return Found::Nothing };
        let mech = game.driver.mechanic as u32;
        if game.rng.upto(100) >= mech + 50 {
            return Found::Nothing;
        }
        let weight = (game.rng.upto(5) + 1) * 100;
        if !game.car().unwrap().can_carry(weight) {
            return Found::Nothing;
        }
        let spaces = game.rng.upto(2) + 1;
        if game.car().unwrap().spaces_left() < spaces as i64 {
            return Found::Nothing;
        }
        let mut v = 5 * (mech / 10) + game.rng.upto(5) + 1;
        if self.is_arena() {
            v = v.min(25);
        }
        let car = game.car_mut().unwrap();
        let c = car.cargo[slot].get_or_insert_with(|| Cargo {
            kind: CargoKind::Salvage,
            name: "Salvaged Parts".to_string(),
            dest: None,
            weight: 0,
            spaces: 0,
            due: None,
            value: 0,
            pay: 0,
        });
        c.weight += weight;
        c.spaces += spaces;
        c.value += v * 100;
        c.pay += v * 100;
        self.cargo_dp[slot] += 2;
        if game.rng.coin() {
            Driver::raise(&mut game.driver.mechanic, 1);
        }
        Found::Parts
    }
}
