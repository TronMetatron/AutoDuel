//! Pictures in a library entry, and a small RGBA canvas to build the PNGs on.
//!
//! An entry starts with an 8-byte header: `u16` height, `i16` hot-spot y, `i16` hot-spot x,
//! `u8` width in bytes (8 pixels each) and `u8` flags:
//!
//! * 1: chunky, two 4-bit pixels per byte (else planar: four 1-bit planes, one after another);
//! * 2: a transparency mask plane comes first (bit set = see-through);
//! * 4: the data is PackBits-compressed;
//! * 8: not a picture but a list of the entries that make up a screen ([`Layout`]).
//!
//! Without a mask, colour 0 is see-through in sprites.
use std::path::Path;

/// The game's sixteen colours (the EGA palette it sets up).
pub const PALETTE: [[u8; 3]; 16] = [
    [0, 0, 0],
    [255, 255, 255],
    [0, 170, 0],
    [0, 170, 170],
    [85, 255, 85],
    [0, 0, 170],
    [85, 85, 255],
    [170, 0, 0],
    [255, 85, 85],
    [255, 255, 85],
    [85, 85, 85],
    [170, 170, 170],
    [255, 85, 255],
    [170, 85, 0],
    [170, 0, 170],
    [85, 255, 255],
];

/// Colour numbers as the game uses them.
pub mod c {
    pub const BLACK: u8 = 0;
    pub const WHITE: u8 = 1;
    pub const GREEN: u8 = 2;
    pub const LIGHT_GREEN: u8 = 4;
    pub const BLUE: u8 = 5;
    pub const LIGHT_BLUE: u8 = 6;
    pub const RED: u8 = 7;
    pub const LIGHT_RED: u8 = 8;
    pub const YELLOW: u8 = 9;
    pub const DARK_GREY: u8 = 10;
    pub const LIGHT_GREY: u8 = 11;
    pub const MAGENTA: u8 = 14;
    pub const LIGHT_CYAN: u8 = 15;
}

/// A decoded picture: colour numbers, -1 for see-through.
#[derive(Clone, Debug)]
pub struct Pic {
    pub w: usize,
    pub h: usize,
    pub hx: i32,
    pub hy: i32,
    pub px: Vec<i8>,
}

impl Pic {
    pub fn at(&self, x: usize, y: usize) -> i8 {
        self.px[y * self.w + x]
    }
}

pub fn packbits(d: &[u8], n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < d.len() && out.len() < n {
        let c = d[i];
        i += 1;
        if c < 128 {
            let k = c as usize + 1;
            out.extend_from_slice(&d[i..(i + k).min(d.len())]);
            i += k;
        } else if c > 128 {
            if let Some(&b) = d.get(i) {
                out.extend(std::iter::repeat_n(b, 257 - c as usize));
            }
            i += 1;
        }
    }
    out.resize(n, 0);
    out
}

pub fn header(d: &[u8]) -> Option<(usize, i32, i32, usize, u8)> {
    if d.len() < 8 {
        return None;
    }
    let h = d[0] as usize | (d[1] as usize) << 8;
    let hy = (d[2] as u16 | (d[3] as u16) << 8) as i16 as i32;
    let hx = (d[4] as u16 | (d[5] as u16) << 8) as i16 as i32;
    Some((h, hy, hx, d[6] as usize, d[7]))
}

/// Decode a picture entry. `opaque`: colour 0 is drawn (backgrounds) instead of see-through.
pub fn decode(d: &[u8], opaque: bool) -> Result<Pic, String> {
    let (h, hy, hx, wb, fl) = header(d).ok_or("picture entry too short")?;
    if fl & 8 != 0 {
        return Err("not a picture (a screen layout)".into());
    }
    let w = wb * 8;
    let masked = fl & 2 != 0;
    let planes = if masked { 5 } else { 4 };
    let size = h * wb * planes;
    let data = if fl & 4 != 0 {
        packbits(&d[8..], size)
    } else {
        let mut v = d[8..].to_vec();
        v.resize(size, 0);
        v
    };
    let mut px = vec![-1i8; w * h];
    if fl & 1 != 0 {
        // chunky (backgrounds and screen pictures; their mask, if any, is not used)
        for y in 0..h {
            for x in 0..w {
                let b = data[y * wb * 4 + x / 2];
                px[y * w + x] = if x % 2 == 0 { (b >> 4) as i8 } else { (b & 15) as i8 };
            }
        }
        return Ok(Pic { w, h, hx, hy, px });
    }
    let bit = |plane: usize, x: usize, y: usize| (data[(plane * h + y) * wb + x / 8] >> (7 - x % 8)) & 1;
    for y in 0..h {
        for x in 0..w {
            let (first, see_through) = if masked { (1, bit(0, x, y) == 1) } else { (0, false) };
            let mut c = 0u8;
            for p in 0..4 {
                c |= bit(first + p, x, y) << p;
            }
            if see_through || (!masked && !opaque && c == 0) {
                continue;
            }
            px[y * w + x] = c as i8;
        }
    }
    Ok(Pic { w, h, hx, hy, px })
}

