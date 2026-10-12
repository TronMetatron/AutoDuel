//! Whole-battle tests: every road and arena event runs, a scripted driver can win, and
//! the bookkeeping (days, write-back, hit sides, salvage) follows the rules.
use super::hits::HitFrom;
use super::motion::stick_heading;
use super::*;
use crate::car::{Car, Weapon};
use crate::rng::Rng;
use crate::tables::w;

/// A cheat driver with a well-armed car.
fn armed_game(d: &GameData, seed: u32) -> Game {
    let mut g = Game::new("Test", true, d, seed);
    // a van: room for every kind of weapon and some salvage
    let mut c = Car::killer_kart(7);
    c.body = 6;
    c.chassis = 3;
    c.set_plant(2);
    let mut mg = Weapon::new(w::MG, 0);
    mg.ammo = 20;
    c.weapons[0] = Some(mg);
    let mut rl = Weapon::new(w::RL, 0);
    rl.ammo = 10;
    c.weapons[1] = Some(rl);
    let mut md = Weapon::new(w::MD, 1);
    md.ammo = 5;
    c.weapons[2] = Some(md);
    let mut ss = Weapon::new(w::SS, 1);
    ss.ammo = 5;
    c.weapons[3] = Some(ss);
    let mut oj = Weapon::new(w::OJ, 1);
    oj.ammo = 5;
    c.weapons[4] = Some(oj);
    c.weapons[5] = Some(Weapon::new(w::LA, 0));
    c.weapons[6] = Some(Weapon::new(w::HR, 0));
    let mut ps = Weapon::new(w::PS, 2);
    ps.ammo = 5;
    c.weapons[7] = Some(ps);
    let mut rr = Weapon::new(w::RR, 3);
    rr.ammo = 10;
    c.weapons[8] = Some(rr);
    let mut sd = Weapon::new(w::SD, 1);
    sd.ammo = 5;
    c.weapons[9] = Some(sd);
    for a in c.armor.iter_mut() {
        a.pts = 20;
        a.max = 20;
    }
    assert!(c.weight_left() > 600 && c.accel_class() < 3 && c.spaces_left() >= 3, "{} {}", c.weight_left(), c.spaces_left());
    g.add_car(c);
    g
}

fn random_input(rng: &mut Rng) -> DriveInput {
    DriveInput {
        dx: rng.upto(2) as i8 - 1,
        dy: rng.upto(2) as i8 - 1,
        fire: rng.coin(),
        next_weapon: rng.upto(20) == 0,
        select: if rng.upto(30) == 0 { Some(rng.upto(9) as u8) } else { None },
        toggle_foot: rng.upto(150) == 0,
    }
}

#[test]
fn every_road_runs_idle_and_random() {
    let d = crate::need_data!();
    for road in 0..d.roads.len() as u8 {
        for e in d.roads[road as usize].exits.clone() {
            // idle: nothing panics, enemies keep their distance or attack
            let mut g = armed_game(&d, 100 + road as u32);
            g.driver.city = e.city;
            let mut b = Battle::new(&mut g, &d, BattleKind::Road { road, from_city: e.city });
            for _ in 0..2000 {
                if b.step(&mut g, &d, &DriveInput::default()).is_some() {
                    break;
                }
            }
            // random driving and shooting
            let mut g = armed_game(&d, 200 + road as u32);
            let mut b = Battle::new(&mut g, &d, BattleKind::Road { road, from_city: e.city });
            let mut rng = Rng::new(7 + road as u32);
            for _ in 0..2000 {
                if b.step(&mut g, &d, &random_input(&mut rng)).is_some() {
                    break;
                }
            }
        }
    }
}

#[test]
fn every_arena_event_runs() {
    let d = crate::need_data!();
    for event in 0..8u8 {
        for random in [false, true] {
            let mut g = armed_game(&d, 300 + event as u32);
            let mut b = Battle::new(&mut g, &d, BattleKind::Arena { event });
            let expected = d.arena.events[event as usize].slots.iter().filter(|s| s.kind.is_some()).count() as u32;
            assert_eq!(b.opponents_left, expected);
            let mut rng = Rng::new(event as u32 + 1);
            for _ in 0..2000 {
                let inp = if random { random_input(&mut rng) } else { DriveInput::default() };
                if b.step(&mut g, &d, &inp).is_some() {
                    break;
                }
            }
        }
    }
}

