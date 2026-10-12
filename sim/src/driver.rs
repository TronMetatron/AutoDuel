//! The driver (player character): the original's 40-byte record at $50-$77.
use crate::date::Date;
use crate::story::QuestState;
use serde::{Deserialize, Serialize};

pub const MONEY_MAX: u32 = 999_999;
pub const STAT_MAX: u8 = 99;

/// What a Gold Cross braintape remembers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Braintape {
    pub name: String,
    pub driving: u8,
    pub marksman: u8,
    pub mechanic: u8,
    /// the clone always wakes in the city where it was first made
    pub city: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Driver {
    pub name: String,
    pub money: u32,
    pub prestige: u8,
    pub driving: u8,
    pub marksman: u8,
    pub mechanic: u8,
    /// 0..=3, 0 = dead
    pub health: u8,
    /// 0..=3, absorbs damage before health
    pub body_armor: u8,
    pub alive: bool,
    pub clone: Option<Braintape>,
    /// city you are in (or last left)
    pub city: u8,
    /// last city entered
    pub last_city: u8,
    pub date: Date,
    /// current car: 0..7 = owned car record, None = no current car
    pub car: Option<u8>,
    /// the current car is here with you (not stored / left behind)
    pub car_with_you: bool,
    /// true while driving the arena loaner (KILLER KART)
    pub loaner: bool,
    /// a big transaction happened today: several buildings are closed until tomorrow
    pub day_used: bool,
    pub quest: QuestState,
    /// cheat driver created with menu option 4
    pub cheat: bool,
}

impl Driver {
    pub fn new(name: &str, cheat: bool) -> Driver {
        let base = if cheat { 20 } else { 0 };
        Driver {
            name: name.to_string(),
            money: if cheat { 5000 } else { 2000 },
            prestige: 0,
            driving: base,
            marksman: base,
            mechanic: base,
            health: 3,
            body_armor: 0,
            alive: true,
            clone: None,
            city: 7,
            last_city: 7,
            date: Date::start(),
            car: None,
            car_with_you: false,
            loaner: false,
            day_used: false,
            quest: QuestState::default(),
            cheat,
        }
    }

    pub fn add_prestige(&mut self, d: i32) {
        self.prestige = (self.prestige as i32 + d).clamp(0, STAT_MAX as i32) as u8;
    }
    /// Adds (or removes) money; returns true if the $999,999 cap was hit ("broken the bank").
    pub fn add_money(&mut self, d: i64) -> bool {
        let v = self.money as i64 + d;
        self.money = v.clamp(0, MONEY_MAX as i64) as u32;
        v > MONEY_MAX as i64
    }
    pub fn can_afford(&self, cost: u32) -> bool {
        self.money >= cost
    }
    pub fn pay(&mut self, cost: u32) -> bool {
        if self.money >= cost {
            self.money -= cost;
            true
        } else {
            false
        }
    }
    pub fn raise(stat: &mut u8, by: u8) {
        *stat = (*stat as u32 + by as u32).min(STAT_MAX as u32) as u8;
    }
    /// Driver points shown on the HUD: health + body armor.
    pub fn driver_points(&self) -> u8 {
        self.health + self.body_armor
    }
    /// Damage to the driver: body armor first, then health. Returns true if this killed him.
    pub fn hurt(&mut self, mut dmg: u32) -> bool {
        let a = (self.body_armor as u32).min(dmg);
        self.body_armor -= a as u8;
        dmg -= a;
        let h = (self.health as u32).min(dmg);
        self.health -= h as u8;
        self.health == 0
    }
    pub fn snapshot(&self, city: u8) -> Braintape {
        Braintape { name: self.name.clone(), driving: self.driving, marksman: self.marksman, mechanic: self.mechanic, city }
    }
}
