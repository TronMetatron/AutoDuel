//! Renders staged driving scenes to PNG files, to check the driving engine's graphics:
//! a road in motion, a fire-fight with projectiles, the driver on foot next to a wreck,
//! and the arena.
//!
//!     cargo run -p cardul-ui --example combat_shots -- OUT_DIR
use cardul_sim::car::{Car, Weapon};
use cardul_sim::combat::{Battle, BattleKind, Enemy, WRECK};
use cardul_sim::tables::{w, ARENA_ENEMIES, ROAD_ENEMIES};
use cardul_sim::Game;
use cardul_ui::screens::drive::DriveScreen;
use cardul_ui::{App, Held, Input};
use std::path::{Path, PathBuf};

/// A van with one of most things, for showing off.
fn war_wagon(city: u8) -> Car {
    let mut c = Car::killer_kart(city);
    c.name = "War Wagon".into();
    c.body = 6;
    c.chassis = 3;
    c.set_plant(2);
    let mut add = |slot: usize, kind: u8, mount: u8, ammo: u8| {
        let mut wp = Weapon::new(kind, mount);
        wp.ammo = ammo;
        c.weapons[slot] = Some(wp);
    };
    add(0, w::MG, 0, 20);
    add(1, w::RL, 0, 10);
    add(2, w::SS, 1, 10);
    add(3, w::OJ, 1, 10);
    add(4, w::PS, 2, 10);
    add(5, w::MD, 1, 10);
    add(6, w::LA, 0, 0);
    add(7, w::RR, 3, 10);
    for a in c.armor.iter_mut() {
        a.pts = 20;
        a.max = 20;
    }
    c
}

struct Stage {
    app: App,
    out: PathBuf,
}

impl Stage {
    fn new(out: &Path) -> Stage {
        let assets = cardul_sim::data::find_assets_dir().expect("assets dir");
        let app = App::new(&assets, std::env::temp_dir().join("cardul-shots")).expect("app");
        Stage { app, out: out.to_path_buf() }
    }
    /// A fresh cheat driver in `city` with the war wagon.
    fn game(&mut self, city: u8, seed: u32) {
        let mut g = Game::new("Shots", true, &self.app.cx.data, seed);
        g.driver.city = city;
        g.add_car(war_wagon(city));
        self.app.cx.game = Some(g);
    }
    fn battle(&mut self, kind: BattleKind, setup: impl FnOnce(&mut Battle, &mut Game)) {
        let (data, game) = self.app.cx.dg();
        let mut b = Battle::new(game, data, kind);
        setup(&mut b, game);
        self.app.reset_to(Box::new(DriveScreen::new(b)));
    }
    fn hold(&mut self, dirs: &str, ticks: u32) {
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
        for _ in 0..ticks {
            self.app.tick(&Input { held: h, ..Default::default() });
        }
    }
    fn key(&mut self, k: cardul_ui::Key) {
        let mut inp = Input::default();
        inp.keys.push(k);
        self.app.tick(&inp);
    }
    fn shot(&mut self, name: &str) {
        self.app.draw();
        let p = self.out.join(name);
        self.app.frame.save_png(&p).expect("png");
        eprintln!("saved {}", p.display());
    }
}

