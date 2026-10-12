//! The Garage (`ga.prg`): battery charge, repairs, storage, mechanic lessons.
//!
//! | key | service | price |
//! |---|---|---|
//! | 1 | charge the battery to 99 | $50 |
//! | 2 | repair (power plant and weapons per lost DP, tires flat per tire, armor per point) | see below |
//! | 3 | store your car here | free |
//! | 4 | take out a car stored here | $50 |
//! | 5 | mechanic lessons (5 days) | $500 |
//! | 6 | exit (not in a car with a flat battery) | |
//!
//! Repairs take no time and don't use the mechanic skill. Repairs and lessons use up the day.
use crate::car::Car;
use crate::game::Game;
use crate::tables::{self, ARMOR_REPAIR_PER_PT, LOCATION_NAMES, TIRES};

pub const BATTERY_PRICE: u32 = 50;
pub const BATTERY_FULL: u8 = 99;
pub const TAKE_OUT_PRICE: u32 = 50;
pub const LESSON_PRICE: u32 = 500;
pub const LESSON_DAYS: u32 = 5;
/// The original lists at most 20 damaged components, 10 per page.
pub const LIST_PAGE: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GarageError {
    /// "I'm sorry, but you don't have a car with you."
    NoCar,
    /// "I'm afraid you don't have enough money."
    Money,
    /// "The batteries don't need charging."
    BatteryFull,
    /// "I don't understand. There's nothing wrong with your car."
    NothingToFix,
    /// armor: more points than are missing
    TooMany,
    /// "I'm sorry, but none of your cars are here."
    NotHere,
}

// ---------------------------------------------------------------- battery

/// 1 - "Have power supply charged $50" (`ga $A5A8`).
pub fn charge_battery(game: &mut Game) -> Result<(), GarageError> {
    let battery = game.car_here().ok_or(GarageError::NoCar)?.battery;
    if battery >= BATTERY_FULL {
        return Err(GarageError::BatteryFull);
    }
    if !game.driver.pay(BATTERY_PRICE) {
        return Err(GarageError::Money);
    }
    game.car_here_mut().unwrap().battery = BATTERY_FULL;
    Ok(())
}

// ---------------------------------------------------------------- repairs

/// A component that can be repaired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Plant,
    /// 0 F-L, 1 F-R, 2 B-L, 3 B-R
    Tire(usize),
    /// facing 0..5 (F B L R U)
    Armor(usize),
    /// weapon slot 0..10
    Weapon(usize),
}

/// One line of the repair list: "Item, Loc, Damaged Pts, Max Pts, Max Cost to Repair".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepairLine {
    pub part: Part,
    /// 10-character item name as in the original list
    pub name: String,
    /// 3-character location
    pub loc: &'static str,
    pub cur: u8,
    pub max: u8,
    /// cost to repair it completely
    pub cost: u32,
}

/// 10-character weapon names of the repair and salvage lists (`ga $A4C6`).
pub const WEAPON_NAMES10: [&str; 12] = [
    "Mach. gun", "Flamethrwr", "Rkt lnchr", "Rcls rifle", "AT gun", "Laser", "Minedrppr", "Spikedrppr",
    "Smokescrn", "Paint spry", "Oil jet", "Hvy rocket",
];

/// Cost to repair one component completely: plant and weapons per lost DP, a damaged tire is
/// replaced at a flat price, armor $50 per point.
pub fn part_cost(car: &Car, part: Part) -> u32 {
    match part {
        Part::Plant => (car.plant.max_dp - car.plant.dp.min(car.plant.max_dp)) as u32 * car.plant_info().repair_per_dp,
        Part::Tire(i) => {
            let t = car.tires[i];
            if t.dp < t.max_dp { TIRES[t.kind as usize].replace_cost } else { 0 }
        }
        Part::Armor(f) => {
            let a = car.armor[f];
            (a.max - a.pts.min(a.max)) as u32 * ARMOR_REPAIR_PER_PT
        }
        Part::Weapon(s) => match &car.weapons[s] {
            Some(w) => (w.max_dp - w.dp.min(w.max_dp)) as u32 * w.info().repair_per_dp,
            None => 0,
        },
    }
}

