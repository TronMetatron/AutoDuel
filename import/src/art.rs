//! The classic graphics (`assets/gfx`, `assets/art.json`) from the DOS game's picture
//! libraries.
use crate::font::Font;
use crate::pic::{self, c, Canvas, Layout, Pic};
use crate::Original;
use serde_json::{json, Value};
use std::path::Path;

/// Which library entry is each of Cardul's 109 shapes (cars, walker, effects).
fn shape_source(nm: usize) -> (&'static str, usize) {
    // the city car's eight headings, in Cardul's numbering: N E S W, then NE SE SW NW
    const CITY_CAR: [usize; 8] = [67, 69, 71, 73, 68, 70, 72, 74];
    match nm {
        0..=66 => ("cars.tlb", nm),
        67 => ("carstoo.tlb", 0),              // spikes
        68 => ("carstoo.tlb", 1),              // mine
        69..=72 => ("cars.tlb", CITY_CAR[nm - 69]),
        73..=103 => ("carstoo.tlb", nm - 71),  // bullet, shell, rockets, smoke, paint, oil, flames, fire
        104..=107 => ("cars.tlb", CITY_CAR[nm - 100]),
        _ => ("cars.tlb", 75),                 // collision probe
    }
}

pub const SHAPE_NAMES: [&str; 109] = [
    "car_heading00", "car_heading01", "car_heading02", "car_heading03", "car_heading04", "car_heading05",
    "car_heading06", "car_heading07", "car_heading08", "car_heading09", "car_heading10", "car_heading11",
    "car_heading12", "car_heading13", "car_heading14", "car_heading15", "walker_f00", "walker_f01", "walker_f02",
    "walker_f03", "walker_f04", "walker_f05", "walker_f06", "walker_f07", "walker_f08", "walker_f09", "walker_f10",
    "vehicle_t0_N", "vehicle_t0_NE", "vehicle_t0_E", "vehicle_t0_SE", "vehicle_t0_S", "vehicle_t0_SW", "vehicle_t0_W",
    "vehicle_t0_NW", "vehicle_t1_N", "vehicle_t1_NE", "vehicle_t1_E", "vehicle_t1_SE", "vehicle_t1_S", "vehicle_t1_SW",
    "vehicle_t1_W", "vehicle_t1_NW", "vehicle_t2_N", "vehicle_t2_NE", "vehicle_t2_E", "vehicle_t2_SE", "vehicle_t2_S",
    "vehicle_t2_SW", "vehicle_t2_W", "vehicle_t2_NW", "vehicle_t3_N", "vehicle_t3_NE", "vehicle_t3_E", "vehicle_t3_SE",
    "vehicle_t3_S", "vehicle_t3_SW", "vehicle_t3_W", "vehicle_t3_NW", "vehicle_t4_N", "vehicle_t4_NE", "vehicle_t4_E",
    "vehicle_t4_SE", "vehicle_t4_S", "vehicle_t4_SW", "vehicle_t4_W", "vehicle_t4_NW", "spikes", "mine", "citycar_N",
    "citycar_E", "citycar_S", "citycar_W", "bullet", "shell", "rocket_N", "rocket_NE", "rocket_E", "rocket_SE",
    "rocket_S", "rocket_SW", "rocket_W", "rocket_NW", "smoke_f0", "smoke_f1", "smoke_f2", "smoke_f3", "paint_f0",
    "paint_f1", "paint_f2", "paint_f3", "oil_slick", "flame_N", "flame_NE", "flame_E", "flame_SE", "flame_S",
    "flame_SW", "flame_W", "flame_NW", "fire_f0", "fire_f1", "fire_f2", "fire_f3", "citycar_NE", "citycar_SE",
    "citycar_SW", "citycar_NW", "collision_probe",
];

/// The libraries, decoded once.
pub struct Libs<'a> {
    o: &'a Original,
    cache: std::cell::RefCell<std::collections::HashMap<&'static str, std::rc::Rc<Vec<Vec<u8>>>>>,
}

