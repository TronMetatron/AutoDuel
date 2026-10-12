//! Software framebuffer. Everything is drawn in C64 screen coordinates (320 x 200); with
//! `scale` > 1 the buffer is scale x larger and images are expected to be authored at that
//! scale (see assets/art.json), so new high-resolution art drops in without code changes.
use crate::assets::Image;

pub const W: i32 = 320;
pub const H: i32 = 200;

/// The C64 "Pepto" palette.
pub mod col {
    pub const BLACK: u32 = 0x000000;
    pub const WHITE: u32 = 0xFFFFFF;
    pub const RED: u32 = 0x68372B;
    pub const CYAN: u32 = 0x70A4B2;
    pub const PURPLE: u32 = 0x6F3D86;
    pub const GREEN: u32 = 0x588D43;
    pub const BLUE: u32 = 0x352879;
    pub const YELLOW: u32 = 0xB8C76F;
    pub const ORANGE: u32 = 0x6F4F25;
    pub const BROWN: u32 = 0x433900;
    pub const LIGHT_RED: u32 = 0x9A6759;
    pub const DARK_GREY: u32 = 0x444444;
    pub const GREY: u32 = 0x6C6C6C;
    pub const LIGHT_GREEN: u32 = 0x9AD284;
    pub const LIGHT_BLUE: u32 = 0x6C5EB5;
    pub const LIGHT_GREY: u32 = 0x959595;
}

pub struct Frame {
    pub scale: i32,
    /// physical size
    pub pw: i32,
    pub ph: i32,
    /// RGBA8
    pub px: Vec<u8>,
    /// clip rectangle in logical coordinates (x0, y0, x1, y1), exclusive end
    pub clip: (i32, i32, i32, i32),
}

impl Frame {
    pub fn new(scale: i32) -> Frame {
        let s = scale.max(1);
        Frame { scale: s, pw: W * s, ph: H * s, px: vec![0; (W * s * H * s * 4) as usize], clip: (0, 0, W, H) }
    }

    pub fn reset_clip(&mut self) {
        self.clip = (0, 0, W, H);
    }
    pub fn set_clip(&mut self, x: i32, y: i32, w: i32, h: i32) {
        self.clip = (x.max(0), y.max(0), (x + w).min(W), (y + h).min(H));
    }

    /// The clip rectangle in physical pixels (x0, y0, x1, y1), exclusive end.
    #[inline]
    fn clip_phys(&self) -> (i32, i32, i32, i32) {
        (self.clip.0 * self.scale, self.clip.1 * self.scale, self.clip.2 * self.scale, self.clip.3 * self.scale)
    }

    #[inline]
    fn set_px(&mut self, x: i32, y: i32, rgb: u32) {
        let i = ((y * self.pw + x) * 4) as usize;
        self.px[i..i + 4].copy_from_slice(&[(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]);
    }
    /// Mix `rgb` over the pixel with opacity `a` (0-255).
    #[inline]
    fn blend_px(&mut self, x: i32, y: i32, rgb: u32, a: u32) {
        if a >= 255 {
            return self.set_px(x, y, rgb);
        }
        let i = ((y * self.pw + x) * 4) as usize;
        let src = [(rgb >> 16) & 255, (rgb >> 8) & 255, rgb & 255];
        for (k, s) in src.iter().enumerate() {
            let d = self.px[i + k] as u32;
            self.px[i + k] = ((s * a + d * (255 - a) + 127) / 255) as u8;
        }
    }

    pub fn clear(&mut self, rgb: u32) {
        let c = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
        for px in self.px.chunks_exact_mut(4) {
            px.copy_from_slice(&c);
        }
    }

    /// Filled rectangle, logical coordinates.
    pub fn fill(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: u32) {
        let s = self.scale;
        let (cx0, cy0, cx1, cy1) = self.clip_phys();
        let (x0, x1) = ((x * s).max(cx0), ((x + w) * s).min(cx1));
        let (y0, y1) = ((y * s).max(cy0), ((y + h) * s).min(cy1));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let c = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
        for yy in y0..y1 {
            let row = ((yy * self.pw + x0) * 4) as usize;
            for px in self.px[row..row + ((x1 - x0) * 4) as usize].chunks_exact_mut(4) {
                px.copy_from_slice(&c);
            }
        }
    }
    /// Rectangle outline with the given line thickness (logical px).
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, t: i32, rgb: u32) {
        self.fill(x, y, w, t, rgb);
        self.fill(x, y + h - t, w, t, rgb);
        self.fill(x, y, t, h, rgb);
        self.fill(x + w - t, y, t, h, rgb);
    }
    pub fn pixel(&mut self, x: i32, y: i32, rgb: u32) {
        self.fill(x, y, 1, 1, rgb);
    }