/// Steer toward a point: the joystick direction of the 8-way bearing.
fn stick_toward(from: (i32, i32), to: (i32, i32)) -> (i8, i8) {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let sx = if dx > 2 { 1 } else if dx < -2 { -1 } else { 0 };
    let sy = if dy > 2 { 1 } else if dy < -2 { -1 } else { 0 };
    (sx, sy)
}

/// A simple arena bot: drive at the nearest opponent and fire when he is close; when
/// stuck against a wall, wander off in a random direction for a while; when all are
/// down, head for the bottom gate.
struct Bot {
    rng: Rng,
    last: (i32, i32),
    still: u32,
    wander: u32,
    dir: (i8, i8),
}

impl Bot {
    fn new(seed: u32) -> Bot {
        Bot { rng: Rng::new(seed), last: (0, 0), still: 0, wander: 0, dir: (0, 0) }
    }
    fn input(&mut self, b: &Battle, d: &GameData) -> (DriveInput, bool) {
        let me = (b.player.x, b.player.y);
        if me == self.last {
            self.still += 1;
        } else {
            self.still = 0;
        }
        self.last = me;
        let target = b
            .enemies
            .iter()
            .flatten()
            .filter(|e| !e.is_wreck())
            .min_by_key(|e| (e.x - me.0).abs() + (e.y - me.1).abs())
            .map(|e| (e.x, e.y));
        let Some(t) = target else {
            let gate = &d.arena.events[0].exits[1];
            let far = (me.0 - gate.start_x).abs() > 20 || (me.1 - gate.start_y).abs() > 20;
            return (DriveInput { dy: 1, ..Default::default() }, far);
        };
        let close = (t.0 - me.0).abs() < 50 && (t.1 - me.1).abs() < 30;
        if self.still > 12 && !close && self.wander == 0 {
            self.wander = 25;
            self.dir = (self.rng.upto(2) as i8 - 1, self.rng.upto(2) as i8 - 1);
        }
        if self.wander > 0 {
            self.wander -= 1;
            return (DriveInput { dx: self.dir.0, dy: self.dir.1, fire: close, ..Default::default() }, false);
        }
        let (dx, dy) = stick_toward(me, t);
        (DriveInput { dx, dy, fire: close, ..Default::default() }, false)
    }
}

/// Amateur Night in the KILLER KART, played by a simple bot. The bot is kept alive and
/// armed (it is a poor driver); the point is that the engine can be won and reports it.
#[test]
fn scripted_player_wins_amateur_night() {
    let d = crate::need_data!();
    let mut wins = 0;
    for seed in 1..=6u32 {
        let mut g = Game::new("Bot", true, &d, seed);
        g.loaner = Some(Car::killer_kart(7));
        g.driver.loaner = true;
        g.driver.car_with_you = true;
        let mut b = Battle::new(&mut g, &d, BattleKind::Arena { event: 0 });
        let mut bot = Bot::new(seed);
        let mut result = None;
        for _ in 0..8000 {
            g.driver.health = 3;
            if let Some(c) = g.car_mut() {
                c.battery = 99;
                let mut mg = Weapon::new(w::MG, 0);
                mg.ammo = 20;
                c.weapons[0] = Some(mg);
            }
            let (inp, teleport) = bot.input(&b, &d);
            if teleport {
                let gate = &d.arena.events[0].exits[1];
                b.player.x = gate.start_x;
                b.player.y = gate.start_y;
            }
            if let Some(end) = b.step(&mut g, &d, &inp) {
                result = Some(end);
                break;
            }
        }
        if result == Some(BattleEnd::Arena(ArenaResult::Won)) {
            wins += 1;
        }
    }
    assert!(wins >= 3, "the bot won only {} of 6 Amateur Nights", wins);
}

#[test]
fn a_day_passes_once_per_trip() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 5);
    g.roads[0].slots = vec![None; 16];
    let day0 = g.driver.date;
    let (drv, pre) = (g.driver.driving, g.driver.prestige);
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 0, from_city: 0 });
    b.enemies = vec![None; ENEMY_SLOTS];
    for _ in 0..TRIP_DAY_FRAMES + 50 {
        b.step(&mut g, &d, &DriveInput::default());
    }
    assert_eq!(g.driver.date, day0.next_day());
    assert_eq!(g.driver.driving, drv + 1);
    assert_eq!(g.driver.prestige, pre + 1);
}

