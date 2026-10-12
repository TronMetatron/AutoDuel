//! World data loaded from `assets/data/*.json`, which `cardul-import` builds from the player's
//! copy of the original game.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CityExit {
    /// N, E, S or W
    pub dir: String,
    /// road segment index 0..16
    pub road: u8,
    pub x_min: i32,
    pub x_span: i32,
    pub y_min: i32,
    pub y_span: i32,
    /// walker position when arriving from this road
    pub arrive_x: i32,
    pub arrive_y: i32,
    pub dest_city: u8,
}

impl CityExit {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x_min && x <= self.x_min + self.x_span && y >= self.y_min && y <= self.y_min + self.y_span
    }
    /// Road heading on leaving the city through this exit (16ths, 0 = east, counter-clockwise).
    pub fn heading(&self) -> u8 {
        match self.dir.as_str() {
            "N" => 4,
            "E" => 0,
            "S" => 12,
            _ => 8,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CityBuilding {
    /// building type 0..15, see [`crate::world::Building`]
    pub kind: u8,
    pub door_x: i32,
    pub door_y: i32,
    pub leave_x: i32,
    pub leave_y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City {
    pub index: u8,
    pub name: String,
    pub exits: Vec<CityExit>,
    pub buildings: Vec<CityBuilding>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Probe {
    pub w: i32,
    pub h: i32,
    pub hx: i32,
    pub hy: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CitiesFile {
    /// walker x (as an unsigned byte) -> hires screen x of the shape's left edge
    pub x_table: Vec<i32>,
    pub probe: Probe,
    pub cities: Vec<City>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RoadExit {
    pub trigger_x: i32,
    pub trigger_y: i32,
    pub start_x: i32,
    pub start_y: i32,
    pub city: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnemySlot {
    pub x: i32,
    pub y: i32,
    /// enemy type, None = empty slot
    pub kind: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Road {
    pub name: String,
    pub index: u8,
    /// map size in tiles (a tile is 128 x 64 world units)
    pub w: i32,
    pub h: i32,
    pub exits: Vec<RoadExit>,
    pub slots: Vec<EnemySlot>,
    /// tile indices, `tiles[row][col]`, 0..31 into the tile set
    #[serde(default)]
    pub tiles: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Arena {
    pub w: i32,
    pub h: i32,
    pub tiles: Vec<Vec<u8>>,
    /// events 0..7: Amateur Night, Division 5/10/15/20, Unlimited, Championship, Practice
    pub events: Vec<Road>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TileObject {
    pub shape: u8,
    /// offset from the tile's grid corner, world units
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Scenery {
    pub index: u8,
    pub name: String,
    pub w: i32,
    pub h: i32,
    pub ax: i32,
    pub ay: i32,
    /// collision half-size in world units
    pub xrad: i32,
    pub yrad: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CargoType {
    pub name: String,
    /// destination building type (see [`crate::world::Building`])
    pub dest_building: u8,
    pub weight: u32,
    pub spaces: u32,
    /// value in dollars
    pub value: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MapMarker {
    pub city: String,
    pub col: i32,
    pub row: i32,
}

#[derive(Clone, Debug)]
pub struct GameData {
    pub x_table: Vec<i32>,
    pub probe: Probe,
    pub cities: Vec<City>,
    pub roads: Vec<Road>,
    pub arena: Arena,
    pub road_tiles: Vec<Vec<TileObject>>,
    pub arena_tiles: Vec<Vec<TileObject>>,
    pub scenery: Vec<Scenery>,
    pub cargo: Vec<CargoType>,
    /// AADA distance table [from][to] (road "length" units used for courier pay)
    pub distances: Vec<Vec<u32>>,
    pub map_markers: Vec<MapMarker>,
}

fn read<T: for<'de> Deserialize<'de>>(dir: &Path, name: &str) -> Result<T, String> {
    let p = dir.join(name);
    let s = std::fs::read_to_string(&p).map_err(|e| format!("{}: {}", p.display(), e))?;
    serde_json::from_str(&s).map_err(|e| format!("{}: {}", p.display(), e))
}

impl GameData {
    /// Load from `assets/data`.
    pub fn load(data_dir: &Path) -> Result<GameData, String> {
        let cf: CitiesFile = read(data_dir, "cities.json")?;
        Ok(GameData {
            x_table: cf.x_table,
            probe: cf.probe,
            cities: cf.cities,
            roads: read(data_dir, "roads.json")?,
            arena: read(data_dir, "arena.json")?,
            road_tiles: read(data_dir, "tiles_road.json")?,
            arena_tiles: read(data_dir, "tiles_arena.json")?,
            scenery: read(data_dir, "scenery.json")?,
            cargo: read(data_dir, "cargo.json")?,
            distances: read(data_dir, "distances.json")?,
            map_markers: read(data_dir, "roadmap_markers.json")?,
        })
    }

    pub fn city(&self, i: u8) -> &City {
        &self.cities[i as usize]
    }
    pub fn city_name(&self, i: u8) -> &str {
        self.cities.get(i as usize).map(|c| c.name.as_str()).unwrap_or("Any City")
    }
    pub fn city_has(&self, city: u8, b: crate::world::Building) -> bool {
        self.city(city).buildings.iter().any(|x| x.kind == b as u8)
    }
    /// Cities that have a building of this kind.
    pub fn cities_with(&self, b: crate::world::Building) -> Vec<u8> {
        self.cities.iter().filter(|c| c.buildings.iter().any(|x| x.kind == b as u8)).map(|c| c.index).collect()
    }
    pub fn distance(&self, from: u8, to: u8) -> u32 {
        self.distances[from as usize][to as usize]
    }
}

/// For tests and tools: the imported world data, or None (with a note) when no copy of the
/// original game has been imported yet (see `cardul-import`).
pub fn test_data() -> Option<GameData> {
    match find_assets_dir() {
        Some(d) => Some(GameData::load(&d.join("data")).expect("assets/data")),
        None => {
            eprintln!("skipped: no game data yet (start Cardul once with your copy of Autoduel, see README)");
            None
        }
    }
}

/// In a test: `let d = need_data!();` gives the imported world data, or ends the test early
/// (passing) when there is none.
#[macro_export]
macro_rules! need_data {
    () => {
        match $crate::data::test_data() {
            Some(d) => d,
            None => return,
        }
    };
}

/// Find `assets/` (with imported world data) next to the executable, in the working directory
/// or up to three parents up.
pub fn find_assets_dir() -> Option<std::path::PathBuf> {
    let mut cands = vec![];
    if let Ok(exe) = std::env::current_exe() {
        let mut d = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..4 {
            if let Some(dd) = d {
                cands.push(dd.join("assets"));
                d = dd.parent().map(|p| p.to_path_buf());
            }
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        let mut d = Some(cwd);
        for _ in 0..4 {
            if let Some(dd) = d {
                cands.push(dd.join("assets"));
                d = dd.parent().map(|p| p.to_path_buf());
            }
        }
    }
    cands.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("assets"));
    cands.into_iter().find(|p| p.join("data").join("cities.json").exists())
}
