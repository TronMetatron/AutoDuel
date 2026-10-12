//! Headless runner: plays a script against the game and saves screenshots. No window needed.
//!
//!     cargo run -p cardul-ui --example play -- script.txt [--saves DIR] [--art classic|enhanced]
//!
//! The runner uses the classic graphics (320x200 screenshots) unless `--art enhanced` is given
//! or the script switches with `art`.
//!
//! Script lines:
//!   key NAME            press a key (up down left right enter esc space backspace tab f1..f12, or one char)
//!   type TEXT           type text (letters, digits, spaces)
//!   hold DIRS TICKS     hold up/down/left/right/fire (joined with +) for TICKS 60 Hz ticks
//!   wait TICKS          do nothing for TICKS ticks
//!   shot FILE.png       save the current screen
//!   art classic|enhanced   switch the graphics (as F10 does)
//!   ending              show the end-of-game newspaper
//!   # comment
//! Debug shortcuts (skip walking around):
//!   newgame NAME [cheat]   create a driver and stand in New York
//!   load FILE              load a saved game
//!   set money|prestige|driving|marksman|mechanic|health|city|armor N
//!   givecar [kart|default|van] give a car that is with you (default: KILLER KART design)
//!   building N             open building type N (0 GO 1 TR 2 AS 3 SA 4 WE 5 GA 6 AR 7 AA 8 DI
//!                          9 FB 10 OU 11 AB 12 OR 13 PE 14 CA 15 HO) in the current city
//!   city                   go to the city screen (at the Truck Stop)
//!   road N                 start driving road N from the current city (or its first end)
//!   arena E                start arena event E (0-6, 7 = practice)
//!   days N                 let N days pass
//!   dump                   print the driver and current car as JSON
//!   damagecar              knock the current car about (armor, a tire, the plant, weapon 1)
//! Walking and driving for real (through the normal input, no teleporting):
//!   goto_door N            walk (or drive) from where you stand to the door of building type N
//!                          in this city, around the other doors and exits, and go in
//!   goto_exit N|E|S|W      walk/drive into that road exit of the city
//!   autodrive [TICKS]      on the road or in the arena: the autopilot drives (to the far end of
//!                          the road / fights and leaves by the gate) until the battle ends
//!   travel CITY [safe]     in your car: drive out and along the roads (autodrive) to city CITY
//!                          by the fewest roads, ending at that city's road entrance; "safe"
//!                          wrecks the road's enemies at the start of each road
//!   where                  print the top screen and the position
//!   monkey TICKS SEED      press random keys and arrows for TICKS ticks (never Q or Esc on the
//!                          title menus); a new cheat driver is made whenever the game ends
//!   doors                  check that every door and road exit of every city can be walked to
//!                          from the Truck Stop and from every road entrance
//!   tour                   open every building of every city (pressing digits 1-6 and a few keys
//!                          in each) and screenshot nothing; reports text that does not fit
//!   bench TICKS            run TICKS ticks (autodrive input if driving, else idle), drawing every
//!                          tick, and print the time per tick and per draw
//!   salvage                on the road: get out, walk to the nearest stopped wreck worth
//!                          searching (not burnt, no laser), search it, walk back and get in
//! Battle debug (road or arena screen):
//!   wreck_all              wreck every enemy still in action
//!   teleport X Y           put the player (and his car) at world X Y
use cardul_ui::assets::ArtSet;
use cardul_ui::input::{Held, Input, Key};
use cardul_ui::App;
use cardul_ui::screens::{buildings, city::{Arrival, CityScreen}, drive::{self, DriveScreen}};
use cardul_sim::combat::autopilot::{ArenaBot, Autopilot};
use cardul_sim::combat::DriveInput;
use cardul_sim::{Building, Car, Game};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

