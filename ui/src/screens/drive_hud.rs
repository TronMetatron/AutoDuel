//! The driving HUD, laid out like the original (`SR_hud_draw` frame, labels `r1 $C269`,
//! values `r2 $7B76`, gauges `r1 $CCB1`):
//!
//! * left: weapon slots 1..0 with their 2-letter abbreviation (selected slot inverted) and
//!   the battery bar (red, text column 2, rows 95-144);
//! * right: armor F B L R U and the four tires (icon glyphs $5B-$60);
//! * bottom: radar box (while the power plant has 5+ DP), odometer, speed bar (green),
//!   "Power Plant NN" and "Driver NN" (health + body armor);
//! * message area: screen y 164-197.
use crate::app::Cx;
use crate::assets::{rgb, Gauge, HudLayout, Image, Rect};
use crate::fb::{col, Frame};
use crate::text;
use cardul_sim::combat::{Battle, BattleKind};
use cardul_sim::tables::WEAPONS;

/// Text rows (pixel y) of the armor values F B L R U and the tires FL FR BL BR.
const ARMOR_Y: [i32; 5] = [16, 24, 32, 40, 48];
const TIRE_Y: [i32; 4] = [88, 96, 104, 112];
/// Tire icon glyph per tire FL FR BL BR (`r1 $C319`: $60, $5B, $5D, $5C).
const TIRE_GLYPH: [usize; 4] = [5, 0, 2, 1];
const VALUE_X: i32 = 296;

/// Radar box interior and its centre (`r1 $CCE4`).
const RADAR: (i32, i32, i32, i32) = (40, 130, 36, 32);
const RADAR_CX: i32 = 58;
const RADAR_CY: i32 = 146;

/// The static frame and labels: the extracted HUD picture if present, else drawn here.
pub fn draw_frame(cx: &Cx, fb: &mut Frame, b: &Battle) {
    let a = &cx.assets;
    let img = if b.is_arena() { a.arena_hud.as_ref().or(a.road_hud.as_ref()) } else { a.road_hud.as_ref() };
    if let Some(img) = img {
        fb.blit(img, 0, 0);
        return;
    }
    // frame lines (2 px thick), as `SR_hud_draw` draws them
    let w = col::WHITE;
    fb.fill(0, 0, 320, 2, w);
    fb.fill(0, 162, 320, 2, w);
    fb.fill(0, 198, 320, 2, w);
    fb.fill(0, 0, 2, 200, w);
    fb.fill(318, 0, 2, 200, w);
    fb.fill(38, 0, 2, 164, w);
    fb.fill(280, 0, 2, 130, w);
    fb.fill(38, 128, 244, 2, w);
    fb.fill(76, 128, 2, 36, w);
    fb.rect(14, 94, 12, 53, 2, w);
    fb.rect(140, 133, 40, 13, 2, w);
    fb.rect(86, 147, 148, 13, 2, w);
    for (i, d) in "1234567890".chars().enumerate() {
        text::draw(fb, a, 8, 8 + 8 * i as i32, &format!("{}..", d), w);
    }
    text::draw(fb, a, 8, 152, "Bat", w);
    text::draw(fb, a, 288, 8, "Arm", w);
    for (i, l) in ["F", "B", "L", "R", "U"].iter().enumerate() {
        text::draw(fb, a, 288, ARMOR_Y[i], l, w);
    }
    text::draw(fb, a, 288, 80, "Trs", w);
    for t in 0..4 {
        text::draw(fb, a, 288, TIRE_Y[t], &text::TIRE_GLYPHS[TIRE_GLYPH[t]].to_string(), w);
    }
    text::draw(fb, a, 200, 136, "Power Plant", w);
    text::draw(fb, a, 240, 152, "Driver", w);
}

