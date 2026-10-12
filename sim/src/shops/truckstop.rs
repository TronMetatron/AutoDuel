//! Truck Stop (`tr.prg`, every city): bus, rumors, battery charge, rooms, body armor.
//!
//! The quest parts (the Watertown password, the brain-tape ambush) and the rumors are in
//! [`super::rumors`] and here.
use crate::data::GameData;
use crate::game::Game;
use crate::world::Building;

pub const BUS_FARE: u32 = 100;
pub const ROOM_PER_NIGHT: u32 = 10;
pub const MAX_NIGHTS: u32 = 9;
pub const ARMOR_PRICE: u32 = 250;
pub const ARMOR_POINTS: u8 = 3;
pub const BATTERY_PRICE: u32 = 50;
pub const BATTERY_FULL: u8 = 99;

/// Result of asking for a room for `nights` nights (`tr $A70B`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Room {
    /// paid and slept: "Have a good sleep!" (no healing)
    Slept { nights: u32 },
    /// you have $10 but not enough for that many nights: "You'll need some more money..."
    NeedMoreMoney,
    /// broke (less than $10), 20%: "...I'll give you a bus ticket, so you can get out of town."
    FreeBusTicket,
    /// broke, otherwise: a night in the lobby (+1 day); 20% chance of losing a health point
    Lobby { lost_health: bool },
}

pub fn rent_room(game: &mut Game, nights: u32) -> Room {
    let nights = nights.clamp(1, MAX_NIGHTS);
    let d = &game.driver;
    if d.money < ROOM_PER_NIGHT {
        if game.rng.upto(100) < 20 {
            game.free_bus_ticket = true;
            return Room::FreeBusTicket;
        }
        let lost = game.rng.upto(100) < 20 && game.driver.health != 1;
        if lost {
            game.driver.health -= 1;
        }
        game.next_day();
        return Room::Lobby { lost_health: lost };
    }
    if !game.driver.pay(ROOM_PER_NIGHT * nights) {
        return Room::NeedMoreMoney;
    }
    game.pass_days(nights);
    Room::Slept { nights }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Armor {
    /// "The set you have has nothing wrong with it!"
    AlreadyHave,
    NoMoney,
    /// "You should always wear body armor!"
    Bought,
}

pub fn buy_armor(game: &mut Game) -> Armor {
    if game.driver.body_armor >= ARMOR_POINTS {
        return Armor::AlreadyHave;
    }
    if !game.driver.pay(ARMOR_PRICE) {
        return Armor::NoMoney;
    }
    game.driver.body_armor = ARMOR_POINTS;
    Armor::Bought
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Battery {
    /// "You don't even have a car with you! How can I charge the batteries?"
    NoCar,
    /// "They don't need charging!"
    Full,
    NoMoney,
    /// "There you go. All charged up!"
    Charged,
}

pub fn charge_battery(game: &mut Game) -> Battery {
    match game.car_here() {
        None => return Battery::NoCar,
        Some(c) if c.battery >= BATTERY_FULL => return Battery::Full,
        _ => {}
    }
    if !game.driver.pay(BATTERY_PRICE) {
        return Battery::NoMoney;
    }
    if let Some(c) = game.car_here_mut() {
        c.battery = BATTERY_FULL;
    }
    Battery::Charged
}

/// Cities the bus goes to from here: every road neighbour, in exit order (N, E, S, W).
pub fn bus_destinations(data: &GameData, city: u8) -> Vec<u8> {
    data.city(city).exits.iter().map(|e| e.dest_city).collect()
}

/// What happens to your car if you take the bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarOnBus {
    /// no car with you
    NoCar,
    /// "We'll put your car in storage at the GARAGE."
    Garage,
    /// no garage here: "Do ya really want to leave your car on the streets?"
    Street,
}

pub fn car_on_bus(game: &Game, data: &GameData) -> CarOnBus {
    if game.car_here().is_none() {
        CarOnBus::NoCar
    } else if data.city_has(game.driver.city, Building::Garage) {
        CarOnBus::Garage
    } else {
        CarOnBus::Street
    }
}

/// Can you pay for the bus (or do you have the free ticket)?
pub fn can_pay_bus(game: &Game) -> bool {
    game.free_bus_ticket || game.driver.money >= BUS_FARE
}

/// Take the bus to `dest`: pay (unless the ticket was free), store the car at the garage or
/// leave it on the street (lost, with any quest item in it), one day passes and you are in `dest`.
/// Returns false if you can't pay.
pub fn take_bus(game: &mut Game, data: &GameData, dest: u8) -> bool {
    if !game.free_bus_ticket && !game.driver.pay(BUS_FARE) {
        return false;
    }
    game.free_bus_ticket = false;
    match car_on_bus(game, data) {
        CarOnBus::NoCar => {}
        CarOnBus::Garage => game.store_current_car(),
        CarOnBus::Street => {
            if let Some(i) = game.driver.car {
                game.lose_car(i as usize);
            }
        }
    }
    game.next_day();
    game.driver.city = dest;
    true
}

/// ".....(after a ___ trip)....." (`tr $AADA`, random 0..3).
pub fn trip_adjective(game: &mut Game) -> &'static str {
    [" pleasant", " rough", "n easy", " very rough"][game.rng.upto(3) as usize]
}