/// The damaged components, in the original order (plant, tires, armor, weapons).
pub fn repair_list(car: &Car) -> Vec<RepairLine> {
    let mut v = vec![];
    let mut push = |part: Part, name: &str, loc: &'static str, cur: u8, max: u8| {
        if cur != max {
            v.push(RepairLine { part, name: name.to_string(), loc, cur, max, cost: part_cost(car, part) });
        }
    };
    push(Part::Plant, "Power plnt", "Ctr", car.plant.dp, car.plant.max_dp);
    for (i, t) in car.tires.iter().enumerate() {
        push(Part::Tire(i), "Tire", LOCATION_NAMES[6 + i], t.dp, t.max_dp);
    }
    for (f, a) in car.armor.iter().enumerate() {
        if a.max > 0 {
            push(Part::Armor(f), "Armor", LOCATION_NAMES[f], a.pts, a.max);
        }
    }
    for (s, w) in car.weapons.iter().enumerate() {
        if let Some(w) = w {
            push(Part::Weapon(s), WEAPON_NAMES10[w.kind as usize], LOCATION_NAMES[w.mount as usize], w.dp, w.max_dp);
        }
    }
    v
}

fn restore(car: &mut Car, part: Part) {
    match part {
        Part::Plant => car.plant.dp = car.plant.max_dp,
        Part::Tire(i) => car.tires[i].dp = car.tires[i].max_dp,
        Part::Armor(f) => car.armor[f].pts = car.armor[f].max,
        Part::Weapon(s) => {
            if let Some(w) = car.weapons[s].as_mut() {
                w.dp = w.max_dp;
            }
        }
    }
}

/// Repair one component completely (for armor see [`repair_armor`]). Returns the price.
pub fn repair(game: &mut Game, part: Part) -> Result<u32, GarageError> {
    let car = game.car_here().ok_or(GarageError::NoCar)?;
    let cost = part_cost(car, part);
    if !repair_list(car).iter().any(|l| l.part == part) {
        return Err(GarageError::NothingToFix);
    }
    if !game.driver.pay(cost) {
        return Err(GarageError::Money);
    }
    restore(game.car_here_mut().unwrap(), part);
    game.driver.day_used = true;
    Ok(cost)
}

/// Armor points missing on a facing ("How many points would you like restored (N max)?").
pub fn armor_missing(car: &Car, facing: usize) -> u8 {
    let a = car.armor[facing];
    a.max - a.pts.min(a.max)
}

/// Restore `pts` armor points on a facing at $50 each. Returns the price.
pub fn repair_armor(game: &mut Game, facing: usize, pts: u8) -> Result<u32, GarageError> {
    let car = game.car_here().ok_or(GarageError::NoCar)?;
    let missing = armor_missing(car, facing);
    if pts == 0 || missing == 0 {
        return Err(GarageError::NothingToFix);
    }
    if pts > missing {
        return Err(GarageError::TooMany);
    }
    let cost = pts as u32 * ARMOR_REPAIR_PER_PT;
    if !game.driver.pay(cost) {
        return Err(GarageError::Money);
    }
    game.car_here_mut().unwrap().armor[facing].pts += pts;
    game.driver.day_used = true;
    Ok(cost)
}

/// FIX (remake convenience): repair everything in one go, at the sum of the single prices.
pub fn repair_all(game: &mut Game) -> Result<u32, GarageError> {
    let car = game.car_here().ok_or(GarageError::NoCar)?;
    if !car.has_damage() {
        return Err(GarageError::NothingToFix);
    }
    let cost = car.full_repair_cost();
    if !game.driver.pay(cost) {
        return Err(GarageError::Money);
    }
    let car = game.car_here_mut().unwrap();
    for l in repair_list(car) {
        restore(car, l.part);
    }
    game.driver.day_used = true;
    Ok(cost)
}

// ---------------------------------------------------------------- storage

/// 3 - "Store your car" (free): it stays in this city. Returns its name.
pub fn store_car(game: &mut Game) -> Result<String, GarageError> {
    if game.driver.loaner {
        return Err(GarageError::NoCar);
    }
    let name = game.car_here().ok_or(GarageError::NoCar)?.name.clone();
    game.store_current_car();
    Ok(name)
}