/// Everything that changes: weapons, gauges, numbers, radar, message.
pub fn draw_values(cx: &Cx, fb: &mut Frame, b: &Battle, ticks: u64) {
    if let Some(h) = &cx.assets.config.hud {
        return draw_values_layout(cx, fb, b, ticks, h);
    }
    let a = &cx.assets;
    let g = cx.game();
    let car = g.car();
    let w = col::WHITE;

    // weapon slots
    for i in 0..10 {
        let y = 8 + 8 * i as i32;
        fb.fill(16, y, 16, 8, col::BLACK);
        let abbr = car.and_then(|c| c.weapons[i]).map(|wp| WEAPONS[wp.kind as usize].abbr).unwrap_or("..");
        text::draw(fb, a, 16, y, abbr, w);
        if b.selected == Some(i as u8) {
            fb.invert(8, y, 24, 8);
        }
    }

    // battery: red from the bottom, 1 row per 2 units
    fb.fill(16, 96, 8, 49, col::BLACK);
    let bat = car.map(|c| c.battery as i32).unwrap_or(0);
    let rows = (bat / 2).min(49);
    fb.fill(16, 145 - rows, 8, rows, col::RED);

    // armor, tires, power plant, driver
    if let Some(c) = car {
        for (i, y) in ARMOR_Y.iter().enumerate() {
            number(cx, fb, VALUE_X, *y, c.armor[i].pts as u32);
        }
        for (t, y) in TIRE_Y.iter().enumerate() {
            number(cx, fb, VALUE_X, *y, c.tires[t].dp as u32);
        }
        number(cx, fb, VALUE_X, 136, c.plant.dp as u32);
    }
    number(cx, fb, VALUE_X, 152, g.driver.driver_points() as u32);

    // radar
    fb.fill(RADAR.0, RADAR.1, RADAR.2, RADAR.3, col::BLACK);
    if b.radar_on(g) {
        let p = &b.player;
        for e in b.enemies.iter().flatten() {
            let (dx, dy) = (e.x - p.x, e.y - p.y);
            if !(-256..256).contains(&dx) || !(-256..256).contains(&dy) {
                continue;
            }
            // 16 world units per radar step; x steps are 8/7 px like the original
            let rx = RADAR.0 + ((dx >> 4) + 16) * 8 / 7;
            let ry = RADAR_CY + (dy >> 4);
            radar_dot(fb, rx, ry);
        }
        if (ticks / 8).is_multiple_of(2) {
            radar_dot(fb, RADAR_CX, RADAR_CY);
        }
    }

    // odometer drums
    odometer(cx, fb, &b.odometer);

    // speed bar: steps 1..9, step i is i rows high
    fb.fill(88, 149, 144, 9, col::BLACK);
    let level = b.player.speed_bar() as i32;
    for i in 1..=level.min(9) {
        fb.fill(72 + 16 * i, 158 - i, 16, i, col::GREEN);
    }

    if let Some(m) = &b.message {
        draw_message(cx, fb, m);
    }
}

/// A message in the message area (second text row of it, like the original).
pub fn draw_message(cx: &Cx, fb: &mut Frame, m: &str) {
    if let Some(h) = &cx.assets.config.hud
        && let Some(r) = h.message {
        // art with its own message panel: the lines centred in it, at the normal text size
        fb.fill_px(r, rgb(h.message_bg.unwrap_or([0, 0, 0])));
        let size = fb.scale;
        let lines = text::wrap(m, 38);
        let n = lines.len().min(3) as i32;
        let y0 = r[1] + (r[3] - r[1] - n * 8 * size) / 2;
        for (i, line) in lines.iter().take(3).enumerate() {
            text::draw_px(fb, &cx.assets, r[0] + 2 * size, y0 + 8 * size * i as i32, line, size, rgb(h.ink));
        }
        return;
    }
    fb.fill(2, 164, 316, 34, col::BLACK);
    for (i, line) in text::wrap(m, 38).iter().take(3).enumerate() {
        text::draw(fb, &cx.assets, 8, 176 + 8 * i as i32, line, col::WHITE);
    }
}

/// A 2-digit value at (x, y), cleared first.
fn number(cx: &Cx, fb: &mut Frame, x: i32, y: i32, v: u32) {
    fb.fill(x, y, 16, 8, col::BLACK);
    text::draw(fb, &cx.assets, x, y, &format!("{:02}", v.min(99)), col::WHITE);
}