impl<'a> Libs<'a> {
    pub fn new(o: &'a Original) -> Libs<'a> {
        Libs { o, cache: Default::default() }
    }
    pub fn lib(&self, name: &'static str) -> Result<std::rc::Rc<Vec<Vec<u8>>>, String> {
        if let Some(l) = self.cache.borrow().get(name) {
            return Ok(l.clone());
        }
        let l = std::rc::Rc::new(crate::tlb::read(name, self.o.get(name)?, &self.o.huffman()?)?);
        self.cache.borrow_mut().insert(name, l.clone());
        Ok(l)
    }
    pub fn entry(&self, name: &'static str, i: usize) -> Result<Vec<u8>, String> {
        self.lib(name)?.get(i).cloned().ok_or_else(|| format!("{}: entry {} is missing", name, i))
    }
    pub fn pic(&self, name: &'static str, i: usize, opaque: bool) -> Result<Pic, String> {
        pic::decode(&self.entry(name, i)?, opaque).map_err(|e| format!("{} entry {}: {}", name, i, e))
    }
    /// A screen put together from its layout entry.
    pub fn screen(&self, name: &'static str, layout: usize, w: usize, h: usize) -> Result<Canvas, String> {
        let l = Layout::decode(&self.entry(name, layout)?).ok_or_else(|| format!("{} entry {}: not a layout", name, layout))?;
        let mut cv = Canvas::new(w, h);
        for (i, y, x) in l.parts {
            if let Ok(p) = self.pic(name, i, true) {
                cv.draw(&p, x - p.hx, y - p.hy);
            }
        }
        Ok(cv)
    }
}

pub struct Built {
    pub shapes: Value,
    pub scenery: Vec<(usize, usize, i32, i32)>,
    pub markers: Value,
}

/// Write every picture into `gfx` and return the shapes' placement data.
pub fn build(o: &Original, font: &Font, roads: &Value, gfx: &Path) -> Result<Built, String> {
    let libs = Libs::new(o);

    // cars, walker, effects: anchored at the pictures' hot-spots
    let mut shapes = vec![];
    for (nm, name) in SHAPE_NAMES.iter().enumerate() {
        let (lib, i) = shape_source(nm);
        let p = libs.pic(lib, i, false)?;
        Canvas::of(&p).save(&gfx.join(format!("shapes/nm_{:03}.png", nm)))?;
        shapes.push(json!({"index": nm, "name": name, "w": p.w, "h": p.h, "ax": p.hx, "ay": p.hy}));
    }
    // roadside objects
    let mut scenery = vec![];
    for i in 0..23 {
        let p = libs.pic("terrain.tlb", i, false)?;
        Canvas::of(&p).save(&gfx.join(format!("scenery/ms_{:03}.png", i)))?;
        scenery.push((p.w, p.h, p.hx, p.hy));
    }
    // the towns (320x164 in four bands) and their streets (black = walkable)
    for (city, &name) in CITY_LIBS.iter().enumerate() {
        let pic = libs.screen(name, 0, 320, 168)?;
        let mut cv = Canvas::filled(320, 168, c::BLACK);
        let mut mask = Canvas::filled(320, 168, c::BLACK);
        for (i, px) in pic.px.iter().enumerate() {
            if px[3] != 0 {
                cv.px[i] = *px;
                if px[..3] == [0, 0, 0] {
                    mask.px[i] = [255, 255, 255, 255];
                }
            }
        }
        cv.save(&gfx.join(format!("city/c{:02x}.png", city)))?;
        mask.save(&gfx.join(format!("city/c{:02x}_mask.png", city)))?;
    }

    title(&libs, gfx)?;
    let paper = libs.pic("win.tlb", 0, true)?;
    Canvas::of(&paper).save(&gfx.join("end_newspaper.png"))?;
    font.strip().save(&gfx.join("font.png"))?;
    suits(&libs)?.save(&gfx.join("suits.png"))?;
    hud(&libs, font, gfx)?;
    text_frame().save(&gfx.join("hud/text_frame.png"))?;
    let markers = crate::map::build(font, roads, &gfx.join("roadmap.png"))?;
    Ok(Built { shapes: Value::Array(shapes), scenery, markers })
}

const CITY_LIBS: [&str; 16] = [
    "city00.tlb", "city01.tlb", "city02.tlb", "city03.tlb", "city04.tlb", "city05.tlb", "city06.tlb", "city07.tlb",
    "city08.tlb", "city09.tlb", "city0a.tlb", "city0b.tlb", "city0c.tlb", "city0d.tlb", "city0e.tlb", "city0f.tlb",
];

/// The title: the desert road, then AUTO, DUEL, the credits, the slogan and the setting sun
/// (which only shows against the sky).
fn title(libs: &Libs, gfx: &Path) -> Result<(), String> {
    let bg = libs.pic("intro.tlb", 0, true)?;
    let bgc = Canvas::of(&bg);
    bgc.save(&gfx.join("title_bg.png"))?;
    let mut t = bgc.clone();
    let sky = pic::rgba(c::LIGHT_BLUE);
    let l = Layout::decode(&libs.entry("intro.tlb", 7)?).ok_or("intro.tlb: the title layout is missing")?;
    for (i, y, x) in l.parts {
        let Ok(p) = libs.pic("intro.tlb", i, false) else { continue };
        if i == 0 || p.w == 0 {
            continue;
        }
        let (px, py) = (x - p.hx, y - p.hy);
        if i == 5 {
            // the sun: its own sky-coloured corners are left out
            t.draw_if(&p, px, py, |col, under| col != c::LIGHT_BLUE && under == sky);
        } else if i < 5 {
            t.draw(&p, px, py);
        }
    }
    t.save(&gfx.join("title.png"))
}

/// Spade, heart, diamond, club: the casino's 16x15 suit cards shrunk to 8x8 marks.
fn suits(libs: &Libs) -> Result<Canvas, String> {
    let mut cv = Canvas::new(32, 8);
    for s in 0..4 {
        let p = libs.pic("casino.tlb", 3 + s, true)?;
        for y in 0..8usize {
            for x in 0..8usize {
                let mut n = 0;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = (2 * x + dx, (2 * y + dy).min(p.h - 1));
                    if sx < p.w && p.at(sx, sy) != c::GREEN as i8 {
                        n += 1;
                    }
                }
                if n >= 2 {
                    cv.set((8 * s + x) as i32, y as i32, [255, 255, 255, 255]);
                }
            }
        }
    }
    Ok(cv)
}

