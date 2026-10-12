//! Component, weapon and enemy tables, exactly as in the original (decoded from `as.prg`,
//! `we.prg`, `ga.prg`, `sa.prg`, `r2/r3`, `a3`, `ar.prg`). See docs/CAR_DUEL_SPEC.md §3, §5, §6.

pub struct BodyInfo {
    pub name: &'static str,
    /// cost with each chassis (Light, Standard, Heavy, Ex. Heavy)
    pub cost: [u32; 4],
    /// max load with each chassis
    pub max_load: [u32; 4],
    pub weight: u32,
    pub spaces: u32,
    pub armor_cost: u32,
    pub armor_weight: u32,
    /// +1 handling class for cars (Subcompact..Luxury), 0 for wagon/pickup/van
    pub hc_bonus: u8,
    /// suspension cost for (Light, Improved, Heavy)
    pub susp_cost: [u32; 3],
}

pub const BODIES: [BodyInfo; 7] = [
    BodyInfo { name: "Subcompact", cost: [240, 300, 450, 600], max_load: [2070, 2300, 2530, 2760], weight: 1000, spaces: 7, armor_cost: 11, armor_weight: 5, hc_bonus: 1, susp_cost: [0, 300, 450] },
    BodyInfo { name: "Compact", cost: [320, 400, 600, 800], max_load: [3330, 3700, 4070, 4440], weight: 1300, spaces: 10, armor_cost: 13, armor_weight: 6, hc_bonus: 1, susp_cost: [0, 400, 600] },
    BodyInfo { name: "Mid-sized", cost: [480, 600, 900, 1200], max_load: [4320, 4800, 5280, 5760], weight: 1600, spaces: 13, armor_cost: 16, armor_weight: 8, hc_bonus: 1, susp_cost: [0, 600, 900] },
    BodyInfo { name: "Luxury", cost: [640, 800, 1200, 1600], max_load: [4950, 5500, 6050, 6600], weight: 1800, spaces: 19, armor_cost: 20, armor_weight: 10, hc_bonus: 1, susp_cost: [0, 800, 1200] },
    BodyInfo { name: "Sta. Wagon", cost: [640, 800, 1200, 1600], max_load: [4950, 5500, 6050, 6600], weight: 2100, spaces: 21, armor_cost: 20, armor_weight: 10, hc_bonus: 0, susp_cost: [0, 800, 1200] },
    BodyInfo { name: "Pickup", cost: [720, 900, 1350, 1800], max_load: [5850, 6500, 7150, 7800], weight: 2100, spaces: 24, armor_cost: 22, armor_weight: 11, hc_bonus: 0, susp_cost: [0, 900, 1350] },
    BodyInfo { name: "Van", cost: [800, 1000, 1500, 2000], max_load: [5400, 6000, 6600, 7200], weight: 2000, spaces: 30, armor_cost: 30, armor_weight: 14, hc_bonus: 0, susp_cost: [0, 1000, 1500] },
];

pub const CHASSIS_NAMES: [&str; 4] = ["Light", "Standard", "Heavy", "Ex. Heavy"];
pub const SUSPENSION_NAMES: [&str; 3] = ["Light", "Improved", "Heavy"];

pub struct PlantInfo {
    pub name: &'static str,
    pub cost: u32,
    pub weight: u32,
    pub spaces: u32,
    pub dp: u8,
    /// power factor (lb the plant can move at full acceleration)
    pub power: u32,
    /// top speed level in the driving engine (1 level = 1 world unit per frame)
    pub top_level: u8,
    /// shown on the car sheet ("Top:")
    pub top_mph: u32,
    pub repair_per_dp: u32,
}

pub const PLANTS: [PlantInfo; 4] = [
    PlantInfo { name: "Small", cost: 500, weight: 500, spaces: 3, dp: 5, power: 1000, top_level: 2, top_mph: 70, repair_per_dp: 50 },
    PlantInfo { name: "Medium", cost: 1000, weight: 700, spaces: 4, dp: 8, power: 2300, top_level: 3, top_mph: 80, repair_per_dp: 100 },
    PlantInfo { name: "Large", cost: 2000, weight: 900, spaces: 5, dp: 10, power: 3600, top_level: 4, top_mph: 90, repair_per_dp: 200 },
    PlantInfo { name: "Super", cost: 3000, weight: 1100, spaces: 6, dp: 12, power: 5000, top_level: 4, top_mph: 90, repair_per_dp: 300 },
];