fn radar_dot(fb: &mut Frame, x: i32, y: i32) {
    let (x0, y0, w, h) = RADAR;
    if x >= x0 && x < x0 + w - 1 && y >= y0 && y < y0 + h - 1 {
        fb.fill(x, y, 2, 2, col::WHITE);
    }
}

/// Four rolling digit drums: drum position p (0..79) shows rows p..p+7 of the stacked
/// digit strip 0..9 (`r1 $CDFE`).
fn odometer(cx: &Cx, fb: &mut Frame, drums: &[u8; 4]) {
    let font = &cx.assets.font;
    fb.fill(142, 135, 36, 9, col::BLACK);
    for (i, p) in drums.iter().enumerate() {
        let x = 144 + 8 * i as i32;
        let y = 136;
        let d = (*p / 8) as i32;
        let o = (*p % 8) as i32;
        let gx = |digit: i32| (0x30 + digit - 0x20) * 8;
        fb.blit_part(font, gx(d), o, 8, 8 - o, x, y, Some(col::WHITE));
        if o > 0 {
            fb.blit_part(font, gx((d + 1) % 10), 0, 8, o, x, y + 8 - o, Some(col::WHITE));
        }
    }
}

// ---- art with its own HUD layout (`art.json` "hud"): every position in art pixels

