//! Every part of the game makes some sound: the cues the screens ask for, and the engine.
use cardul_sim::game::DeathOutcome;
use cardul_sim::{Building, Car, Game, Sfx};
use cardul_ui::assets::ArtSet;
use cardul_ui::screens::city::{Arrival, CityScreen};
use cardul_ui::screens::{buildings, death::DeathScreen, drive};
use cardul_ui::{App, Input, Key};
use std::path::PathBuf;

/// The game with the classic art, or None (the test is skipped) before the original game has
/// been imported.
fn app(name: &str) -> Option<App> {
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets");
    let saves = std::env::temp_dir().join(format!("cardul-sound-{}-{}", name, std::process::id()));
    cardul_import::has_game_data(&assets).then_some(())?;
    Some(App::with_art(&assets, saves, Some(ArtSet::Classic)).expect("app"))
}

fn with_driver(app: &mut App) {
    let g = Game::new("Tester", false, &app.cx.data, 1);
    app.cx.start_game(g, None);
}

fn press(app: &mut App, k: Key) {
    let mut inp = Input::default();
    inp.keys.push(k);
    app.tick(&inp);
}

fn idle(app: &mut App, n: u32) {
    for _ in 0..n {
        app.tick(&Input::default());
    }
}

fn hold(app: &mut App, f: impl Fn(&mut Input), n: u32) {
    let mut inp = Input::default();
    f(&mut inp);
    for _ in 0..n {
        app.tick(&inp);
    }
}

#[test]
fn title_tune_then_menu_clicks() {
    let Some(mut a) = app("title") else { return };
    idle(&mut a, 2);
    assert!(a.take_sfx().contains(&Sfx::Title));
    idle(&mut a, 20);
    press(&mut a, Key::Enter);
    a.take_sfx();
    press(&mut a, Key::Char('x'));
    assert_eq!(a.take_sfx(), vec![Sfx::Key]);
}

#[test]
fn shops_ring_the_till_and_beep_at_refusals() {
    let Some(mut a) = app("shop") else { return };
    with_driver(&mut a);
    let s = buildings::open(&mut a.cx, Building::TruckStop);
    a.reset_to(s);
    idle(&mut a, 300);
    a.take_sfx();
    press(&mut a, Key::Char('5')); // body armor, $250
    assert!(a.take_sfx().contains(&Sfx::Cash));
    idle(&mut a, 100);
    press(&mut a, Key::Char('5')); // already have it
    assert!(a.take_sfx().contains(&Sfx::Beep));
}

#[test]
fn walking_in_town_and_going_in() {
    let Some(mut a) = app("town") else { return };
    with_driver(&mut a);
    // out of the New York Assembly Line and back in: footsteps, then the door
    let s = CityScreen::arrive(&mut a.cx, Arrival::Building(Building::Assembly));
    a.reset_to(Box::new(s));
    a.take_sfx();
    hold(&mut a, |i| i.held.right = true, 30);
    assert!(a.take_sfx().contains(&Sfx::Step));
    assert_eq!(a.engine(), None, "no car, no engine");
    hold(&mut a, |i| i.held.left = true, 60);
    assert!(a.take_sfx().contains(&Sfx::Door));
}

#[test]
fn the_engine_runs_in_the_car() {
    let Some(mut a) = app("engine") else { return };
    with_driver(&mut a);
    a.cx.g().add_car(Car::killer_kart(7));
    let s = CityScreen::arrive(&mut a.cx, Arrival::TruckStop);
    a.reset_to(Box::new(s));
    let idle_revs = a.engine().expect("idling in town");
    let road = drive::road(&mut a.cx, 10, 7);
    a.reset_to(road);
    hold(&mut a, |i| i.held.down = true, 120);
    let revs = a.engine().expect("driving");
    assert!(revs > idle_revs, "{revs} > {idle_revs}");
    // F7: sound off silences the engine too
    press(&mut a, Key::F(7));
    assert_eq!(a.engine(), None);
}

#[test]
fn arriving_and_dying() {
    let Some(mut a) = app("arrive") else { return };
    with_driver(&mut a);
    let s = CityScreen::arrive(&mut a.cx, Arrival::Road(10));
    a.reset_to(Box::new(s));
    assert!(a.take_sfx().contains(&Sfx::Arrive));
    a.reset_to(Box::new(DeathScreen::new(DeathOutcome::GameOver)));
    idle(&mut a, 2);
    assert!(a.take_sfx().contains(&Sfx::Death));
}
