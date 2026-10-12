//! The ending (`end.prg`): the newspaper with the driver's name in the headline slot, then the
//! congratulations page. Afterwards the driver plays on (the screen pops back to the building).
use crate::app::{Cx, Screen, Trans};
use crate::fb::{col, Frame};
use crate::input::Input;
use crate::text;

/// The name slot in the newspaper picture: text row y 144-151, columns 1-17 (x 8-143).
const SLOT_X: i32 = 8;
const SLOT_Y: i32 = 144;
const SLOT_CHARS: usize = 17;
/// The newspaper stays up about four seconds (`SR_wait_key($3C)`), or until a key.
const PAPER_TICKS: u32 = 240;
/// Keys are ignored for this long after a page appears, so a key held from the FBI's text
/// doesn't skip it.
const GRACE: u32 = 30;

/// The original's text page; AUTODUEL is renamed CARDUL.
const CONGRATULATIONS: [&str; 20] = [
    "",
    "",
    "",
    "Congratulations for completing",
    "",
    "CARDUL",
    "",
    "We hope you have enjoyed playing the",
    "game, and would like to encourage you",
    "to try our other role-playing and",
    "strategy games.",
    "",
    "If you would like to continue with",
    "your CARDUL driver, simply continue",
    "with your current driver.",
    "",
    "",
    "",
    "...Thanks and happy gaming from",
    "the folks at Origin Systems...",
];

pub struct EndingScreen {
    /// the headline: driver name + "."
    headline: String,
    /// 0 = newspaper, 1 = congratulations
    page: u8,
    t: u32,
}

impl EndingScreen {
    pub fn new(cx: &mut Cx) -> EndingScreen {
        let name = cx.game().driver.name.trim().to_string();
        let mut headline: String = name.chars().take(SLOT_CHARS - 1).collect();
        headline.push('.');
        EndingScreen { headline, page: 0, t: 0 }
    }
}

impl Screen for EndingScreen {
    fn update(&mut self, _cx: &mut Cx, inp: &Input) -> Trans {
        self.t += 1;
        let key = self.t > GRACE && inp.any_key();
        match self.page {
            0 if key || self.t >= PAPER_TICKS => {
                self.page = 1;
                self.t = 0;
                Trans::None
            }
            1 if key => Trans::Pop,
            _ => Trans::None,
        }
    }

    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        let a = &cx.assets;
        fb.clear(col::BLACK);
        if self.page == 0 {
            fb.blit(&a.end_newspaper, 0, 0);
            if let Some(n) = &a.config.news_name {
                // art with its own name bar: the name in it, at the normal text size (an
                // empty bar: the paper has no place for it)
                let r = n.bar;
                if r[2] <= r[0] {
                    return;
                }
                let size = fb.scale;
                let y = r[1] + (r[3] - r[1] - 8 * size) / 2;
                text::draw_px(fb, a, r[0] + 3 * size, y, &self.headline, size, crate::assets::rgb(n.ink));
                return;
            }
            // the original prints the name with the font and then inverts the cells: dark
            // letters on the white paper
            let w = 8 * self.headline.chars().count() as i32;
            fb.fill(SLOT_X, SLOT_Y, w, 8, col::WHITE);
            text::draw(fb, a, SLOT_X, SLOT_Y, &self.headline, col::BLACK);
        } else {
            for (i, l) in CONGRATULATIONS.iter().enumerate() {
                let x = (40 - l.chars().count() as i32) / 2;
                text::at(fb, a, x, i as i32, l, col::WHITE);
            }
        }
    }
}