pub struct TireInfo {
    pub name: &'static str,
    /// short name on the car sheet
    pub short: &'static str,
    /// for a set of four
    pub set_cost: u32,
    pub set_weight: u32,
    pub dp: u8,
    /// garage price to replace one tire
    pub replace_cost: u32,
    /// salvage-yard deduction per lost DP
    pub salvage_per_dp: u32,
}

pub const TIRES: [TireInfo; 4] = [
    TireInfo { name: "Standard", short: "Stand", set_cost: 200, set_weight: 120, dp: 4, replace_cost: 50, salvage_per_dp: 5 },
    TireInfo { name: "Heavy-duty", short: "Hvy Dt", set_cost: 400, set_weight: 160, dp: 6, replace_cost: 100, salvage_per_dp: 10 },
    TireInfo { name: "Puncture-resistant", short: "Pnc Rs", set_cost: 800, set_weight: 200, dp: 9, replace_cost: 200, salvage_per_dp: 20 },
    TireInfo { name: "Solid", short: "Solid", set_cost: 2000, set_weight: 300, dp: 12, replace_cost: 500, salvage_per_dp: 50 },
];

/// Armor per facing: 0..=60 points.
pub const ARMOR_MAX: u8 = 60;
/// Garage armor repair price per point (FIX candidate; kept as original).
pub const ARMOR_REPAIR_PER_PT: u32 = 50;
pub const MAGAZINE_MAX: u8 = 20;

pub struct WeaponInfo {
    pub name: &'static str,
    /// 6-char name used on the car sheet / HUD lists
    pub short: &'static str,
    /// 3-char HUD abbreviation
    pub abbr: &'static str,
    pub price: u32,
    pub weight: u32,
    pub spaces: u32,
    pub dp: u8,
    /// None = no ammunition (laser: battery; heavy rocket: one shot)
    pub ammo_cost: Option<u32>,
    pub ammo_weight: u32,
    /// allowed mounts [Front, Back, Left, Right]
    pub mounts: [bool; 4],
    pub repair_per_dp: u32,
    /// to-hit base target (2..13 roll must reach it); None = no roll (droppers, clouds)
    pub to_hit: Option<u8>,
    /// damage is 1..=dmg+1 (the original's `random(dmg)+1`); 0 = special
    pub dmg: u32,
    /// frames until the weapon can fire again (the original counter; real interval = +1)
    pub cooldown: u8,
}

