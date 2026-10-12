//! Road and arena driving screen: runs the [`Battle`] at the driving speed setting and
//! draws the playfield and the HUD (layout of the original, see `drive_hud.rs`).
//!
//! The playfield is the original's 240 x 128 window (screen x 40-279, y 0-127) with the
//! player at screen (160, 64). One world unit is 2 x 2 screen pixels, for scenery and
//! vehicles alike (FIX: the original drew vehicles on a 7/8 "Apple" horizontal scale).
use crate::app::{Cx, Screen, Trans};
use crate::fb::{col, Frame};
use crate::input::{Input, Key};
use crate::screens::city::{after_death, Arrival, CityScreen};
use crate::screens::hotkeys;
use cardul_sim::combat::motion::car_frame;
use cardul_sim::combat::{shape, Battle, BattleEnd, BattleKind, DriveInput};

#[path = "drive_hud.rs"]
mod hud;

/// Screen position of the player (the camera centre), original layout.
pub const CENTER_X: i32 = 160;
pub const CENTER_Y: i32 = 64;
/// The playfield window (inside the HUD frame lines), original layout.
pub const VIEW_X: i32 = 40;
pub const VIEW_Y: i32 = 2;
pub const VIEW_W: i32 = 240;
pub const VIEW_H: i32 = 126;

/// The playfield window and camera centre of the art in use ([`crate::assets::DriveView`]).
#[derive(Clone, Copy)]
struct View {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    cx: i32,
    cy: i32,
}

impl View {
    fn of(cx: &Cx) -> View {
        match cx.assets.config.drive_view {
            Some(v) => View { x: v.rect[0], y: v.rect[1], w: v.rect[2], h: v.rect[3], cx: v.center[0], cy: v.center[1] },
            None => View { x: VIEW_X, y: VIEW_Y, w: VIEW_W, h: VIEW_H, cx: CENTER_X, cy: CENTER_Y },
        }
    }
    /// World -> screen for a camera centred on `cam`.
    fn to_screen(self, cam: (i32, i32), x: i32, y: i32) -> (i32, i32) {
        (self.cx + 2 * (x - cam.0), self.cy + 2 * (y - cam.1))
    }
}

/// Start driving road `road`, leaving `from_city`.
pub fn road(cx: &mut Cx, road: u8, from_city: u8) -> Box<dyn Screen> {
    let (data, game) = cx.dg();
    let b = Battle::new(game, data, BattleKind::Road { road, from_city });
    Box::new(DriveScreen::new(b))
}

/// Start arena event 0..6 or practice (7). Pushed by the Arena building, which reads
/// `cx.arena_result` when it resumes.
pub fn arena(cx: &mut Cx, event: u8) -> Box<dyn Screen> {
    let (data, game) = cx.dg();
    let b = Battle::new(game, data, BattleKind::Arena { event });
    Box::new(DriveScreen::new(b))
}

pub struct DriveScreen {
    battle: Battle,
    acc: u32,
    paused: bool,
    /// key presses waiting for the next simulation frame (they arrive at 60 Hz)
    pending: DriveInput,
    /// render ticks, for blinking
    ticks: u64,
}

impl DriveScreen {
    pub fn new(battle: Battle) -> DriveScreen {
        DriveScreen { battle, acc: 0, paused: false, pending: DriveInput::default(), ticks: 0 }
    }
    pub fn battle(&self) -> &Battle {
        &self.battle
    }
    /// Tools/tests: change the running battle.
    pub fn battle_mut(&mut self) -> &mut Battle {
        &mut self.battle
    }

    /// Remember this tick's key presses until the simulation consumes them.
    fn latch_keys(&mut self, inp: &Input) {
        for k in &inp.keys {
            match k {
                Key::Space => self.pending.next_weapon = true,
                Key::Char('g') => self.pending.toggle_foot = true,
                Key::Char('0') => self.pending.select = Some(9),
                Key::Char(c @ '1'..='9') => self.pending.select = Some(*c as u8 - b'1'),
                _ => {}
            }
        }
    }

    fn finish(&mut self, cx: &mut Cx, end: BattleEnd) -> Trans {
        match end {
            BattleEnd::Arrived { city, road, .. } => {
                let g = cx.g();
                g.driver.city = city;
                Trans::Replace(Box::new(CityScreen::arrive(cx, Arrival::Road(road))))
            }
            BattleEnd::Died => after_death(cx),
            BattleEnd::Arena(r) => {
                cx.arena_result = Some(r);
                Trans::Pop
            }
        }
    }
}

