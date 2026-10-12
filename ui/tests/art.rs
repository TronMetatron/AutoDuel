//! The two art sets: the enhanced one loads, every screen draws with it, and F10 switches.
use cardul_sim::{Building, Game};
use cardul_ui::assets::ArtSet;
use cardul_ui::screens::buildings;
use cardul_ui::screens::city::{Arrival, CityScreen};
use cardul_ui::screens::{drive, info};
use cardul_ui::{App, Input, Key};
use std::path::{Path, PathBuf};

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets")
}

fn saves(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cardul-art-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// The enhanced set, and the classic one imported from the original game (tests needing them
/// are skipped otherwise).
fn enhanced_installed() -> bool {
    ArtSet::Enhanced.available(&assets()) && imported()
}

fn imported() -> bool {
    cardul_import::has_game_data(&assets())
}

fn press(app: &mut App, k: Key) {
    let mut inp = Input::default();
    inp.keys.push(k);
    app.tick(&inp);
}

fn with_driver(app: &mut App) {
    let g = Game::new("Tester", false, &app.cx.data, 1);
    app.cx.start_game(g, None);
}

#[test]
fn enhanced_art_is_the_whole_set_at_4x() {
    if !enhanced_installed() {
        return;
    }
    let a = cardul_ui::assets::Assets::load_art(&assets(), ArtSet::Enhanced).unwrap();
    assert_eq!(a.scale, 4);
    let c = cardul_ui::assets::Assets::load(&assets()).unwrap();
    // the full-screen pictures, the font and the suits are at 4x the classic ones
    let pairs = [(&a.title, &c.title), (&a.title_bg, &c.title_bg), (&a.roadmap, &c.roadmap), (&a.end_newspaper, &c.end_newspaper), (&a.suits, &c.suits), (&a.font, &c.font)];
    for (e, o) in pairs {
        assert_eq!((e.w, e.h, e.scale), (o.w * 4, o.h * 4, 4));
    }
    for (e, o) in a.cities.iter().zip(&c.cities) {
        assert_eq!((e.w, e.h, e.scale), (o.w * 4, o.h * 4, 4));
    }
    // every sprite is the enhanced set's own, at 4x its own size and hot-spot data
    for (e, m) in a.shapes.iter().zip(&a.shape_meta).chain(a.scenery.iter().zip(&a.scenery_meta)) {
        assert_eq!((e.w, e.h, e.scale), (m.w * 4, m.h * 4, 4), "{}", m.name);
    }
    let h = a.config.hud.as_ref().expect("HUD layout");
    assert_eq!((h.weapons.len(), h.armor.len(), h.tires.len()), (10, 5, 4));
    assert!(a.hud_speed.is_some() && a.hud_battery.is_some() && a.ground_road.is_some() && a.text_frame.is_some());
    assert_eq!(a.config.roadmap.as_ref().unwrap().markers.len(), 16);
}

#[test]
fn every_screen_draws_with_the_enhanced_art() {
    if !enhanced_installed() {
        return;
    }
    let mut app = App::with_art(&assets(), saves("draw"), Some(ArtSet::Enhanced)).unwrap();
    assert_eq!((app.frame.pw, app.frame.ph), (1280, 800));
    app.draw(); // title
    with_driver(&mut app);
    let city = CityScreen::arrive(&mut app.cx, Arrival::Start);
    app.reset_to(Box::new(city));
    app.draw();
    for k in [Building::TruckStop, Building::Weapons, Building::Assembly, Building::Casino] {
        let s = buildings::open(&mut app.cx, k);
        app.reset_to(s);
        app.tick(&Input::default());
        app.draw();
    }
    app.reset_to(Box::new(info::MapScreen::new()));
    app.draw();
    app.cx.g().add_car(cardul_sim::Car::killer_kart(7));
    let road = drive::road(&mut app.cx, 10, 7);
    app.reset_to(road);
    for _ in 0..60 {
        let mut inp = Input::default();
        inp.held.down = true;
        app.tick(&inp);
        app.draw();
    }
}

#[test]
fn f10_switches_the_graphics_and_remembers() {
    if !enhanced_installed() {
        return;
    }
    let dir = saves("f10");
    let mut app = App::with_art(&assets(), dir.clone(), Some(ArtSet::Classic)).unwrap();
    assert_eq!(app.frame.pw, 320);
    press(&mut app, Key::F(10));
    assert_eq!(app.cx.settings.art, ArtSet::Enhanced);
    assert_eq!(app.frame.pw, 1280);
    app.draw();
    // a new start without a choice on the command line takes the saved one
    let again = App::new(&assets(), dir.clone()).unwrap();
    assert_eq!(again.cx.settings.art, ArtSet::Enhanced);
    press(&mut app, Key::F(10));
    assert_eq!((app.cx.settings.art, app.frame.pw), (ArtSet::Classic, 320));
    assert_eq!(App::new(&assets(), dir).unwrap().cx.settings.art, ArtSet::Classic);
}

#[test]
fn g_on_the_main_menu_switches_the_graphics() {
    if !enhanced_installed() {
        return;
    }
    let mut app = App::with_art(&assets(), saves("menu"), Some(ArtSet::Classic)).unwrap();
    for _ in 0..20 {
        app.tick(&Input::default());
    }
    press(&mut app, Key::Enter); // past the title picture
    app.tick(&Input::default());
    press(&mut app, Key::Char('g'));
    assert_eq!(app.cx.settings.art, ArtSet::Enhanced);
}

#[test]
fn without_the_enhanced_folder_the_classic_art_is_used() {
    if !imported() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("cardul-art-classic-only-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // an assets folder with only the classic pictures
    for sub in ["data", "gfx"] {
        copy_dir(&assets().join(sub), &dir.join(sub));
    }
    let app = App::with_art(&dir, saves("noenh"), Some(ArtSet::Enhanced)).unwrap();
    assert_eq!((app.cx.settings.art, app.frame.pw), (ArtSet::Classic, 320));
    let _ = std::fs::remove_dir_all(&dir);
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

#[test]
fn the_classic_art_is_built_from_the_original_game() {
    if cardul_import::imported(&assets()).is_none() {
        return;
    }
    let c = cardul_ui::assets::Assets::load(&assets()).unwrap();
    assert_eq!((c.scale, c.title.w, c.title.h, c.end_newspaper.w), (1, 320, 200, 320));
    assert_eq!((c.font.w, c.font.h, c.suits.w), (91 * 8, 8, 32));
    for (city, mask) in c.cities.iter().zip(&c.city_masks) {
        assert_eq!((city.w, city.h, mask.w, mask.h), (320, 168, 320, 168));
    }
    assert_eq!((c.shapes.len(), c.scenery.len()), (109, 23));
    for (img, m) in c.shapes.iter().zip(&c.shape_meta).chain(c.scenery.iter().zip(&c.scenery_meta)) {
        assert_eq!((img.w, img.h), (m.w, m.h), "{}", m.name);
    }
    // the DOS screen layout: its HUD boxes, playfield and text-screen frame
    let h = c.config.hud.as_ref().expect("HUD layout");
    assert_eq!((h.weapons.len(), h.armor.len(), h.tires.len()), (10, 5, 4));
    assert!(h.ammo.is_some() && h.bar.is_some() && c.hud_speed.is_some() && c.hud_battery.is_some());
    assert!(c.config.drive_view.is_some() && c.text_frame.is_some() && c.road_hud.is_some());
}