pub const WEAPONS: [WeaponInfo; 12] = [
    WeaponInfo { name: "Machine gun", short: "M. Gun", abbr: "MG", price: 1000, weight: 150, spaces: 1, dp: 3, ammo_cost: Some(25), ammo_weight: 2, mounts: [true, true, true, true], repair_per_dp: 100, to_hit: Some(7), dmg: 2, cooldown: 3 },
    WeaponInfo { name: "Flamethrower", short: "Flmthr", abbr: "FT", price: 550, weight: 465, spaces: 3, dp: 2, ammo_cost: Some(25), ammo_weight: 5, mounts: [false, true, true, true], repair_per_dp: 50, to_hit: Some(6), dmg: 2, cooldown: 2 },
    WeaponInfo { name: "Rocket launcher", short: "Rkt L.", abbr: "RL", price: 1050, weight: 215, spaces: 3, dp: 2, ammo_cost: Some(35), ammo_weight: 5, mounts: [true, true, true, true], repair_per_dp: 100, to_hit: Some(8), dmg: 18, cooldown: 2 },
    WeaponInfo { name: "Recoilless rifle", short: "R. Rif", abbr: "RR", price: 1550, weight: 315, spaces: 3, dp: 4, ammo_cost: Some(35), ammo_weight: 5, mounts: [true, true, true, true], repair_per_dp: 150, to_hit: Some(7), dmg: 12, cooldown: 2 },
    WeaponInfo { name: "Anti-tank gun", short: "AT Gun", abbr: "AT", price: 2050, weight: 615, spaces: 4, dp: 5, ammo_cost: Some(50), ammo_weight: 10, mounts: [true, true, false, false], repair_per_dp: 200, to_hit: Some(8), dmg: 18, cooldown: 2 },
    WeaponInfo { name: "Laser", short: "Laser", abbr: "LA", price: 8000, weight: 500, spaces: 2, dp: 2, ammo_cost: None, ammo_weight: 0, mounts: [true, true, true, true], repair_per_dp: 800, to_hit: Some(6), dmg: 18, cooldown: 2 },
    WeaponInfo { name: "Minedropper", short: "Minedp", abbr: "MD", price: 550, weight: 165, spaces: 3, dp: 2, ammo_cost: Some(50), ammo_weight: 5, mounts: [false, true, false, false], repair_per_dp: 50, to_hit: None, dmg: 0, cooldown: 2 },
    WeaponInfo { name: "Spikedropper", short: "Spikdp", abbr: "SD", price: 150, weight: 40, spaces: 2, dp: 4, ammo_cost: Some(20), ammo_weight: 5, mounts: [false, true, false, false], repair_per_dp: 10, to_hit: None, dmg: 0, cooldown: 2 },
    WeaponInfo { name: "Smokescreen", short: "Smoksn", abbr: "SS", price: 300, weight: 40, spaces: 2, dp: 4, ammo_cost: Some(10), ammo_weight: 5, mounts: [false, true, true, true], repair_per_dp: 25, to_hit: None, dmg: 0, cooldown: 4 },
    WeaponInfo { name: "Paint spray", short: "Pnt Sp", abbr: "PS", price: 400, weight: 25, spaces: 1, dp: 2, ammo_cost: Some(10), ammo_weight: 2, mounts: [false, true, true, true], repair_per_dp: 40, to_hit: None, dmg: 0, cooldown: 4 },
    WeaponInfo { name: "Oil jet", short: "Oil Jt", abbr: "OJ", price: 250, weight: 25, spaces: 2, dp: 3, ammo_cost: Some(10), ammo_weight: 2, mounts: [false, true, false, false], repair_per_dp: 25, to_hit: None, dmg: 0, cooldown: 4 },
    WeaponInfo { name: "Heavy rocket", short: "Hv Rkt", abbr: "HR", price: 200, weight: 100, spaces: 1, dp: 2, ammo_cost: None, ammo_weight: 0, mounts: [true, true, true, true], repair_per_dp: 20, to_hit: Some(9), dmg: 30, cooldown: 2 },
];

/// Weapon type indices.
pub mod w {
    pub const MG: u8 = 0;
    pub const FT: u8 = 1;
    pub const RL: u8 = 2;
    pub const RR: u8 = 3;
    pub const AT: u8 = 4;
    pub const LA: u8 = 5;
    pub const MD: u8 = 6;
    pub const SD: u8 = 7;
    pub const SS: u8 = 8;
    pub const PS: u8 = 9;
    pub const OJ: u8 = 10;
    pub const HR: u8 = 11;
}

/// Mount locations (component slot `location`): 0 Frt 1 Bck 2 Lft 3 Rgt 4 Und 5 Ctr 6 F-L 7 F-R 8 B-L 9 B-R
pub const LOCATION_NAMES: [&str; 10] = ["Frt", "Bck", "Lft", "Rgt", "Und", "Ctr", "F-L", "F-R", "B-L", "B-R"];
pub const MOUNT_NAMES: [&str; 4] = ["Front", "Back", "Left side", "Right side"];
pub const FACING_NAMES: [&str; 5] = ["Front", "Back", "Left", "Right", "Under"];

/// Default mount for a newly chosen weapon: back for FT/MD/SD/SS/PS/OJ, front otherwise.
pub fn default_mount(kind: u8) -> u8 {
    match kind {
        1 | 6 | 7 | 8 | 9 | 10 => 1,
        _ => 0,
    }
}

