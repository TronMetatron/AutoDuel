//! The game's 8x8 bitmap font (91 glyphs, codes $20-$7A).
//!
//! Glyph order: $20-$40 punctuation/digits, $41-$5A lowercase a-z, $5B-$60 tire-icon pieces,
//! $61-$7A uppercase A-Z.
use crate::assets::Assets;
use crate::fb::Frame;

/// Private-use characters for the tire icon pieces ($5B-$60) in strings.
pub const TIRE_GLYPHS: [char; 6] = ['\u{E05B}', '\u{E05C}', '\u{E05D}', '\u{E05E}', '\u{E05F}', '\u{E060}'];

pub fn glyph_code(c: char) -> u8 {
    match c {
        ' '..='@' => c as u8,
        'a'..='z' => 0x41 + (c as u8 - b'a'),
        'A'..='Z' => 0x61 + (c as u8 - b'A'),
        '\u{E05B}'..='\u{E060}' => (c as u32 - 0xE000) as u8,
        '[' | '{' => b'(',
        ']' | '}' => b')',
        '_' => b'-',
        '|' => b'!',
        '~' => b'-',
        _ => b'?',
    }
}

/// Text that did not fit where it was put (cut off or running off the screen), for tools and
/// tests: each distinct string once.
static OVERFLOW: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Note a string that did not fit (see [`take_overflows`]).
pub fn report_overflow(s: &str) {
    if let Ok(mut v) = OVERFLOW.lock()
        && !v.iter().any(|x| x == s) {
        v.push(s.to_string());
    }
}

/// The strings that did not fit since the last call.
pub fn take_overflows() -> Vec<String> {
    OVERFLOW.lock().map(|mut v| std::mem::take(&mut *v)).unwrap_or_default()
}

/// Draw `s` at logical (x, y) in colour `rgb`; returns the x after the last glyph.
pub fn draw(fb: &mut Frame, a: &Assets, x: i32, y: i32, s: &str, rgb: u32) -> i32 {
    if x + 8 * s.trim_end().chars().count() as i32 > 320 {
        report_overflow(s);
    }
    let mut cx = x;
    for ch in s.chars() {
        let code = glyph_code(ch);
        if code > 0x20 {
            fb.blit_part(&a.font, (code as i32 - 0x20) * 8, 0, 8, 8, cx, y, Some(rgb));
        }
        cx += 8;
    }
    cx
}

/// Draw text with a solid background box (inverse video when bg is white and fg black).
pub fn draw_bg(fb: &mut Frame, a: &Assets, x: i32, y: i32, s: &str, fg: u32, bg: u32) -> i32 {
    fb.fill(x, y, 8 * s.chars().count() as i32, 8, bg);
    draw(fb, a, x, y, s, fg)
}

/// Draw at a text cell (40 x 25 grid).
pub fn at(fb: &mut Frame, a: &Assets, col: i32, row: i32, s: &str, rgb: u32) -> i32 {
    draw(fb, a, col * 8, row * 8, s, rgb)
}

/// Format money like the original: "$2,000" style without separators would be "$2000";
/// we use thousands separators for readability.
pub fn money(v: u32) -> String {
    let s = v.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    format!("${}", out)
}

/// Word-wrap `s` to `width` columns, honouring explicit newlines.
pub fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut lines = vec![];
    for para in s.split('\n') {
        let mut cur = String::new();
        for word in para.split(' ') {
            if cur.is_empty() {
                cur = word.to_string();
            } else if cur.chars().count() + 1 + word.chars().count() <= width {
                cur.push(' ');
                cur.push_str(word);
            } else {
                lines.push(std::mem::take(&mut cur));
                cur = word.to_string();
            }
            while cur.chars().count() > width {
                let head: String = cur.chars().take(width).collect();
                let tail: String = cur.chars().skip(width).collect();
                lines.push(head);
                cur = tail;
            }
        }
        lines.push(cur);
    }
    lines
}

/// Draw `s` with its top-left at frame pixel (x, y), each font pixel `size` frame pixels
/// (for art laid out on its own pixel grid, see [`crate::assets::HudLayout`]); returns the x
/// after the last glyph.
pub fn draw_px(fb: &mut Frame, a: &Assets, x: i32, y: i32, s: &str, size: i32, rgb: u32) -> i32 {
    let mut cx = x;
    for ch in s.chars() {
        let code = glyph_code(ch);
        if code > 0x20 {
            fb.glyph_px(&a.font, code as i32 - 0x20, 0, 8, cx, y, size, rgb);
        }
        cx += 8 * size;
    }
    cx
}

/// Width in frame pixels of `s` drawn with [`draw_px`].
pub fn width_px(s: &str, size: i32) -> i32 {
    8 * size * s.chars().count() as i32
}