#[test]
fn survivors_are_written_back_and_wrecks_cleared() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 9);
    g.roads[3].slots = vec![None; 16];
    g.roads[3].slots[0] = Some(4);
    g.roads[3].slots[1] = Some(7);
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 3, from_city: 3 });
    assert_eq!(b.enemies[0].as_ref().map(|e| e.kind), Some(4));
    b.enemies[1].as_mut().unwrap().state = WRECK;
    // teleport next to the far end and arrive
    let far = d.roads[3].exits.iter().find(|e| e.city != 3).unwrap().clone();
    b.player.x = far.trigger_x - 20;
    b.player.y = far.trigger_y;
    b.player.heading = 0;
    let mut end = None;
    for _ in 0..40 {
        if let Some(e) = b.step(&mut g, &d, &DriveInput { dx: 1, ..Default::default() }) {
            end = Some(e);
            break;
        }
    }
    assert_eq!(end, Some(BattleEnd::Arrived { city: far.city, road: 3, on_foot: false }));
    assert_eq!(g.roads[3].slots[0], Some(4));
    assert_eq!(g.roads[3].slots[1], None);
}

#[test]
fn the_origin_exit_needs_leaving_first() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 11);
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 0, from_city: 0 });
    b.enemies = vec![None; ENEMY_SLOTS];
    // start (128, 32) heading south; the Watertown trigger is (128, 0)
    assert_eq!((b.player.x, b.player.y, b.player.heading), (128, 32, 12));
    b.player.y = 10;
    assert_eq!(b.step(&mut g, &d, &DriveInput::default()), None);
    b.player.y = 120;
    b.step(&mut g, &d, &DriveInput::default());
    b.player.y = 10;
    assert!(matches!(b.step(&mut g, &d, &DriveInput::default()), Some(BattleEnd::Arrived { city: 0, .. })));
}

#[test]
fn hit_sides_face_the_shot() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 3);
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 0, from_city: 0 });
    b.player.new_heading = 0; // facing east
    let shot = |dir: u8| HitFrom { pos: (0, 0), dir };
    assert_eq!(b.hit_side(shot(8)), crate::car::F); // flying west: came from ahead
    assert_eq!(b.hit_side(shot(0)), crate::car::B);
    assert_eq!(b.hit_side(shot(12)), crate::car::L); // flying south: came from the north = left
    assert_eq!(b.hit_side(shot(4)), crate::car::R);
}

#[test]
fn walking_salvage_and_getting_back_in() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 21);
    g.driver.mechanic = 60;
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 3, from_city: 3 });
    b.enemies = vec![None; ENEMY_SLOTS];
    let (x, y) = (b.player.x, b.player.y);
    // a stopped laser wreck (nothing) and a stopped AT wreck (parts: we carry no AT gun)
    let mut la = Enemy::new(&crate::tables::ROAD_ENEMIES[4], 4, 0, x + 3, y);
    la.state = WRECK;
    b.enemies[0] = Some(la);
    b.step(&mut g, &d, &DriveInput { toggle_foot: true, ..Default::default() });
    assert!(b.player.on_foot);
    assert_eq!(b.enemies[0].as_ref().unwrap().state, SALVAGED);
    assert_eq!(b.message.as_deref(), Some("You found nothing."));
    let mut at = Enemy::new(&crate::tables::ROAD_ENEMIES[5], 5, 1, x + 4, y + 4);
    at.state = WRECK;
    b.enemies[1] = Some(at);
    b.step(&mut g, &d, &DriveInput::default());
    assert_eq!(b.message.as_deref(), Some("You found some parts."));
    let parts = g.car().unwrap().cargo.iter().flatten().find(|c| c.kind == CargoKind::Salvage).cloned().unwrap();
    assert!(parts.weight >= 100 && parts.value >= 3100 && parts.value <= 3600);
    // walk away and back, then get in
    for _ in 0..3 {
        b.step(&mut g, &d, &DriveInput { dx: -1, ..Default::default() });
    }
    b.step(&mut g, &d, &DriveInput { toggle_foot: true, ..Default::default() });
    assert!(!b.player.on_foot);
    assert_eq!((b.player.x, b.player.y), (x, y));
}

#[test]
fn leaving_a_road_on_foot_loses_the_car() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 31);
    g.driver.prestige = 10;
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 3, from_city: 3 });
    b.enemies = vec![None; ENEMY_SLOTS];
    b.step(&mut g, &d, &DriveInput { toggle_foot: true, ..Default::default() });
    let far = d.roads[3].exits.iter().find(|e| e.city != 3).unwrap().clone();
    b.player.x = far.trigger_x - 18;
    b.player.y = far.trigger_y;
    let mut end = None;
    for _ in 0..10 {
        end = b.step(&mut g, &d, &DriveInput { dx: 1, ..Default::default() });
        if end.is_some() {
            break;
        }
    }
    assert_eq!(end, Some(BattleEnd::Arrived { city: far.city, road: 3, on_foot: true }));
    assert_eq!(g.driver.prestige, 8);
    assert!(g.car().is_none());
}

