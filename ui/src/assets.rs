//! Images and their metadata, loaded from `assets/gfx` (PNG) and `assets/data` (JSON).
//!
//! There are two art sets: the classic pictures in `assets/gfx`, made from the player's own
//! copy of Autoduel by `cardul-import`, and the enhanced ones in `assets/gfx_enhanced`. Each
//! set's folder may hold an `art.json` with its scale and, for art laid out differently from
//! the original, where the game should put its numbers ([`ArtConfig`]); the classic set's is
//! `assets/art.json`. A picture missing from the enhanced folder is taken from the classic one.
//! A set may also carry its own `shapes.json` / `scenery.json` (sizes and hot-spots of its
//! sprites); otherwise those in `assets/data` are used.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Which pictures the game shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtSet {
    /// the original graphics, from the player's copy of the game
    Classic,
    /// the "Auto Duel Enhanced" pack
    Enhanced,
}

impl ArtSet {
    pub fn name(self) -> &'static str {
        match self {
            ArtSet::Classic => "Classic",
            ArtSet::Enhanced => "Enhanced",
        }
    }
    /// Folder under the assets folder.
    pub fn folder(self) -> &'static str {
        match self {
            ArtSet::Classic => "gfx",
            ArtSet::Enhanced => "gfx_enhanced",
        }
    }
    pub fn other(self) -> ArtSet {
        match self {
            ArtSet::Classic => ArtSet::Enhanced,
            ArtSet::Enhanced => ArtSet::Classic,
        }
    }
    /// Is this set installed in `assets_dir`?
    pub fn available(self, assets_dir: &Path) -> bool {
        match self {
            ArtSet::Classic => assets_dir.join("gfx").is_dir(),
            ArtSet::Enhanced => assets_dir.join("gfx_enhanced").join("art.json").is_file(),
        }
    }
    /// Parse "classic" / "enhanced" (also "old" / "new").
    pub fn parse(s: &str) -> Option<ArtSet> {
        match s.trim().to_ascii_lowercase().as_str() {
            "classic" | "old" | "original" | "c64" => Some(ArtSet::Classic),
            "enhanced" | "new" | "modern" => Some(ArtSet::Enhanced),
            _ => None,
        }
    }
}

/// An RGBA image; pixels are 0xAARRGGBB.
#[derive(Clone, Debug, Default)]
pub struct Image {
    pub w: i32,
    pub h: i32,
    /// the art scale this image was authored at (1 = original C64 resolution)
    pub scale: i32,
    pub px: Vec<u32>,
}

