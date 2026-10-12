//! Whole road trips and arena fights driven by the autopilot through the real engine.
use cardul_sim::car::Car;
use cardul_sim::combat::autopilot::{ArenaBot, Autopilot};
use cardul_sim::combat::{ArenaResult, Battle, BattleEnd, BattleKind, ENEMY_SLOTS};
use cardul_sim::{Game, GameData};

/// Drive road `road` from `from` with no enemies; returns the end and the frames used.
fn drive_empty(d: &GameData, road: u8, from: u8, seed: u32) -> (Option<BattleEnd>, u32) {
    let mut g = Game::new("Auto", false, d, seed);
    g.driver.city = from;
    g.add_car(Car::default_design("Auto", from));
    g.roads[road as usize].slots = vec![None; ENEMY_SLOTS];
    let mut b = Battle::new(&mut g, d, BattleKind::Road { road, from_city: from });
    b.enemies = vec![None; ENEMY_SLOTS];
    let mut ap = Autopilot::for_battle(&b, d);
    for f in 0..20_000 {
        let inp = ap.input(&b, &g);
        if let Some(e) = b.step(&mut g, d, &inp) {
            return (Some(e), f);
        }
    }
    eprintln!("road {} from {}: stuck at {:?}", road, from, (b.player.x, b.player.y));
    (None, 20_000)
}

#[test]
fn every_road_can_be_driven_end_to_end() {
    let d = cardul_sim::need_data!();
    let mut failed = vec![];
    for road in 0..d.roads.len() as u8 {
        for e in d.roads[road as usize].exits.clone() {
            let (end, frames) = drive_empty(&d, road, e.city, 7);
            let other = d.roads[road as usize].exits.iter().find(|x| x.city != e.city).unwrap().city;
            match end {
                Some(BattleEnd::Arrived { city, road: r, on_foot: false }) if city == other && r == road => {
                    eprintln!("road {:2} from {:2} -> {:2}: {} frames", road, e.city, city, frames)
                }
                other => failed.push((road, e.city, other)),
            }
        }
    }
    assert!(failed.is_empty(), "{:?}", failed);
}

#[test]
fn amateur_night_can_be_won_without_help() {
    let d = cardul_sim::need_data!();
    let mut wins = 0;
    let mut results = vec![];
    for seed in 1..=8u32 {
        let mut g = Game::new("Bot", false, &d, seed);
        cardul_sim::shops::arena::start_event(&mut g, 0);
        let mut b = Battle::new(&mut g, &d, BattleKind::Arena { event: 0 });
        let mut bot = ArenaBot::new(&b, &d);
        let mut end = None;
        for _ in 0..20_000 {
            let inp = bot.input(&b, &g, &d);
            if let Some(e) = b.step(&mut g, &d, &inp) {
                end = Some(e);
                break;
            }
        }
        results.push((end, b.opponents_left, g.driver.health));
        if end == Some(BattleEnd::Arena(ArenaResult::Won)) {
            wins += 1;
        }
    }
    eprintln!("{:?}", results);
    assert!(wins >= 4, "won {} of 8: {:?}", wins, results);
}