#[test]
fn weapons_fire_spend_ammo_and_cool_down() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 41);
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 3, from_city: 3 });
    b.enemies = vec![None; ENEMY_SLOTS];
    let w0 = g.car().unwrap().weight();
    // machine gun: one round every 4th frame while held
    for _ in 0..8 {
        b.step(&mut g, &d, &DriveInput { fire: true, ..Default::default() });
    }
    assert_eq!(g.car().unwrap().weapons[0].unwrap().ammo, 18);
    assert_eq!(g.car().unwrap().weight(), w0 - 4);
    // mines drop behind
    b.step(&mut g, &d, &DriveInput { select: Some(2), ..Default::default() });
    b.step(&mut g, &d, &DriveInput { fire: true, ..Default::default() });
    assert!(b.pproj.iter().any(|p| p.kind == pk::MD));
    // the heavy rocket is used up with its launcher
    b.step(&mut g, &d, &DriveInput { select: Some(6), fire: true, ..Default::default() });
    assert!(g.car().unwrap().weapons[6].is_none());
    assert!(b.pproj.iter().any(|p| p.kind == pk::HR));
    // the laser runs on the battery
    let bat = g.car().unwrap().battery;
    b.step(&mut g, &d, &DriveInput { select: Some(5), fire: true, ..Default::default() });
    assert_eq!(g.car().unwrap().battery, bat - 1);
    // an empty slot: "No weapon!" (once its cooldown has run out)
    for _ in 0..4 {
        b.step(&mut g, &d, &DriveInput { select: Some(6), fire: true, ..Default::default() });
    }
    assert_eq!(b.message.as_deref(), Some("No weapon!"));
    assert!(stick_heading(0, 0).is_none());
}

#[test]
fn enemy_fire_damages_the_car() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 51);
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 3, from_city: 3 });
    b.enemies = vec![None; ENEMY_SLOTS];
    // a laser car parked right ahead, facing us
    let (x, y) = (b.player.x, b.player.y);
    let mut e = Enemy::new(&crate::tables::ROAD_ENEMIES[10], 10, 0, x + 30, y);
    e.heading = 8;
    b.enemies[0] = Some(e);
    let armor0: u32 = g.car().unwrap().armor.iter().map(|a| a.pts as u32).sum();
    for _ in 0..300 {
        if b.step(&mut g, &d, &DriveInput::default()).is_some() {
            break;
        }
    }
    let armor1: u32 = g.car().unwrap().armor.iter().map(|a| a.pts as u32).sum();
    assert!(armor1 < armor0, "no damage taken");
    // with the geometric fix the front took it
    assert!(g.car().unwrap().armor[crate::car::F].pts < 20);
}

#[test]
fn arena_results() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 61);
    let mut b = Battle::new(&mut g, &d, BattleKind::Arena { event: 1 });
    // leaving in the car with opponents left
    assert_eq!(b.leave_arena(&mut g, 1), ArenaResult::Fled);
    // won in the car / on foot / on foot with a flat battery
    b.opponents_left = 0;
    assert_eq!(b.leave_arena(&mut g, 1), ArenaResult::Won);
    b.player.get_out();
    assert_eq!(b.leave_arena(&mut g, 1), ArenaResult::WonOnFoot);
    b.out_of_power = true;
    assert_eq!(b.leave_arena(&mut g, 1), ArenaResult::CarTowed);
    // Amateur Night counts a win on foot as a win
    assert_eq!(b.leave_arena(&mut g, 0), ArenaResult::Won);
    // practice
    b.hit_taken = true;
    assert_eq!(b.leave_arena(&mut g, 7), ArenaResult::Practice { hit: true, on_foot: true });
    // walking out with opponents left loses the car (not the Amateur Night loaner)
    b.opponents_left = 2;
    assert!(g.car().is_some());
    assert_eq!(b.leave_arena(&mut g, 0), ArenaResult::Fled);
    assert!(g.car().is_some());
    assert_eq!(b.leave_arena(&mut g, 3), ArenaResult::Fled);
    assert!(g.car().is_none());
}