/// Cars stored in this city (car slots), not counting the one you have with you.
pub fn stored_here(game: &Game) -> Vec<usize> {
    let city = game.driver.city;
    let with_you = if game.driver.car_with_you && !game.driver.loaner { game.driver.car } else { None };
    game.owned_cars().filter(|(i, c)| c.city == city && with_you != Some(*i as u8)).map(|(i, _)| i).collect()
}

/// 4 - "Take a car out of storage $50". The car you have with you is stored here first.
pub fn take_out(game: &mut Game, slot: usize) -> Result<(), GarageError> {
    if !stored_here(game).contains(&slot) {
        return Err(GarageError::NotHere);
    }
    if !game.driver.can_afford(TAKE_OUT_PRICE) {
        return Err(GarageError::Money);
    }
    if game.driver.car_with_you && !game.driver.loaner {
        game.store_current_car();
    }
    game.driver.pay(TAKE_OUT_PRICE);
    game.driver.car = Some(slot as u8);
    game.driver.car_with_you = true;
    let city = game.driver.city;
    if let Some(c) = game.cars[slot].as_mut() {
        c.city = city;
    }
    Ok(())
}

/// 6 - Exit: "You can't leave in that car! Its batteries need charging."
pub fn can_leave(game: &Game) -> bool {
    !matches!(game.car_here(), Some(c) if c.battery == 0)
}

// ---------------------------------------------------------------- lessons

/// Mechanic gain from one course of lessons (`ga $A650`): q = mechanic / 24,
/// gain = 4 - q + r where r is random 0..=(5 - q) (the caller rolls it).
pub fn lesson_gain(mechanic: u8, r: u32) -> u8 {
    let q = (mechanic / 24) as u32;
    (4 + r).saturating_sub(q) as u8
}

/// Range of the random part for a mechanic skill: 0..=(5 - q).
pub fn lesson_roll_max(mechanic: u8) -> u32 {
    5u32.saturating_sub((mechanic / 24) as u32)
}

/// 5 - "Take mechanic lessons $500": five days pass. Returns the new mechanic skill.
pub fn take_lessons(game: &mut Game) -> Result<u8, GarageError> {
    if !game.driver.pay(LESSON_PRICE) {
        return Err(GarageError::Money);
    }
    let m = game.driver.mechanic;
    let r = game.rng.upto(lesson_roll_max(m));
    crate::driver::Driver::raise(&mut game.driver.mechanic, lesson_gain(m, r));
    game.pass_days(LESSON_DAYS);
    game.driver.day_used = true;
    Ok(game.driver.mechanic)
}