fn enemy(kind: u8, slot: usize, x: i32, y: i32, heading: u8) -> Option<Enemy> {
    let mut e = Enemy::new(&ROAD_ENEMIES[kind as usize], kind, slot, x, y);
    e.heading = heading;
    Some(e)
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/cardul-shots".into()));
    std::fs::create_dir_all(&out).expect("out dir");
    let mut s = Stage::new(&out);

    // 1. a road in motion: the long straight near the Albany end of the NY Thruway,
    //    with a car coming the other way
    s.game(3, 11);
    s.battle(BattleKind::Road { road: 3, from_city: 3 }, |b, _| {
        b.enemies = vec![None; 16];
        b.player.x = 2410;
        b.player.y = 826;
        b.player.heading = 0;
        b.enemies[0] = enemy(12, 0, 2560, 830, 8);
        b.enemies[1] = enemy(4, 1, 2640, 818, 8);
    });
    s.hold("right", 84);
    s.shot("road_motion.png");
    s.hold("right+fire", 16);
    s.shot("road_motion_2.png");

    // 2. a fire-fight: rockets, gun fire, smoke and oil, a burning wreck
    s.game(3, 12);
    s.battle(BattleKind::Road { road: 3, from_city: 3 }, |b, _| {
        b.enemies = vec![None; 16];
        let (x, y) = (b.player.x, b.player.y);
        b.enemies[0] = enemy(5, 0, x + 50, y + 2, 8);
        b.enemies[1] = enemy(1, 1, x + 30, y - 22, 12);
        let mut wreck = enemy(3, 2, x + 22, y + 22, 2).unwrap();
        wreck.state = WRECK;
        wreck.burning = true;
        b.enemies[2] = Some(wreck);
        b.player.heading = 0;
        b.player.new_heading = 0;
    });
    // smoke and oil behind, then rockets and the gun ahead
    s.key(cardul_ui::Key::Char('3'));
    s.hold("fire", 8);
    s.key(cardul_ui::Key::Char('4'));
    s.hold("fire", 8);
    s.key(cardul_ui::Key::Char('2'));
    s.hold("right+fire", 14);
    s.key(cardul_ui::Key::Char('1'));
    s.hold("fire", 6);
    s.shot("combat.png");
    s.hold("fire", 30);
    s.shot("combat_2.png");

    // 3. on foot next to a stopped wreck
    s.game(3, 13);
    s.battle(BattleKind::Road { road: 3, from_city: 3 }, |b, _| {
        b.enemies = vec![None; 16];
        let (x, y) = (b.player.x, b.player.y);
        let mut wreck = enemy(5, 0, x + 24, y + 4, 6).unwrap();
        wreck.state = WRECK;
        b.enemies[0] = Some(wreck);
    });
    s.key(cardul_ui::Key::Char('g'));
    s.hold("right", 40);
    s.hold("right+down", 12);
    s.shot("on_foot.png");
    s.hold("right", 32);
    s.shot("on_foot_salvage.png");

    // 4. the arena: Division 20, opponents closing in
    s.game(4, 14);
    s.battle(BattleKind::Arena { event: 4 }, |b, _| {
        let (x, y) = (b.player.x, b.player.y);
        let mut e1 = Enemy::new(&ARENA_ENEMIES[10], 10, 0, x - 40, y - 40);
        e1.heading = 14;
        let mut e2 = Enemy::new(&ARENA_ENEMIES[12], 12, 2, x + 36, y - 30);
        e2.heading = 10;
        b.enemies[0] = Some(e1);
        b.enemies[2] = Some(e2);
    });
    s.hold("up", 30);
    s.hold("up+fire", 12);
    s.shot("arena.png");
    s.hold("up+left+fire", 30);
    s.shot("arena_2.png");

    // 5. transitions: arriving at the end of a road, death, leaving the arena
    s.game(3, 15);
    s.battle(BattleKind::Road { road: 3, from_city: 3 }, |b, _| {
        b.enemies = vec![None; 16];
        b.player.x = 2944 - 30;
        b.player.y = 576;
    });
    s.hold("right", 60);
    s.shot("arrive_city.png");

    s.game(3, 16);
    s.battle(BattleKind::Road { road: 3, from_city: 3 }, |b, g| {
        b.enemies = vec![None; 16];
        let (x, y) = (b.player.x, b.player.y);
        for (i, (k, dx, dy)) in [(15u8, 30, 0), (14, 24, -20), (13, 24, 20)].iter().enumerate() {
            b.enemies[i] = enemy(*k, i, x + dx, y + dy, 8);
            b.enemies[i].as_mut().unwrap().state = 3;
        }
        g.driver.clone = None;
        if let Some(c) = g.car_mut() {
            for a in c.armor.iter_mut() {
                a.pts = 0;
            }
        }
    });
    s.hold("", 400);
    s.shot("death.png");

    s.game(4, 17);
    s.battle(BattleKind::Arena { event: 7 }, |_, _| {});
    s.hold("down", 120);
    eprintln!("arena result: {:?}", s.app.cx.arena_result);
}