#[test]
fn arena_gate_and_battery() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 62);
    let mut b = Battle::new(&mut g, &d, BattleKind::Arena { event: 7 });
    assert_eq!((b.player.x, b.player.y, b.player.heading), (624, 945, 4));
    let bat = g.car().unwrap().battery;
    let mut end = None;
    for _ in 0..60 {
        // back into the gate behind us
        end = b.step(&mut g, &d, &DriveInput { dy: 1, ..Default::default() });
        if end.is_some() {
            break;
        }
    }
    assert_eq!(end, Some(BattleEnd::Arena(ArenaResult::Practice { hit: false, on_foot: false })));
    assert_eq!(g.car().unwrap().battery, bat - 1);
}

#[test]
fn paint_and_oil_on_enemies() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 63);
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 3, from_city: 3 });
    b.enemies = vec![None; ENEMY_SLOTS];
    let (x, y) = (b.player.x, b.player.y);
    let mut e = Enemy::new(&crate::tables::ROAD_ENEMIES[0], 0, 0, x + 40, y);
    e.state = 3;
    b.enemies[0] = Some(e);
    b.cloud_hits_enemy(&mut g, 0, pk::PAINT);
    // painted: no shots for 32 frames, +2 to rolls for 15
    for f in 0..31 {
        b.step(&mut g, &d, &DriveInput::default());
        assert!(b.eproj.iter().next().is_none(), "fired while painted (frame {})", f);
        let e = b.enemies[0].as_ref().unwrap();
        assert_eq!(e.paint > 0, f < 14, "frame {}", f);
    }
    // smoke: blind through its next AI pass
    for _ in 0..40 {
        b.cloud_hits_enemy(&mut g, 0, pk::SMOKE);
        b.step(&mut g, &d, &DriveInput::default());
        assert!(b.eproj.iter().next().is_none(), "fired from inside smoke");
    }
    // oil: spins for 16 frames
    b.cloud_hits_enemy(&mut g, 0, pk::OIL);
    assert!(b.enemies[0].as_ref().unwrap().spin);
    for _ in 0..16 {
        b.step(&mut g, &d, &DriveInput::default());
    }
    assert!(!b.enemies[0].as_ref().unwrap().spin);
}

#[test]
fn tape_fills_every_slot_and_presets_always_appear() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 64);
    let b = Battle::new(&mut g, &d, BattleKind::Road { road: 0, from_city: 0 });
    assert!(b.enemies[14].as_ref().map(|e| e.kind) == Some(5), "preset 14:5 on road 0");
    let mut g = armed_game(&d, 65);
    let q = crate::story::BRAIN_TAPE as usize;
    g.give_quest_item(q);
    assert!(g.driver.quest.carrying_tape());
    let b = Battle::new(&mut g, &d, BattleKind::Road { road: 0, from_city: 0 });
    assert_eq!(b.enemies.iter().flatten().count(), 16);
}

#[test]
fn crashes_cost_front_armor_at_speed() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 66);
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 10, from_city: 7 });
    b.enemies = vec![None; ENEMY_SLOTS];
    let f0 = g.car().unwrap().armor[crate::car::F].pts;
    // straight down from New York runs into a fence at full speed
    let mut crashed = false;
    for _ in 0..60 {
        b.step(&mut g, &d, &DriveInput { dy: 1, ..Default::default() });
        if b.sfx.contains(&Sfx::Crash) {
            crashed = true;
            break;
        }
        b.sfx.clear();
    }
    assert!(crashed);
    assert_eq!(b.player.speed, 0);
    assert_eq!(g.car().unwrap().armor[crate::car::F].pts, f0 - 1);
    assert_eq!(b.message, Some(format!("Crash! Front armor down to {}.", f0 - 1)));
}

#[test]
fn killed_at_the_exit_is_dead_not_arrived() {
    let d = crate::need_data!();
    let mut g = armed_game(&d, 70);
    let mut b = Battle::new(&mut g, &d, BattleKind::Road { road: 3, from_city: 3 });
    b.enemies = vec![None; ENEMY_SLOTS];
    let far = d.roads[3].exits.iter().find(|e| e.city != 3).unwrap().clone();
    b.player.x = far.trigger_x - 2;
    b.player.y = far.trigger_y;
    g.driver.health = 0;
    let mut end = None;
    for _ in 0..DEATH_FRAMES + 5 {
        end = b.step(&mut g, &d, &DriveInput::default());
        if end.is_some() {
            break;
        }
    }
    assert_eq!(end, Some(BattleEnd::Died));
}
