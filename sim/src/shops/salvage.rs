//! The Salvage Yard (`sa.prg`): sell your car, weapons off your car, or salvaged parts; the
//! fixed-autoduels quest (clue in Buffalo, password in Washington).
//!
//! * Car: floor((value - cost to repair all damage) / 10) x 5, i.e. half (`sa $A73A`). The
//!   repair cost here counts tires per lost DP ({5, 10, 20, 50}), unlike the Garage.
//! * Weapon: (price - repair cost) / 2, at least $5 (`sa $AA53`). Its ammunition is lost.
//!   Selling a weapon uses up the day.
//! * Salvaged parts: their accumulated value.
use crate::car::{Car, Weapon};
use crate::cargo::CargoKind;
use crate::game::Game;
use super::goldcross::{hand_over_quest_item, HandOver};

/// The fixed-autoduels quest (quest index 3 in [`crate::story::QUESTS`]).
pub const FIXES_QUEST: usize = 3;
const CLUE_CITY: u8 = 2;
const GIVER_CITY: u8 = 15;
const WEAPON_OFFER_MIN: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SalvageError {
    /// "Where's th' car?"
    NoCar,
    /// "There's nothin' there!"
    NoWeapon,
    /// "You ain't got nothin' to sell!"
    NothingToSell,
}

/// What the yard pays for a car: half of (value - repairs), rounded down to $5.
pub fn car_offer(car: &Car) -> u32 {
    let net = car.value().saturating_sub(car.salvage_repair_cost());
    net / 10 * 5
}

/// What the yard pays for a weapon: (price - repairs) / 2, at least $5.
/// FIX: the original subtracted the BCD repair cost in binary mode (`sa $AAC2`), so damaged
/// weapons got odd offers; this is the intended arithmetic.
pub fn weapon_offer(w: &Weapon) -> u32 {
    let repair = (w.max_dp - w.dp.min(w.max_dp)) as u32 * w.info().repair_per_dp;
    (w.info().price.saturating_sub(repair) / 2).max(WEAPON_OFFER_MIN)
}

/// 1 - "Sell a car": the car with you. Any quest item in it is lost. Returns the money paid.
pub fn sell_car(game: &mut Game) -> Result<u32, SalvageError> {
    if game.driver.loaner {
        return Err(SalvageError::NoCar);
    }
    let offer = car_offer(game.car_here().ok_or(SalvageError::NoCar)?);
    let slot = game.driver.car.expect("car with you") as usize;
    game.lose_car(slot);
    game.driver.add_money(offer as i64);
    Ok(offer)
}

/// 2 - "Sell weapons off your car": weapon slot `slot`. Returns the money paid.
pub fn sell_weapon(game: &mut Game, slot: usize) -> Result<u32, SalvageError> {
    let car = game.car_here_mut().ok_or(SalvageError::NoCar)?;
    let w = car.weapons.get_mut(slot).and_then(|w| w.take()).ok_or(SalvageError::NoWeapon)?;
    let offer = weapon_offer(&w);
    game.driver.add_money(offer as i64);
    game.driver.day_used = true;
    Ok(offer)
}

/// The value of all salvaged parts in the car with you (None if there are none).
pub fn salvage_value(car: &Car) -> Option<u32> {
    let mut found = false;
    let mut v = 0;
    for c in car.cargo.iter().flatten() {
        if c.kind == CargoKind::Salvage {
            found = true;
            v += c.value;
        }
    }
    found.then_some(v)
}

/// 3 - "Sell salvaged goods": all of them. Returns the money paid.
pub fn sell_salvage(game: &mut Game) -> Result<u32, SalvageError> {
    let v = salvage_value(game.car_here().ok_or(SalvageError::NoCar)?).ok_or(SalvageError::NothingToSell)?;
    for slot in 0..3 {
        if matches!(&game.car_here().unwrap().cargo[slot], Some(c) if c.kind == CargoKind::Salvage) {
            game.remove_cargo(slot);
        }
    }
    game.driver.add_money(v as i64);
    Ok(v)
}

/// Buffalo clue (`sa $A49E`): prestige 80+ and the fixed-autoduels quest not taken.
pub fn big_clue(game: &Game) -> Option<&'static str> {
    let d = &game.driver;
    (d.city == CLUE_CITY && d.prestige >= 80 && !d.quest.taken[FIXES_QUEST]).then_some(
        "There are some hot goods around that the feds want. Hit Baltimore for more info. Remember \"Big\".",
    )
}

/// Does the Washington yard ask for the password (`sa $A14D`)? Prestige 80+, the quest not
/// taken yet and no quest item being carried.
pub fn wants_password(game: &Game) -> bool {
    let d = &game.driver;
    d.city == GIVER_CITY && d.prestige >= 80 && !d.quest.taken[FIXES_QUEST] && d.quest.free()
}

