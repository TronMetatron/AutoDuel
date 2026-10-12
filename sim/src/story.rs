//! The five password quests and their bookkeeping (see docs/CAR_DUEL_SPEC.md §11).
//!
//! Original: one "taken" flag per quest, the item rides in a cargo slot, only one item at a
//! time, losing an item aborts the quest for good.
//! FIX: losing the brain tape (quest 5) no longer soft-locks the game: the Outlaw HQ will hand
//! out a new tape if you come back without one (they don't know you lost it).
use crate::world::Building;
use serde::{Deserialize, Serialize};

pub struct QuestInfo {
    pub prestige: u8,
    pub giver: Building,
    pub giver_city: u8,
    pub password: &'static str,
    pub item: &'static str,
    pub dest: Building,
    pub dest_city: u8,
    /// None = no deadline
    pub days: Option<u32>,
    pub pay: u32,
    pub value: u32,
}

/// City indices: 0 Watertown 1 Manchester 2 Buffalo 3 Syracuse 4 Albany 5 Boston 6 Scranton
/// 7 New York 8 Providence 9 Pittsburgh 10 Harrisburg 11 Philadelphia 12 Atlantic City
/// 13 Baltimore 14 Dover 15 Washington.
pub const QUESTS: [QuestInfo; 5] = [
    QuestInfo { prestige: 20, giver: Building::Arena, giver_city: 10, password: "SAN ANTONIO ROSE", item: "Decoy prize for the Arena.", dest: Building::Arena, dest_city: 1, days: Some(10), pay: 6000, value: 0 },
    QuestInfo { prestige: 40, giver: Building::GoldCross, giver_city: 7, password: "GREAT WHITE WHALE", item: "Cloned heart for Senator.", dest: Building::GoldCross, dest_city: 5, days: Some(5), pay: 8000, value: 800 },
    QuestInfo { prestige: 60, giver: Building::TruckStop, giver_city: 0, password: "SHE SELLS SHELLS", item: "Evidence against the Mob.", dest: Building::Aada, dest_city: 11, days: Some(8), pay: 11000, value: 110000 },
    QuestInfo { prestige: 80, giver: Building::Salvage, giver_city: 15, password: "LITTLE BIG HORN", item: "Proof of fixed autoduels.", dest: Building::Fbi, dest_city: 7, days: Some(8), pay: 15000, value: 150000 },
    QuestInfo { prestige: 95, giver: Building::OutlawHq, giver_city: 0, password: "RUMPLESTILTSKIN", item: "Bootleg brain tape.", dest: Building::Fbi, dest_city: 7, days: None, pay: 50000, value: 300000 },
];

pub const BRAIN_TAPE: u8 = 4;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct QuestState {
    /// a quest's item has been handed over (each quest can be started once)
    pub taken: [bool; 5],
    /// which quest item you are carrying right now (in some car's cargo)
    pub carrying: Option<u8>,
    /// quests delivered
    pub done: [bool; 5],
    /// the brain tape reached the FBI: the game is won
    pub won: bool,
}

impl QuestState {
    /// Can a quest giver hand out a new item? Only one quest item at a time.
    pub fn free(&self) -> bool {
        self.carrying.is_none()
    }
    pub fn carrying_tape(&self) -> bool {
        self.carrying == Some(BRAIN_TAPE) && !self.won
    }
}

/// Passwords are typed letters/spaces and compared case-insensitively, ignoring extra spaces.
pub fn password_matches(typed: &str, quest: usize) -> bool {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase();
    norm(typed) == norm(QUESTS[quest].password)
}
