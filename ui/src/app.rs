//! The screen stack and the shared context every screen gets.
use crate::assets::{ArtSet, Assets};
use crate::fb::{col, Frame};
use crate::input::{Input, Key};
use crate::text;
use cardul_sim::combat::{ArenaResult, Sfx};
use cardul_sim::{Game, GameData};
use std::path::{Path, PathBuf};

/// Ticks per second the front end must call [`App::tick`] at.
pub const TICK_HZ: u32 = 60;

#[derive(Clone, Debug)]
pub struct Settings {
    /// driving/arena simulation frames per second (the original ran as fast as the C64 could,
    /// roughly 10-20 fps); F9 cycles through [`SIM_SPEEDS`]
    pub sim_hz: u32,
    pub sound: bool,
    /// the graphics shown; F10 (or G on the main menu) switches, and the choice is kept in
    /// `<saves>/settings.json`
    pub art: ArtSet,
}

/// What is kept in `<saves>/settings.json`.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct SavedSettings {
    #[serde(default)]
    pub art: Option<ArtSet>,
}

impl SavedSettings {
    pub fn load(save_dir: &Path) -> SavedSettings {
        std::fs::read_to_string(save_dir.join("settings.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }
    pub fn save(&self, save_dir: &Path) {
        let _ = std::fs::create_dir_all(save_dir);
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(save_dir.join("settings.json"), s);
        }
    }
}

pub const SIM_SPEEDS: [u32; 5] = [10, 12, 15, 20, 30];

/// Shared state for all screens.
pub struct Cx {
    pub data: GameData,
    pub assets: Assets,
    /// the loaded driver (None on the title / setup screens)
    pub game: Option<Game>,
    pub save_dir: PathBuf,
    /// the file the current driver is saved in (None until a new driver's first save picks
    /// a free name); see [`Cx::start_game`]
    pub save_file: Option<PathBuf>,
    pub sfx: Vec<Sfx>,
    pub tick: u64,
    pub settings: Settings,
    /// result of the last arena event, read by the Arena building when it resumes
    pub arena_result: Option<ArenaResult>,
    /// set to leave the program
    pub quit: bool,
    /// short overlay message (text, until tick)
    pub toast: Option<(String, u64)>,
    /// a screen asks for the other art set ([`App::tick`] switches after the update)
    pub switch_art: bool,
}

impl Cx {
    /// The current game. Panics if no driver is loaded (screens that use it only exist then).
    pub fn g(&mut self) -> &mut Game {
        self.game.as_mut().expect("no driver loaded")
    }
    pub fn game(&self) -> &Game {
        self.game.as_ref().expect("no driver loaded")
    }
    /// Split borrow: the read-only data and the mutable game.
    pub fn dg(&mut self) -> (&GameData, &mut Game) {
        (&self.data, self.game.as_mut().expect("no driver loaded"))
    }
    pub fn sfx(&mut self, s: Sfx) {
        if self.settings.sound {
            self.sfx.push(s);
        }
    }
    pub fn toast(&mut self, s: &str) {
        self.toast = Some((s.to_string(), self.tick + 2 * TICK_HZ as u64));
    }
    /// Make `g` the current driver. `file` is the save it was loaded from; None for a new
    /// driver, whose first save then picks a file name no other driver uses.
    pub fn start_game(&mut self, g: Game, file: Option<PathBuf>) {
        self.game = Some(g);
        self.save_file = file;
    }

    /// Save the current game (the original saves on every city entry and building visit).
    /// FIX: a new driver with the same name as a saved one no longer overwrites that save.
    pub fn save(&mut self) {
        let Some(g) = &self.game else { return };
        let path = match &self.save_file {
            Some(p) => p.clone(),
            None => {
                let base = g.save_file_name();
                let stem = base.trim_end_matches(".json").to_string();
                let mut p = self.save_dir.join(&base);
                let mut n = 2;
                while p.exists() {
                    p = self.save_dir.join(format!("{}_{}.json", stem, n));
                    n += 1;
                }
                self.save_file = Some(p.clone());
                p
            }
        };
        if let Err(e) = g.save_to(&path) {
            eprintln!("save failed: {}", e);
        } else if let Some(name) = path.file_name() {
            let _ = std::fs::write(self.save_dir.join("last.txt"), name.to_string_lossy().as_bytes());
        }
    }
    /// Path of the most recently played driver, if any.
    pub fn last_save(&self) -> Option<PathBuf> {
        let name = std::fs::read_to_string(self.save_dir.join("last.txt")).ok()?;
        let p = self.save_dir.join(name.trim());
        if p.exists() { Some(p) } else { None }
    }
}

pub enum Trans {
    None,
    Push(Box<dyn Screen>),
    Pop,
    Replace(Box<dyn Screen>),
    /// clear the stack and start over with this screen
    Reset(Box<dyn Screen>),
    Quit,
}

pub trait Screen {
    /// Called once per 60 Hz tick while this screen is on top.
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans;
    /// Draw the whole screen.
    fn draw(&self, cx: &Cx, fb: &mut Frame);
    /// Called when a screen pushed on top of this one is popped.
    fn resume(&mut self, _cx: &mut Cx) {}
    /// Should a key pressed on this screen click ([`Sfx::Key`])? Menus and text screens yes;
    /// the city and the road (arrows repeat, weapons have their own sounds) no.
    fn key_clicks(&self) -> bool {
        true
    }
    /// The car's engine on this screen: None for silence, else its revs 0.0 (idle) to 1.0.
    fn engine(&self, _cx: &Cx) -> Option<f32> {
        None
    }
    /// Test/debug hook: the concrete screen, for tools such as the headless runner.
    fn as_any(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
}

pub struct App {
    pub cx: Cx,
    stack: Vec<Box<dyn Screen>>,
    pub frame: Frame,
    /// the art set not in use, once loaded (switching back is then instant)
    spare: Option<Assets>,
}

impl App {
    /// The game, with the graphics last chosen (`<saves>/settings.json`), else the enhanced
    /// ones if they are installed.
    pub fn new(assets_dir: &Path, save_dir: PathBuf) -> Result<App, String> {
        App::with_art(assets_dir, save_dir, None)
    }

    /// The game with art set `art` (None: as [`App::new`]); if the enhanced set can't be
    /// loaded the classic one is used.
    pub fn with_art(assets_dir: &Path, save_dir: PathBuf, art: Option<ArtSet>) -> Result<App, String> {
        let data = GameData::load(&assets_dir.join("data"))?;
        let want = art.or(SavedSettings::load(&save_dir).art).unwrap_or(if ArtSet::Enhanced.available(assets_dir) {
            ArtSet::Enhanced
        } else {
            ArtSet::Classic
        });
        let assets = match Assets::load_art(assets_dir, want) {
            Ok(a) => a,
            Err(e) if want != ArtSet::Classic => {
                eprintln!("{} graphics: {}; using the classic ones", want.name(), e);
                Assets::load_art(assets_dir, ArtSet::Classic)?
            }
            Err(e) => return Err(e),
        };
        let scale = assets.scale;
        let art = assets.art;
        let _ = std::fs::create_dir_all(&save_dir);
        let cx = Cx {
            data,
            assets,
            game: None,
            save_dir,
            save_file: None,
            sfx: vec![],
            tick: 0,
            settings: Settings { sim_hz: 15, sound: true, art },
            arena_result: None,
            quit: false,
            toast: None,
            switch_art: false,
        };
        Ok(App { cx, stack: vec![Box::new(crate::screens::setup::TitleScreen::new())], frame: Frame::new(scale), spare: None })
    }

    /// Show art set `art` from now on and remember the choice. Loading a set the first time
    /// takes a moment; the previous set is kept, so switching back is instant.
    pub fn set_art(&mut self, art: ArtSet) -> Result<(), String> {
        if self.cx.assets.art == art {
            return Ok(());
        }
        let next = match self.spare.take() {
            Some(a) if a.art == art => a,
            other => {
                self.spare = other;
                Assets::load_art(&self.cx.assets.dir.clone(), art)?
            }
        };
        let old = std::mem::replace(&mut self.cx.assets, next);
        self.spare = Some(old);
        self.frame = Frame::new(self.cx.assets.scale);
        self.cx.settings.art = art;
        SavedSettings { art: Some(art) }.save(&self.cx.save_dir);
        Ok(())
    }

    /// F10 / the main menu: the other art set.
    fn toggle_art(&mut self) {
        let art = self.cx.settings.art.other();
        let m = match self.set_art(art) {
            Ok(()) => format!("{} graphics", art.name()),
            Err(e) => {
                eprintln!("{}", e);
                format!("No {} graphics", art.name().to_lowercase())
            }
        };
        self.cx.toast(&m);
        self.cx.sfx(Sfx::Click);
    }

    /// Start directly on a given screen (tools/tests).
    pub fn reset_to(&mut self, s: Box<dyn Screen>) {
        self.stack = vec![s];
    }
    /// The screen on top (tools/tests).
    pub fn top_mut(&mut self) -> Option<&mut Box<dyn Screen>> {
        self.stack.last_mut()
    }
    /// The top screen as a concrete type, if it is one (tools/tests).
    pub fn top_as<T: 'static>(&mut self) -> Option<&mut T> {
        self.stack.last_mut()?.as_any()?.downcast_mut::<T>()
    }
    /// The top screen and the shared context, borrowed together (tools/tests).
    pub fn top_and_cx(&mut self) -> (Option<&mut Box<dyn Screen>>, &mut Cx) {
        (self.stack.last_mut(), &mut self.cx)
    }
    /// Number of screens on the stack (tools/tests).
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn tick(&mut self, inp: &Input) {
        self.cx.tick += 1;
        if inp.keys.iter().any(|k| !matches!(k, Key::F(_))) && self.stack.last().is_some_and(|s| s.key_clicks()) {
            self.cx.sfx(Sfx::Key);
        }
        if inp.pressed(Key::F(9)) {
            let i = SIM_SPEEDS.iter().position(|v| *v == self.cx.settings.sim_hz).unwrap_or(2);
            self.cx.settings.sim_hz = SIM_SPEEDS[(i + 1) % SIM_SPEEDS.len()];
            let m = format!("Driving speed {}", self.cx.settings.sim_hz);
            self.cx.toast(&m);
            self.cx.sfx(Sfx::Click);
        }
        if inp.pressed(Key::F(7)) {
            self.cx.settings.sound = !self.cx.settings.sound;
            let m = if self.cx.settings.sound { "Sound on" } else { "Sound off" };
            self.cx.toast(m);
            self.cx.sfx(Sfx::Click);
        }
        if inp.pressed(Key::F(10)) {
            self.toggle_art();
        }
        let Some(top) = self.stack.last_mut() else {
            self.cx.quit = true;
            return;
        };
        let t = top.update(&mut self.cx, inp);
        match t {
            Trans::None => {}
            Trans::Push(s) => self.stack.push(s),
            Trans::Pop => {
                self.stack.pop();
                if let Some(s) = self.stack.last_mut() {
                    s.resume(&mut self.cx);
                }
            }
            Trans::Replace(s) => {
                self.stack.pop();
                self.stack.push(s);
            }
            Trans::Reset(s) => {
                self.stack.clear();
                self.stack.push(s);
            }
            Trans::Quit => self.cx.quit = true,
        }
        if self.stack.is_empty() {
            self.cx.quit = true;
        }
        if std::mem::take(&mut self.cx.switch_art) {
            self.toggle_art();
        }
    }

    pub fn draw(&mut self) -> &Frame {
        self.frame.reset_clip();
        if let Some(top) = self.stack.last() {
            top.draw(&self.cx, &mut self.frame);
        }
        self.frame.reset_clip();
        // the toast sits at the right end of the top border row, where it covers no screen's
        // text, instead of a box in the middle of whatever screen is up (play-test)
        if let Some((m, until)) = &self.cx.toast
            && self.cx.tick < *until {
            let s = format!(" {} ", m);
            let x = 320 - 8 - 8 * s.chars().count() as i32;
            text::draw_bg(&mut self.frame, &self.cx.assets, x, 0, &s, col::BLACK, col::WHITE);
        }
        &self.frame
    }

    pub fn take_sfx(&mut self) -> Vec<Sfx> {
        std::mem::take(&mut self.cx.sfx)
    }
    /// The engine sound to play now: None for none (sound off, or no car running on this
    /// screen), else revs from 0.0 (idle) to 1.0 (top speed).
    pub fn engine(&self) -> Option<f32> {
        if !self.cx.settings.sound {
            return None;
        }
        self.stack.last()?.engine(&self.cx).map(|v| v.clamp(0.0, 1.0))
    }
    pub fn wants_quit(&self) -> bool {
        self.cx.quit
    }
    /// Number of 60 Hz ticks per driving-simulation frame at the current speed setting.
    pub fn sim_hz(&self) -> u32 {
        self.cx.settings.sim_hz
    }
}