/// Enemy vehicle types.
#[derive(Clone, Copy, Debug)]
pub struct EnemyType {
    /// hit points (single pool)
    pub hp: u32,
    pub weapon: u8,
    /// 0 F, 1 B, 2 L, 3 R (only matters for mines and arena type 0)
    pub mount: u8,
    /// sub-speed added per frame (32 = +1 speed level)
    pub accel: u8,
    /// top speed level
    pub max: u8,
    /// frames between steering decisions
    pub ai_period: u8,
    /// which of the five vehicle sprite sets (non-modshaps 27 + 8*body)
    pub body: u8,
}

const fn et(hp: u32, weapon: u8, mount: u8, accel: u8, max: u8, ai_period: u8, body: u8) -> EnemyType {
    EnemyType { hp, weapon, mount, accel, max, ai_period, body }
}

/// Road enemy types 0..15 (`r3 $15DF..`, `r2 $8573/$8583`). Type 15 only from random spawns.
pub const ROAD_ENEMIES: [EnemyType; 16] = [
    et(6, w::MG, 0, 4, 3, 3, 0),
    et(8, w::RR, 3, 4, 3, 3, 1),
    et(12, w::MG, 0, 4, 3, 4, 2),
    et(16, w::RR, 0, 4, 3, 3, 3),
    et(20, w::LA, 1, 12, 4, 2, 4),
    et(24, w::AT, 0, 6, 4, 4, 0),
    et(28, w::RR, 2, 6, 4, 3, 1),
    et(32, w::LA, 3, 6, 4, 4, 2),
    et(36, w::AT, 0, 8, 4, 4, 3),
    et(40, w::MG, 1, 12, 4, 2, 4),
    et(44, w::LA, 0, 8, 4, 4, 0),
    et(48, w::MD, 1, 10, 4, 4, 1),
    et(52, w::MG, 3, 10, 4, 3, 2),
    et(56, w::LA, 3, 10, 4, 3, 3),
    et(60, w::AT, 1, 12, 4, 2, 0),
    et(90, w::LA, 3, 12, 4, 2, 3),
];

/// Arena enemy types 0..15 (`a3 $1637-$1697`).
pub const ARENA_ENEMIES: [EnemyType; 16] = [
    et(5, w::MG, 0, 6, 3, 4, 2),
    et(8, w::MG, 3, 4, 3, 4, 0),
    et(12, w::MG, 0, 4, 3, 4, 2),
    et(16, w::RR, 0, 4, 3, 3, 3),
    et(20, w::RR, 1, 6, 4, 4, 4),
    et(24, w::AT, 0, 6, 4, 4, 0),
    et(28, w::RR, 2, 6, 4, 3, 1),
    et(32, w::AT, 3, 6, 4, 4, 2),
    et(36, w::AT, 0, 8, 4, 4, 3),
    et(40, w::MG, 1, 8, 4, 3, 4),
    et(44, w::LA, 0, 8, 4, 4, 0),
    et(48, w::MG, 2, 10, 4, 4, 1),
    et(52, w::MG, 3, 10, 4, 3, 2),
    et(56, w::LA, 3, 10, 4, 3, 3),
    et(60, w::AT, 1, 12, 4, 3, 2),
    et(64, w::LA, 3, 12, 4, 3, 1),
];

/// Arena events: names and prizes (`ar.prg $A308`).
pub const ARENA_EVENT_NAMES: [&str; 8] = [
    "Amateur Night", "Division 5", "Division 10", "Division 15", "Division 20", "Unlimited", "Championship", "Practice",
];
pub const ARENA_PRIZES: [u32; 7] = [1500, 2200, 3000, 4000, 5500, 7000, 9500];

/// Building names as shown on screen.
pub const BUILDING_NAMES: [&str; 16] = [
    "Gold Cross", "Truck Stop", "Assembly Line", "Salvage Yard", "Weapon Shop", "Garage", "Arena", "AADA",
    "Joe's Bar", "F.B.I.", "Outlaw HQ", "Abandoned Building", "Origin Systems", "Pet Store", "Casino", "Hotel",
];
