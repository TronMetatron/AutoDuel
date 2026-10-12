//! The game's text font.
//!
//! The DOS game keeps its letters in `init.ovl` as 8x12 cells (ASCII 0x20-0x7F, twelve bytes
//! each, one per row). On the 200-line EGA screen it shows them 8 rows high; dropping rows 0, 2,
//! 6 and 8 of every cell gives that look.
use crate::pic::Canvas;

/// Rows kept from each 12-row cell.
const ROWS: [usize; 8] = [1, 3, 4, 5, 7, 9, 10, 11];
/// The cell for '!' (the font is found by looking for a blank cell followed by this one).
const BANG: [u8; 12] = [0x00, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x00];

pub struct Font {
    /// ASCII 0x20..0x7F
    glyphs: Vec<[u8; 8]>,
    /// the six tire-icon pieces (Cardul's codes $5B-$60)
    pub tires: [[u8; 8]; 6],
}

impl Font {
    /// Find the font in `init.ovl`.
    pub fn from_overlay(ovl: &[u8]) -> Result<Font, String> {
        let at = (0..ovl.len().saturating_sub(24))
            .find(|&o| ovl[o..o + 12].iter().all(|&b| b == 0) && ovl[o + 12..o + 24] == BANG)
            .ok_or("the font was not found in init.ovl")?;
        if at + 96 * 12 > ovl.len() {
            return Err("the font in init.ovl is cut short".into());
        }
        let glyphs = (0..96)
            .map(|k| {
                let cell = &ovl[at + 12 * k..at + 12 * k + 12];
                let mut g = [0u8; 8];
                for (i, &r) in ROWS.iter().enumerate() {
                    g[i] = cell[r];
                }
                g
            })
            .collect();
        Ok(Font { glyphs, tires: [[0; 8]; 6] })
    }

    /// Eight rows of `ch` (unknown characters show as '?').
    pub fn glyph(&self, ch: char) -> [u8; 8] {
        let c = ch as u32;
        if (0x20..0x80).contains(&c) { self.glyphs[(c - 0x20) as usize] } else { self.glyphs[('?' as u32 - 0x20) as usize] }
    }

    /// The strip Cardul draws text with: 91 cells of 8x8 for its codes $20-$7A ($20-$40
    /// punctuation and digits, $41-$5A lower case, $5B-$60 tire-icon pieces, $61-$7A upper
    /// case), white on transparent.
    pub fn strip(&self) -> Canvas {
        let mut cv = Canvas::new(91 * 8, 8);
        for code in 0x20u32..=0x7A {
            let rows = match code {
                0x41..=0x5A => self.glyph(char::from_u32(code + 0x20).unwrap()),
                0x5B..=0x60 => self.tires[(code - 0x5B) as usize],
                0x61..=0x7A => self.glyph(char::from_u32(code - 0x20).unwrap()),
                _ => self.glyph(char::from_u32(code).unwrap()),
            };
            let x0 = (code - 0x20) as i32 * 8;
            for (r, row) in rows.iter().enumerate() {
                for b in 0..8 {
                    if row >> (7 - b) & 1 != 0 {
                        cv.set(x0 + b, r as i32, [255, 255, 255, 255]);
                    }
                }
            }
        }
        cv
    }
}
