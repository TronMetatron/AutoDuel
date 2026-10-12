//! Cardul ships without anything from the original game. The player supplies their own copy of
//! Autoduel for DOS; this crate checks it and builds Cardul's classic graphics
//! (`assets/gfx`, `assets/art.json`) and world data (`assets/data`) from its files.
//!
//! ```no_run
//! let o = cardul_import::Original::open(std::path::Path::new("original")).unwrap();
//! cardul_import::import(&o, std::path::Path::new("assets")).unwrap();
//! ```
pub mod art;
pub mod font;
pub mod map;
pub mod pic;
pub mod tlb;
pub mod world;

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Bumped whenever the import's output changes, so older imports are redone.
pub const VERSION: u32 = 1;

/// The files the import reads (names as on the DOS disk, any case).
pub const NEEDED: [&str; 31] = [
    "table.dec", "cars.tlb", "carstoo.tlb", "terrain.tlb", "frame.tlb", "intro.tlb", "win.tlb", "casino.tlb",
    "city00.tlb", "city01.tlb", "city02.tlb", "city03.tlb", "city04.tlb", "city05.tlb", "city06.tlb", "city07.tlb",
    "city08.tlb", "city09.tlb", "city0a.tlb", "city0b.tlb", "city0c.tlb", "city0d.tlb", "city0e.tlb", "city0f.tlb",
    "citydat", "roadmap", "roadmod", "arenstuf", "drivers", "autoduel.ovl", "init.ovl",
];

/// Why a folder can't be used.
#[derive(Debug)]
pub enum Problem {
    /// no Autoduel files in or under the folder
    NotFound(PathBuf),
    /// some of [`NEEDED`] are missing
    Missing { dir: PathBuf, files: Vec<String> },
    /// a file is there but isn't what the game's files look like
    Bad(String),
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Problem::NotFound(d) => write!(f, "No Autoduel (DOS) files were found in {}.", d.display()),
            Problem::Missing { dir, files } => {
                write!(f, "{} is missing some of Autoduel's files: {}.", dir.display(), files.join(", "))
            }
            Problem::Bad(m) => write!(f, "These don't look like Autoduel's files: {}", m),
        }
    }
}

/// A player's copy of the DOS game.
pub struct Original {
    /// the folder the files are in
    pub dir: PathBuf,
    files: HashMap<String, Vec<u8>>,
}

/// The folder that holds the game's files: `dir` itself or a folder up to two levels below it.
pub fn locate(dir: &Path) -> Option<PathBuf> {
    fn has(d: &Path) -> bool {
        std::fs::read_dir(d).map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case("citydat"))).unwrap_or(false)
    }
    fn walk(d: &Path, depth: u32) -> Option<PathBuf> {
        if has(d) {
            return Some(d.to_path_buf());
        }
        if depth == 0 {
            return None;
        }
        let mut subs: Vec<PathBuf> = std::fs::read_dir(d).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        subs.sort();
        subs.iter().find_map(|s| walk(s, depth - 1))
    }
    walk(dir, 2)
}

impl Original {
    /// Read the game's files from `dir` (or the folder under it that has them) and check them.
    pub fn open(dir: &Path) -> Result<Original, Problem> {
        let found = locate(dir).ok_or_else(|| Problem::NotFound(dir.to_path_buf()))?;
        let mut names: HashMap<String, PathBuf> = HashMap::new();
        for e in std::fs::read_dir(&found).map_err(|_| Problem::NotFound(dir.to_path_buf()))?.flatten() {
            if e.path().is_file() {
                names.insert(e.file_name().to_string_lossy().to_ascii_lowercase(), e.path());
            }
        }
        let missing: Vec<String> = NEEDED.iter().filter(|n| !names.contains_key(**n)).map(|n| n.to_uppercase()).collect();
        if !missing.is_empty() {
            return Err(Problem::Missing { dir: found, files: missing });
        }
        let mut files = HashMap::new();
        for n in NEEDED {
            let p = &names[n];
            let d = std::fs::read(p).map_err(|e| Problem::Bad(format!("{}: {}", p.display(), e)))?;
            files.insert(n.to_string(), d);
        }
        let o = Original { dir: found, files };
        o.check().map_err(Problem::Bad)?;
        Ok(o)
    }

    pub fn get(&self, name: &str) -> Result<&[u8], String> {
        self.files.get(name).map(|v| v.as_slice()).ok_or_else(|| format!("{} is missing", name.to_uppercase()))
    }

    pub fn huffman(&self) -> Result<tlb::Huffman, String> {
        tlb::Huffman::new(self.get("table.dec")?)
    }

    /// Quick checks that these are the game's files (the full import checks the rest).
    fn check(&self) -> Result<(), String> {
        world::cities(self)?;
        let h = self.huffman()?;
        tlb::read("city00.tlb", self.get("city00.tlb")?, &h)?;
        tlb::read("cars.tlb", self.get("cars.tlb")?, &h)?;
        Ok(())
    }
}

