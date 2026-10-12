//! The import, run on a real copy of Autoduel (DOS): set `CARDUL_ORIGINAL` to its folder, or put
//! it in `original/` at the repository root. Without one these tests are skipped.
use cardul_import::{import, Original};
use std::path::PathBuf;

fn original() -> Option<Original> {
    let dir = std::env::var_os("CARDUL_ORIGINAL")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../original"));
    match Original::open(&dir) {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("skipped: {e}");
            None
        }
    }
}

fn json(dir: &std::path::Path, name: &str) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("data").join(name)).unwrap()).unwrap()
}

#[test]
fn imports_everything_the_game_needs() {
    let Some(o) = original() else { return };
    let out = std::env::temp_dir().join(format!("cardul-import-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let s = import(&o, &out).unwrap();
    // 109 shapes, 23 roadside objects, 16 towns and their masks, 6 screens and the font, 5 HUD
    assert_eq!(s.pictures, 109 + 23 + 32 + 6 + 5);
    assert_eq!(cardul_import::imported(&out), Some(cardul_import::VERSION));
    assert!(!cardul_import::needs_import(&out));

    let cities = json(&out, "cities.json");
    let names: Vec<&str> = cities["cities"].as_array().unwrap().iter().map(|c| c["name"].as_str().unwrap()).collect();
    assert_eq!(names, cardul_import::world::CITY_NAMES);
    assert_eq!(cities["x_table"].as_array().unwrap().len(), 256);

    let roads = json(&out, "roads.json");
    assert_eq!(roads.as_array().unwrap().len(), 17);
    assert_eq!(roads[0]["name"], "Nth-Sth Expwy");
    for r in roads.as_array().unwrap() {
        let (w, h) = (r["w"].as_u64().unwrap(), r["h"].as_u64().unwrap());
        assert_eq!(r["tiles"].as_array().unwrap().len() as u64, h);
        assert_eq!(r["tiles"][0].as_array().unwrap().len() as u64, w);
        assert_eq!(r["slots"].as_array().unwrap().len(), 16);
    }
    let arena = json(&out, "arena.json");
    assert_eq!(arena["events"].as_array().unwrap().len(), 8);
    assert_eq!(arena["events"][0]["name"], "Amateur Night");

    let cargo = json(&out, "cargo.json");
    assert_eq!(cargo.as_array().unwrap().len(), 100);
    assert_eq!(cargo[0]["name"], "Legal documents");
    let d = json(&out, "distances.json");
    for i in 0..16 {
        assert_eq!(d[i][i], 0);
        for j in 0..16 {
            assert_eq!(d[i][j], d[j][i], "distance {i} -> {j}");
        }
    }
    assert_eq!(json(&out, "shapes.json").as_array().unwrap().len(), 109);
    assert_eq!(json(&out, "scenery.json").as_array().unwrap().len(), 23);
    assert_eq!(json(&out, "roadmap_markers.json").as_array().unwrap().len(), 16);
    for f in ["art.json", "gfx/title.png", "gfx/font.png", "gfx/hud/road_hud.png", "gfx/hud/text_frame.png", "gfx/city/c0f_mask.png"] {
        assert!(out.join(f).is_file(), "{f}");
    }
    // a second import replaces the first
    import(&o, &out).unwrap();
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn a_folder_without_the_game_is_refused() {
    let dir = std::env::temp_dir().join(format!("cardul-import-empty-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    assert!(matches!(Original::open(&dir), Err(cardul_import::Problem::NotFound(_))));
    // some files but not all
    std::fs::write(dir.join("sub").join("CITYDAT"), [0u8; 1920]).unwrap();
    match Original::open(&dir) {
        Err(cardul_import::Problem::Missing { files, .. }) => assert!(files.contains(&"TABLE.DEC".to_string())),
        Err(e) => panic!("{e}"),
        Ok(_) => panic!("accepted"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn packbits() {
    // 3 literals, then 'x' four times
    assert_eq!(cardul_import::pic::packbits(&[2, b'a', b'b', b'c', 0xFD, b'x'], 7), b"abcxxxx".to_vec());
}
