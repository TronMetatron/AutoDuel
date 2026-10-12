//! The world data (`assets/data/*.json`) from the DOS game's data files.
//!
//! * `citydat`: sixteen 120-byte city records (name, exits, buildings);
//! * `roadmap`: the tile maps of the 17 roads, 512 bytes each;
//! * `roadmod`: the 32 roadside tiles (32 objects of shape, x, y each);
//! * `drivers`: the four saved-driver slots; each ends with the 17 road records (name, size,
//!   exits and the places where cars wait);
//! * `arenstuf`: the 8 arena events, the arena's tile map and its tiles;
//! * `autoduel.ovl`: the courier jobs (names, weights, values, destinations) and the AADA
//!   distance table.
//!
//! City coordinates: the DOS game puts the walker at picture x = 2x + 166, y = 2y + 4. Cardul
//! keeps x and stores y - 30, so its "screen y = 64 + 2y" lands on the same row.
use crate::Original;
use serde_json::{json, Value};

/// The sixteen Fortress Towns, in the game's order.
pub const CITY_NAMES: [&str; 16] = [
    "Watertown",
    "Manchester",
    "Buffalo",
    "Syracuse",
    "Albany",
    "Boston",
    "Scranton",
    "New York",
    "Providence",
    "Pittsburgh",
    "Harrisburg",
    "Philadelphia",
    "Atlantic City",
    "Baltimore",
    "Dover",
    "Washington",
];

/// Walker x -> picture x.
pub const CITY_X0: i32 = 166;
/// Walker y as the DOS game stores it, minus this, is Cardul's.
pub const CITY_Y_SHIFT: i32 = 30;

/// Collision half-sizes of the 23 roadside objects (world units). Game rules, as in the
/// original's tables.
const XRAD: [i32; 23] = [2, 4, 12, 2, 2, 2, 2, 2, 3, 8, 9, 1, 1, 2, 1, 2, 1, 7, 5, 4, 6, 6, 14];
const YRAD: [i32; 23] = [4, 2, 2, 12, 2, 2, 2, 2, 2, 6, 4, 2, 1, 3, 2, 2, 3, 4, 3, 2, 2, 14, 6];
pub const SCENERY_NAMES: [&str; 23] = [
    "rail_v",
    "rail_h_short",
    "rail_h_long",
    "rail_v_long",
    "rail_corner_a",
    "rail_corner_b",
    "rail_corner_c",
    "rail_corner_d",
    "grass_tuft",
    "tree_a",
    "tree_b",
    "grave_cross",
    "headstone",
    "pole",
    "post_small",
    "rock",
    "sign",
    "building",
    "cow_a",
    "cow_b",
    "car_wreck",
    "rubble_wall_v",
    "rubble_wall_h",
];

/// Cars waiting on the roads when a new game starts: (road, slot, enemy type). Game rules.
const ROAD_PRESETS: [(usize, usize, u8); 18] = [
    (0, 14, 5),
    (2, 2, 11),
    (4, 11, 3),
    (5, 10, 6),
    (5, 13, 9),
    (6, 0, 6),
    (6, 3, 13),
    (7, 14, 2),
    (8, 6, 5),
    (8, 11, 5),
    (10, 7, 5),
    (11, 4, 1),
    (11, 15, 9),
    (12, 6, 2),
    (14, 12, 12),
    (15, 7, 4),
    (16, 3, 4),
    (16, 14, 13),
];

fn s8(v: u8) -> i32 {
    v as i8 as i32
}
fn s16(d: &[u8], i: usize) -> i32 {
    (d[i] as u16 | (d[i + 1] as u16) << 8) as i16 as i32
}
fn u16le(d: &[u8], i: usize) -> u32 {
    d[i] as u32 | (d[i + 1] as u32) << 8
}
fn name_of(d: &[u8]) -> String {
    let end = d.iter().position(|&b| b == 0).unwrap_or(d.len());
    String::from_utf8_lossy(&d[..end]).trim().to_string()
}

