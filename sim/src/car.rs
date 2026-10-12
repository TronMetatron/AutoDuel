//! Cars: components, derived statistics and the design check (`as.prg $A515`).
use crate::cargo::Cargo;
use crate::tables::{self, BODIES, PLANTS, TIRES, WEAPONS};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plant {
    pub kind: u8,
    pub dp: u8,
    pub max_dp: u8,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Tire {
    pub kind: u8,
    pub dp: u8,
    pub max_dp: u8,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Armor {
    pub pts: u8,
    pub max: u8,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Weapon {
    pub kind: u8,
    pub dp: u8,
    pub max_dp: u8,
    /// 0 Front, 1 Back, 2 Left, 3 Right
    pub mount: u8,
    /// rounds; ignored for laser and heavy rocket
    pub ammo: u8,
}

impl Weapon {
    pub fn new(kind: u8, mount: u8) -> Weapon {
        let i = &WEAPONS[kind as usize];
        Weapon { kind, dp: i.dp, max_dp: i.dp, mount, ammo: 0 }
    }
    pub fn info(&self) -> &'static tables::WeaponInfo {
        &WEAPONS[self.kind as usize]
    }
    pub fn uses_ammo(&self) -> bool {
        self.info().ammo_cost.is_some()
    }
}

/// Armor facings.
pub const F: usize = 0;
pub const B: usize = 1;
pub const L: usize = 2;
pub const R: usize = 3;
pub const U: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Car {
    pub name: String,
    /// 0..6, see [`tables::BODIES`]
    pub body: u8,
    /// 0 Light, 1 Standard, 2 Heavy, 3 Ex. Heavy
    pub chassis: u8,
    /// 0 Light, 1 Improved, 2 Heavy
    pub suspension: u8,
    pub plant: Plant,
    /// front-left, front-right, back-left, back-right
    pub tires: [Tire; 4],
    /// Front, Back, Left, Right, Under (no top armor)
    pub armor: [Armor; 5],
    pub weapons: [Option<Weapon>; 10],
    pub cargo: [Option<Cargo>; 3],
    /// city where the car is (stored there, or where it was left)
    pub city: u8,
    /// power units 0..=99
    pub battery: u8,
}

/// Why a design is refused ("I'm sorry, but that design just won't work!").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DesignErrors {
    pub money: bool,
    pub spaces: bool,
    pub weight: bool,
    pub power: bool,
}

impl DesignErrors {
    pub fn any(&self) -> bool {
        self.money || self.spaces || self.weight || self.power
    }
}

impl Car {
    /// The Assembly Line's starting design (`as $AAEE`): Subcompact, Light chassis and suspension,
    /// Small plant, Standard tires, nothing else.
    pub fn default_design(name: &str, city: u8) -> Car {
        let mut c = Car {
            name: name.to_string(),
            body: 0,
            chassis: 0,
            suspension: 0,
            plant: Plant { kind: 0, dp: 5, max_dp: 5 },
            tires: [Tire { kind: 0, dp: 4, max_dp: 4 }; 4],
            armor: [Armor { pts: 0, max: 0 }; 5],
            weapons: [None; 10],
            cargo: [None, None, None],
            city,
            battery: 99,
        };
        c.set_plant(0);
        c.set_tires(0);
        c
    }

    /// KILLER KART, the Amateur Night loaner (car record 9).
    pub fn killer_kart(city: u8) -> Car {
        let mut c = Car::default_design("KILLER KART", city);
        c.chassis = 1;
        c.suspension = 2;
        c.set_plant(1);
        c.set_tires(1);
        for (i, p) in [8u8, 6, 5, 5, 0].iter().enumerate() {
            c.armor[i] = Armor { pts: *p, max: *p };
        }
        let mut mg = Weapon::new(tables::w::MG, 0);
        mg.ammo = 20;
        c.weapons[0] = Some(mg);
        c
    }

    pub fn set_plant(&mut self, kind: u8) {
        let dp = PLANTS[kind as usize].dp;
        self.plant = Plant { kind, dp, max_dp: dp };
    }
    pub fn set_tires(&mut self, kind: u8) {
        let dp = TIRES[kind as usize].dp;
        self.tires = [Tire { kind, dp, max_dp: dp }; 4];
    }

    pub fn body_info(&self) -> &'static tables::BodyInfo {
        &BODIES[self.body as usize]
    }
    pub fn plant_info(&self) -> &'static tables::PlantInfo {
        &PLANTS[self.plant.kind as usize]
    }
    pub fn tire_info(&self) -> &'static tables::TireInfo {
        &TIRES[self.tires[0].kind as usize]
    }

    pub fn armor_points_designed(&self) -> u32 {
        self.armor.iter().map(|a| a.max as u32).sum()
    }

    /// Car value ("Cost:$"): body+chassis, suspension, plant, tires, designed armor, weapons.
    /// Ammunition is not included (original).
    pub fn value(&self) -> u32 {
        let b = self.body_info();
        let mut v = b.cost[self.chassis as usize] + b.susp_cost[self.suspension as usize] + self.plant_info().cost
            + self.tire_info().set_cost
            + self.armor_points_designed() * b.armor_cost;
        for w in self.weapons.iter().flatten() {
            v += w.info().price;
        }
        v
    }

    /// Cost of the ammunition currently loaded.
    pub fn ammo_value(&self) -> u32 {
        self.weapons.iter().flatten().map(|w| w.info().ammo_cost.unwrap_or(0) * w.ammo as u32).sum()
    }

    /// Total weight including ammunition and cargo.
    pub fn weight(&self) -> u32 {
        let b = self.body_info();
        let mut wt = b.weight + self.plant_info().weight + self.tire_info().set_weight + self.armor_points_designed() * b.armor_weight;
        for w in self.weapons.iter().flatten() {
            wt += w.info().weight + w.info().ammo_weight * w.ammo as u32;
        }
        for c in self.cargo.iter().flatten() {
            wt += c.weight;
        }
        wt
    }
    pub fn max_load(&self) -> u32 {
        self.body_info().max_load[self.chassis as usize]
    }
    pub fn weight_left(&self) -> i64 {
        self.max_load() as i64 - self.weight() as i64
    }
    pub fn spaces(&self) -> u32 {
        self.body_info().spaces
    }
    pub fn spaces_used(&self) -> u32 {
        let mut s = self.plant_info().spaces;
        for w in self.weapons.iter().flatten() {
            s += w.info().spaces;
        }
        for c in self.cargo.iter().flatten() {
            s += c.spaces;
        }
        s
    }
    pub fn spaces_left(&self) -> i64 {
        self.spaces() as i64 - self.spaces_used() as i64
    }
    /// Handling class 0..3: suspension index + 1 for cars (Subcompact..Luxury).
    pub fn handling_class(&self) -> u8 {
        self.suspension + self.body_info().hc_bonus
    }
    /// Acceleration class: floor(weight / power). 0 = 15 mph, 1 = 10, 2 = 5, >= 3 = can't move.
    pub fn accel_class(&self) -> u32 {
        self.weight() / self.plant_info().power
    }
    pub fn accel_mph(&self) -> Option<u32> {
        match self.accel_class() {
            0 => Some(15),
            1 => Some(10),
            2 => Some(5),
            _ => None,
        }
    }
    pub fn top_mph(&self) -> u32 {
        self.plant_info().top_mph
    }

    /// The Assembly Line check. `money` = cash available for car + ammunition.
    pub fn design_errors(&self, money: u32) -> DesignErrors {
        DesignErrors {
            money: (self.value() + self.ammo_value()) as u64 > money as u64,
            spaces: self.spaces_left() < 0,
            weight: self.weight_left() < 0,
            power: self.accel_class() >= 3,
        }
    }

    /// Would the car still be able to move with `extra` more pounds aboard?
    pub fn can_carry(&self, extra_weight: u32) -> bool {
        self.weight_left() >= extra_weight as i64 && (self.weight() + extra_weight) / self.plant_info().power < 3
    }

    pub fn free_weapon_slot(&self) -> Option<usize> {
        self.weapons.iter().position(|w| w.is_none())
    }
    pub fn free_cargo_slot(&self) -> Option<usize> {
        self.cargo.iter().position(|c| c.is_none())
    }
    pub fn has_damage(&self) -> bool {
        self.plant.dp < self.plant.max_dp
            || self.tires.iter().any(|t| t.dp < t.max_dp)
            || self.armor.iter().any(|a| a.pts < a.max)
            || self.weapons.iter().flatten().any(|w| w.dp < w.max_dp)
    }

    /// Cost to repair everything at a Garage (plant/weapons per DP, tires flat per tire, armor $50/pt).
    pub fn full_repair_cost(&self) -> u32 {
        let mut c = (self.plant.max_dp - self.plant.dp) as u32 * self.plant_info().repair_per_dp;
        for t in &self.tires {
            if t.dp < t.max_dp {
                c += TIRES[t.kind as usize].replace_cost;
            }
        }
        for a in &self.armor {
            c += (a.max - a.pts.min(a.max)) as u32 * tables::ARMOR_REPAIR_PER_PT;
        }
        for w in self.weapons.iter().flatten() {
            c += (w.max_dp - w.dp) as u32 * w.info().repair_per_dp;
        }
        c
    }

    /// The repair deduction the Salvage Yard uses (tires per lost DP instead of flat).
    pub fn salvage_repair_cost(&self) -> u32 {
        let mut c = (self.plant.max_dp - self.plant.dp) as u32 * self.plant_info().repair_per_dp;
        for t in &self.tires {
            c += (t.max_dp - t.dp) as u32 * TIRES[t.kind as usize].salvage_per_dp;
        }
        for a in &self.armor {
            c += (a.max - a.pts.min(a.max)) as u32 * tables::ARMOR_REPAIR_PER_PT;
        }
        for w in self.weapons.iter().flatten() {
            c += (w.max_dp - w.dp) as u32 * w.info().repair_per_dp;
        }
        c
    }

    /// Driving-engine handling base: driving skill / 2 + {24, 32, 66, 100}[class], −50 per flat tire.
    /// FIX: the original halved the BCD byte (skill 50 -> 40); this uses the real value.
    pub fn handling_base(&self, driving_skill: u8) -> i32 {
        let base = [24, 32, 66, 100][self.handling_class().min(3) as usize];
        let mut h = driving_skill as i32 / 2 + base;
        h -= 50 * self.tires.iter().filter(|t| t.dp == 0).count() as i32;
        h.max(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starter_and_killer_kart_match_the_original() {
        let c = Car::default_design("X", 7);
        assert_eq!(c.value(), 940);
        assert_eq!(c.weight(), 1620);
        assert_eq!(c.weight_left(), 450);
        assert_eq!(c.spaces_left(), 4);
        assert_eq!(c.handling_class(), 1);
        assert_eq!(c.accel_mph(), Some(10));
        let k = Car::killer_kart(7);
        assert_eq!(k.value(), 3414);
        assert_eq!(k.weight_left(), 130);
        assert_eq!(k.spaces_left(), 2);
        assert_eq!(k.handling_class(), 3);
        assert_eq!(k.accel_mph(), Some(15));
    }
}
