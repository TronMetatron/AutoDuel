//! The whole saved game: driver, up to 8 cars, persistent road enemies, random state.
use crate::car::Car;
use crate::cargo::{Cargo, CargoKind};
use crate::data::GameData;
use crate::date::Date;
use crate::driver::Driver;
use crate::rng::Rng;
use crate::story::{QUESTS, BRAIN_TAPE};
use crate::world::{Building, RoadState};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MAX_CARS: usize = 8;

/// Today's three AADA courier offers (regenerated each day; a taken offer is replaced the next
/// time the list is shown).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AadaBoard {
    pub date: Option<Date>,
    pub offers: Vec<Option<JobOffer>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobOffer {
    /// index into the cargo table
    pub cargo: u8,
    pub dest_city: u8,
    /// days to deliver (5..10)
    pub days: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    pub version: u32,
    pub driver: Driver,
    pub cars: Vec<Option<Car>>,
    /// the Amateur Night loaner while it is in use
    pub loaner: Option<Car>,
    pub roads: Vec<RoadState>,
    pub rng: Rng,
    pub aada: AadaBoard,
    /// the Truck Stop gave you a free bus ticket (when broke)
    pub free_bus_ticket: bool,
    /// set by the FBI briefing (quest 5 can start)
    pub fbi_briefed: bool,
}

/// What happened when the driver died.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeathOutcome {
    /// no clone: the driver is gone for good
    GameOver,
    /// revived as the clone at the Gold Cross in this city
    Cloned { city: u8 },
}

/// Message lines produced by automatic deliveries on entering a building.
#[derive(Clone, Debug, Default)]
pub struct DeliveryReport {
    pub lines: Vec<String>,
    /// the brain tape reached the FBI
    pub won: bool,
}

impl Game {
    pub fn new(name: &str, cheat: bool, data: &GameData, seed: u32) -> Game {
        Game {
            version: 1,
            driver: Driver::new(name, cheat),
            cars: vec![None; MAX_CARS],
            loaner: None,
            roads: data.roads.iter().map(RoadState::from_data).collect(),
            rng: Rng::new(seed),
            aada: AadaBoard::default(),
            free_bus_ticket: false,
            fbi_briefed: false,
        }
    }

    // ---------------------------------------------------------------- cars
    /// The car you are driving (the loaner during Amateur Night).
    pub fn car(&self) -> Option<&Car> {
        if self.driver.loaner {
            return self.loaner.as_ref();
        }
        self.driver.car.and_then(|i| self.cars[i as usize].as_ref())
    }
    pub fn car_mut(&mut self) -> Option<&mut Car> {
        if self.driver.loaner {
            return self.loaner.as_mut();
        }
        match self.driver.car {
            Some(i) => self.cars[i as usize].as_mut(),
            None => None,
        }
    }
    /// The current car, only if it is here with you.
    pub fn car_here(&self) -> Option<&Car> {
        if self.driver.car_with_you { self.car() } else { None }
    }
    pub fn car_here_mut(&mut self) -> Option<&mut Car> {
        if self.driver.car_with_you { self.car_mut() } else { None }
    }
    pub fn free_car_slot(&self) -> Option<usize> {
        self.cars.iter().position(|c| c.is_none())
    }
    pub fn owned_cars(&self) -> impl Iterator<Item = (usize, &Car)> {
        self.cars.iter().enumerate().filter_map(|(i, c)| c.as_ref().map(|c| (i, c)))
    }
    /// Add a new car and make it current and with you. Returns its slot.
    pub fn add_car(&mut self, car: Car) -> Option<usize> {
        let i = self.free_car_slot()?;
        self.cars[i] = Some(car);
        self.driver.car = Some(i as u8);
        self.driver.car_with_you = true;
        Some(i)
    }
    /// Store the current car in this city (Garage, Assembly Line, bus).
    pub fn store_current_car(&mut self) {
        let city = self.driver.city;
        if let Some(c) = self.car_mut() {
            c.city = city;
        }
        self.driver.car_with_you = false;
    }
    /// The car is gone (destroyed, sold, abandoned). Any quest item in it is lost.
    pub fn lose_car(&mut self, slot: usize) {
        if let Some(c) = self.cars[slot].take() {
            self.cargo_lost(&c.cargo);
        }
        if self.driver.car == Some(slot as u8) {
            self.driver.car = None;
            self.driver.car_with_you = false;
        }
    }
    pub fn lose_current_car(&mut self) {
        if self.driver.loaner {
            self.loaner = None;
            self.driver.loaner = false;
            self.driver.car_with_you = false;
            return;
        }
        if let Some(i) = self.driver.car {
            self.lose_car(i as usize);
        }
    }