/// Shortest 4-way walk in the city from `from` to a cell where `goal` holds, never stepping
/// on a cell where `avoid` holds (other doors and exits would be entered on the way).
fn city_path(app: &App, city: u8, from: (i32, i32), goal: &dyn Fn(i32, i32) -> bool, avoid: &dyn Fn(i32, i32) -> bool) -> Option<Vec<(i32, i32)>> {
    let mut prev: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    let mut q = VecDeque::new();
    q.push_back(from);
    prev.insert(from, from);
    while let Some((x, y)) = q.pop_front() {
        if (x, y) != from && goal(x, y) {
            let mut path = vec![(x, y)];
            let mut c = (x, y);
            while c != from {
                c = prev[&c];
                path.push(c);
            }
            path.reverse();
            return Some(path);
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (x + dx, y + dy);
            if prev.contains_key(&n) || n.0 < -128 || n.0 > 127 || n.1 < -40 || n.1 > 60 {
                continue;
            }
            if !CityScreen::free(&app.cx, city, n.0, n.1) || (avoid(n.0, n.1) && !goal(n.0, n.1)) {
                continue;
            }
            prev.insert(n, (x, y));
            q.push_back(n);
        }
    }
    None
}

/// Hold the arrows along a city path (3 ticks per walker step).
fn walk_path(app: &mut App, path: &[(i32, i32)]) {
    for w in path.windows(2) {
        let (dx, dy) = (w[1].0 - w[0].0, w[1].1 - w[0].1);
        let h = Held { up: dy < 0, down: dy > 0, left: dx < 0, right: dx > 0, fire: false };
        for _ in 0..3 {
            app.tick(&Input { held: h, ..Default::default() });
        }
        if let Some(c) = app.top_as::<CityScreen>() {
            let (_, x, y) = c.pos();
            if (x, y) != w[1] {
                eprintln!("walk: expected {:?}, at {:?} ({:?})", w[1], (x, y), c.messages());
                return;
            }
        }
    }
    app.tick(&Input::default());
}

fn city_walk_to(app: &mut App, goal: &dyn Fn(&cardul_sim::data::City, i32, i32) -> bool, what: &str) {
    let Some(c) = app.top_as::<CityScreen>() else { panic!("{}: not in a city", what) };
    let (city, x, y) = c.pos();
    let data = app.cx.data.city(city).clone();
    let avoid = |x: i32, y: i32| data.buildings.iter().any(|b| (b.door_x, b.door_y) == (x, y)) || data.exits.iter().any(|e| e.contains(x, y));
    let g = |x: i32, y: i32| goal(&data, x, y);
    match city_path(app, city, (x, y), &g, &avoid) {
        Some(p) => {
            eprintln!("{}: {} steps", what, p.len() - 1);
            walk_path(app, &p);
        }
        None => panic!("{}: no way there from {:?} in {}", what, (x, y), data.name),
    }
}

/// Exit direction out of `from` on the first road of a shortest route to `to`.
fn next_hop(data: &cardul_sim::GameData, from: u8, to: u8) -> Option<String> {
    let mut prev: HashMap<u8, (u8, String)> = HashMap::new();
    let mut q = VecDeque::from([from]);
    while let Some(c) = q.pop_front() {
        if c == to {
            let mut c = c;
            loop {
                let (p, dir) = prev[&c].clone();
                if p == from {
                    return Some(dir);
                }
                c = p;
            }
        }
        for e in &data.city(c).exits {
            if e.dest_city != from && !prev.contains_key(&e.dest_city) {
                prev.insert(e.dest_city, (c, e.dir.clone()));
                q.push_back(e.dest_city);
            }
        }
    }
    None
}

fn autodrive(app: &mut App, max: u32) -> u32 {
    let mut pilot: Option<Pilot> = None;
    let mut n = 0;
    while n < max {
        let (top, cx) = app.top_and_cx();
        let Some(ds) = top.and_then(|t| t.as_any()).and_then(|a| a.downcast_mut::<DriveScreen>()) else { break };
        let b = ds.battle();
        let game = cx.game.as_ref().unwrap();
        let p = pilot.get_or_insert_with(|| {
            if b.is_arena() {
                Pilot::Arena(ArenaBot::new(b, &cx.data))
            } else {
                let mut a = Autopilot::for_battle(b, &cx.data);
                a.shoot = true;
                Pilot::Road(a)
            }
        });
        let d = match p {
            Pilot::Road(a) => a.input(b, game),
            Pilot::Arena(a) => a.input(b, game, &cx.data),
        };
        if std::env::var("AUTODRIVE_DEBUG").is_ok() && n % 400 == 0 {
            let st = match p {
                Pilot::Road(a) => a.debug_state(),
                Pilot::Arena(a) => a.pilot.debug_state(),
            };
            eprintln!("autodrive f{} at {:?} h{} s{} in {:?}: {}", b.frame, (b.player.x, b.player.y), b.player.heading, b.player.speed, (d.dx, d.dy), st);
        }
        app.tick(&to_input(&d));
        n += 1;
    }
    n
}