/// The changing parts of the HUD on art laid out by [`HudLayout`]: values centred in their
/// boxes, gauges lit segment by segment from their "all lit" pictures.
fn draw_values_layout(cx: &Cx, fb: &mut Frame, b: &Battle, ticks: u64, h: &HudLayout) {
    let a = &cx.assets;
    let g = cx.game();
    let car = g.car();
    let (px, ink, fill) = (h.text_px, rgb(h.ink), rgb(h.box_fill));

    // weapon slots: an empty one keeps the picture's two dots
    for (i, r) in h.weapons.iter().enumerate().take(10) {
        let w = car.and_then(|c| c.weapons[i]);
        let sel = b.selected == Some(i as u8);
        if w.is_none() && !sel {
            continue;
        }
        let (bg, fg) = if sel { (rgb(h.select_box), rgb(h.select_ink)) } else { (fill, ink) };
        fb.fill_px(*r, bg);
        let abbr = w.map(|wp| WEAPONS[wp.kind as usize].abbr).unwrap_or("..");
        centred(cx, fb, *r, abbr, px, fg);
    }

    // battery (0..49 rows in the original) and speed (0..9)
    if let Some(img) = &a.hud_battery {
        let rows = (car.map(|c| c.battery as i32).unwrap_or(0) / 2).min(49);
        gauge(fb, img, &h.battery, rows, 49, false);
    }
    if let Some(img) = &a.hud_speed {
        gauge(fb, img, &h.speed, (b.player.speed_bar() as i32).min(9), 9, true);
    }

    // armor, tires, power plant, driver
    let value = |fb: &mut Frame, r: Rect, v: u32| {
        fb.fill_px(r, fill);
        centred(cx, fb, r, &format!("{:02}", v.min(99)), px, ink);
    };
    if let Some(c) = car {
        for (i, r) in h.armor.iter().enumerate().take(5) {
            value(fb, *r, c.armor[i].pts as u32);
        }
        for (t, r) in h.tires.iter().enumerate().take(4) {
            value(fb, *r, c.tires[t].dp as u32);
        }
        value(fb, h.plant, c.plant.dp as u32);
    }
    value(fb, h.driver, g.driver.driver_points() as u32);

    // radar: 512 x 512 world units around the car across the box
    if b.radar_on(g) {
        let r = h.radar;
        let dot = |fb: &mut Frame, dx: i32, dy: i32| {
            let x = r[0] + (dx + 256) * (r[2] - r[0]) / 512;
            let y = r[1] + (dy + 256) * (r[3] - r[1]) / 512;
            let d = fb.scale + 2;
            fb.fill_px([(x - d / 2).max(r[0]), (y - d / 2).max(r[1]), (x + d / 2).min(r[2]), (y + d / 2).min(r[3])], rgb(h.radar_dot));
        };
        let p = &b.player;
        for e in b.enemies.iter().flatten() {
            let (dx, dy) = (e.x - p.x, e.y - p.y);
            if (-256..256).contains(&dx) && (-256..256).contains(&dy) {
                dot(fb, dx, dy);
            }
        }
        if (ticks / 8).is_multiple_of(2) {
            dot(fb, 0, 0);
        }
    }

    // the selected weapon's shots ("--" for none, or a weapon without ammunition)
    if let Some(r) = h.ammo {
        fb.fill_px(r, fill);
        let shots = b
            .selected
            .and_then(|i| car.and_then(|c| c.weapons[i as usize]))
            .filter(|w| w.uses_ammo())
            .map(|w| format!("{:02}", w.ammo.min(99)))
            .unwrap_or_else(|| "--".to_string());
        centred(cx, fb, r, &shots, px, ink);
    }
    // title bar: the road (or the arena's town) and the date
    if let Some(r) = h.bar {
        let bar_ink = rgb(h.bar_ink.unwrap_or(h.ink));
        let place = match b.kind {
            BattleKind::Road { road, .. } => cx.data.roads.get(road as usize).map(|r| r.name.as_str()).unwrap_or(""),
            BattleKind::Arena { .. } => cx.data.city_name(g.driver.city),
        };
        text::draw_px(fb, a, r[0], r[1], place, px, bar_ink);
        let date = g.driver.date.long();
        text::draw_px(fb, a, r[2] - text::width_px(&date, px), r[1], &date, px, bar_ink);
    }

    // odometer: four rolling drums centred in the box
    let r = h.odometer;
    fb.fill_px([r[0] + 2, r[1] + 2, r[2] - 2, r[3] - 2], fill);
    let x0 = r[0] + (r[2] - r[0] - 4 * 8 * px) / 2;
    let y = r[1] + (r[3] - r[1] - 8 * px) / 2;
    for (i, p) in b.odometer.iter().enumerate() {
        let x = x0 + 8 * px * i as i32;
        let (d, o) = ((*p / 8) as i32, (*p % 8) as i32);
        let cell = |digit: i32| 0x30 + digit - 0x20;
        fb.glyph_px(&a.font, cell(d), o, 8, x, y, px, ink);
        if o > 0 {
            fb.glyph_px(&a.font, cell((d + 1) % 10), 0, o, x, y + (8 - o) * px, px, ink);
        }
    }

    if let Some(m) = &b.message {
        draw_message(cx, fb, m);
    }
}

/// `s` centred in box `r` (art pixels), each font pixel `px` art pixels.
fn centred(cx: &Cx, fb: &mut Frame, r: Rect, s: &str, px: i32, rgb: u32) {
    let x = r[0] + (r[2] - r[0] - text::width_px(s, px)) / 2;
    let y = r[1] + (r[3] - r[1] - 8 * px) / 2;
    text::draw_px(fb, &cx.assets, x, y, s, px, rgb);
}

/// Light the first segments of a gauge: `v` of `max` (any charge shows at least one segment).
fn gauge(fb: &mut Frame, img: &Image, gd: &Gauge, v: i32, max: i32, horizontal: bool) {
    let n = gd.cuts.len() as i32 - 1;
    if v <= 0 || n <= 0 || max <= 0 {
        return;
    }
    let lit = ((v * n + max - 1) / max).clamp(1, n) as usize;
    let (start, end) = (gd.cuts[0], gd.cuts[lit]);
    let [ax, ay] = gd.at;
    if horizontal {
        fb.blit_px(img, [start - ax, 0, end - ax, img.h], start, ay);
    } else {
        // fills upwards: cuts run from the bottom (start) to the top
        fb.blit_px(img, [0, end - ay, img.w, start - ay], ax, end);
    }
}