/// What [`import`] made.
#[derive(Debug)]
pub struct Summary {
    pub pictures: usize,
    pub source: PathBuf,
}

fn write_json(dir: &Path, name: &str, v: &serde_json::Value) -> Result<(), String> {
    let p = dir.join(name);
    let s = serde_json::to_string_pretty(v).map_err(|e| e.to_string())?;
    std::fs::write(&p, s).map_err(|e| format!("{}: {}", p.display(), e))
}

fn count_png(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| {
            let p = e.path();
            if p.is_dir() { count_png(&p) } else { usize::from(p.extension().is_some_and(|x| x == "png")) }
        })
        .sum()
}

/// Build the classic graphics and the world data into `assets` (replacing any earlier import).
pub fn import(o: &Original, assets: &Path) -> Result<Summary, String> {
    let tmp = assets.join(".import");
    let _ = std::fs::remove_dir_all(&tmp);
    let (gfx, data) = (tmp.join("gfx"), tmp.join("data"));
    std::fs::create_dir_all(&gfx).map_err(|e| format!("{}: {}", gfx.display(), e))?;
    std::fs::create_dir_all(&data).map_err(|e| format!("{}: {}", data.display(), e))?;

    let roads = world::roads(o)?;
    let (arena, arena_tiles) = world::arena(o)?;
    let (cargo, distances) = world::cargo(o)?;
    let mut font = font::Font::from_overlay(o.get("init.ovl")?)?;
    {
        let libs = art::Libs::new(o);
        for t in 0..4 {
            let p = libs.pic("frame.tlb", 4 + t, false)?;
            for (r, row) in font.tires[t].iter_mut().enumerate().take(p.h.min(8)) {
                *row = (0..8.min(p.w)).fold(0u8, |acc, x| acc | (u8::from(p.at(x, r) > 0) << (7 - x)));
            }
        }
        font.tires[4] = font.tires[0];
        font.tires[5] = font.tires[1];
    }
    let built = art::build(o, &font, &roads, &gfx)?;

    write_json(&data, "cities.json", &world::cities(o)?)?;
    write_json(&data, "roads.json", &roads)?;
    write_json(&data, "arena.json", &arena)?;
    write_json(&data, "tiles_road.json", &world::road_tiles(o)?)?;
    write_json(&data, "tiles_arena.json", &arena_tiles)?;
    write_json(&data, "cargo.json", &cargo)?;
    write_json(&data, "distances.json", &distances)?;
    write_json(&data, "shapes.json", &built.shapes)?;
    write_json(&data, "scenery.json", &world::scenery(&built.scenery))?;
    write_json(&data, "roadmap_markers.json", &built.markers)?;
    write_json(&tmp, "art.json", &art::config())?;
    let sizes: serde_json::Map<String, serde_json::Value> =
        NEEDED.iter().map(|n| (n.to_string(), serde_json::json!(o.files[*n].len()))).collect();
    write_json(&tmp, "original.json", &serde_json::json!({"version": VERSION, "game": "Autoduel (DOS)", "files": sizes}))?;

    // swap the new import in
    for name in ["gfx", "data", "art.json", "original.json"] {
        let (from, to) = (tmp.join(name), assets.join(name));
        if to.is_dir() {
            std::fs::remove_dir_all(&to).map_err(|e| format!("{}: {}", to.display(), e))?;
        } else if to.exists() {
            std::fs::remove_file(&to).map_err(|e| format!("{}: {}", to.display(), e))?;
        }
        std::fs::rename(&from, &to).map_err(|e| format!("{}: {}", to.display(), e))?;
    }
    let _ = std::fs::remove_dir_all(&tmp);
    Ok(Summary { pictures: count_png(&assets.join("gfx")), source: o.dir.clone() })
}

/// The importer version that made the files in `assets`, if any.
pub fn imported(assets: &Path) -> Option<u32> {
    let s = std::fs::read_to_string(assets.join("original.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&s).ok()?;
    if !assets.join("data").join("cities.json").is_file() || !assets.join("gfx").is_dir() {
        return None;
    }
    v["version"].as_u64().map(|v| v as u32)
}

/// Are there classic graphics and world data in `assets` (imported, or built some other way)?
pub fn has_game_data(assets: &Path) -> bool {
    assets.join("data").join("cities.json").is_file() && assets.join("gfx").is_dir()
}

/// Does `assets` need importing: no game data yet, or an import by an older version of this
/// crate? (Game data built some other way is left alone.)
pub fn needs_import(assets: &Path) -> bool {
    match imported(assets) {
        Some(v) => v != VERSION,
        None => !has_game_data(assets),
    }
}