impl Image {
    pub fn load(path: &Path, scale: i32) -> Result<Image, String> {
        let f = std::fs::File::open(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let mut dec = png::Decoder::new(std::io::BufReader::new(f));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
        let mut rd = dec.read_info().map_err(|e| format!("{}: {}", path.display(), e))?;
        let mut buf = vec![0u8; rd.output_buffer_size().ok_or("png size")?];
        let info = rd.next_frame(&mut buf).map_err(|e| format!("{}: {}", path.display(), e))?;
        let (w, h) = (info.width as i32, info.height as i32);
        let mut px = Vec::with_capacity((w * h) as usize);
        match info.color_type {
            png::ColorType::Rgba => {
                for c in buf[..(w * h * 4) as usize].chunks_exact(4) {
                    px.push((c[3] as u32) << 24 | (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for c in buf[..(w * h * 2) as usize].chunks_exact(2) {
                    let g = c[0] as u32;
                    px.push((c[1] as u32) << 24 | g << 16 | g << 8 | g);
                }
            }
            other => return Err(format!("{}: unsupported colour type {:?}", path.display(), other)),
        }
        Ok(Image { w, h, scale: scale.max(1), px })
    }
    /// Logical size (C64 pixels).
    pub fn lw(&self) -> i32 {
        self.w / self.scale.max(1)
    }
    pub fn lh(&self) -> i32 {
        self.h / self.scale.max(1)
    }
    /// Pixel at logical coordinates (top-left sample), 0 if outside.
    pub fn at(&self, x: i32, y: i32) -> u32 {
        let (x, y) = (x * self.scale, y * self.scale);
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return 0;
        }
        self.px[(y * self.w + x) as usize]
    }
}

/// Placement data for a shape image: the anchor (hot-spot) is drawn at the object's position.
#[derive(Clone, Debug, Deserialize)]
pub struct ShapeMeta {
    pub index: u32,
    pub name: String,
    pub w: i32,
    pub h: i32,
    pub ax: i32,
    pub ay: i32,
}

/// A rectangle in pixels of the art (x0, y0, x1, y1; end exclusive).
pub type Rect = [i32; 4];
/// A colour as [r, g, b].
pub type Rgb = [u8; 3];

/// Pack an [`Rgb`] as the framebuffer's 0xRRGGBB.
pub fn rgb(c: Rgb) -> u32 {
    (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32
}

/// `art.json`: the art's scale, and where the game draws on art that is not laid out like the
/// original. Every position is in pixels of the art (a 1280x800 screen at scale 4).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ArtConfig {
    /// integer scale of the art relative to the C64 screen (1 = original)
    #[serde(default = "one")]
    pub art_scale: i32,
    /// the driving HUD's value boxes and gauges (absent: the original layout)
    #[serde(default)]
    pub hud: Option<HudLayout>,
    /// the road map's city dots and the season line
    #[serde(default)]
    pub roadmap: Option<MapLayout>,
    /// where the driver's name goes in the newspaper
    #[serde(default)]
    pub news_name: Option<NewsName>,
    /// frame picture for the text screens (buildings, menus)
    #[serde(default)]
    pub text_frame: Option<String>,
    /// the driving screen's playfield window and where the player is in it (absent: the
    /// original layout)
    #[serde(default)]
    pub drive_view: Option<DriveView>,
}

/// The playfield on the driving screen, in logical (320x200) pixels.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct DriveView {
    /// x, y, width, height
    pub rect: [i32; 4],
    /// the player's screen position (the camera centre)
    pub center: [i32; 2],
}
fn one() -> i32 {
    1
}

#[derive(Clone, Debug, Deserialize)]
pub struct HudLayout {
    /// size of one font pixel in art pixels (the 8x8 font is drawn 8 * text_px high)
    pub text_px: i32,
    pub ink: Rgb,
    /// a value box's empty inside
    #[serde(rename = "box")]
    pub box_fill: Rgb,
    /// the selected weapon slot
    pub select_box: Rgb,
    pub select_ink: Rgb,
    /// inside of the weapon slots 1..0
    pub weapons: Vec<Rect>,
    /// armor F B L R U
    pub armor: Vec<Rect>,
    /// tires FL FR BL BR
    pub tires: Vec<Rect>,
    pub plant: Rect,
    pub driver: Rect,
    pub odometer: Rect,
    pub speed: Gauge,
    pub battery: Gauge,
    pub radar: Rect,
    pub radar_dot: Rgb,
    /// inside of the message panel and its colour
    #[serde(default)]
    pub message: Option<Rect>,
    #[serde(default)]
    pub message_bg: Option<Rgb>,
    /// the selected weapon's shots
    #[serde(default)]
    pub ammo: Option<Rect>,
    /// a title bar: where you are on the left, the date on the right
    #[serde(default)]
    pub bar: Option<Rect>,
    #[serde(default)]
    pub bar_ink: Option<Rgb>,
}

/// A segmented gauge: `image` (all segments lit) is drawn at `at`, one segment at a time.
/// `cuts` are the boundaries between segments along the gauge, in the order it fills: x
/// increasing for a horizontal gauge, y decreasing for a vertical one.
#[derive(Clone, Debug, Deserialize)]
pub struct Gauge {
    pub image: String,
    pub at: [i32; 2],
    pub cuts: Vec<i32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MapLayout {
    /// centre of each city's dot
    pub markers: HashMap<String, [i32; 2]>,
    /// colour of the ring around the current city
    pub marker: Rgb,
    /// where the season line starts (font drawn 3 art pixels per font pixel)
    pub season: [i32; 2],
    pub season_ink: Rgb,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NewsName {
    pub bar: Rect,
    pub ink: Rgb,
}

fn read_config(path: &Path) -> Result<ArtConfig, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| format!("{}: {}", path.display(), e)),
        Err(_) => Ok(ArtConfig { art_scale: 1, ..Default::default() }),
    }
}

pub struct Assets {
    pub dir: PathBuf,
    /// the art set these pictures are from
    pub art: ArtSet,
    /// that set's `art.json`
    pub config: ArtConfig,
    /// scale of that set's art (the framebuffer is made at this scale)
    pub scale: i32,
    pub title: Image,
    pub title_bg: Image,
    pub end_newspaper: Image,
    pub roadmap: Image,
    pub font: Image,
    pub suits: Image,
    pub road_hud: Option<Image>,
    pub arena_hud: Option<Image>,
    pub cities: Vec<Image>,
    /// walkable mask per city: white = street
    pub city_masks: Vec<Image>,
    /// vehicles, walker, projectiles, effects (non-modshaps 0..108)
    pub shapes: Vec<Image>,
    pub shape_meta: Vec<ShapeMeta>,
    /// roadside/arena scenery (modshaps 0..22)
    pub scenery: Vec<Image>,
    pub scenery_meta: Vec<ShapeMeta>,
    /// ground under the road and arena views (the classic view is black)
    pub ground_road: Option<Image>,
    pub ground_arena: Option<Image>,
    /// the HUD gauges with every segment lit ([`HudLayout`])
    pub hud_speed: Option<Image>,
    pub hud_battery: Option<Image>,
    /// [`ArtConfig::text_frame`]
    pub text_frame: Option<Image>,
}

impl Assets {
    /// The classic art.
    pub fn load(dir: &Path) -> Result<Assets, String> {
        Assets::load_art(dir, ArtSet::Classic)
    }