pub fn cities(o: &Original) -> Result<Value, String> {
    let d = o.get("citydat")?;
    if d.len() < 16 * 120 {
        return Err("citydat is too short".into());
    }
    const DIRS: [&str; 4] = ["N", "E", "S", "W"];
    let mut out = vec![];
    for c in 0..16 {
        let r = &d[c * 120..(c + 1) * 120];
        let name = name_of(&r[..16]);
        // FIX: the DOS data spells it "Pittsburg"
        let name = if name == "Pittsburg" { "Pittsburgh".to_string() } else { name };
        if name != CITY_NAMES[c] {
            return Err(format!("citydat: city {} is '{}', expected '{}'", c, name, CITY_NAMES[c]));
        }
        let mut exits = vec![];
        for (k, dir) in DIRS.iter().enumerate() {
            let e = &r[0x11 + 10 * k..0x11 + 10 * k + 10];
            if e[0] == 0xFF {
                continue;
            }
            exits.push(json!({
                "dir": dir, "road": e[0], "x_min": s8(e[1]), "x_span": e[2],
                "y_min": s8(e[3]) - CITY_Y_SHIFT, "y_span": e[4],
                "arrive_x": s8(e[5]), "arrive_y": s8(e[6]) - CITY_Y_SHIFT, "dest_city": e[9],
            }));
        }
        let mut blds = vec![];
        for k in 0..10 {
            let b = &r[0x39 + 5 * k..0x39 + 5 * k + 5];
            if b[0] == 0xFF {
                continue;
            }
            blds.push(json!({
                "kind": b[0], "door_x": s8(b[1]), "door_y": s8(b[2]) - CITY_Y_SHIFT,
                "leave_x": s8(b[3]), "leave_y": s8(b[4]) - CITY_Y_SHIFT,
            }));
        }
        out.push(json!({"index": c, "name": name, "exits": exits, "buildings": blds}));
    }
    let x_table: Vec<i32> = (0..256).map(|x| 2 * (x as u8 as i8 as i32) + CITY_X0).collect();
    Ok(json!({"x_table": x_table, "probe": {"w": 10, "h": 10, "hx": 2, "hy": 2}, "cities": out}))
}

/// A road record (also the arena events): name, size, two exits, 16 car places.
fn road_record(r: &[u8], kinds: bool) -> Value {
    let mut exits = vec![];
    for k in 0..2 {
        let e = &r[0x13 + 9 * k..0x13 + 9 * k + 9];
        exits.push(json!({"trigger_x": s16(e, 0), "trigger_y": s16(e, 2), "start_x": s16(e, 4), "start_y": s16(e, 6), "city": e[8]}));
    }
    let mut slots = vec![];
    for k in 0..16 {
        let e = &r[0x25 + 5 * k..0x25 + 5 * k + 5];
        let kind = if kinds && e[4] < 0x80 { Some(e[4]) } else { None };
        slots.push(json!({"x": s16(e, 0), "y": s16(e, 2), "kind": kind}));
    }
    json!({"name": name_of(&r[..16]), "index": r[0x10], "w": r[0x11], "h": r[0x12], "exits": exits, "slots": slots})
}

/// Where the 17 road records start in `drivers` (the first slot that has them intact).
fn find_roads(d: &[u8]) -> Option<usize> {
    let ok = |o: usize| {
        (0..17).all(|i| {
            let r = &d[o + 120 * i..o + 120 * (i + 1)];
            r[0x10] as usize == i && r[0x11] > 0 && r[0x12] > 0 && r[..16].iter().all(|&b| b == 0 || (0x20..0x7f).contains(&b))
        })
    };
    (0..d.len().saturating_sub(17 * 120)).find(|&o| ok(o))
}

pub fn roads(o: &Original) -> Result<Value, String> {
    let d = o.get("drivers")?;
    let base = find_roads(d).ok_or("drivers: the road records were not found")?;
    let maps = o.get("roadmap")?;
    if maps.len() < 17 * 512 {
        return Err("roadmap is too short".into());
    }
    let mut out = vec![];
    for i in 0..17 {
        // the slots' contents are the saved game's; a new game starts with the presets
        let mut rec = road_record(&d[base + 120 * i..base + 120 * (i + 1)], false);
        for &(_, slot, kind) in ROAD_PRESETS.iter().filter(|p| p.0 == i) {
            rec["slots"][slot]["kind"] = json!(kind);
        }
        let (w, h) = (rec["w"].as_u64().unwrap() as usize, rec["h"].as_u64().unwrap() as usize);
        if w * h > 512 {
            return Err(format!("drivers: road {} is {}x{} tiles", i, w, h));
        }
        let m = &maps[i * 512..(i + 1) * 512];
        let tiles: Vec<Vec<u8>> = (0..h).map(|y| m[y * w..(y + 1) * w].to_vec()).collect();
        rec["tiles"] = json!(tiles);
        out.push(rec);
    }
    Ok(Value::Array(out))
}

fn tiles(d: &[u8]) -> Value {
    let mut out = vec![];
    for t in 0..32 {
        let mut objs = vec![];
        for k in 0..32 {
            let e = &d[t * 96 + k * 3..t * 96 + k * 3 + 3];
            if e[0] < 0x17 {
                objs.push(json!({"shape": e[0], "x": s8(e[1]), "y": s8(e[2])}));
            }
        }
        out.push(Value::Array(objs));
    }
    Value::Array(out)
}

