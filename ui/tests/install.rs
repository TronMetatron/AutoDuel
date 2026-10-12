//! The first-start setup screen: it asks for the game, then imports it.
use cardul_ui::install::{import_from, Installer, Outcome};
use cardul_ui::{Input, Key};
use std::path::PathBuf;

fn original() -> Option<PathBuf> {
    let dir = std::env::var_os("CARDUL_ORIGINAL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../original"));
    cardul_import::locate(&dir).map(|_| dir)
}

fn key(k: Key) -> Input {
    let mut i = Input::default();
    i.keys.push(k);
    i
}

#[test]
fn asks_for_the_game_then_imports_it() {
    let tmp = std::env::temp_dir().join(format!("cardul-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let (assets, place) = (tmp.join("assets"), tmp.join("original"));
    std::fs::create_dir_all(&assets).unwrap();

    // nothing there yet: the screen says where to put the files, Enter looks again
    let err = import_from(std::slice::from_ref(&place), &assets).unwrap_err();
    let mut inst = Installer::new(assets.clone(), vec![place.clone()], Some(err));
    assert!(place.is_dir(), "the folder to put the game in is made");
    assert_eq!(inst.draw().pw, 320);
    assert!(inst.tick(&key(Key::Enter)).is_none());
    assert!(inst.tick(&Input::default()).is_none());

    // a folder dropped on the window is used
    if let Some(dir) = original() {
        match inst.retry(Some(&dir)) {
            Some(Outcome::Imported(s)) => assert!(s.pictures > 100),
            _ => panic!("the dropped folder was not imported"),
        }
        assert!(cardul_import::has_game_data(&assets) && !cardul_import::needs_import(&assets));
    }
    assert!(matches!(inst.tick(&key(Key::Esc)), Some(Outcome::Quit)));
    let _ = std::fs::remove_dir_all(&tmp);
}
