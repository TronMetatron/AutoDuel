//! AADA chapter office (`aa.prg`): the Road Status Bulletin and courier jobs.
//!
//! Three jobs are offered per day ([`crate::game::AadaBoard`]). A job is a random cargo type
//! (`random(99)`) going to a random city that has the cargo's destination building (not this
//! city), due in 5..10 days. Pay depends on value, distance and speed:
//!
//! ```text
//! pay = max(1, value / 100) × (11 − days) × distance[here][dest] × 10, clamped to $50..$15,000
//! ```
//!
//! FIX: the original clamp compared only the hundreds, so $15,010-$15,090 slipped through.
use crate::cargo::{Cargo, CargoKind};
use crate::data::GameData;
use crate::date::Date;
use crate::game::{Game, JobOffer};
use crate::rng::Rng;
use crate::world::Building;

pub const JOBS_PER_DAY: usize = 3;
pub const PAY_MIN: u32 = 50;
pub const PAY_MAX: u32 = 15_000;
/// You may carry at most this many payloads (one per cargo slot).
pub const MAX_PAYLOADS: usize = 3;

/// Courier pay for a cargo `value` (dollars), `days` to deliver and the AADA `distance`.
pub fn courier_pay(value: u32, days: u32, distance: u32) -> u32 {
    let base = (value / 100).max(1);
    let pay = base * 11u32.saturating_sub(days) * distance * 10;
    pay.clamp(PAY_MIN, PAY_MAX)
}

/// Days to deliver: 5 + `aada_rand(5)`. The original's private generator reduces a BCD sum into
/// 1..5 and only gives 0 when the sum is 00, so 5 days is rare (about 2%) and 6..10 are even.
pub fn roll_days(rng: &mut Rng) -> u8 {
    if rng.upto(49) == 0 { 5 } else { 6 + rng.upto(4) as u8 }
}

/// Draw a new job for `here` (`aa $A288`): cargo `random(99)`, a random destination city with
/// that building; if it is this city, start again with a new cargo.
pub fn generate_offer(rng: &mut Rng, data: &GameData, here: u8) -> JobOffer {
    loop {
        let cargo = rng.upto(data.cargo.len() as u32 - 1) as usize;
        let Some(b) = Building::from_u8(data.cargo[cargo].dest_building) else { continue };
        let cities = data.cities_with(b);
        if cities.is_empty() {
            continue;
        }
        let city = cities[rng.upto(cities.len() as u32 - 1) as usize];
        if city == here {
            continue;
        }
        return JobOffer { cargo: cargo as u8, dest_city: city, days: roll_days(rng) };
    }
}

/// Make sure today's board is filled (`aada_list_jobs`): a new board each day, and a taken
/// offer is replaced when the list is shown again.
pub fn refresh_board(game: &mut Game, data: &GameData) {
    let today = game.driver.date;
    let here = game.driver.city;
    if game.aada.date != Some(today) {
        game.aada.date = Some(today);
        game.aada.offers = vec![None; JOBS_PER_DAY];
    }
    game.aada.offers.resize(JOBS_PER_DAY, None);
    for i in 0..JOBS_PER_DAY {
        if game.aada.offers[i].is_none() {
            let o = generate_offer(&mut game.rng, data, here);
            game.aada.offers[i] = Some(o);
        }
    }
}

/// Everything shown about a job, and what goes into the cargo slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobView {
    pub cargo: u8,
    pub name: String,
    pub dest: Building,
    pub dest_city: u8,
    pub weight: u32,
    pub spaces: u32,
    pub due: Date,
    pub value: u32,
    pub pay: u32,
}

impl JobView {
    pub fn new(offer: &JobOffer, data: &GameData, here: u8, today: Date) -> JobView {
        let ct = &data.cargo[offer.cargo as usize];
        JobView {
            cargo: offer.cargo,
            name: ct.name.clone(),
            dest: Building::from_u8(ct.dest_building).unwrap_or(Building::Aada),
            dest_city: offer.dest_city,
            weight: ct.weight,
            spaces: ct.spaces,
            due: today.plus_days(offer.days as u32),
            value: ct.value,
            pay: courier_pay(ct.value, offer.days as u32, data.distance(here, offer.dest_city)),
        }
    }
    pub fn to_cargo(&self) -> Cargo {
        Cargo {
            kind: CargoKind::Job(self.cargo),
            name: self.name.clone(),
            dest: Some((self.dest, self.dest_city)),
            weight: self.weight,
            spaces: self.spaces,
            due: Some(self.due),
            value: self.value,
            pay: self.pay,
        }
    }
}

/// Why a job is refused (`aa $A65A`), checked in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    NoCar,
    TooManyPayloads,
    NoSpace,
    TooHeavy,
    NeedExperience,
    LightlyArmed,
}