pub fn road_tiles(o: &Original) -> Result<Value, String> {
    let d = o.get("roadmod")?;
    if d.len() < 3072 {
        return Err("roadmod is too short".into());
    }
    Ok(tiles(d))
}

/// `arenstuf`: 8 event records (960 bytes), the 8x16 tile map (128), padding (384) and the 32
/// tiles (3072).
pub fn arena(o: &Original) -> Result<(Value, Value), String> {
    let d = o.get("arenstuf")?;
    if d.len() < 4544 {
        return Err("arenstuf is too short".into());
    }
    let events: Vec<Value> = (0..8).map(|i| road_record(&d[i * 120..(i + 1) * 120], true)).collect();
    let map: Vec<Vec<u8>> = (0..16).map(|y| d[960 + 8 * y..960 + 8 * (y + 1)].to_vec()).collect();
    Ok((json!({"w": 8, "h": 16, "tiles": map, "events": events}), tiles(&d[1472..])))
}

/// The AADA distance table: 16x16, zero on the diagonal, (nearly) symmetric.
fn find_distances(d: &[u8]) -> Option<usize> {
    (600..d.len().saturating_sub(256)).find(|&o| {
        let m = |a: usize, b: usize| d[o + 16 * a + b];
        (0..16).all(|i| m(i, i) == 0)
            && (0..16).all(|i| (0..16).all(|j| m(i, j) <= 40 && (i == j || m(i, j) > 0)))
            && (0..16).map(|i| (0..16).filter(|&j| m(i, j) != m(j, i)).count()).sum::<usize>() <= 4
    })
}

pub fn cargo(o: &Original) -> Result<(Value, Value), String> {
    let d = o.get("autoduel.ovl")?;
    let dist = find_distances(d).ok_or("autoduel.ovl: the distance table was not found")?;
    // weights (u16), values (u16, hundreds), destinations (high nibble), spaces, distances
    let (weights, values, dests, spaces) = (dist - 600, dist - 400, dist - 200, dist - 100);
    // the names: a table of 100 pointers into a run of strings starting "Legal documents"
    let first = find(d, b"Legal documents\0").ok_or("autoduel.ovl: the job names were not found")?;
    let mut starts = vec![first];
    while starts.len() < 120 {
        let s = *starts.last().unwrap();
        let e = s + d[s..].iter().position(|&b| b == 0).ok_or("autoduel.ovl: job names")?;
        starts.push(e + 1);
    }
    let table = (0..d.len().saturating_sub(200)).find_map(|p| {
        let delta = u16le(d, p + 2).wrapping_sub(starts[0] as u32) & 0xFFFF;
        let at = |k: usize| (u16le(d, p + 2 * k).wrapping_sub(delta) & 0xFFFF) as usize;
        (at(0) == starts[0] && at(1) == starts[0] && at(2) == starts[1] && at(3) == starts[2]).then_some((p, delta))
    });
    let (table, delta) = table.ok_or("autoduel.ovl: the job name table was not found")?;
    let mut jobs = vec![];
    for i in 0..100 {
        let s = (u16le(d, table + 2 * i).wrapping_sub(delta) & 0xFFFF) as usize;
        let name = name_of(d.get(s..).ok_or("autoduel.ovl: job names")?);
        let (w, v, dest, sp) = (u16le(d, weights + 2 * i), u16le(d, values + 2 * i), d[dests + i], d[spaces + i]);
        if name.is_empty() || dest & 15 != 0 || sp > 9 || w > 5000 {
            return Err(format!("autoduel.ovl: job {} does not look right", i));
        }
        jobs.push(json!({"name": name, "dest_building": dest >> 4, "weight": w, "spaces": sp, "value": v * 100}));
    }
    let mut m: Vec<Vec<u32>> = (0..16).map(|r| d[dist + 16 * r..dist + 16 * r + 16].iter().map(|&b| b as u32).collect()).collect();
    // FIX: Syracuse -> Philadelphia says 10, the other way 12
    if m[3][11] != m[11][3] {
        m[3][11] = m[11][3];
    }
    Ok((Value::Array(jobs), json!(m)))
}

pub fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub fn scenery(meta: &[(usize, usize, i32, i32)]) -> Value {
    let v: Vec<Value> = meta
        .iter()
        .enumerate()
        .map(|(i, &(w, h, ax, ay))| {
            json!({"index": i, "name": SCENERY_NAMES[i], "w": w, "h": h, "ax": ax, "ay": ay, "xrad": XRAD[i], "yrad": YRAD[i]})
        })
        .collect();
    Value::Array(v)
}