    /// Draw an image with its top-left corner at logical (x, y). Transparent pixels are skipped.
    pub fn blit(&mut self, img: &Image, x: i32, y: i32) {
        self.blit_tinted(img, x, y, None);
    }
    /// Draw an image; non-transparent pixels are replaced by `tint` if given.
    pub fn blit_tinted(&mut self, img: &Image, x: i32, y: i32, tint: Option<u32>) {
        let s = img.scale.max(1);
        self.blit_part(img, 0, 0, img.w / s, img.h / s, x, y, tint);
    }
    /// Draw a sub-rectangle of an image (logical coordinates of the source as well).
    #[allow(clippy::too_many_arguments)]
    pub fn blit_part(&mut self, img: &Image, sx: i32, sy: i32, sw: i32, sh: i32, x: i32, y: i32, tint: Option<u32>) {
        // images are authored at img.scale; each image pixel covers f x f frame pixels
        let s = img.scale.max(1);
        let f = (self.scale / s).max(1);
        // the source rectangle in image pixels; frame position of image pixel (0, 0)
        let (ix0, iy0, ix1, iy1) = (sx * s, sy * s, (sx + sw) * s, (sy + sh) * s);
        let ox = x * s * f - sx * s * f;
        let oy = y * s * f - sy * s * f;
        self.blit_pixels(img, (ix0, iy0, ix1, iy1), ox, oy, f, tint);
    }

    /// Draw image pixels `ix0..ix1, iy0..iy1` with image pixel (0, 0) at frame pixel (ox, oy),
    /// each image pixel as an f x f block. Pixels are mixed by their alpha; with `tint` the
    /// colour is replaced (the alpha still counts).
    fn blit_pixels(&mut self, img: &Image, (ix0, iy0, ix1, iy1): (i32, i32, i32, i32), ox: i32, oy: i32, f: i32, tint: Option<u32>) {
        let f = f.max(1);
        let (cx0, cy0, cx1, cy1) = self.clip_phys();
        let (ix0, iy0) = (ix0.max(0), iy0.max(0));
        let (ix1, iy1) = (ix1.min(img.w), iy1.min(img.h));
        // only the image pixels whose f x f block can touch the clip rectangle
        let ix_lo = ix0.max((cx0 - ox).div_euclid(f));
        let ix_hi = ix1.min((cx1 - ox + f - 1).div_euclid(f));
        let iy_lo = iy0.max((cy0 - oy).div_euclid(f));
        let iy_hi = iy1.min((cy1 - oy + f - 1).div_euclid(f));
        for iy in iy_lo..iy_hi {
            let src = (iy * img.w) as usize;
            for ix in ix_lo..ix_hi {
                let p = img.px[src + ix as usize];
                let a = p >> 24;
                if a < 8 {
                    continue;
                }
                let rgb = tint.unwrap_or(p & 0xFFFFFF);
                let (px0, py0) = (ox + ix * f, oy + iy * f);
                if f == 1 {
                    self.blend_px(px0, py0, rgb, a);
                } else {
                    for yy in py0.max(cy0)..(py0 + f).min(cy1) {
                        for xx in px0.max(cx0)..(px0 + f).min(cx1) {
                            self.blend_px(xx, yy, rgb, a);
                        }
                    }
                }
            }
        }
    }

    // ---- drawing in frame pixels (for art laid out finer than the C64's 8x8 cells)