    /// The pictures of art set `art`; any the set doesn't have come from the classic set.
    pub fn load_art(dir: &Path, art: ArtSet) -> Result<Assets, String> {
        let classic_cfg = read_config(&dir.join("art.json"))?;
        let classic_scale = classic_cfg.art_scale.max(1);
        let base = dir.join("gfx");
        let (config, own) = match art {
            ArtSet::Classic => (classic_cfg, base.clone()),
            ArtSet::Enhanced => {
                let own = dir.join(art.folder());
                if !art.available(dir) {
                    return Err(format!("{} not found", own.join("art.json").display()));
                }
                (read_config(&own.join("art.json"))?, own)
            }
        };
        let scale = config.art_scale.max(1);
        let img = |rel: &str| {
            let p = own.join(rel);
            if p.exists() { Image::load(&p, scale) } else { Image::load(&base.join(rel), classic_scale) }
        };
        let opt = |rel: &str| img(rel).ok();
        // pictures only this set has (no classic fallback)
        let own_opt = |rel: &str| Image::load(&own.join(rel), scale).ok();
        let g = &base;
        // sprite sizes and hot-spots: the set's own for its own pictures, the game data's for
        // pictures taken from the classic set
        let read_meta = |p: &Path| -> Result<Vec<ShapeMeta>, String> {
            serde_json::from_str(&std::fs::read_to_string(p).map_err(|e| format!("{}: {}", p.display(), e))?)
                .map_err(|e| format!("{}: {}", p.display(), e))
        };
        let sprites = |name: &str, file: &dyn Fn(u32) -> String| -> Result<(Vec<Image>, Vec<ShapeMeta>), String> {
            let base_meta = read_meta(&dir.join("data").join(name))?;
            let own_meta = if own != base && own.join(name).is_file() { read_meta(&own.join(name))? } else { vec![] };
            let (mut imgs, mut metas) = (vec![], vec![]);
            for m in base_meta {
                let rel = file(m.index);
                match own_meta.iter().find(|o| o.index == m.index) {
                    Some(o) if own.join(&rel).is_file() => {
                        imgs.push(Image::load(&own.join(&rel), scale)?);
                        metas.push(o.clone());
                    }
                    _ => {
                        imgs.push(Image::load(&base.join(&rel), classic_scale)?);
                        metas.push(m);
                    }
                }
            }
            Ok((imgs, metas))
        };
        let (shapes, shape_meta) = sprites("shapes.json", &|i| format!("shapes/nm_{:03}.png", i))?;
        let (scenery, scenery_meta) = sprites("scenery.json", &|i| format!("scenery/ms_{:03}.png", i))?;
        let mut cities = vec![];
        let mut city_masks = vec![];
        for c in 0..16 {
            cities.push(img(&format!("city/c{:02x}.png", c))?);
            // masks are always at the original resolution (they are game data, not art)
            city_masks.push(Image::load(&g.join(format!("city/c{:02x}_mask.png", c)), 1)?);
        }
        let hud_speed = config.hud.as_ref().and_then(|h| own_opt(&h.speed.image));
        let hud_battery = config.hud.as_ref().and_then(|h| own_opt(&h.battery.image));
        let text_frame = config.text_frame.as_ref().and_then(|f| own_opt(f));
        Ok(Assets {
            dir: dir.to_path_buf(),
            art,
            scale,
            title: img("title.png")?,
            title_bg: img("title_bg.png")?,
            end_newspaper: img("end_newspaper.png")?,
            roadmap: img("roadmap.png")?,
            font: img("font.png")?,
            suits: img("suits.png")?,
            road_hud: opt("hud/road_hud.png"),
            arena_hud: opt("hud/arena_hud.png"),
            cities,
            city_masks,
            shapes,
            shape_meta,
            scenery,
            scenery_meta,
            ground_road: own_opt("ground_road.png"),
            ground_arena: own_opt("ground_arena.png"),
            hud_speed,
            hud_battery,
            text_frame,
            config,
        })
    }