    // ---------------------------------------------------------------- cargo & quests
    fn cargo_lost(&mut self, cargo: &[Option<Cargo>]) {
        for c in cargo.iter().flatten() {
            if let CargoKind::Quest(q) = c.kind
                && self.driver.quest.carrying == Some(q) {
                self.driver.quest.carrying = None;
            }
        }
    }
    /// Remove one cargo slot of the current car (sold, destroyed). Handles quest items.
    pub fn remove_cargo(&mut self, slot: usize) -> Option<Cargo> {
        let c = self.car_mut()?.cargo[slot].take()?;
        if let CargoKind::Quest(q) = c.kind
            && self.driver.quest.carrying == Some(q) {
            self.driver.quest.carrying = None;
        }
        Some(c)
    }
    /// Give the quest item for quest `q` (needs a car with a free slot; the caller checks).
    pub fn give_quest_item(&mut self, q: usize) -> bool {
        let info = &QUESTS[q];
        let today = self.driver.date;
        let item = Cargo {
            kind: CargoKind::Quest(q as u8),
            name: info.item.to_string(),
            dest: Some((info.dest, info.dest_city)),
            weight: 0,
            spaces: 0,
            due: info.days.map(|d| today.plus_days(d)),
            value: info.value,
            pay: info.pay,
        };
        let Some(car) = self.car_here_mut() else { return false };
        let Some(slot) = car.free_cargo_slot() else { return false };
        car.cargo[slot] = Some(item);
        self.driver.quest.taken[q] = true;
        self.driver.quest.carrying = Some(q as u8);
        if q as u8 == BRAIN_TAPE {
            // "you won't have a clone to fall back on"
            self.driver.clone = None;
        }
        true
    }

    /// Automatic delivery when entering a building with your car (`SR_cargo`, `subrout $4C49`).
    pub fn deliver_cargo(&mut self, building: Building, data: &GameData) -> DeliveryReport {
        let mut rep = DeliveryReport::default();
        if !building.accepts_deliveries() || !self.driver.car_with_you {
            return rep;
        }
        let city = self.driver.city;
        let today = self.driver.date;
        let Some(car) = self.car_mut() else { return rep };
        let mut delivered = vec![];
        for slot in 0..3 {
            let hit = matches!(&car.cargo[slot], Some(c) if c.dest == Some((building, city)));
            if hit {
                delivered.push(car.cargo[slot].take().unwrap());
            }
        }
        for c in delivered {
            let on_time = !c.is_overdue(today);
            let quest = match c.kind {
                CargoKind::Quest(q) => Some(q),
                _ => None,
            };
            let delta = if quest.is_some() && quest != Some(BRAIN_TAPE) { 12 } else { 2 };
            if let Some(q) = quest {
                if self.driver.quest.carrying == Some(q) {
                    self.driver.quest.carrying = None;
                }
                self.driver.quest.done[q as usize] = true;
                if q == BRAIN_TAPE {
                    self.driver.quest.won = true;
                    rep.won = true;
                }
            }
            if on_time {
                self.driver.add_prestige(delta);
                self.driver.add_money(c.pay as i64);
                rep.lines.push(format!("Thanks for the package! (${})", c.pay));
            } else {
                self.driver.add_prestige(-delta);
                rep.lines.push(format!("{} is late. No pay.", c.name));
            }
        }
        let _ = data;
        rep
    }

    // ---------------------------------------------------------------- time
    /// Advance one day (`SR_time_add`): new AADA offers, the "day used" flag clears.
    pub fn next_day(&mut self) {
        self.driver.date = self.driver.date.next_day();
        self.driver.day_used = false;
        self.aada.date = None;
    }
    pub fn pass_days(&mut self, n: u32) {
        for _ in 0..n {
            self.next_day();
        }
    }

    /// IRS (`city $1023`): on entering a city screen with $900,000 or more, lose $350,000.
    /// Returns true if it happened.
    pub fn irs_check(&mut self) -> bool {
        if self.driver.money >= 900_000 {
            self.driver.money -= 350_000;
            true
        } else {
            false
        }
    }