impl Refusal {
    pub fn message(self) -> &'static str {
        match self {
            Refusal::NoCar => "Is this a joke or something?  Where's\nyour car?",
            Refusal::TooManyPayloads => {
                "I'm sorry, but AADA regulations\nprohibit carrying more than three\npayloads at a time."
            }
            Refusal::NoSpace => "You don't have enough space for that\njob.",
            Refusal::TooHeavy => "That payload is too heavy for your\ncar.",
            Refusal::NeedExperience => "I'm sorry, but we would really\nprefer someone with a little more\nexperience.",
            Refusal::LightlyArmed => "I'm sorry, but your car is too\nlightly armed for a payload of this\nvalue.",
        }
    }
    /// The first two end the job list; the others let you pick another job.
    pub fn back_to_menu(self) -> bool {
        matches!(self, Refusal::NoCar | Refusal::TooManyPayloads)
    }
}

/// The acceptance checks, in the original order.
pub fn check_accept(game: &Game, job: &JobView) -> Result<(), Refusal> {
    let car = match game.car_here() {
        Some(c) if !game.driver.loaner => c,
        _ => return Err(Refusal::NoCar),
    };
    if car.free_cargo_slot().is_none() {
        return Err(Refusal::TooManyPayloads);
    }
    if car.spaces_left() < job.spaces as i64 {
        return Err(Refusal::NoSpace);
    }
    if car.weight_left() < job.weight as i64 {
        return Err(Refusal::TooHeavy);
    }
    // load class: the car must still be able to move with the extra weight
    if !car.can_carry(job.weight) {
        return Err(Refusal::TooHeavy);
    }
    if (game.driver.prestige as u32) * 3 < job.pay / 100 {
        return Err(Refusal::NeedExperience);
    }
    if car.value() < job.pay {
        return Err(Refusal::LightlyArmed);
    }
    Ok(())
}

/// Accept today's offer `i`: load the cargo, mark the offer taken and use up the day.
/// "Very good. I'll have it loaded immediately."
pub fn accept(game: &mut Game, data: &GameData, i: usize) -> Result<JobView, Refusal> {
    let Some(Some(offer)) = game.aada.offers.get(i).cloned() else { return Err(Refusal::TooManyPayloads) };
    let job = JobView::new(&offer, data, game.driver.city, game.driver.date);
    check_accept(game, &job)?;
    let car = game.car_here_mut().expect("checked");
    let slot = car.free_cargo_slot().expect("checked");
    car.cargo[slot] = Some(job.to_cargo());
    game.aada.offers[i] = None;
    game.driver.day_used = true;
    Ok(job)
}

/// One line of the Road Status Bulletin: destination city and traffic word.
pub fn road_bulletin(game: &Game, data: &GameData, city: u8) -> Vec<(u8, &'static str)> {
    data.city(city)
        .exits
        .iter()
        .map(|e| (e.dest_city, game.roads.get(e.road as usize).map(|r| r.traffic_word()).unwrap_or("not")))
        .collect()
}

