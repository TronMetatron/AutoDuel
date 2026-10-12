//! Whole game flows across the modules, played through the real engines and shop rules:
//! Amateur Night (win, flee, die), a courier job driven down a real road (on time and late),
//! death with and without a clone, the story chain, and saving and loading.
use cardul_sim::car::Car;
use cardul_sim::cargo::CargoKind;
use cardul_sim::combat::autopilot::{ArenaBot, Autopilot};
use cardul_sim::combat::{ArenaResult, Battle, BattleEnd, BattleKind, DriveInput, ENEMY_SLOTS};
use cardul_sim::game::{DeathOutcome, JobOffer};
use cardul_sim::shops::{aada, arena, goldcross, outlaw, rumors, salvage};
use cardul_sim::story::{password_matches, QUESTS};
use cardul_sim::world::Building;
use cardul_sim::{Game, GameData};

/// Run a battle with an input policy until it ends (or `max` frames).
fn run(g: &mut Game, d: &GameData, b: &mut Battle, max: u32, mut policy: impl FnMut(&Battle, &Game) -> DriveInput) -> Option<BattleEnd> {
    for _ in 0..max {
        let inp = policy(b, g);
        if let Some(e) = b.step(g, d, &inp) {
            return Some(e);
        }
    }
    None
}

/// Drive road `road` from `from` with no enemies on it, like the city screen does it.
fn drive_road(g: &mut Game, d: &GameData, road: u8, from: u8) -> BattleEnd {
    g.roads[road as usize].slots = vec![None; ENEMY_SLOTS];
    let mut b = Battle::new(g, d, BattleKind::Road { road, from_city: from });
    let mut ap = Autopilot::for_battle(&b, d);
    let end = run(g, d, &mut b, 20_000, |b, g| ap.input(b, g)).expect("the road trip ends");
    if let BattleEnd::Arrived { city, .. } = end {
        // what the drive screen does on arrival
        g.driver.city = city;
    }
    end
}

// ------------------------------------------------------------------------------- arena

#[test]
fn amateur_night_win_pays_and_returns_the_loaner() {
    let d = cardul_sim::need_data!();
    let mut g = Game::new("Ann", false, &d, 4);
    assert_eq!(arena::check_entry(&g, arena::AMATEUR_NIGHT), Ok(arena::Entry::Ok));
    arena::start_event(&mut g, arena::AMATEUR_NIGHT);
    assert_eq!(g.car().unwrap().name, "KILLER KART");
    let mut b = Battle::new(&mut g, &d, BattleKind::Arena { event: 0 });
    assert_eq!(b.opponents_left, 5);
    assert!(b.enemies.iter().flatten().all(|e| e.kind == 0));
    let mut bot = ArenaBot::new(&b, &d);
    let end = run(&mut g, &d, &mut b, 20_000, |b, g| bot.input(b, g, &d));
    assert_eq!(end, Some(BattleEnd::Arena(ArenaResult::Won)));
    assert_eq!(b.opponents_left, 0);
    let out = arena::finish_event(&mut g, 0, ArenaResult::Won);
    assert_eq!(out, arena::Outcome::Won { prize: 1500 });
    let dr = &g.driver;
    assert_eq!((dr.money, dr.prestige, dr.driving), (3500, 2, 1));
    assert!(!dr.loaner && !dr.car_with_you && g.loaner.is_none() && g.car().is_none());
    assert!(dr.day_used);
    assert_eq!(arena::check_entry(&g, 0), Err(arena::Refusal::Closed));
}

#[test]
fn fleeing_amateur_night_costs_prestige() {
    let d = cardul_sim::need_data!();
    let mut g = Game::new("Ann", false, &d, 5);
    g.driver.prestige = 3;
    arena::start_event(&mut g, 0);
    let mut b = Battle::new(&mut g, &d, BattleKind::Arena { event: 0 });
    // straight back out through the gate behind you
    let end = run(&mut g, &d, &mut b, 200, |_, _| DriveInput { dy: 1, ..Default::default() });
    assert_eq!(end, Some(BattleEnd::Arena(ArenaResult::Fled)));
    assert_eq!(arena::finish_event(&mut g, 0, ArenaResult::Fled), arena::Outcome::Lost);
    assert_eq!((g.driver.prestige, g.driver.money), (2, 2000));
    assert!(g.loaner.is_none() && !g.driver.loaner && g.driver.day_used);
}

#[test]
fn dying_in_amateur_night_without_a_clone_ends_the_driver() {
    let d = cardul_sim::need_data!();
    let mut g = Game::new("Ann", false, &d, 6);
    arena::start_event(&mut g, 0);
    let mut b = Battle::new(&mut g, &d, BattleKind::Arena { event: 0 });
    // drive up to the first opponent and sit there with the gun silent until shot dead
    let mut bot = ArenaBot::new(&b, &d);
    bot.pilot.shoot = false;
    let end = run(&mut g, &d, &mut b, 20_000, |b, g| {
        let mut i = bot.input(b, g, &d);
        i.fire = false;
        i
    });
    assert_eq!(end, Some(BattleEnd::Died));
    assert!(b.is_dying() || g.driver.health == 0);
    assert_eq!(g.kill_driver(), DeathOutcome::GameOver);
    assert!(!g.driver.alive && g.loaner.is_none() && !g.driver.loaner);
}