fn to_input(d: &DriveInput) -> Input {
    let mut inp = Input {
        held: Held { up: d.dy < 0, down: d.dy > 0, left: d.dx < 0, right: d.dx > 0, fire: d.fire },
        ..Default::default()
    };
    if d.toggle_foot {
        inp.keys.push(Key::Char('g'));
    }
    if d.next_weapon {
        inp.keys.push(Key::Space);
    }
    if let Some(s) = d.select {
        inp.keys.push(Key::Char(if s == 9 { '0' } else { (b'1' + s) as char }));
    }
    inp
}

enum Pilot {
    Road(Autopilot),
    Arena(ArenaBot),
}

fn key_of(name: &str) -> Key {
    match name.to_lowercase().as_str() {
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "enter" => Key::Enter,
        "esc" => Key::Esc,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "tab" => Key::Tab,
        n if n.starts_with('f') && n.len() > 1 && n[1..].parse::<u8>().is_ok() => Key::F(n[1..].parse().unwrap()),
        n => Key::Char(n.chars().next().unwrap()),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let script = std::fs::read_to_string(&args[1]).expect("script");
    let saves = args.iter().position(|a| a == "--saves").map(|i| PathBuf::from(&args[i + 1])).unwrap_or_else(|| PathBuf::from("/tmp/cardul-saves"));
    let assets = std::env::var("CARDUL_ASSETS").map(PathBuf::from).ok().or_else(cardul_sim::data::find_assets_dir).expect("assets dir");
    let art = args.iter().position(|a| a == "--art").map(|i| ArtSet::parse(&args[i + 1]).expect("--art classic|enhanced"));
    let mut app = App::with_art(&assets, saves, Some(art.unwrap_or(ArtSet::Classic))).expect("app");
    let idle = Input::default();
    for (lineno, line) in script.lines().enumerate() {
        let line = match line.find(" #") {
            Some(i) => line[..i].trim(),
            None => line.trim(),
        };
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
        match cmd {
            "ending" => {
                let s = cardul_ui::screens::ending::EndingScreen::new(&mut app.cx);
                app.reset_to(Box::new(s));
            }
            "art" => {
                let a = ArtSet::parse(rest).expect("art classic|enhanced");
                app.set_art(a).expect("art set");
            }
            "key" => {
                let k = key_of(rest.trim());
                let mut inp = Input::default();
                inp.keys.push(k);
                if let Key::Char(c) = k {
                    inp.chars.push(c);
                }
                app.tick(&inp);
                app.tick(&idle);
            }
            "type" => {
                for c in rest.chars() {
                    let mut inp = Input::default();
                    inp.chars.push(c);
                    if c == ' ' {
                        inp.keys.push(Key::Space);
                    } else {
                        inp.keys.push(Key::Char(c.to_ascii_lowercase()));
                    }
                    app.tick(&inp);
                }
            }
            "hold" => {
                let mut parts = rest.split_whitespace();
                let dirs = parts.next().unwrap_or("");
                let n: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
                let mut h = Held::default();
                for d in dirs.split('+') {
                    match d {
                        "up" => h.up = true,
                        "down" => h.down = true,
                        "left" => h.left = true,
                        "right" => h.right = true,
                        "fire" => h.fire = true,
                        _ => {}
                    }
                }
                for _ in 0..n {
                    app.tick(&Input { held: h, ..Default::default() });
                }
            }
            "wait" => {
                for _ in 0..rest.trim().parse::<u32>().unwrap_or(1) {
                    app.tick(&idle);
                }
            }
            "shot" => {
                app.draw();
                app.frame.save_png(std::path::Path::new(rest.trim())).expect("png");
                eprintln!("saved {}", rest.trim());
            }
            "newgame" => {
                let mut p = rest.split_whitespace();
                let name = p.next().unwrap_or("Tester");
                let cheat = p.next() == Some("cheat");
                let g = Game::new(name, cheat, &app.cx.data, 12345);
                app.cx.start_game(g, None);
                let s = CityScreen::arrive(&mut app.cx, Arrival::Start);
                app.reset_to(Box::new(s));
            }
            "load" => {
                let g = Game::load_from(std::path::Path::new(rest.trim())).expect("load");
                app.cx.start_game(g, Some(PathBuf::from(rest.trim())));
                let s = CityScreen::arrive(&mut app.cx, Arrival::TruckStop);
                app.reset_to(Box::new(s));
            }
            "set" => {
                let mut p = rest.split_whitespace();
                let what = p.next().unwrap_or("");
                let v: u32 = p.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                let d = &mut app.cx.g().driver;
                match what {
                    "money" => d.money = v,
                    "prestige" => d.prestige = v as u8,
                    "driving" => d.driving = v as u8,
                    "marksman" => d.marksman = v as u8,
                    "mechanic" => d.mechanic = v as u8,
                    "health" => d.health = v as u8,
                    "armor" => d.body_armor = v as u8,
                    "city" => d.city = v as u8,
                    _ => panic!("set what?"),
                }
            }
            "givecar" => {
                let city = app.cx.game().driver.city;
                let car = match rest.trim() {
                    "default" => Car::default_design("Test Car", city),
                    "van" => {
                        // room for cargo and salvage
                        let mut c = Car::default_design("Test Van", city);
                        c.body = 6;
                        c.chassis = 3;
                        c.set_plant(2);
                        c
                    }
                    _ => {
                        let mut c = Car::killer_kart(city);
                        c.name = "Test Kart".into();
                        c
                    }
                };
                app.cx.g().add_car(car);
            }
            "building" => {
                let k = Building::from_u8(rest.trim().parse().unwrap()).unwrap();
                let s = buildings::open(&mut app.cx, k);
                app.reset_to(s);
            }
            "city" => {
                let s = CityScreen::arrive(&mut app.cx, Arrival::TruckStop);
                app.reset_to(Box::new(s));
            }
            "road" => {
                let r: u8 = rest.trim().parse().unwrap();
                let here = app.cx.game().driver.city;
                let ends: Vec<u8> = app.cx.data.roads[r as usize].exits.iter().map(|e| e.city).collect();
                let from = if ends.contains(&here) { here } else { ends[0] };
                app.cx.g().driver.city = from;
                let s = drive::road(&mut app.cx, r, from);
                app.reset_to(s);
            }
            "arena" => {
                let e: u8 = rest.trim().parse().unwrap();
                let s = drive::arena(&mut app.cx, e);
                app.reset_to(s);
            }
            "days" => {
                let n: u32 = rest.trim().parse().unwrap();
                app.cx.g().pass_days(n);
            }
            "goto_door" => {
                let k: u8 = rest.trim().parse().unwrap();
                city_walk_to(&mut app, &|c, x, y| c.buildings.iter().any(|b| b.kind == k && (b.door_x, b.door_y) == (x, y)), &format!("goto_door {}", k));
            }
            "goto_exit" => {
                let d = rest.trim().to_uppercase();
                city_walk_to(&mut app, &|c, x, y| c.exits.iter().any(|e| e.dir == d && e.contains(x, y)), &format!("goto_exit {}", d));
            }
            "autodrive" => {
                let max: u32 = rest.trim().parse().unwrap_or(60 * 900);
                let n = autodrive(&mut app, max);
                eprintln!("autodrive: {} ticks", n);
            }
            "travel" => {
                let mut a = rest.split_whitespace();
                let to: u8 = a.next().unwrap().parse().unwrap();
                let safe = a.next() == Some("safe");
                for _ in 0..16 {
                    let Some(c) = app.top_as::<CityScreen>() else { panic!("travel: not in a city") };
                    let (here, _, _) = c.pos();
                    if here == to {
                        break;
                    }
                    let dir = next_hop(&app.cx.data, here, to).expect("no route");
                    // a low battery: charge it at the Truck Stop first ($50)
                    if app.cx.game().car().map(|c| c.battery < 40).unwrap_or(false) {
                        city_walk_to(&mut app, &|c, x, y| c.buildings.iter().any(|b| b.kind == 1 && (b.door_x, b.door_y) == (x, y)), "goto_door 1 (charge)");
                        for _ in 0..200 {
                            app.tick(&Input::default());
                        }
                        for k in ['3', '6'] {
                            app.tick(&Input { keys: vec![Key::Char(k)], ..Default::default() });
                            for _ in 0..150 {
                                app.tick(&Input::default());
                            }
                        }
                        eprintln!("travel: charged, battery {:?}", app.cx.game().car().map(|c| c.battery));
                    }
                    eprintln!("travel: {} -> {} via {}", app.cx.data.city_name(here), app.cx.data.city_name(to), dir);
                    city_walk_to(&mut app, &|c, x, y| c.exits.iter().any(|e| e.dir == dir && e.contains(x, y)), &format!("goto_exit {}", dir));
                    if safe
                        && let Some(d) = app.top_as::<DriveScreen>() {
                        d.battle_mut().debug_wreck_all();
                    }
                    let n = autodrive(&mut app, 60 * 600);
                    let g = app.cx.game();
                    eprintln!("travel: drove {} ticks, now in {} on {}, health {}", n, app.cx.data.city_name(g.driver.city), g.driver.date.long(), g.driver.health);
                    if app.top_as::<CityScreen>().is_none() {
                        if let Some(d) = app.top_as::<DriveScreen>() {
                            let b = d.battle();
                            eprintln!(
                                "travel: stuck on {:?} at {:?}, wrecks {:?}",
                                b.kind,
                                (b.player.x, b.player.y),
                                b.enemies.iter().flatten().map(|e| (e.x, e.y, e.state)).collect::<Vec<_>>()
                            );
                            app.draw();
                            let _ = app.frame.save_png(std::path::Path::new("travel_stuck.png"));
                        }
                        eprintln!("travel: stopped (not in a city)");
                        break;
                    }
                }
            }
            "salvage" => {
                let (top, cx) = app.top_and_cx();
                let ds = top.and_then(|t| t.as_any()).and_then(|a| a.downcast_mut::<DriveScreen>()).expect("not driving");
                let b = ds.battle();
                let me = (b.player.x, b.player.y);
                let wreck = b
                    .enemies
                    .iter()
                    .flatten()
                    .filter(|e| e.state == cardul_sim::combat::WRECK && e.speed == 0 && !e.burning)
                    .filter(|e| e.weapon != cardul_sim::tables::w::LA && e.weapon != cardul_sim::tables::w::HR)
                    .min_by_key(|e| (e.x - me.0).abs() + (e.y - me.1).abs())
                    .map(|e| (e.x, e.y))
                    .expect("no wreck to search");
                let mut pilot = Autopilot::for_battle(b, &cx.data);
                pilot.walk_to = Some(wreck);
                eprintln!("salvage: wreck at {:?}, car at {:?}", wreck, me);
                app.tick(&Input { keys: vec![Key::Char('g')], ..Default::default() });
                // every wreck passed on the way is searched too; report each message
                let mut found: Vec<String> = vec![];
                let mut last = None;
                for _ in 0..60 * 900 {
                    let (top, cx) = app.top_and_cx();
                    let Some(ds) = top.and_then(|t| t.as_any()).and_then(|a| a.downcast_mut::<DriveScreen>()) else { break };
                    let b = ds.battle();
                    let m = b.message.clone().filter(|m| m.contains("found"));
                    if m.is_some() && m != last {
                        found.push(m.clone().unwrap());
                    }
                    last = m;
                    let done = b.enemies.iter().flatten().any(|e| (e.x, e.y) == wreck && e.state == cardul_sim::combat::SALVAGED);
                    if done && !b.player.on_foot {
                        break;
                    }
                    let d = pilot.input(b, cx.game.as_ref().unwrap());
                    app.tick(&to_input(&d));
                }
                if let Some(d) = app.top_as::<DriveScreen>() {
                    let t = d.battle().enemies.iter().flatten().find(|e| (e.x, e.y) == wreck).map(|e| (e.kind, e.weapon, e.burning, e.state));
                    eprintln!("salvage: target (kind, weapon, burning, state) {:?}", t);
                }
                eprintln!("salvage: {:?}", found);
            }
            "monkey" => {
                let mut a = rest.split_whitespace();
                let n: u32 = a.next().and_then(|s| s.parse().ok()).unwrap_or(10_000);
                let mut rng = cardul_sim::Rng::new(a.next().and_then(|s| s.parse().ok()).unwrap_or(1));
                let keys: Vec<Key> = {
                    let mut v = vec![Key::Up, Key::Down, Key::Left, Key::Right, Key::Enter, Key::Esc, Key::Space, Key::Backspace];
                    v.extend((1..=6).map(Key::F));
                    v.push(Key::F(8));
                    v.extend("0123456789".chars().map(Key::Char));
                    v.extend("abdeghlnpxyz=-".chars().map(Key::Char));
                    v
                };
                let mut held = Held::default();
                let mut screens = std::collections::BTreeMap::new();
                for t in 0..n {
                    if app.cx.game.is_none() || app.wants_quit() {
                        app.cx.quit = false;
                        let g = Game::new("Monkey", true, &app.cx.data, rng.next_u32());
                        app.cx.start_game(g, None);
                        let s = CityScreen::arrive(&mut app.cx, Arrival::Start);
                        app.reset_to(Box::new(s));
                    }
                    // now and then jump to a random building of this city (or a random city)
                    if t % 1500 == 0
                        && (app.top_as::<CityScreen>().is_some() || app.top_as::<DriveScreen>().is_some()) {
                        if rng.upto(3) == 0 {
                            app.cx.g().driver.city = rng.upto(15) as u8;
                        }
                        let city = app.cx.game().driver.city;
                        let kinds: Vec<u8> = app.cx.data.city(city).buildings.iter().map(|b| b.kind).collect();
                        let k = Building::from_u8(kinds[rng.upto(kinds.len() as u32 - 1) as usize]).unwrap();
                        let s = buildings::open(&mut app.cx, k);
                        app.reset_to(s);
                    }
                    let mut inp = Input::default();
                    if rng.upto(7) == 0 {
                        let k = keys[rng.upto(keys.len() as u32 - 1) as usize];
                        inp.keys.push(k);
                        if let Key::Char(c) = k {
                            inp.chars.push(c);
                        }
                    }
                    if rng.upto(30) == 0 {
                        held = Held { up: rng.coin(), down: rng.coin(), left: rng.coin(), right: rng.coin(), fire: rng.coin() };
                    }
                    inp.held = held;
                    app.tick(&inp);
                    if t % 50 == 0 {
                        app.draw();
                        let name = if app.top_as::<CityScreen>().is_some() { "city" } else if app.top_as::<DriveScreen>().is_some() { "drive" } else { "other" };
                        *screens.entry(name).or_insert(0u32) += 1;
                    }
                }
                eprintln!("monkey: {} ticks, samples per screen {:?}", n, screens);
            }
            "doors" => {
                let mut bad = 0;
                for city in 0..16u8 {
                    let c = app.cx.data.city(city).clone();
                    let ts = c.buildings.iter().find(|b| b.kind == 1).map(|b| (b.leave_x, b.leave_y)).unwrap();
                    let mut starts = vec![("truck stop".to_string(), ts)];
                    for e in &c.exits {
                        starts.push((format!("entrance {}", e.dir), (e.arrive_x, e.arrive_y)));
                    }
                    for b in &c.buildings {
                        starts.push((format!("leaving {:?}", Building::from_u8(b.kind).unwrap()), (b.leave_x, b.leave_y)));
                    }
                    let avoid = |x: i32, y: i32| c.buildings.iter().any(|b| (b.door_x, b.door_y) == (x, y)) || c.exits.iter().any(|e| e.contains(x, y));
                    for (sname, start) in &starts {
                        if !CityScreen::free(&app.cx, city, start.0, start.1) {
                            eprintln!("doors: {} {}: start {:?} is inside a wall", c.name, sname, start);
                            bad += 1;
                            continue;
                        }
                        for b in &c.buildings {
                            let goal = |x: i32, y: i32| (x, y) == (b.door_x, b.door_y);
                            if city_path(&app, city, *start, &goal, &avoid).is_none() {
                                eprintln!("doors: {} from {}: can't reach {:?}", c.name, sname, Building::from_u8(b.kind).unwrap());
                                bad += 1;
                            }
                        }
                        for e in &c.exits {
                            let goal = |x: i32, y: i32| e.contains(x, y);
                            if city_path(&app, city, *start, &goal, &avoid).is_none() {
                                eprintln!("doors: {} from {}: can't reach exit {}", c.name, sname, e.dir);
                                bad += 1;
                            }
                        }
                    }
                }
                eprintln!("doors: {} problems", bad);
            }
            "tour" => {
                let keys = ["1", "x", "2", "x", "3", "x", "4", "x", "5", "x", "6", "esc", "y", "esc"];
                for city in 0..16u8 {
                    let kinds: Vec<u8> = app.cx.data.city(city).buildings.iter().map(|b| b.kind).collect();
                    for k in kinds {
                        app.cx.g().driver.city = city;
                        let s = buildings::open(&mut app.cx, Building::from_u8(k).unwrap());
                        app.reset_to(s);
                        for key in keys {
                            let mut inp = Input::default();
                            let kk = key_of(key);
                            inp.keys.push(kk);
                            if let Key::Char(c) = kk {
                                inp.chars.push(c);
                            }
                            app.tick(&inp);
                            for _ in 0..80 {
                                app.tick(&Input::default());
                                app.draw();
                            }
                        }
                        if app.cx.game.is_none() || !app.cx.game().driver.alive {
                            eprintln!("tour: driver gone in {:?} of {}", Building::from_u8(k), city);
                            break;
                        }
                    }
                }
                eprintln!("tour: done");
            }
            "bench" => {
                let n: u32 = rest.trim().parse().unwrap_or(600);
                let mut pilot: Option<Autopilot> = None;
                let (mut t_tick, mut t_draw) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
                let (mut max_tick, mut max_draw) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
                for _ in 0..n {
                    let inp = {
                        let (top, cx) = app.top_and_cx();
                        match top.and_then(|t| t.as_any()).and_then(|a| a.downcast_mut::<DriveScreen>()) {
                            Some(ds) => {
                                let b = ds.battle();
                                let p = pilot.get_or_insert_with(|| {
                                    let mut a = Autopilot::for_battle(b, &cx.data);
                                    a.shoot = true;
                                    a
                                });
                                to_input(&p.input(b, cx.game.as_ref().unwrap()))
                            }
                            None => Input::default(),
                        }
                    };
                    let t0 = std::time::Instant::now();
                    app.tick(&inp);
                    let t1 = std::time::Instant::now();
                    app.draw();
                    let t2 = std::time::Instant::now();
                    t_tick += t1 - t0;
                    t_draw += t2 - t1;
                    max_tick = max_tick.max(t1 - t0);
                    max_draw = max_draw.max(t2 - t1);
                }
                eprintln!(
                    "bench: {} ticks, tick avg {:.3} ms max {:.3} ms, draw avg {:.3} ms max {:.3} ms",
                    n,
                    t_tick.as_secs_f64() * 1000.0 / n as f64,
                    max_tick.as_secs_f64() * 1000.0,
                    t_draw.as_secs_f64() * 1000.0 / n as f64,
                    max_draw.as_secs_f64() * 1000.0
                );
            }
            "wreck_all" => {
                app.top_as::<DriveScreen>().expect("not driving").battle_mut().debug_wreck_all();
            }
            "teleport" => {
                let v: Vec<i32> = rest.split_whitespace().map(|s| s.parse().unwrap()).collect();
                let p = &mut app.top_as::<DriveScreen>().expect("not driving").battle_mut().player;
                p.x = v[0];
                p.y = v[1];
                p.car_x = v[0];
                p.car_y = v[1];
            }
            "where" => {
                if let Some(c) = app.top_as::<CityScreen>() {
                    eprintln!("where: city {:?}", c.pos());
                } else if let Some(d) = app.top_as::<DriveScreen>() {
                    let b = d.battle();
                    eprintln!("where: {:?} at {:?} frame {} enemies {}", b.kind, (b.player.x, b.player.y), b.frame, b.enemies.iter().flatten().filter(|e| !e.is_wreck()).count());
                } else {
                    eprintln!("where: screen depth {}", app.depth());
                }
            }
            "damagecar" => {
                let c = app.cx.g().car_mut().expect("no car");
                c.armor[0].pts /= 2;
                c.armor[1].pts = 0;
                c.tires[1].dp = 1;
                c.plant.dp = c.plant.dp.saturating_sub(2);
                if let Some(w) = c.weapons[0].as_mut() {
                    w.dp = 1;
                }
            }
            "dump" => {
                let g = app.cx.game();
                println!("{}", serde_json::to_string_pretty(&g.driver).unwrap());
                if let Some(c) = g.car() {
                    println!("{}", serde_json::to_string_pretty(c).unwrap());
                }
            }
            other => panic!("unknown command {}", other),
        }
        for o in cardul_ui::text::take_overflows() {
            eprintln!("OVERFLOW (line {}): {:?}", lineno + 1, o);
        }
        if app.wants_quit() {
            eprintln!("app quit");
            break;
        }
    }
}
