//! The road map. The DOS game has no map picture, so Cardul draws one: the Fortress Towns at
//! their places on the real map (by latitude and longitude), the roads between them from the
//! road records, and the names in the game's font.
use crate::font::Font;
use crate::pic::{c, Canvas};
use serde_json::{json, Value};
use std::path::Path;

/// Each town's text cell (its dot is the cell's centre; the game rings the cell of the town
/// you are in) and where its name goes: 'r' right, 'l' left, 'a' above, 'b' below, 'B' below
/// and to the left.
const TOWNS: [(&str, i32, i32, char); 16] = [
    ("Watertown", 14, 3, 'r'),
    ("Manchester", 27, 7, 'r'),
    ("Buffalo", 5, 7, 'b'),
    ("Syracuse", 13, 7, 'a'),
    ("Albany", 20, 8, 'r'),
    ("Boston", 28, 9, 'r'),
    ("Scranton", 15, 13, 'r'),
    ("New York", 20, 16, 'r'),
    ("Providence", 27, 11, 'r'),
    ("Pittsburgh", 2, 17, 'a'),
    ("Harrisburg", 11, 18, 'B'),
    ("Philadelphia", 16, 19, 'r'),
    ("Atlantic City", 18, 21, 'r'),
    ("Baltimore", 12, 21, 'l'),
    ("Dover", 15, 22, 'b'),
    ("Washington", 11, 23, 'l'),
];

fn centre(i: usize) -> (i32, i32) {
    (TOWNS[i].1 * 8 + 4, TOWNS[i].2 * 8 + 4)
}

/// Draw the map into `path`; returns the town markers (`roadmap_markers.json`).
pub fn build(font: &Font, roads: &Value, path: &Path) -> Result<Value, String> {
    let mut m = Canvas::filled(320, 200, c::BLACK);
    m.frame(0, 0, 320, 200, 2, c::LIGHT_GREY);
    let title = "NORTHEASTERN UNITED STATES";
    m.text(font, (320 - 8 * title.len() as i32) / 2, 5, title, c::YELLOW);
    // roads: each joins the towns of its two ends
    for r in roads.as_array().ok_or("roads")? {
        let ends: Vec<usize> =
            r["exits"].as_array().into_iter().flatten().filter_map(|e| e["city"].as_u64()).map(|v| v as usize).filter(|&v| v < 16).collect();
        if let [a, b] = ends[..] {
            let ((x0, y0), (x1, y1)) = (centre(a), centre(b));
            m.line((x0 - 1, y0 - 1), (x1 - 1, y1 - 1), c::LIGHT_GREY);
        }
    }
    let mut markers = vec![];
    for (i, &(name, col, row, side)) in TOWNS.iter().enumerate() {
        let (x, y) = centre(i);
        m.fill(x - 2, y - 2, x + 2, y + 2, c::WHITE);
        let w = 8 * name.len() as i32;
        let (tx, ty) = match side {
            'r' => (x + 7, y - 4),
            'l' => (x - 7 - w, y - 4),
            'a' => (x - w / 2, y - 14),
            'B' => (x + 4 - w, y + 6),
            _ => (x - w / 2, y + 6),
        };
        m.text(font, tx.clamp(3, 317 - w), ty, name, c::LIGHT_CYAN);
        markers.push(json!({"city": name, "col": col, "row": row}));
    }
    m.save(path)?;
    Ok(Value::Array(markers))
}