/// Clue given on entering the AADA (`aa $A11E`): prestige ≥ 40 and the clone-heart quest not
/// taken, in Buffalo ("White") or Pittsburgh ("Whale").
/// FIX: the Pittsburgh clue sent you to the Washington Truck Stop (a loop); it now names the
/// New York Gold Cross, where the password is wanted.
pub fn heart_clue(game: &Game) -> Option<&'static str> {
    let d = &game.driver;
    if d.prestige < 40 || d.quest.taken[1] {
        return None;
    }
    match d.city {
        2 => Some("Gold Cross needs a reliable driver.\nThey know more in Pittsburgh.\nRemember the word \"White\"."),
        9 => Some("The clone heart is in New York. Ask\nat the Gold Cross there.\nRemember \"Whale\"."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::Car;

    #[test]
    fn pay_formula_and_clamps() {
        // the worked example: value $5,000, 6 days, distance 4
        assert_eq!(courier_pay(5000, 6, 4), 10_000);
        // value 0 counts as 1 (and the minimum applies)
        assert_eq!(courier_pay(0, 10, 2), PAY_MIN);
        assert_eq!(courier_pay(200, 10, 2), PAY_MIN);
        assert_eq!(courier_pay(100, 5, 1), 60);
        // the top clamp, including the original's 15,010..15,090 hole
        assert_eq!(courier_pay(10_000, 5, 10), PAY_MAX);
        assert_eq!(courier_pay(4300, 6, 7), PAY_MAX); // 15,050 in the original
        assert_eq!(courier_pay(15_000, 10, 10), 15_000);
        assert_eq!(courier_pay(1500, 10, 10), 1500);
    }

    #[test]
    fn deadlines_are_5_to_10_days() {
        let mut rng = Rng::new(99);
        let mut seen = [0u32; 11];
        for _ in 0..5000 {
            let d = roll_days(&mut rng);
            assert!((5..=10).contains(&d));
            seen[d as usize] += 1;
        }
        assert!(seen[5] > 0 && seen[5] < 250);
        assert!(seen[6..=10].iter().all(|&n| n > 800));
    }

    #[test]
    fn offers_never_target_this_city() {
        let d = crate::need_data!();
        let mut rng = Rng::new(5);
        for here in 0..16u8 {
            for _ in 0..200 {
                let o = generate_offer(&mut rng, &d, here);
                assert_ne!(o.dest_city, here);
                let b = Building::from_u8(d.cargo[o.cargo as usize].dest_building).unwrap();
                assert!(d.city_has(o.dest_city, b));
                assert!((5..=10).contains(&o.days));
            }
        }
    }

    #[test]
    fn board_refreshes_daily_and_replaces_taken() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        refresh_board(&mut g, &d);
        let first = format!("{:?}", g.aada.offers);
        refresh_board(&mut g, &d);
        assert_eq!(first, format!("{:?}", g.aada.offers));
        g.aada.offers[1] = None;
        refresh_board(&mut g, &d);
        assert!(g.aada.offers.iter().all(|o| o.is_some()));
        g.next_day();
        assert_eq!(g.aada.date, None);
        refresh_board(&mut g, &d);
        assert_eq!(g.aada.date, Some(g.driver.date));
    }

    fn job(pay: u32, weight: u32, spaces: u32) -> JobView {
        JobView {
            cargo: 0,
            name: "Test".into(),
            dest: Building::Fbi,
            dest_city: 7,
            weight,
            spaces,
            due: Date::start().plus_days(6),
            value: 1000,
            pay,
        }
    }

    #[test]
    fn acceptance_checks_in_order() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        assert_eq!(check_accept(&g, &job(50, 0, 0)), Err(Refusal::NoCar));
        g.add_car(Car::default_design("C", 7)); // value 940, 450 lb and 4 spaces left
        g.driver.prestige = 0;
        assert_eq!(check_accept(&g, &job(90, 0, 0)), Ok(()));
        assert_eq!(check_accept(&g, &job(50, 0, 5)), Err(Refusal::NoSpace));
        assert_eq!(check_accept(&g, &job(50, 451, 1)), Err(Refusal::TooHeavy));
        // prestige 0 allows pay/100 = 0 only; prestige 3 allows up to $999
        assert_eq!(check_accept(&g, &job(100, 0, 0)), Err(Refusal::NeedExperience));
        g.driver.prestige = 3;
        assert_eq!(check_accept(&g, &job(999, 0, 0)), Err(Refusal::LightlyArmed));
        assert_eq!(check_accept(&g, &job(940, 0, 0)), Ok(()));
        // power: a car that would be too slow with the extra load is "too heavy"
        let mut heavy = Car::default_design("H", 7);
        heavy.body = 6; // a van with an extra-heavy chassis carries more than the small plant can pull
        heavy.chassis = 3;
        g.cars[0] = Some(heavy);
        let left = g.car().unwrap().weight_left() as u32;
        assert!(left > 1500);
        assert_eq!(check_accept(&g, &job(50, 1500, 0)), Err(Refusal::TooHeavy));
        // three payloads at most
        let c = g.car_mut().unwrap();
        for s in 0..3 {
            c.cargo[s] = Some(job(50, 0, 0).to_cargo());
        }
        assert_eq!(check_accept(&g, &job(50, 0, 0)), Err(Refusal::TooManyPayloads));
        g.driver.car_with_you = false;
        assert_eq!(check_accept(&g, &job(50, 0, 0)), Err(Refusal::NoCar));
    }

    #[test]
    fn accepting_loads_the_cargo() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        g.add_car(Car::default_design("C", 7));
        g.driver.prestige = 99;
        refresh_board(&mut g, &d);
        // find an offer this car can take
        let ok = (0..3).find(|&i| {
            let o = g.aada.offers[i].clone().unwrap();
            check_accept(&g, &JobView::new(&o, &d, 7, g.driver.date)).is_ok()
        });
        if let Some(i) = ok {
            let j = accept(&mut g, &d, i).unwrap();
            assert!(g.driver.day_used);
            assert!(g.aada.offers[i].is_none());
            let c = g.car().unwrap().cargo.iter().flatten().next().unwrap().clone();
            assert_eq!(c.pay, j.pay);
            assert_eq!(c.kind, CargoKind::Job(j.cargo));
            assert_eq!(c.due, Some(j.due));
        }
    }

    #[test]
    fn clue_conditions() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        g.driver.city = 2;
        assert!(heart_clue(&g).is_none());
        g.driver.prestige = 40;
        assert!(heart_clue(&g).unwrap().contains("White"));
        g.driver.city = 9;
        assert!(heart_clue(&g).unwrap().contains("Whale"));
        g.driver.quest.taken[1] = true;
        assert!(heart_clue(&g).is_none());
    }
}