impl Screen for DriveScreen {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        self.ticks += 1;
        if let Some(t) = hotkeys(cx, inp, false) {
            return t;
        }
        if inp.pressed(Key::Char('p')) || inp.pressed(Key::Esc) {
            self.paused = !self.paused;
            cx.sfx(cardul_sim::Sfx::Click);
        }
        if self.paused {
            return Trans::None;
        }
        self.latch_keys(inp);
        self.acc += cx.settings.sim_hz;
        while self.acc >= crate::app::TICK_HZ {
            self.acc -= crate::app::TICK_HZ;
            let di = DriveInput {
                dx: inp.held.dx(),
                dy: inp.held.dy(),
                fire: inp.held.fire,
                ..std::mem::take(&mut self.pending)
            };
            let (data, game) = cx.dg();
            let end = self.battle.step(game, data, &di);
            for s in std::mem::take(&mut self.battle.sfx) {
                cx.sfx(s);
            }
            if let Some(end) = end {
                return self.finish(cx, end);
            }
        }
        Trans::None
    }

    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        fb.clear(col::BLACK);
        hud::draw_frame(cx, fb, &self.battle);
        let v = View::of(cx);
        fb.set_clip(v.x, v.y, v.w, v.h);
        draw_playfield(cx, fb, &self.battle, v);
        fb.reset_clip();
        hud::draw_values(cx, fb, &self.battle, self.ticks);
        if self.paused {
            hud::draw_message(cx, fb, "Paused.");
        }
    }
    fn as_any(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
    fn key_clicks(&self) -> bool {
        false
    }
    /// Revs follow the speed bar; on foot, paused or once the battle is over, silence.
    fn engine(&self, cx: &Cx) -> Option<f32> {
        let b = &self.battle;
        let running = cx.game.as_ref().and_then(|g| g.car()).map(|c| c.plant.dp > 0 && c.battery > 0).unwrap_or(false);
        if self.paused || b.player.on_foot || b.is_dying() || !running {
            return None;
        }
        Some(b.player.speed_bar() as f32 / 9.0)
    }
}

/// Scenery, mines and oil, the cars, the walker, clouds and rockets, tracers, fires.
fn draw_playfield(cx: &Cx, fb: &mut Frame, b: &Battle, v: View) {
    let to_screen = |cam: (i32, i32), x: i32, y: i32| v.to_screen(cam, x, y);
    let a = &cx.assets;
    let p = &b.player;
    let cam = (p.x, p.y);
    let near = |x: i32, y: i32| (x - cam.0).abs() < 80 && (y - cam.1).abs() < 48;

    // the ground (art sets that have one; the original's is black), moving with the world
    let ground = if b.is_arena() { a.ground_arena.as_ref() } else { a.ground_road.as_ref() };
    if let Some(gr) = ground {
        let (tw, th) = (gr.lw().max(1), gr.lh().max(1));
        let (sx, sy) = to_screen(cam, 0, 0);
        let x0 = v.x - (v.x - sx).rem_euclid(tw);
        let y0 = v.y - (v.y - sy).rem_euclid(th);
        for y in (y0..v.y + v.h).step_by(th as usize) {
            for x in (x0..v.x + v.w).step_by(tw as usize) {
                fb.blit(gr, x, y);
            }
        }
    }
    // scenery of the tiles around the player
    let view = b.map(&cx.data);
    for o in view.objects_in(cam.0 - 70, cam.1 - 40, cam.0 + 70, cam.1 + 40) {
        let (sx, sy) = to_screen(cam, o.x, o.y);
        a.draw_scenery(fb, o.shape as usize, sx, sy);
    }
    // things on the ground
    for pr in b.pproj.iter().chain(b.eproj.iter()).filter(|pr| pr.on_ground()) {
        if let (Some(s), true) = (pr.shape(), near(pr.x, pr.y)) {
            let (sx, sy) = to_screen(cam, pr.x, pr.y);
            a.draw_shape(fb, s, sx, sy);
        }
    }
    // enemy cars (wrecks keep their last frame, as in the original)
    for e in b.enemies.iter().flatten() {
        if near(e.x, e.y) {
            let (sx, sy) = to_screen(cam, e.x, e.y);
            a.draw_shape(fb, e.shape(), sx, sy);
        }
    }
    // the player's car, and the walker
    if p.on_foot {
        if p.has_car && near(p.car_x, p.car_y) {
            let (sx, sy) = to_screen(cam, p.car_x, p.car_y);
            a.draw_shape(fb, car_frame(p.heading), sx, sy);
        }
        a.draw_shape(fb, 16 + p.walk_frame as usize, v.cx, v.cy);
    } else {
        a.draw_shape(fb, car_frame(p.heading), v.cx, v.cy);
    }
    // clouds and rockets
    for pr in b.pproj.iter().chain(b.eproj.iter()).filter(|pr| !pr.on_ground()) {
        if let (Some(s), true) = (pr.shape(), near(pr.x, pr.y)) {
            let (sx, sy) = to_screen(cam, pr.x, pr.y);
            a.draw_shape(fb, s, sx, sy);
        }
    }
    // tracers and flame bursts of this frame
    for t in &b.traces {
        let (sx, sy) = to_screen(cam, t.x, t.y);
        a.draw_shape(fb, t.shape, sx, sy);
    }
    // burning wrecks (a random flame frame each draw)
    for (i, e) in b.enemies.iter().enumerate() {
        if let Some(e) = e
            && e.burning && near(e.x, e.y) {
            let (sx, sy) = to_screen(cam, e.x, e.y);
            let f = (b.frame as usize * 7 + i * 3) % 4;
            a.draw_shape(fb, shape::FIRE + f, sx, sy);
        }
    }
}
