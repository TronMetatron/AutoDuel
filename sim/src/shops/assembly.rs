//! The Assembly Line (`as.prg`): design and buy a new car.
//!
//! The design screen has 40 cursor fields in the original order (`field_offsets as $A46F`):
//!
//! | field | what | range |
//! |---|---|---|
//! | 0 | body | 0-6 |
//! | 1 | chassis | 0-3 |
//! | 2 | suspension | 0-2 |
//! | 3-7 | armor Front, Back, Left, Right, Under | 0-60 |
//! | 8 | power plant | 0-3 (DP reset to the maximum) |
//! | 9 | tires | 0-3 (all four) |
//! | 10-29 | 10 x (weapon type 0-11 or none, then its mount) | |
//! | 30-39 | ammunition for weapon slots 1-10 | 0-20 |
//!
//! Left/right change a field by one and wrap around (`field_adjust as $A2B3`). The price,
//! weight, spaces and the design check are [`Car`] methods (`compute_car_stats as $A515`).
use crate::car::{Armor, Car, DesignErrors, Weapon};
use crate::game::Game;
use crate::tables::{self, ARMOR_MAX, MAGAZINE_MAX, WEAPONS};

/// Number of cursor fields on the design screen.
pub const FIELDS: usize = 40;
/// Longest car name.
pub const NAME_LEN: usize = 16;

/// One cursor field of the design screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Body,
    Chassis,
    Suspension,
    /// facing 0..5 (F B L R U)
    Armor(usize),
    Plant,
    Tires,
    /// weapon slot 0..10: the weapon type (or none)
    WeaponType(usize),
    /// weapon slot 0..10: where it is mounted
    WeaponMount(usize),
    /// weapon slot 0..10: rounds loaded
    Ammo(usize),
}

impl Field {
    /// The field under cursor position `i` (0..40), in the original order.
    pub fn at(i: usize) -> Field {
        match i % FIELDS {
            0 => Field::Body,
            1 => Field::Chassis,
            2 => Field::Suspension,
            i @ 3..=7 => Field::Armor(i - 3),
            8 => Field::Plant,
            9 => Field::Tires,
            i @ 10..=29 => {
                let slot = (i - 10) / 2;
                if (i - 10) % 2 == 0 { Field::WeaponType(slot) } else { Field::WeaponMount(slot) }
            }
            i => Field::Ammo(i - 30),
        }
    }
}

/// Wrap `v + delta` into `0..n`.
fn wrap(v: i32, delta: i32, n: i32) -> u8 {
    (v + delta).rem_euclid(n) as u8
}

/// Change a design field by `delta`.
///
/// `delta` = ±1 is the original arrow-key behaviour: every field wraps around. Larger steps
/// (the remake's ±10 keys) clamp at the ends of armor and ammunition instead of wrapping.
pub fn adjust(car: &mut Car, field: Field, delta: i32) {
    if delta == 0 {
        return;
    }
    let big = delta.abs() > 1;
    match field {
        Field::Body => car.body = wrap(car.body as i32, delta.signum(), tables::BODIES.len() as i32),
        Field::Chassis => car.chassis = wrap(car.chassis as i32, delta.signum(), 4),
        Field::Suspension => car.suspension = wrap(car.suspension as i32, delta.signum(), 3),
        Field::Armor(f) => {
            let n = ARMOR_MAX as i32 + 1;
            let cur = car.armor[f].max as i32;
            let v = if big { (cur + delta).clamp(0, ARMOR_MAX as i32) as u8 } else { wrap(cur, delta, n) };
            car.armor[f] = Armor { pts: v, max: v };
        }
        Field::Plant => {
            let k = wrap(car.plant.kind as i32, delta.signum(), tables::PLANTS.len() as i32);
            car.set_plant(k);
        }
        Field::Tires => {
            let k = wrap(car.tires[0].kind as i32, delta.signum(), tables::TIRES.len() as i32);
            car.set_tires(k);
        }
        Field::WeaponType(s) => {
            // 0..11 = weapon types, 12 = none; changing the type resets mount, DP and ammo
            let none = WEAPONS.len() as i32;
            let cur = car.weapons[s].map(|w| w.kind as i32).unwrap_or(none);
            let k = wrap(cur, delta.signum(), none + 1);
            car.weapons[s] = if k as i32 == none { None } else { Some(Weapon::new(k, tables::default_mount(k))) };
        }
        Field::WeaponMount(s) => {
            // only the allowed mounts, in F B L R order (MD/SD/OJ back only, AT front/back,
            // FT/SS/PS back/left/right) — the same rules as `as $A3A6` and `we $AFC7`
            if let Some(w) = car.weapons[s].as_mut() {
                let allowed = allowed_mounts(w.kind);
                let i = allowed.iter().position(|m| *m == w.mount).unwrap_or(0) as i32;
                w.mount = allowed[wrap(i, delta.signum(), allowed.len() as i32) as usize];
            }
        }
        Field::Ammo(s) => {
            if let Some(w) = car.weapons[s].as_mut()
                && w.uses_ammo() {
                let n = MAGAZINE_MAX as i32 + 1;
                let cur = w.ammo as i32;
                w.ammo = if big { (cur + delta).clamp(0, MAGAZINE_MAX as i32) as u8 } else { wrap(cur, delta, n) };
            }
        }
    }
}