/// A screen made of several entries of the same library (flag 8): entry number and position.
/// The entries are drawn at `(x - hx, y - hy)`.
pub struct Layout {
    pub parts: Vec<(usize, i32, i32)>,
}

impl Layout {
    pub fn decode(d: &[u8]) -> Option<Layout> {
        let (_, _, _, _, fl) = header(d)?;
        if fl & 8 == 0 {
            return None;
        }
        let parts = d[8..]
            .chunks_exact(4)
            .map(|c| (c[0] as usize, c[1] as i32, (c[2] as i32) << 8 | c[3] as i32))
            .collect();
        Some(Layout { parts })
    }
}

/// An RGBA picture being put together.
#[derive(Clone)]
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[u8; 4]>,
}

pub fn rgba(c: u8) -> [u8; 4] {
    let [r, g, b] = PALETTE[(c & 15) as usize];
    [r, g, b, 255]
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, px: vec![[0, 0, 0, 0]; w * h] }
    }
    pub fn filled(w: usize, h: usize, c: u8) -> Canvas {
        Canvas { w, h, px: vec![rgba(c); w * h] }
    }
    pub fn of(p: &Pic) -> Canvas {
        let mut cv = Canvas::new(p.w, p.h);
        cv.draw(p, 0, 0);
        cv
    }
    pub fn set(&mut self, x: i32, y: i32, v: [u8; 4]) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = v;
        }
    }
    pub fn get(&self, x: i32, y: i32) -> [u8; 4] {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize]
        } else {
            [0, 0, 0, 0]
        }
    }
    /// Draw a picture with its top-left corner at (x, y).
    pub fn draw(&mut self, p: &Pic, x: i32, y: i32) {
        self.draw_if(p, x, y, |_, _| true);
    }
    /// Draw only the pixels for which `keep(colour, pixel underneath)` holds.
    pub fn draw_if(&mut self, p: &Pic, x: i32, y: i32, keep: impl Fn(u8, [u8; 4]) -> bool) {
        for yy in 0..p.h {
            for xx in 0..p.w {
                let c = p.at(xx, yy);
                if c < 0 {
                    continue;
                }
                let (tx, ty) = (x + xx as i32, y + yy as i32);
                if keep(c as u8, self.get(tx, ty)) {
                    self.set(tx, ty, rgba(c as u8));
                }
            }
        }
    }
    pub fn fill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u8) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.set(x, y, rgba(c));
            }
        }
    }
    /// A `t`-pixel border inside the rectangle.
    pub fn frame(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, t: i32, c: u8) {
        self.fill(x0, y0, x1, y0 + t, c);
        self.fill(x0, y1 - t, x1, y1, c);
        self.fill(x0, y0, x0 + t, y1, c);
        self.fill(x1 - t, y0, x1, y1, c);
    }
    /// A 2-pixel-wide line.
    pub fn line(&mut self, (x0, y0): (i32, i32), (x1, y1): (i32, i32), c: u8) {
        let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=n {
            let x = x0 + (x1 - x0) * i / n;
            let y = y0 + (y1 - y0) * i / n;
            self.fill(x, y, x + 2, y + 2, c);
        }
    }
    /// Text in an 8x8 font strip (`glyph(code)` = eight rows, code 0x20.. as in the game's
    /// charset); returns the x after the text.
    pub fn text(&mut self, font: &crate::font::Font, x: i32, y: i32, s: &str, c: u8) -> i32 {
        let mut cx = x;
        for ch in s.chars() {
            for (r, row) in font.glyph(ch).iter().enumerate() {
                for b in 0..8 {
                    if row >> (7 - b) & 1 != 0 {
                        self.set(cx + b, y + r as i32, rgba(c));
                    }
                }
            }
            cx += 8;
        }
        cx
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
        }
        let f = std::fs::File::create(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| format!("{}: {}", path.display(), e))?;
        let data: Vec<u8> = self.px.iter().flatten().copied().collect();
        wr.write_image_data(&data).map_err(|e| format!("{}: {}", path.display(), e))
    }
}