    // ---------------------------------------------------------------- death
    /// The driver died. With a clone: the car he was in is lost, the clone wakes at its Gold Cross
    /// with the braintape's name and skills; money, prestige, date and quests carry over; then
    /// prestige -4 and one day passes. Without a clone the driver is dead.
    /// FIX: only the car with you is lost; a current car stored in a garage stays there (the
    /// original's clone simply has no car with it). The active quest is aborted either way
    /// (clone activation sets the quest stage to idle), so its item is taken out of the cargo.
    pub fn kill_driver(&mut self) -> DeathOutcome {
        if self.driver.loaner || self.driver.car_with_you {
            self.lose_current_car();
        }
        if let Some(q) = self.driver.quest.carrying.take() {
            for car in self.cars.iter_mut().flatten() {
                for slot in car.cargo.iter_mut() {
                    if matches!(slot, Some(c) if c.kind == CargoKind::Quest(q)) {
                        *slot = None;
                    }
                }
            }
        }
        self.driver.car_with_you = false;
        let Some(tape) = self.driver.clone.take() else {
            self.driver.alive = false;
            self.driver.health = 0;
            return DeathOutcome::GameOver;
        };
        let d = &mut self.driver;
        d.name = tape.name;
        d.driving = tape.driving;
        d.marksman = tape.marksman;
        d.mechanic = tape.mechanic;
        d.health = 3;
        d.body_armor = 0;
        d.alive = true;
        d.city = tape.city;
        d.last_city = tape.city;
        d.car_with_you = false;
        d.loaner = false;
        d.add_prestige(-4);
        self.next_day();
        DeathOutcome::Cloned { city: tape.city }
    }

    /// The driver died inside a building (shot at the Truck Stop, in a bar fight, at the Outlaw
    /// HQ): a car parked outside stays in this city, then as [`Game::kill_driver`].
    pub fn kill_driver_in_town(&mut self) -> DeathOutcome {
        if self.driver.car_with_you && !self.driver.loaner && self.car().is_some() {
            self.store_current_car();
        }
        self.kill_driver()
    }

    // ---------------------------------------------------------------- save / load
    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| format!("{}: {}", path.display(), e))
    }
    pub fn load_from(path: &Path) -> Result<Game, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let mut g: Game = serde_json::from_str(&s).map_err(|e| format!("{}: {}", path.display(), e))?;
        g.after_load();
        Ok(g)
    }

    /// Tidy up a game saved in the middle of something (the window was closed during an arena
    /// event): FIX: the Amateur Night loaner goes back to the arena instead of staying with you.
    pub fn after_load(&mut self) {
        if self.driver.loaner || self.loaner.is_some() {
            self.loaner = None;
            self.driver.loaner = false;
            self.driver.car_with_you = false;
        }
    }
    /// File name for this driver's save.
    pub fn save_file_name(&self) -> String {
        let clean: String = self
            .driver
            .name
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect();
        format!("{}.json", if clean.trim_matches('_').is_empty() { "driver".to_string() } else { clean })
    }
}

/// Saved drivers in a folder: (path, game), most recently modified first.
pub fn list_saves(dir: &Path) -> Vec<(PathBuf, Game)> {
    let mut v = vec![];
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "json").unwrap_or(false)
                && let Ok(g) = Game::load_from(&p) {
                let t = e.metadata().and_then(|m| m.modified()).ok();
                v.push((t, p, g));
            }
        }
    }
    v.sort_by_key(|e| std::cmp::Reverse(e.0));
    v.into_iter().map(|(_, p, g)| (p, g)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_game_and_delivery() {
        let d = crate::need_data!();
        let mut g = Game::new("Test", false, &d, 1);
        assert_eq!(g.driver.money, 2000);
        assert_eq!(d.cities.len(), 16);
        assert_eq!(d.roads.len(), 17);
        let mut car = Car::default_design("Car", 7);
        car.cargo[0] = Some(Cargo {
            kind: CargoKind::Job(0),
            name: "Legal documents".into(),
            dest: Some((Building::Fbi, 7)),
            weight: 0,
            spaces: 0,
            due: Some(g.driver.date.plus_days(3)),
            value: 5000,
            pay: 400,
        });
        g.add_car(car);
        let r = g.deliver_cargo(Building::Fbi, &d);
        assert_eq!(r.lines.len(), 1);
        assert_eq!(g.driver.money, 2400);
        assert_eq!(g.driver.prestige, 2);
    }

    #[test]
    fn clone_revival() {
        let d = crate::need_data!();
        let mut g = Game::new("Test", false, &d, 1);
        g.driver.clone = Some(g.driver.snapshot(5));
        g.driver.prestige = 10;
        assert_eq!(g.kill_driver(), DeathOutcome::Cloned { city: 5 });
        assert_eq!(g.driver.prestige, 6);
        assert_eq!(g.kill_driver(), DeathOutcome::GameOver);
    }
}