/// Mounts a weapon type may use (0 Front, 1 Back, 2 Left, 3 Right).
pub fn allowed_mounts(kind: u8) -> Vec<u8> {
    WEAPONS[kind as usize].mounts.iter().enumerate().filter(|(_, ok)| **ok).map(|(i, _)| i as u8).collect()
}

/// Money left after paying for the design and its ammunition (negative = can't afford).
pub fn money_left(car: &Car, money: u32) -> i64 {
    money as i64 - car.value() as i64 - car.ammo_value() as i64
}

/// One line per failed design rule, saying by how much it fails.
/// FIX: the original only printed "that design just won't work!" and showed garbage in the
/// failing fields; this says which rule fails.
pub fn explain(car: &Car, money: u32) -> Vec<String> {
    let e = car.design_errors(money);
    let mut v = vec![];
    if e.money {
        v.push(format!("It costs ${} more than you have.", -money_left(car, money)));
    }
    if e.weight {
        v.push(format!("It is {} lb over its max weight.", -car.weight_left()));
    }
    if e.spaces {
        let n = -car.spaces_left();
        v.push(format!("It needs {} more space{}.", n, if n == 1 { "" } else { "s" }));
    }
    if e.power {
        v.push(format!("The plant can't move {} lb.", car.weight()));
    }
    v
}

/// What happens when you walk in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// all 8 car records are in use: "you don't have enough room for any more cars"
    NoRoom,
    /// ready to design; `stored` = the name of the car that was put in storage here
    Design { stored: Option<String> },
}

/// Entering the Assembly Line: a car you have with you goes into storage at the garage here.
/// FIX: the original stored the car before finding out there was no room for a new one; here
/// nothing changes when there is no room.
pub fn enter(game: &mut Game) -> Entry {
    if game.free_car_slot().is_none() {
        return Entry::NoRoom;
    }
    let mut stored = None;
    if !game.driver.loaner
        && let Some(c) = game.car_here() {
        stored = Some(c.name.clone());
        game.store_current_car();
    }
    Entry::Design { stored }
}

