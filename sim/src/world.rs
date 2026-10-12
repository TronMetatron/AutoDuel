//! Buildings and per-road persistent state.
use serde::{Deserialize, Serialize};

/// Building types, in the original's order (overlay files GO TR AS SA WE GA AR AA DI FB OU AB OR PE CA HO).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Building {
    GoldCross = 0,
    TruckStop = 1,
    Assembly = 2,
    Salvage = 3,
    Weapons = 4,
    Garage = 5,
    Arena = 6,
    Aada = 7,
    Bar = 8,
    Fbi = 9,
    OutlawHq = 10,
    Abandoned = 11,
    Origin = 12,
    PetStore = 13,
    Casino = 14,
    Hotel = 15,
}

impl Building {
    pub const ALL: [Building; 16] = [
        Building::GoldCross, Building::TruckStop, Building::Assembly, Building::Salvage, Building::Weapons,
        Building::Garage, Building::Arena, Building::Aada, Building::Bar, Building::Fbi, Building::OutlawHq,
        Building::Abandoned, Building::Origin, Building::PetStore, Building::Casino, Building::Hotel,
    ];
    pub fn from_u8(v: u8) -> Option<Building> {
        Building::ALL.get(v as usize).copied()
    }
    pub fn name(self) -> &'static str {
        crate::tables::BUILDING_NAMES[self as usize]
    }
    /// "It's closed. Try again tomorrow." once a big transaction used up the day (`city $1415`).
    pub fn closes_after_business(self) -> bool {
        matches!(
            self,
            Building::Assembly | Building::Salvage | Building::Weapons | Building::Garage | Building::Arena
                | Building::Aada | Building::Origin | Building::PetStore
        )
    }
    /// Courier cargo is delivered automatically on entry (all except DI, OU, AB).
    pub fn accepts_deliveries(self) -> bool {
        !matches!(self, Building::Bar | Building::OutlawHq | Building::Abandoned)
    }
}

/// Persistent enemies on a road: survivors are written back when you leave (original behaviour).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoadState {
    /// enemy type per slot (None = empty); positions come from the road data
    pub slots: Vec<Option<u8>>,
}

impl RoadState {
    pub fn from_data(r: &crate::data::Road) -> RoadState {
        RoadState { slots: r.slots.iter().map(|s| s.kind).collect() }
    }
    /// AADA "Road Status Bulletin" wording: not / lightly / moderately / heavily trafficked.
    pub fn traffic_word(&self) -> &'static str {
        match self.slots.iter().filter(|s| s.is_some()).count() {
            0 => "not",
            1..=4 => "lightly",
            5..=12 => "moderately",
            _ => "heavily",
        }
    }
}