    /// Filled rectangle in frame pixels (x0, y0, x1, y1; end exclusive), clipped.
    pub fn fill_px(&mut self, r: [i32; 4], rgb: u32) {
        let (cx0, cy0, cx1, cy1) = self.clip_phys();
        let (x0, x1) = (r[0].max(cx0), r[2].min(cx1));
        let (y0, y1) = (r[1].max(cy0), r[3].min(cy1));
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let c = [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255];
        for yy in y0..y1 {
            let row = ((yy * self.pw + x0) * 4) as usize;
            for px in self.px[row..row + ((x1 - x0) * 4) as usize].chunks_exact_mut(4) {
                px.copy_from_slice(&c);
            }
        }
    }
    /// Rectangle outline in frame pixels, `t` pixels thick.
    pub fn rect_px(&mut self, r: [i32; 4], t: i32, rgb: u32) {
        let [x0, y0, x1, y1] = r;
        self.fill_px([x0, y0, x1, y0 + t], rgb);
        self.fill_px([x0, y1 - t, x1, y1], rgb);
        self.fill_px([x0, y0, x0 + t, y1], rgb);
        self.fill_px([x1 - t, y0, x1, y1], rgb);
    }
    /// Draw image pixels `src` (x0, y0, x1, y1 in the image's own pixels) with their top-left
    /// corner at frame pixel (x, y). An image at the frame's scale is copied 1:1.
    pub fn blit_px(&mut self, img: &Image, src: [i32; 4], x: i32, y: i32) {
        let f = (self.scale / img.scale.max(1)).max(1);
        self.blit_pixels(img, (src[0], src[1], src[2], src[3]), x - src[0] * f, y - src[1] * f, f, None);
    }
    /// Draw rows `r0..r1` of an 8x8 glyph cell of `font` (glyph `cell` from the left) with each
    /// font pixel as a `size` x `size` block, the cell's top-left at frame pixel (x, y). A
    /// high-resolution font (scale > 1, anti-aliased) is resampled to that size and blended.
    #[allow(clippy::too_many_arguments)]
    pub fn glyph_px(&mut self, font: &Image, cell: i32, r0: i32, r1: i32, x: i32, y: i32, size: i32, rgb: u32) {
        let (cx0, cy0, cx1, cy1) = self.clip_phys();
        let (r0, r1) = (r0.max(0), r1.min(8));
        let s = font.scale.max(1);
        if s == 1 {
            for gy in r0..r1 {
                for gx in 0..8 {
                    let a = font.at(cell * 8 + gx, gy) >> 24;
                    if a < 128 {
                        continue;
                    }
                    let (px0, py0) = (x + gx * size, y + (gy - r0) * size);
                    for yy in py0.max(cy0)..(py0 + size).min(cy1) {
                        for xx in px0.max(cx0)..(px0 + size).min(cx1) {
                            self.set_px(xx, yy, rgb);
                        }
                    }
                }
            }
            return;
        }
        // the glyph's own pixels: columns gx0..gx0 + 8s, rows r0*s..r1*s
        let (gx0, gy0, gy1) = (cell * 8 * s, r0 * s, r1 * s);
        let alpha = |ix: i32, iy: i32| -> f32 {
            let ix = ix.clamp(gx0, gx0 + 8 * s - 1);
            let iy = iy.clamp(gy0, gy1 - 1);
            if ix < 0 || iy < 0 || ix >= font.w || iy >= font.h {
                return 0.0;
            }
            (font.px[(iy * font.w + ix) as usize] >> 24) as f32
        };
        let k = s as f32 / size.max(1) as f32;
        for dy in 0..(r1 - r0) * size {
            let yy = y + dy;
            if yy < cy0 || yy >= cy1 {
                continue;
            }
            let fy = gy0 as f32 + (dy as f32 + 0.5) * k - 0.5;
            let (iy, ty) = (fy.floor() as i32, fy - fy.floor());
            for dx in 0..8 * size {
                let xx = x + dx;
                if xx < cx0 || xx >= cx1 {
                    continue;
                }
                let fx = gx0 as f32 + (dx as f32 + 0.5) * k - 0.5;
                let (ix, tx) = (fx.floor() as i32, fx - fx.floor());
                let top = alpha(ix, iy) * (1.0 - tx) + alpha(ix + 1, iy) * tx;
                let bot = alpha(ix, iy + 1) * (1.0 - tx) + alpha(ix + 1, iy + 1) * tx;
                let a = (top * (1.0 - ty) + bot * ty) as u32;
                if a >= 8 {
                    self.blend_px(xx, yy, rgb, a.min(255));
                }
            }
        }
    }
    /// Invert colours in a rectangle of frame pixels.
    pub fn invert_px(&mut self, r: [i32; 4]) {
        for yy in r[1].max(0)..r[3].min(self.ph) {
            for xx in r[0].max(0)..r[2].min(self.pw) {
                let i = ((yy * self.pw + xx) * 4) as usize;
                self.px[i] = 255 - self.px[i];
                self.px[i + 1] = 255 - self.px[i + 1];
                self.px[i + 2] = 255 - self.px[i + 2];
            }
        }
    }

    /// Invert colours in a logical rectangle (used for highlighted text, like the C64 RVS mode).
    pub fn invert(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let s = self.scale;
        for yy in (y * s).max(0)..((y + h) * s).min(self.ph) {
            for xx in (x * s).max(0)..((x + w) * s).min(self.pw) {
                let i = ((yy * self.pw + xx) * 4) as usize;
                self.px[i] = 255 - self.px[i];
                self.px[i + 1] = 255 - self.px[i + 1];
                self.px[i + 2] = 255 - self.px[i + 2];
            }
        }
    }

    /// Save as PNG (used by the headless screenshot tool).
    pub fn save_png(&self, path: &std::path::Path) -> Result<(), String> {
        let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let w = std::io::BufWriter::new(f);
        let mut enc = png::Encoder::new(w, self.pw as u32, self.ph as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(&self.px).map_err(|e| e.to_string())
    }
}
