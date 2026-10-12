//! The Weapon Shop (`we.prg`): buy weapons and ammunition; the decoy-quest clues.
//!
//! Buying a weapon checks, in this order (`we $A5FF`): a free weapon slot, weight capacity,
//! spaces, power (the acceleration class must stay below 3) and money. Each failure is
//! "I hate to tell you this, but you don't have enough <X> for that weapon." New weapons
//! come with no ammunition. Buying ammunition checks weight, magazine room (20 rounds),
//! power and money (`we $A267`).
use crate::car::{Car, Weapon};
use crate::game::Game;
use crate::tables::{MAGAZINE_MAX, WEAPONS};

pub use super::assembly::allowed_mounts;

/// Why a purchase is refused; [`Shortfall::word`] fills in "you don't have enough <X>".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortfall {
    /// all 10 weapon slots are in use
    WeaponSpace,
    WeightCapacity,
    /// spaces for a weapon; magazine room for ammunition
    Space,
    /// the car would be too heavy to move
    Power,
    Money,
}

impl Shortfall {
    /// The original's word list (`we $A9A1`).
    pub fn word(self) -> &'static str {
        match self {
            Shortfall::WeaponSpace => "weapon space",
            Shortfall::WeightCapacity => "weight capacity",
            Shortfall::Space => "space",
            Shortfall::Power => "power",
            Shortfall::Money => "money",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponError {
    /// "You don't have a car for me to put anything on, sir!"
    NoCar,
    Short(Shortfall),
    /// not one of the weapon's allowed mounts
    BadMount,
}

/// Would `extra` more pounds still let the car move (acceleration class < 3)?
fn has_power_for(car: &Car, extra: u32) -> bool {
    (car.weight() + extra) / car.plant_info().power < 3
}

/// The purchase checks for weapon `kind`, in the original order. Ok = the slot it would use.
pub fn check_weapon(car: &Car, money: u32, kind: u8) -> Result<usize, Shortfall> {
    let info = &WEAPONS[kind as usize];
    let slot = car.free_weapon_slot().ok_or(Shortfall::WeaponSpace)?;
    if car.weight_left() < info.weight as i64 {
        return Err(Shortfall::WeightCapacity);
    }
    if car.spaces_left() < info.spaces as i64 {
        return Err(Shortfall::Space);
    }
    if !has_power_for(car, info.weight) {
        return Err(Shortfall::Power);
    }
    if money < info.price {
        return Err(Shortfall::Money);
    }
    Ok(slot)
}

/// Buy and install a weapon ("I'll get my men right on it!"). It comes with no ammunition;
/// the car's value goes up by the price. Uses up the day. Returns the slot.
pub fn buy_weapon(game: &mut Game, kind: u8, mount: u8) -> Result<usize, WeaponError> {
    let money = game.driver.money;
    let car = game.car_here().ok_or(WeaponError::NoCar)?;
    let slot = check_weapon(car, money, kind).map_err(WeaponError::Short)?;
    if !allowed_mounts(kind).contains(&mount) {
        return Err(WeaponError::BadMount);
    }
    game.driver.pay(WEAPONS[kind as usize].price);
    game.car_here_mut().unwrap().weapons[slot] = Some(Weapon::new(kind, mount));
    game.driver.day_used = true;
    Ok(slot)
}

/// Weapon slots that take ammunition (everything but the laser and the heavy rocket), in
/// slot order: the "What weapon do you want ammunition for, sir?" list.
pub fn ammo_slots(car: &Car) -> Vec<usize> {
    car.weapons.iter().enumerate().filter(|(_, w)| matches!(w, Some(w) if w.uses_ammo())).map(|(i, _)| i).collect()
}

/// "You have room for N rounds in the magazine."
pub fn magazine_room(car: &Car, slot: usize) -> u8 {
    car.weapons[slot].map(|w| MAGAZINE_MAX.saturating_sub(w.ammo)).unwrap_or(0)
}

/// Price per round for the weapon in `slot`.
pub fn round_price(car: &Car, slot: usize) -> u32 {
    car.weapons[slot].and_then(|w| w.info().ammo_cost).unwrap_or(0)
}

/// The ammunition checks (weight, magazine, power, money). Ok = the price.
pub fn check_ammo(car: &Car, money: u32, slot: usize, rounds: u8) -> Result<u32, Shortfall> {
    let w = car.weapons[slot].ok_or(Shortfall::Space)?;
    let info = w.info();
    let price = info.ammo_cost.ok_or(Shortfall::Space)? * rounds as u32;
    let weight = info.ammo_weight * rounds as u32;
    if car.weight_left() < weight as i64 {
        return Err(Shortfall::WeightCapacity);
    }
    if w.ammo as u32 + rounds as u32 > MAGAZINE_MAX as u32 {
        return Err(Shortfall::Space);
    }
    // FIX: the original refused only an acceleration class of exactly 3
    if !has_power_for(car, weight) {
        return Err(Shortfall::Power);
    }
    if money < price {
        return Err(Shortfall::Money);
    }
    Ok(price)
}

/// Load `rounds` into the weapon in `slot` ("Thank you"). Returns the price. The car's value
/// doesn't change (ammunition is not part of it).
pub fn buy_ammo(game: &mut Game, slot: usize, rounds: u8) -> Result<u32, WeaponError> {
    let money = game.driver.money;
    let car = game.car_here().ok_or(WeaponError::NoCar)?;
    let price = check_ammo(car, money, slot, rounds).map_err(WeaponError::Short)?;
    game.driver.pay(price);
    if let Some(w) = game.car_here_mut().unwrap().weapons[slot].as_mut() {
        w.ammo += rounds;
    }
    Ok(price)
}

/// Quest 1 clue on entering a Weapon Shop (`we $A14D`): prestige 20+, the decoy quest not
/// taken yet, in Scranton, Dover or Syracuse.
pub fn clue(game: &Game) -> Option<&'static str> {
    let d = &game.driver;
    if d.prestige < 20 || d.quest.taken[0] {
        return None;
    }
    match d.city {
        6 => Some("Wanna help the Autoduellists? Go to the Dover Weapon Shop. Remember the word \"San\"."),
        14 => Some("Organized autoduelling needs you. Check in at the Syracuse Weapon Shop. Remember \"Antonio\"."),
        3 => Some("They need a hand at the Harrisburg Arena. Stop by the Scranton Weapon Shop for more info. Remember \"Rose\"."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tables::w;

    fn game() -> Option<Game> {
        let d = crate::data::test_data()?;
        Some(Game::new("Test", false, &d, 5))
    }

    #[test]
    fn weapon_checks_in_order() {
        let mut c = Car::default_design("X", 7); // 450 lb, 4 spaces left
        assert_eq!(check_weapon(&c, 10_000, w::MG), Ok(0));
        assert_eq!(check_weapon(&c, 999, w::MG), Err(Shortfall::Money));
        // anti-tank gun: 615 lb > 450 lb left
        assert_eq!(check_weapon(&c, 10_000, w::AT), Err(Shortfall::WeightCapacity));
        // two rocket launchers fit the weight but not the spaces
        c.weapons[0] = Some(Weapon::new(w::RL, 0));
        assert_eq!(check_weapon(&c, 10_000, w::RL), Err(Shortfall::Space));
        // all slots full
        for s in 0..10 {
            c.weapons[s] = Some(Weapon::new(w::HR, 0));
        }
        assert_eq!(check_weapon(&c, 0, w::HR), Err(Shortfall::WeaponSpace));
    }

    #[test]
    fn power_is_checked_after_weight_and_space() {
        let mut h = Car::default_design("H", 7);
        h.body = 1; // compact: 3700 max load at standard
        h.chassis = 1;
        for f in 0..5 {
            h.armor[f].max = 31;
            h.armor[f].pts = 31;
        }
        // 1300 + 500 + 120 + 155 * 6 = 2850 lb, class 2; 2850 + 215 = 3065 -> class 3
        assert_eq!(h.weight(), 2850);
        assert!(h.weight_left() >= 215 && h.spaces_left() >= 3);
        assert_eq!(check_weapon(&h, 10_000, w::RL), Err(Shortfall::Power));
    }

    #[test]
    fn buying_a_weapon() {
        let Some(mut g) = game() else { return };
        g.driver.money = 1200;
        assert_eq!(buy_weapon(&mut g, w::MG, 0), Err(WeaponError::NoCar));
        g.add_car(Car::default_design("X", 7));
        assert_eq!(buy_weapon(&mut g, w::MD, 0), Err(WeaponError::BadMount));
        assert_eq!(buy_weapon(&mut g, w::MG, 2), Ok(0));
        let c = g.car().unwrap();
        assert_eq!(c.weapons[0].map(|w| (w.kind, w.mount, w.ammo, w.dp)), Some((w::MG, 2, 0, 3)));
        assert_eq!(c.value(), 940 + 1000);
        assert_eq!(g.driver.money, 200);
        assert!(g.driver.day_used);
        assert_eq!(allowed_mounts(w::AT), vec![0, 1]);
        assert_eq!(allowed_mounts(w::SS), vec![1, 2, 3]);
        assert_eq!(allowed_mounts(w::OJ), vec![1]);
    }

    #[test]
    fn ammunition_limits() {
        let Some(mut g) = game() else { return };
        let mut car = Car::default_design("X", 7);
        car.weapons[0] = Some(Weapon::new(w::MG, 0));
        car.weapons[1] = Some(Weapon::new(w::HR, 0));
        car.weapons[2] = Some(Weapon { ammo: 15, ..Weapon::new(w::SD, 1) });
        g.add_car(car);
        g.driver.money = 600;
        let c = g.car().unwrap();
        assert_eq!(ammo_slots(c), vec![0, 2]);
        assert_eq!(magazine_room(c, 0), 20);
        assert_eq!(magazine_room(c, 2), 5);
        assert_eq!(round_price(c, 0), 25);
        assert_eq!(check_ammo(c, 600, 2, 6), Err(Shortfall::Space));
        assert_eq!(check_ammo(c, 600, 0, 20), Ok(500));
        assert_eq!(check_ammo(c, 499, 0, 20), Err(Shortfall::Money));
        assert_eq!(buy_ammo(&mut g, 0, 20), Ok(500));
        assert_eq!(g.driver.money, 100);
        assert_eq!(g.car().unwrap().weapons[0].unwrap().ammo, 20);
        assert_eq!(buy_ammo(&mut g, 0, 1), Err(WeaponError::Short(Shortfall::Space)));
        assert!(!g.driver.day_used, "ammunition doesn't use up the day");
        // weight: a car with 10 lb left can't take 3 spikedropper rounds (15 lb)
        let mut tight = Car::default_design("T", 7);
        tight.weapons[0] = Some(Weapon::new(w::SD, 1));
        tight.armor[0].max = 80; // 400 lb of armor (beyond the design range, just for the test)
        tight.armor[0].pts = 80;
        assert_eq!(tight.weight_left(), 10);
        assert_eq!(check_ammo(&tight, 1000, 0, 3), Err(Shortfall::WeightCapacity));
        assert_eq!(check_ammo(&tight, 1000, 0, 2), Ok(40));
    }

    #[test]
    fn decoy_clues() {
        let Some(mut g) = game() else { return };
        g.driver.city = 6;
        assert_eq!(clue(&g), None);
        g.driver.prestige = 20;
        assert!(clue(&g).unwrap().contains("\"San\""));
        g.driver.city = 14;
        assert!(clue(&g).unwrap().contains("Antonio"));
        g.driver.city = 3;
        assert!(clue(&g).unwrap().contains("Rose"));
        g.driver.city = 7;
        assert_eq!(clue(&g), None);
        g.driver.city = 3;
        g.driver.quest.taken[0] = true;
        assert_eq!(clue(&g), None);
    }
}