/// Location name for a component location index (for lists).
pub fn location_name(i: u8) -> &'static str {
    tables::LOCATION_NAMES.get(i as usize).copied().unwrap_or("???")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::Weapon;
    use crate::tables::w;

    fn game_with_kart() -> Option<Game> {
        let d = crate::data::test_data()?;
        let mut g = Game::new("Test", false, &d, 3);
        g.add_car(Car::killer_kart(7));
        Some(g)
    }

    #[test]
    fn lesson_gains_match_the_table() {
        // (mechanic, min gain, max gain) from docs/vehicles.md §5.2
        for (m, lo, hi) in [(0u8, 4u8, 9u8), (23, 4, 9), (24, 3, 7), (47, 3, 7), (48, 2, 5), (72, 1, 3), (96, 0, 1), (99, 0, 1)] {
            assert_eq!(lesson_gain(m, 0), lo, "min at {}", m);
            assert_eq!(lesson_gain(m, lesson_roll_max(m)), hi, "max at {}", m);
        }
    }

    #[test]
    fn lessons_cost_time_and_money_and_cap_at_99() {
        let Some(mut g) = game_with_kart() else { return };
        g.driver.money = 1000;
        let day = g.driver.date;
        let m = take_lessons(&mut g).unwrap();
        assert!((4..=9).contains(&m));
        assert_eq!(g.driver.money, 500);
        assert_eq!(g.driver.date, day.plus_days(5));
        assert!(g.driver.day_used);
        g.driver.mechanic = 98;
        g.driver.money = 500;
        assert!((98..=99).contains(&take_lessons(&mut g).unwrap()));
        assert_eq!(take_lessons(&mut g), Err(GarageError::Money));
    }

    #[test]
    fn repair_costs() {
        let Some(mut g) = game_with_kart() else { return };
        g.driver.money = 10_000;
        {
            let c = g.car_here_mut().unwrap();
            c.plant.dp = 5; // medium plant, 3 DP lost at $100
            c.tires[2].dp = 5; // heavy-duty: $100 flat
            c.tires[3].dp = 0;
            c.armor[0].pts = 2; // 6 points at $50
            c.weapons[0].as_mut().unwrap().dp = 1; // machine gun: 2 DP at $100
        }
        let list = repair_list(g.car().unwrap());
        let costs: Vec<(Part, u32)> = list.iter().map(|l| (l.part, l.cost)).collect();
        assert_eq!(
            costs,
            vec![(Part::Plant, 300), (Part::Tire(2), 100), (Part::Tire(3), 100), (Part::Armor(0), 300), (Part::Weapon(0), 200)]
        );
        assert_eq!(list[4].name, "Mach. gun");
        assert_eq!(list[4].loc, "Frt");
        assert_eq!(g.car().unwrap().full_repair_cost(), 1000);
        assert_eq!(repair(&mut g, Part::Plant), Ok(300));
        assert_eq!(g.car().unwrap().plant.dp, 8);
        assert_eq!(repair_armor(&mut g, 0, 7), Err(GarageError::TooMany));
        assert_eq!(repair_armor(&mut g, 0, 4), Ok(200));
        assert_eq!(g.car().unwrap().armor[0].pts, 6);
        assert!(g.driver.day_used);
        assert_eq!(repair_all(&mut g), Ok(100 + 100 + 100 + 200));
        assert!(!g.car().unwrap().has_damage());
        assert_eq!(g.driver.money, 10_000 - 300 - 200 - 500);
        assert_eq!(repair_all(&mut g), Err(GarageError::NothingToFix));
    }

    #[test]
    fn repairs_need_money() {
        let Some(mut g) = game_with_kart() else { return };
        g.driver.money = 50;
        g.car_here_mut().unwrap().weapons[1] = Some(Weapon { dp: 0, ..Weapon::new(w::LA, 0) });
        assert_eq!(repair(&mut g, Part::Weapon(1)), Err(GarageError::Money));
        assert_eq!(g.car().unwrap().weapons[1].unwrap().dp, 0);
        assert!(!g.driver.day_used);
    }

    #[test]
    fn battery_storage_and_exit() {
        let Some(mut g) = game_with_kart() else { return };
        assert_eq!(charge_battery(&mut g), Err(GarageError::BatteryFull));
        g.car_here_mut().unwrap().battery = 0;
        assert!(!can_leave(&g));
        g.driver.money = 60;
        assert_eq!(charge_battery(&mut g), Ok(()));
        assert_eq!((g.driver.money, g.car().unwrap().battery), (10, 99));
        assert!(can_leave(&g));
        // store the kart, add a second car here and one elsewhere
        assert_eq!(store_car(&mut g), Ok("KILLER KART".to_string()));
        assert!(!g.driver.car_with_you);
        assert_eq!(store_car(&mut g), Err(GarageError::NoCar));
        g.cars[1] = Some(Car::default_design("Second", 7));
        g.cars[2] = Some(Car::default_design("Elsewhere", 3));
        assert_eq!(stored_here(&g), vec![0, 1]);
        assert_eq!(take_out(&mut g, 1), Err(GarageError::Money));
        g.driver.money = 100;
        assert_eq!(take_out(&mut g, 2), Err(GarageError::NotHere));
        assert_eq!(take_out(&mut g, 1), Ok(()));
        assert_eq!((g.driver.car, g.driver.car_with_you, g.driver.money), (Some(1), true, 50));
        assert_eq!(stored_here(&g), vec![0]);
        // taking out another car puts the current one into storage first
        assert_eq!(take_out(&mut g, 0), Ok(()));
        assert_eq!(stored_here(&g), vec![1]);
    }
}