// ------------------------------------------------------------------------------- courier

/// A New York driver with a starter car and a courier job to Philadelphia on today's board.
fn courier_setup(d: &GameData) -> (Game, aada::JobView) {
    let mut g = Game::new("Cora", false, d, 8);
    g.driver.prestige = 10;
    g.add_car(Car::default_design("Hauler", 7));
    // a light cargo going to a Philadelphia building that takes deliveries
    let cargo = d
        .cargo
        .iter()
        .position(|c| [Building::Aada as u8, Building::PetStore as u8, Building::TruckStop as u8].contains(&c.dest_building) && c.weight <= 200 && c.spaces <= 3)
        .expect("a cargo for Philadelphia") as u8;
    g.aada.date = Some(g.driver.date);
    g.aada.offers = vec![Some(JobOffer { cargo, dest_city: 11, days: 6 }), None, None];
    let job = aada::accept(&mut g, d, 0).expect("the job is accepted");
    assert!(g.driver.day_used);
    assert_eq!(job.dest_city, 11);
    assert_eq!(job.due, g.driver.date.plus_days(6));
    (g, job)
}

#[test]
fn courier_job_driven_to_philadelphia_and_paid() {
    let d = cardul_sim::need_data!();
    let (mut g, job) = courier_setup(&d);
    let (money, prestige, date) = (g.driver.money, g.driver.prestige, g.driver.date);
    let end = drive_road(&mut g, &d, 10, 7);
    assert_eq!(end, BattleEnd::Arrived { city: 11, road: 10, on_foot: false });
    // the trip took a day (and gave driving and prestige +1)
    assert_eq!(g.driver.date, date.next_day());
    assert!(!g.driver.day_used);
    // the wrong building takes nothing; the destination pays
    assert!(g.deliver_cargo(Building::Bar, &d).lines.is_empty());
    let rep = g.deliver_cargo(job.dest, &d);
    assert_eq!(rep.lines, vec![format!("Thanks for the package! (${})", job.pay)]);
    assert_eq!(g.driver.money, money + job.pay);
    assert_eq!(g.driver.prestige, prestige + 1 + 2);
    assert!(g.car().unwrap().cargo.iter().all(|c| c.is_none()));
}

#[test]
fn late_courier_delivery_pays_nothing() {
    let d = cardul_sim::need_data!();
    let (mut g, job) = courier_setup(&d);
    let (money, prestige) = (g.driver.money, g.driver.prestige);
    drive_road(&mut g, &d, 10, 7);
    g.pass_days(6); // the trip took a day: now one day past the deadline
    let rep = g.deliver_cargo(job.dest, &d);
    assert_eq!(rep.lines, vec![format!("{} is late. No pay.", job.name)]);
    assert_eq!(g.driver.money, money);
    assert_eq!(g.driver.prestige, prestige + 1 - 2);
}

// ------------------------------------------------------------------------------- death

#[test]
fn road_death_with_a_clone_wakes_at_its_gold_cross() {
    let d = cardul_sim::need_data!();
    let mut g = Game::new("Dee", false, &d, 9);
    g.driver.money = 20_000;
    g.driver.prestige = 30;
    g.driver.city = 5;
    goldcross::create_clone(&mut g).unwrap();
    // a car stored in Boston and the one he drives out of New York
    g.add_car(Car::default_design("Stored", 5));
    g.store_current_car();
    g.driver.city = 7;
    g.add_car(Car::default_design("Doomed", 7));
    let money = g.driver.money;
    let date = g.driver.date;
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 10, from_city: 7 });
    g.driver.health = 0;
    let end = run(&mut g, &d, &mut b, 100, |_, _| DriveInput::default());
    assert_eq!(end, Some(BattleEnd::Died));
    assert_eq!(g.kill_driver(), DeathOutcome::Cloned { city: 5 });
    let dr = &g.driver;
    assert_eq!((dr.city, dr.money, dr.prestige, dr.health), (5, money, 26, 3));
    assert!(dr.alive && dr.clone.is_none() && !dr.car_with_you);
    assert_eq!(dr.date, date.next_day());
    let names: Vec<_> = g.owned_cars().map(|(_, c)| c.name.clone()).collect();
    assert_eq!(names, vec!["Stored".to_string()], "the car he died in is gone, the stored one stays");
}