/// A fresh design: the Assembly Line's template car (`as $AAEE`), built in this city.
pub fn new_design(game: &Game, name: &str) -> Car {
    let name: String = name.chars().take(NAME_LEN).collect();
    Car::default_design(&name, game.driver.city)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuyError {
    /// "I'm sorry, but that design just won't work!"
    Design(DesignErrors),
    /// no free car record (can't normally happen: checked on entry)
    NoRoom,
}

/// "D": pay for the car and its ammunition and drive it out ("A very good choice...").
/// The new car is in this city, with you, fully charged; the day is used up.
pub fn buy(game: &mut Game, mut car: Car) -> Result<usize, BuyError> {
    let e = car.design_errors(game.driver.money);
    if e.any() {
        return Err(BuyError::Design(e));
    }
    if game.free_car_slot().is_none() {
        return Err(BuyError::NoRoom);
    }
    let price = car.value() + car.ammo_value();
    car.city = game.driver.city;
    game.driver.pay(price);
    game.driver.day_used = true;
    let slot = game.add_car(car).expect("free slot checked");
    Ok(slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Option<Game> {
        let d = crate::data::test_data()?;
        Some(Game::new("Test", false, &d, 7))
    }

    #[test]
    fn field_order_matches_the_original() {
        assert_eq!(Field::at(0), Field::Body);
        assert_eq!(Field::at(3), Field::Armor(0));
        assert_eq!(Field::at(7), Field::Armor(4));
        assert_eq!(Field::at(8), Field::Plant);
        assert_eq!(Field::at(9), Field::Tires);
        assert_eq!(Field::at(10), Field::WeaponType(0));
        assert_eq!(Field::at(11), Field::WeaponMount(0));
        assert_eq!(Field::at(29), Field::WeaponMount(9));
        assert_eq!(Field::at(30), Field::Ammo(0));
        assert_eq!(Field::at(39), Field::Ammo(9));
    }

    #[test]
    fn fields_wrap() {
        let mut c = Car::default_design("X", 7);
        adjust(&mut c, Field::Body, -1);
        assert_eq!(c.body, 6);
        adjust(&mut c, Field::Body, 1);
        assert_eq!(c.body, 0);
        adjust(&mut c, Field::Armor(0), -1);
        assert_eq!(c.armor[0], Armor { pts: 60, max: 60 });
        adjust(&mut c, Field::Armor(0), 1);
        assert_eq!(c.armor[0].max, 0);
        adjust(&mut c, Field::Armor(1), 10);
        adjust(&mut c, Field::Armor(1), 100);
        assert_eq!(c.armor[1].max, 60, "big steps clamp");
        adjust(&mut c, Field::Plant, -1);
        assert_eq!((c.plant.kind, c.plant.dp, c.plant.max_dp), (3, 12, 12));
        adjust(&mut c, Field::Tires, 1);
        assert!(c.tires.iter().all(|t| t.kind == 1 && t.dp == 6));
    }

    #[test]
    fn weapon_type_and_mount_cycles() {
        let mut c = Car::default_design("X", 7);
        // none -> machine gun (front) and back to none
        adjust(&mut c, Field::WeaponType(0), 1);
        assert_eq!(c.weapons[0].map(|w| (w.kind, w.mount, w.ammo)), Some((0, 0, 0)));
        adjust(&mut c, Field::WeaponType(0), -1);
        assert!(c.weapons[0].is_none());
        // none -> heavy rocket going backwards
        adjust(&mut c, Field::WeaponType(0), -1);
        assert_eq!(c.weapons[0].unwrap().kind, tables::w::HR);
        // flamethrower: defaults to back, cycles back/left/right
        c.weapons[1] = Some(Weapon::new(tables::w::FT, tables::default_mount(tables::w::FT)));
        assert_eq!(c.weapons[1].unwrap().mount, 1);
        adjust(&mut c, Field::WeaponMount(1), 1);
        assert_eq!(c.weapons[1].unwrap().mount, 2);
        adjust(&mut c, Field::WeaponMount(1), 1);
        assert_eq!(c.weapons[1].unwrap().mount, 3);
        adjust(&mut c, Field::WeaponMount(1), 1);
        assert_eq!(c.weapons[1].unwrap().mount, 1);
        // anti-tank gun toggles front/back; minedropper stays at the back
        c.weapons[2] = Some(Weapon::new(tables::w::AT, 0));
        adjust(&mut c, Field::WeaponMount(2), -1);
        assert_eq!(c.weapons[2].unwrap().mount, 1);
        c.weapons[3] = Some(Weapon::new(tables::w::MD, 1));
        adjust(&mut c, Field::WeaponMount(3), 1);
        assert_eq!(c.weapons[3].unwrap().mount, 1);
        // ammo wraps 0..20; laser has none
        adjust(&mut c, Field::Ammo(2), -1);
        assert_eq!(c.weapons[2].unwrap().ammo, 20);
        c.weapons[4] = Some(Weapon::new(tables::w::LA, 0));
        adjust(&mut c, Field::Ammo(4), 1);
        assert_eq!(c.weapons[4].unwrap().ammo, 0);
        // mount and ammo of an empty slot do nothing
        adjust(&mut c, Field::Ammo(9), 1);
        adjust(&mut c, Field::WeaponMount(9), 1);
        assert!(c.weapons[9].is_none());
    }

    #[test]
    fn design_check_and_purchase() {
        let Some(mut g) = game() else { return };
        assert_eq!(enter(&mut g), Entry::Design { stored: None });
        let mut c = new_design(&g, "My very long car name");
        assert_eq!(c.name.len(), NAME_LEN);
        // the template costs $940; add a machine gun with 10 rounds ($1,000 + $250)
        adjust(&mut c, Field::WeaponType(0), 1);
        for _ in 0..10 {
            adjust(&mut c, Field::Ammo(0), 1);
        }
        assert_eq!(money_left(&c, 2000), 2000 - 940 - 1000 - 250);
        let e = c.design_errors(2000);
        assert!(e.money && !e.spaces && !e.weight && !e.power);
        assert_eq!(explain(&c, 2000), vec!["It costs $190 more than you have.".to_string()]);
        assert!(matches!(buy(&mut g, c.clone()), Err(BuyError::Design(_))));
        g.driver.money = 5000;
        let slot = buy(&mut g, c).unwrap();
        assert_eq!(g.driver.money, 5000 - 2190);
        assert!(g.driver.day_used);
        assert_eq!(g.driver.car, Some(slot as u8));
        assert!(g.driver.car_with_you);
        assert_eq!(g.car().unwrap().city, g.driver.city);
    }

    #[test]
    fn spaces_weight_and_power_errors() {
        // Compact, Ex. Heavy chassis, small plant, 200 armor points: W = 3120 -> can't move
        let mut c = Car::default_design("X", 7);
        c.body = 1;
        c.chassis = 3;
        for f in 0..5 {
            c.armor[f] = Armor { pts: 40, max: 40 };
        }
        assert_eq!(c.weight(), 3120);
        let e = c.design_errors(999_999);
        assert!(e.power && !e.weight && !e.spaces && !e.money);
        // three smokescreens in a subcompact: 6 spaces + 3 for the plant > 7
        let mut s = Car::default_design("Y", 7);
        for i in 0..3 {
            s.weapons[i] = Some(Weapon::new(tables::w::SS, 1));
        }
        let e = s.design_errors(999_999);
        assert!(e.spaces && !e.weight);
        assert_eq!(explain(&s, 999_999), vec!["It needs 2 more spaces.".to_string()]);
        // 60 points all round on a light subcompact: far over the max load
        let mut h = Car::default_design("Z", 7);
        for f in 0..5 {
            h.armor[f] = Armor { pts: 60, max: 60 };
        }
        assert!(h.design_errors(999_999).weight);
    }

    #[test]
    fn entering_stores_your_car_unless_full() {
        let Some(mut g) = game() else { return };
        g.add_car(Car::default_design("Old", 3));
        g.driver.city = 5;
        assert_eq!(enter(&mut g), Entry::Design { stored: Some("Old".into()) });
        assert!(!g.driver.car_with_you);
        assert_eq!(g.cars[0].as_ref().unwrap().city, 5);
        for i in 1..8 {
            g.cars[i] = Some(Car::default_design("Filler", 0));
        }
        g.driver.car_with_you = true;
        assert_eq!(enter(&mut g), Entry::NoRoom);
        assert!(g.driver.car_with_you, "nothing changes when there is no room");
    }
}