/// The driving HUD: the game's frame (rows 0-173) with its labels, the gauges' "all lit"
/// pictures, and where the numbers go ([`HUD`]).
fn hud(libs: &Libs, font: &Font, gfx: &Path) -> Result<(), String> {
    let frame = libs.screen("frame.tlb", 0, 320, 200)?;
    let mut h = Canvas::filled(320, 200, c::BLACK);
    for (i, px) in frame.px.iter().enumerate() {
        if px[3] != 0 {
            h.px[i] = *px;
        }
    }
    let w = c::WHITE;
    h.text(font, 8, 6, "Wpn", w);
    for i in 0..10 {
        let y = 16 + 8 * i;
        h.text(font, 8, y, &format!("{}..", (i + 1) % 10), w);
    }
    h.text(font, 8, 160, "Bat", w);
    h.text(font, 288, 6, "Amm", w);
    h.text(font, 288, 32, "Arm", w);
    for (i, l) in ["F", "B", "L", "R", "U"].iter().enumerate() {
        h.text(font, 288, 44 + 8 * i as i32, l, w);
    }
    h.text(font, 288, 92, "Trs", w);
    // the tire icons FL FR BL BR
    for t in 0..4 {
        if let Ok(p) = libs.pic("frame.tlb", 4 + t, false) {
            h.draw(&p, 288, 104 + 8 * t as i32);
        }
    }
    h.text(font, 200, 144, "Power Plant", w);
    h.text(font, 240, 160, "Driver", w);
    // the radar screen
    h.fill(40, 140, 80, 172, c::LIGHT_RED);
    h.save(&gfx.join("hud/road_hud.png"))?;
    h.save(&gfx.join("hud/arena_hud.png"))?;

    // speed: nine 16-pixel steps, each a row taller (2..10 rows)
    let mut sp = Canvas::new(144, 10);
    for i in 0..9 {
        sp.fill(16 * i, 10 - (i + 2), 16 * i + 16, 10, c::LIGHT_RED);
    }
    sp.save(&gfx.join("hud/speed_lit.png"))?;
    Canvas::filled(8, 50, c::MAGENTA).save(&gfx.join("hud/bat_lit.png"))
}

/// Where the HUD's numbers go (pixels of the 320x200 screen).
pub fn hud_layout() -> Value {
    let rgb = |n: u8| pic::PALETTE[n as usize];
    json!({
        "text_px": 1,
        "ink": rgb(c::WHITE),
        "box": rgb(c::BLACK),
        "select_box": rgb(c::WHITE),
        "select_ink": rgb(c::BLACK),
        "weapons": (0..10).map(|i| [16, 16 + 8 * i, 32, 24 + 8 * i]).collect::<Vec<_>>(),
        "armor": (0..5).map(|i| [296, 44 + 8 * i, 312, 52 + 8 * i]).collect::<Vec<_>>(),
        "tires": (0..4).map(|i| [296, 104 + 8 * i, 312, 112 + 8 * i]).collect::<Vec<_>>(),
        "plant": [296, 144, 312, 152],
        "driver": [296, 160, 312, 168],
        "odometer": [142, 143, 178, 153],
        "speed": {"image": "hud/speed_lit.png", "at": [88, 160], "cuts": (0..10).map(|i| 88 + 16 * i).collect::<Vec<_>>()},
        "battery": {"image": "hud/bat_lit.png", "at": [16, 106], "cuts": (0..50).map(|i| 156 - i).collect::<Vec<_>>()},
        "radar": [40, 140, 80, 172],
        "radar_dot": rgb(c::WHITE),
        "message": [6, 174, 314, 200],
        "message_bg": rgb(c::BLACK),
        "ammo": [296, 16, 312, 24],
        "bar": [40, 1, 280, 9],
        "bar_ink": rgb(c::BLACK),
    })
}

/// The frame of the text screens (buildings, menus): the game's grey panel with a light
/// border, and the black message strip under it.
fn text_frame() -> Canvas {
    let mut f = Canvas::filled(320, 200, c::BLACK);
    f.fill(0, 0, 320, 166, c::DARK_GREY);
    f.frame(0, 0, 320, 166, 2, c::LIGHT_GREY);
    f
}

/// `art.json` of the classic set.
pub fn config() -> Value {
    json!({
        "art_scale": 1,
        "source": "dos",
        "hud": hud_layout(),
        "drive_view": {"rect": [40, 10, 240, 128], "center": [160, 74]},
        // the DOS newspaper has no line for the hero's name
        "news_name": {"bar": [0, 0, 0, 0], "ink": [0, 0, 0]},
        "text_frame": "hud/text_frame.png",
    })
}