    /// Frame of a text screen: the set's frame picture, or the original's white lines.
    /// `divider`: the line above the three-line message area.
    pub fn draw_text_frame(&self, fb: &mut crate::fb::Frame, divider: bool) {
        use crate::fb::col;
        match &self.text_frame {
            Some(f) => {
                fb.blit(f, 0, 0);
                if !divider {
                    // inside the frame from just above the divider to the bottom border
                    fb.fill(4, 162, 312, 34, col::BLACK);
                }
            }
            None => {
                fb.rect(2, 2, 316, 196, 2, col::WHITE);
                if divider {
                    fb.fill(2, 20 * 8 + 3, 316, 2, col::WHITE);
                }
            }
        }
    }

    /// Draw shape `i` (non-modshaps) with its anchor at logical (x, y).
    pub fn draw_shape(&self, fb: &mut crate::fb::Frame, i: usize, x: i32, y: i32) {
        if let (Some(img), Some(m)) = (self.shapes.get(i), self.shape_meta.get(i)) {
            fb.blit(img, x - m.ax, y - m.ay);
        }
    }
    pub fn draw_shape_tinted(&self, fb: &mut crate::fb::Frame, i: usize, x: i32, y: i32, tint: u32) {
        if let (Some(img), Some(m)) = (self.shapes.get(i), self.shape_meta.get(i)) {
            fb.blit_tinted(img, x - m.ax, y - m.ay, Some(tint));
        }
    }
    /// Draw scenery object `i` (modshaps) with its anchor at logical (x, y).
    pub fn draw_scenery(&self, fb: &mut crate::fb::Frame, i: usize, x: i32, y: i32) {
        if let (Some(img), Some(m)) = (self.scenery.get(i), self.scenery_meta.get(i)) {
            fb.blit(img, x - m.ax, y - m.ay);
        }
    }
}
