//! Fixes from the first AI play-test (11 Oct 2026), played through the real screens and input.
use cardul_sim::{Building, Game};
use cardul_ui::screens::buildings;
use cardul_ui::screens::city::{Arrival, CityScreen};
use cardul_ui::assets::ArtSet;
use cardul_ui::{App, Input, Key};
use std::path::PathBuf;

/// The game with the classic art, or None (the test is skipped) before the original game has
/// been imported.
fn app(name: &str) -> Option<App> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
    let saves = std::env::temp_dir().join(format!("cardul-playtest-{}-{}", name, std::process::id()));
    cardul_import::has_game_data(&assets).then_some(())?;
    Some(App::with_art(&assets, saves, Some(ArtSet::Classic)).expect("app"))
}

fn key(app: &mut App, k: Key) {
    let mut inp = Input::default();
    inp.keys.push(k);
    if let Key::Char(c) = k {
        inp.chars.push(c);
    }
    app.tick(&inp);
}

fn wait(app: &mut App, ticks: u32) {
    for _ in 0..ticks {
        app.tick(&Input::default());
    }
}

fn hold(app: &mut App, left: bool, right: bool, up: bool, down: bool, ticks: u32) {
    let mut inp = Input::default();
    (inp.held.left, inp.held.right, inp.held.up, inp.held.down) = (left, right, up, down);
    for _ in 0..ticks {
        app.tick(&inp);
    }
}

/// A new driver in New York, inside building `kind`, its greeting up.
fn in_building(name: &str, kind: Building) -> Option<App> {
    let mut app = app(name)?;
    let g = Game::new("Tester", false, &app.cx.data, 1);
    app.cx.start_game(g, None);
    let s = buildings::open(&mut app.cx, kind);
    app.reset_to(s);
    wait(&mut app, 300);
    Some(app)
}

#[test]
fn a_key_over_a_reply_also_answers_the_menu() {
    let Some(mut app) = in_building("typeahead", Building::TruckStop) else { return };
    let money = app.cx.game().driver.money;
    key(&mut app, Key::Char('5')); // body armor
    wait(&mut app, 5);
    assert_eq!(app.cx.game().driver.money, money - 250);
    // "You should always wear body armor!" is up; 6 = leave
    key(&mut app, Key::Char('6'));
    wait(&mut app, 100);
    assert!(app.top_as::<CityScreen>().is_some(), "6 over the reply should leave the Truck Stop");
}

#[test]
fn space_over_a_reply_only_skips_it() {
    let Some(mut app) = in_building("skip", Building::TruckStop) else { return };
    key(&mut app, Key::Char('3')); // batteries: no car
    wait(&mut app, 5);
    key(&mut app, Key::Space);
    wait(&mut app, 100);
    assert!(app.top_as::<CityScreen>().is_none());
}

#[test]
fn pushing_into_the_wall_near_a_door_finds_it() {
    // the New York Assembly Line door is the end of a passage one row high
    for (up, rows) in [(true, 3i32), (false, 3)] {
        let Some(mut app) = app("door") else { return };
        let b = app.cx.data.city(7).buildings.iter().find(|b| b.kind == Building::Assembly as u8).unwrap().clone();
        let g = Game::new("Tester", false, &app.cx.data, 1);
        app.cx.start_game(g, None);
        let s = CityScreen::arrive(&mut app.cx, Arrival::Building(Building::Assembly));
        app.reset_to(Box::new(s));
        hold(&mut app, false, true, false, false, 30); // out to the street: 10 steps
        hold(&mut app, false, false, up, !up, 3 * rows as u32);
        let (_, x, y) = app.top_as::<CityScreen>().unwrap().pos();
        assert_eq!((x, y), (b.leave_x + 10, if up { b.leave_y - rows } else { b.leave_y + rows }));
        hold(&mut app, true, false, false, false, 60);
        assert!(app.top_as::<CityScreen>().is_none(), "pushing left from y={} should reach the door", y);
    }
}

#[test]
fn esc_on_the_main_menu_asks_before_quitting() {
    let Some(mut app) = app("quit") else { return };
    wait(&mut app, 20);
    key(&mut app, Key::Esc); // leaves the title picture
    wait(&mut app, 2);
    key(&mut app, Key::Esc);
    wait(&mut app, 2);
    assert!(!app.wants_quit());
    key(&mut app, Key::Char('n'));
    wait(&mut app, 2);
    key(&mut app, Key::Esc);
    wait(&mut app, 2);
    key(&mut app, Key::Char('y'));
    assert!(app.wants_quit());
}

#[test]
fn unspent_creation_points_are_confirmed() {
    let Some(mut app) = app("create") else { return };
    wait(&mut app, 20);
    key(&mut app, Key::Enter);
    wait(&mut app, 2);
    key(&mut app, Key::Char('3'));
    for c in "Ann".chars() {
        let mut inp = Input::default();
        inp.keys.push(Key::Char(c.to_ascii_lowercase()));
        inp.chars.push(c);
        app.tick(&inp);
    }
    key(&mut app, Key::Enter);
    key(&mut app, Key::Right);
    key(&mut app, Key::Char('d'));
    assert!(app.cx.game.is_none(), "49 points left: D asks first");
    key(&mut app, Key::Char('n'));
    key(&mut app, Key::Char('d'));
    key(&mut app, Key::Char('y'));
    let d = &app.cx.game().driver;
    assert_eq!((d.name.as_str(), d.driving), ("Ann", 1));
}