/// The brain-tape ambush on entering any Truck Stop (`tr $A14C`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ambush {
    /// "Fortunately, the shot misses."
    Missed,
    /// "Unfortunately, you are hit." — 2 points, body armor first
    Hit { died: bool },
}

pub const AMBUSH_DAMAGE: u32 = 2;

/// Someone recognizes you while you carry the brain tape: ⅓ miss, otherwise 2 damage.
pub fn tape_ambush(game: &mut Game) -> Option<Ambush> {
    if !game.driver.quest.carrying_tape() {
        return None;
    }
    if game.rng.upto(2) == 0 {
        return Some(Ambush::Missed);
    }
    let died = game.driver.hurt(AMBUSH_DAMAGE);
    Some(Ambush::Hit { died })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::Car;

    #[test]
    fn rooms_and_lobby() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        g.driver.money = 50;
        let day0 = g.driver.date;
        assert_eq!(rent_room(&mut g, 9), Room::NeedMoreMoney);
        assert_eq!(rent_room(&mut g, 5), Room::Slept { nights: 5 });
        assert_eq!(g.driver.money, 0);
        assert_eq!(g.driver.date, day0.plus_days(5));
        // broke: either a ticket or the lobby
        let mut tickets = 0;
        let mut lost = 0;
        for _ in 0..500 {
            g.driver.health = 3;
            g.free_bus_ticket = false;
            match rent_room(&mut g, 1) {
                Room::FreeBusTicket => {
                    tickets += 1;
                    assert!(g.free_bus_ticket);
                }
                Room::Lobby { lost_health } => {
                    if lost_health {
                        lost += 1;
                        assert_eq!(g.driver.health, 2);
                    }
                }
                r => panic!("{:?}", r),
            }
        }
        assert!(tickets > 50 && tickets < 160, "{}", tickets);
        assert!(lost > 30 && lost < 130, "{}", lost);
        g.driver.health = 1;
        for _ in 0..50 {
            rent_room(&mut g, 1);
            assert_eq!(g.driver.health, 1);
        }
    }

    #[test]
    fn armor_and_battery() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        assert_eq!(buy_armor(&mut g), Armor::Bought);
        assert_eq!(g.driver.money, 1750);
        assert_eq!(buy_armor(&mut g), Armor::AlreadyHave);
        assert_eq!(charge_battery(&mut g), Battery::NoCar);
        g.add_car(Car::default_design("C", 7));
        assert_eq!(charge_battery(&mut g), Battery::Full);
        g.car_mut().unwrap().battery = 10;
        assert_eq!(charge_battery(&mut g), Battery::Charged);
        assert_eq!(g.car().unwrap().battery, 99);
        assert_eq!(g.driver.money, 1700);
    }

    #[test]
    fn bus_stores_or_abandons_the_car() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        // New York has a garage
        g.add_car(Car::default_design("C", 7));
        assert_eq!(bus_destinations(&d, 7), vec![4, 11, 6]);
        assert_eq!(car_on_bus(&g, &d), CarOnBus::Garage);
        assert!(take_bus(&mut g, &d, 6));
        assert_eq!((g.driver.city, g.driver.money), (6, 1900));
        assert!(!g.driver.car_with_you);
        assert_eq!(g.car().unwrap().city, 7);
        // Scranton has no garage: the car is lost with its quest item
        g.driver.car_with_you = true;
        g.driver.prestige = 60;
        g.give_quest_item(2);
        assert_eq!(car_on_bus(&g, &d), CarOnBus::Street);
        g.free_bus_ticket = true;
        assert!(take_bus(&mut g, &d, 3));
        assert_eq!(g.driver.money, 1900);
        assert!(g.car().is_none());
        assert_eq!(g.driver.quest.carrying, None);
        assert!(g.driver.quest.taken[2]);
        g.driver.money = 99;
        assert!(!can_pay_bus(&g));
        assert!(!take_bus(&mut g, &d, 2));
    }

    #[test]
    fn ambush_only_with_the_tape() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        assert_eq!(tape_ambush(&mut g), None);
        g.add_car(Car::default_design("C", 0));
        g.give_quest_item(4);
        let mut hits = 0;
        for _ in 0..300 {
            g.driver.health = 3;
            g.driver.body_armor = 0;
            match tape_ambush(&mut g).unwrap() {
                Ambush::Hit { died } => {
                    hits += 1;
                    assert!(!died);
                    assert_eq!(g.driver.health, 1);
                }
                Ambush::Missed => assert_eq!(g.driver.health, 3),
            }
        }
        assert!(hits > 160 && hits < 240, "{}", hits);
    }
}