#[test]
fn death_in_town_keeps_the_stored_car_and_the_parked_one() {
    let d = cardul_sim::need_data!();
    let mut g = Game::new("Ed", false, &d, 10);
    g.driver.clone = Some(g.driver.snapshot(7));
    g.add_car(Car::default_design("Parked", 4));
    g.driver.city = 4;
    assert!(matches!(g.kill_driver_in_town(), DeathOutcome::Cloned { city: 7 }));
    let c = g.cars[0].as_ref().expect("the parked car stays in Albany");
    assert_eq!(c.city, 4);
    assert!(!g.driver.car_with_you);
}

#[test]
fn death_without_a_clone_is_final() {
    let d = cardul_sim::need_data!();
    let mut g = Game::new("Fay", false, &d, 11);
    g.add_car(Car::default_design("Car", 7));
    assert_eq!(g.kill_driver(), DeathOutcome::GameOver);
    assert!(!g.driver.alive);
}

// ------------------------------------------------------------------------------- story

/// Every quest: the giver hands the item over after the password, at the right prestige, and
/// delivering it at its destination pays and gives +12 (the brain tape ends the game).
#[test]
fn the_story_chain_through_the_quest_rules() {
    let d = cardul_sim::need_data!();
    let mut g = Game::new("Hero", true, &d, 12);
    g.add_car(Car::default_design("Hero's car", 7));
    for (q, info) in QUESTS.iter().enumerate() {
        assert!(password_matches(&info.password.to_lowercase(), q));
        g.driver.prestige = info.prestige;
        g.driver.city = info.giver_city;
        // the giver wants the password only now
        let wants = match q {
            0 => arena::decoy_password_due(&g),
            1 => goldcross::wants_password(&g),
            2 => rumors::mob_password_due(&g),
            3 => salvage::wants_password(&g),
            _ => outlaw::password_due(&g),
        };
        assert!(wants, "quest {}", q);
        g.driver.prestige = info.prestige - 1;
        let not_yet = match q {
            0 => arena::decoy_password_due(&g),
            1 => goldcross::wants_password(&g),
            2 => rumors::mob_password_due(&g),
            3 => salvage::wants_password(&g),
            _ => outlaw::password_due(&g),
        };
        assert!(!not_yet, "quest {} below its prestige", q);
        g.driver.prestige = info.prestige;
        assert_eq!(rumors::hand_over(&mut g, q), rumors::Handover::Loaded);
        assert_eq!(g.driver.quest.carrying, Some(q as u8));
        let item = g.car().unwrap().cargo.iter().flatten().find(|c| c.kind == CargoKind::Quest(q as u8)).unwrap().clone();
        assert_eq!(item.dest, Some((info.dest, info.dest_city)));
        // deliver
        g.driver.city = info.dest_city;
        let money = g.driver.money;
        let prestige = g.driver.prestige;
        let rep = g.deliver_cargo(info.dest, &d);
        assert_eq!(rep.lines.len(), 1, "quest {}", q);
        assert_eq!(g.driver.money, money + info.pay);
        assert!(g.driver.quest.done[q]);
        assert_eq!(g.driver.quest.carrying, None);
        if q < 4 {
            assert_eq!(g.driver.prestige, (prestige + 12).min(99));
            assert!(!rep.won);
        } else {
            assert!(rep.won && g.driver.quest.won);
        }
    }
    // after the ending the driver plays on: no more quest rumors, the HQ is just a building
    assert_eq!(outlaw::title(&g), "An Abandoned Building");
    assert!(!outlaw::password_due(&g));
    assert!(g.driver.alive);
}

// ------------------------------------------------------------------------------- saves

#[test]
fn save_and_load_round_trip() {
    let d = cardul_sim::need_data!();
    let (mut g, _) = courier_setup(&d);
    g.driver.clone = Some(g.driver.snapshot(5));
    let dir = std::env::temp_dir().join(format!("cardul-test-{}", std::process::id()));
    let path = dir.join(g.save_file_name());
    g.save_to(&path).unwrap();
    let back = Game::load_from(&path).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), serde_json::to_string(&g).unwrap());
    let listed = cardul_sim::game::list_saves(&dir);
    assert_eq!(listed.len(), 1);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}

/// A save written by the first release still loads (new fields must be `#[serde(default)]`).
#[test]
fn version_1_saves_still_load() {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/save_v1.json");
    let g = Game::load_from(&p).expect("old save loads");
    assert_eq!(g.driver.name, "Old Timer");
    assert!(g.car().is_some());
}

#[test]
fn a_save_made_during_amateur_night_returns_the_loaner() {
    let d = cardul_sim::need_data!();
    let mut g = Game::new("Gil", false, &d, 13);
    arena::start_event(&mut g, 0);
    let dir = std::env::temp_dir().join(format!("cardul-test-loaner-{}", std::process::id()));
    let path = dir.join("gil.json");
    g.save_to(&path).unwrap();
    let back = Game::load_from(&path).unwrap();
    assert!(!back.driver.loaner && back.loaner.is_none() && !back.driver.car_with_you && back.car().is_none());
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&dir);
}
