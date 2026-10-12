//! Cargo carried in a car's three cargo slots: AADA jobs, quest items and salvaged parts.
use crate::date::Date;
use crate::world::Building;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CargoKind {
    /// an AADA courier job (index into the cargo table)
    Job(u8),
    /// a story item, quest 0..4
    Quest(u8),
    /// "Salvaged Parts" picked up from wrecks; sold at any Salvage Yard
    Salvage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cargo {
    pub kind: CargoKind,
    pub name: String,
    /// destination building and city (None for salvage)
    pub dest: Option<(Building, u8)>,
    pub weight: u32,
    pub spaces: u32,
    /// None = no deadline
    pub due: Option<Date>,
    /// declared value in dollars (black-market price base, salvage sale price)
    pub value: u32,
    /// payment on on-time delivery
    pub pay: u32,
}

impl Cargo {
    pub fn is_overdue(&self, today: Date) -> bool {
        matches!(self.due, Some(d) if today > d)
    }
}