/// The password answer: None = wrong password (nothing happens).
pub fn give_password(game: &mut Game, typed: &str) -> Option<HandOver> {
    if !crate::story::password_matches(typed, FIXES_QUEST) {
        return None;
    }
    Some(hand_over_quest_item(game, FIXES_QUEST))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cargo::Cargo;
    use crate::tables::w;

    fn game() -> Option<Game> {
        let d = crate::data::test_data()?;
        Some(Game::new("Test", false, &d, 11))
    }

    fn parts(value: u32) -> Cargo {
        Cargo { kind: CargoKind::Salvage, name: "Salvaged Parts".into(), dest: None, weight: 200, spaces: 1, due: None, value, pay: 0 }
    }

    #[test]
    fn car_resale_value() {
        let mut c = Car::default_design("X", 7);
        assert_eq!(car_offer(&c), 470); // 940 / 10 * 5
        let mut k = Car::killer_kart(7);
        assert_eq!(car_offer(&k), 1705); // 3414 -> 341 * 5
        // damage: 2 plant DP ($200), a heavy-duty tire down 3 DP ($30), 4 armor ($200), MG 1 DP ($100)
        k.plant.dp -= 2;
        k.tires[0].dp -= 3;
        k.armor[1].pts -= 4;
        k.weapons[0].as_mut().unwrap().dp -= 1;
        assert_eq!(k.salvage_repair_cost(), 530);
        assert_eq!(car_offer(&k), (3414 - 530) / 10 * 5);
        // a wreck worth less than its repairs fetches nothing
        c.plant.dp = 0;
        c.tires.iter_mut().for_each(|t| t.dp = 0);
        assert_eq!(c.salvage_repair_cost(), 250 + 80);
        c.plant.kind = 3;
        c.plant.max_dp = 12;
        assert_eq!(car_offer(&c), 0);
    }

    #[test]
    fn weapon_resale_value() {
        assert_eq!(weapon_offer(&Weapon::new(w::MG, 0)), 500);
        assert_eq!(weapon_offer(&Weapon::new(w::SD, 1)), 75);
        // a machine gun with 1 of 3 DP: (1000 - 200) / 2
        assert_eq!(weapon_offer(&Weapon { dp: 1, ..Weapon::new(w::MG, 0) }), 400);
        // a wrecked laser: (8000 - 1600) / 2
        assert_eq!(weapon_offer(&Weapon { dp: 0, ..Weapon::new(w::LA, 0) }), 3200);
        assert_eq!(weapon_offer(&Weapon { dp: 0, ..Weapon::new(w::HR, 0) }), 80);
    }

    #[test]
    fn selling_a_weapon_returns_weight_and_space() {
        let Some(mut g) = game() else { return };
        let mut c = Car::default_design("X", 7);
        c.weapons[3] = Some(Weapon { ammo: 10, ..Weapon::new(w::MG, 0) });
        g.add_car(c);
        let before = g.car().unwrap().weight_left();
        assert_eq!(sell_weapon(&mut g, 2), Err(SalvageError::NoWeapon));
        assert_eq!(sell_weapon(&mut g, 3), Ok(500));
        let car = g.car().unwrap();
        assert_eq!(car.weight_left(), before + 150 + 20);
        assert_eq!(car.value(), 940);
        assert_eq!(g.driver.money, 2500);
        assert!(g.driver.day_used);
    }

    #[test]
    fn selling_the_car_loses_its_quest_item() {
        let Some(mut g) = game() else { return };
        assert_eq!(sell_car(&mut g), Err(SalvageError::NoCar));
        g.add_car(Car::default_design("X", 7));
        g.give_quest_item(FIXES_QUEST);
        assert_eq!(g.driver.quest.carrying, Some(FIXES_QUEST as u8));
        assert_eq!(sell_car(&mut g), Ok(470));
        assert_eq!(g.driver.money, 2470);
        assert!(g.cars.iter().all(|c| c.is_none()));
        assert_eq!((g.driver.car, g.driver.car_with_you), (None, false));
        assert_eq!(g.driver.quest.carrying, None);
        assert!(g.driver.quest.taken[FIXES_QUEST], "the quest is lost for good");
    }

    #[test]
    fn selling_salvaged_goods() {
        let Some(mut g) = game() else { return };
        let mut c = Car::default_design("X", 7);
        assert_eq!(salvage_value(&c), None);
        c.cargo[0] = Some(parts(120));
        c.cargo[2] = Some(parts(35));
        c.cargo[1] = Some(Cargo { kind: CargoKind::Job(3), ..parts(9999) });
        g.add_car(c);
        assert_eq!(salvage_value(g.car().unwrap()), Some(155));
        assert_eq!(sell_salvage(&mut g), Ok(155));
        let car = g.car().unwrap();
        assert!(car.cargo[0].is_none() && car.cargo[2].is_none() && car.cargo[1].is_some());
        assert_eq!(sell_salvage(&mut g), Err(SalvageError::NothingToSell));
        assert_eq!(g.driver.money, 2155);
    }

    #[test]
    fn fixed_autoduels_quest() {
        let Some(mut g) = game() else { return };
        g.driver.prestige = 80;
        g.driver.city = 2;
        assert!(big_clue(&g).unwrap().contains("\"Big\""));
        assert!(!wants_password(&g));
        g.driver.city = 15;
        assert!(wants_password(&g));
        assert_eq!(give_password(&mut g, "little big man"), None);
        let mut c = Car::default_design("X", 15);
        for s in 0..3 {
            c.cargo[s] = Some(parts(1));
        }
        g.add_car(c);
        assert_eq!(give_password(&mut g, "LITTLE BIG HORN"), Some(HandOver::NoRoom));
        g.car_here_mut().unwrap().cargo[1] = None;
        assert_eq!(give_password(&mut g, "little big horn"), Some(HandOver::Loaded));
        assert!(g.driver.quest.taken[FIXES_QUEST]);
        assert_eq!(g.car().unwrap().cargo[1].as_ref().unwrap().name, "Proof of fixed autoduels.");
        assert!(!wants_password(&g));
        g.driver.city = 2;
        assert_eq!(big_clue(&g), None);
    }
}
