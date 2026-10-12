//! Cardul: the native shell (window, input, display, sound) around `cardul_ui`.
mod audio;
mod cli;
mod display;
mod input;

use bevy::log::{Level, LogPlugin};
use bevy::prelude::*;
use bevy::window::{ExitSystems, FileDragAndDrop, MonitorSelection, WindowCloseRequested, WindowMode};
use cardul_ui::assets::ArtSet;
use cardul_ui::install::{Installer, Outcome};
use std::path::{Path, PathBuf};

/// The game, or the first-start setup screen until the player's copy of Autoduel has been
/// imported. The game's screens are `Box<dyn Screen>` (not `Send`), so it lives in the main
/// world as non-send data and its systems run on the main thread.
pub struct CardulApp {
    mode: Mode,
    assets: PathBuf,
    saves: PathBuf,
    art: Option<ArtSet>,
    quit: bool,
}

enum Mode {
    Setup(Box<Installer>),
    Play(Box<cardul_ui::App>),
}

impl CardulApp {
    fn play(assets: &Path, saves: PathBuf, art: Option<ArtSet>) -> Result<cardul_ui::App, String> {
        cardul_ui::App::with_art(assets, saves, art).map_err(|e| format!("Cardul could not load its assets from {}:\n  {e}", assets.display()))
    }
    pub fn tick(&mut self, inp: &cardul_ui::Input) {
        match &mut self.mode {
            Mode::Play(app) => app.tick(inp),
            Mode::Setup(inst) => match inst.tick(inp) {
                Some(Outcome::Imported(s)) => self.start(s),
                Some(Outcome::Quit) => self.quit = true,
                None => {}
            },
        }
    }
    /// A file or folder dropped on the window (the setup screen looks there).
    pub fn dropped(&mut self, path: &Path) {
        if let Mode::Setup(inst) = &mut self.mode
            && let Some(Outcome::Imported(s)) = inst.retry(Some(path)) {
            self.start(s);
        }
    }
    fn start(&mut self, s: cardul_import::Summary) {
        println!("Cardul: built {} pictures and the world data from {}", s.pictures, s.source.display());
        match CardulApp::play(&self.assets, self.saves.clone(), self.art) {
            Ok(app) => self.mode = Mode::Play(Box::new(app)),
            Err(e) => {
                eprintln!("{e}");
                self.quit = true;
            }
        }
    }
    pub fn tick_count(&self) -> u64 {
        match &self.mode {
            Mode::Play(app) => app.cx.tick,
            Mode::Setup(inst) => inst.tick_count(),
        }
    }
    pub fn draw(&mut self) -> &cardul_ui::fb::Frame {
        match &mut self.mode {
            Mode::Play(app) => app.draw(),
            Mode::Setup(inst) => inst.draw(),
        }
    }
    pub fn frame_size(&mut self) -> (i32, i32) {
        let f = self.draw();
        (f.pw, f.ph)
    }
    pub fn wants_quit(&self) -> bool {
        self.quit || matches!(&self.mode, Mode::Play(app) if app.wants_quit())
    }
    pub fn take_sfx(&mut self) -> Vec<cardul_sim::Sfx> {
        match &mut self.mode {
            Mode::Play(app) => app.take_sfx(),
            Mode::Setup(_) => vec![],
        }
    }
    pub fn engine(&self) -> Option<f32> {
        match &self.mode {
            Mode::Play(app) => app.engine(),
            Mode::Setup(_) => None,
        }
    }
    /// Save the current driver (window closed).
    pub fn save(&mut self) {
        if let Mode::Play(app) = &mut self.mode
            && app.cx.game.is_some() {
            app.cx.save();
        }
    }
}

fn main() -> AppExit {
    let opts = match cli::parse(std::env::args().skip(1)) {
        Ok(cli::Parsed::Run(o)) => o,
        Ok(cli::Parsed::Help) => {
            println!("{}", cli::USAGE);
            return AppExit::Success;
        }
        Err(e) => {
            eprintln!("cardul: {e}\n\n{}", cli::USAGE);
            return AppExit::error();
        }
    };
    let assets = match cli::resolve_assets(opts.assets) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            return AppExit::error();
        }
    };
    let saves = opts.saves.unwrap_or_else(|| cli::default_saves(&assets));
    // the classic graphics and the world data come from the player's copy of the game
    let places = cli::original_places(opts.original, &assets);
    let mut problem = None;
    if opts.reimport || cardul_import::needs_import(&assets) {
        match cardul_ui::install::import_from(&places, &assets) {
            Ok(s) => println!("Cardul: built {} pictures and the world data from {}", s.pictures, s.source.display()),
            Err(e) => problem = Some(e),
        }
    }
    let mode = if cardul_import::has_game_data(&assets) {
        match CardulApp::play(&assets, saves.clone(), opts.art) {
            Ok(app) => Mode::Play(Box::new(app)),
            Err(e) => {
                eprintln!("{e}");
                return AppExit::error();
            }
        }
    } else {
        Mode::Setup(Box::new(Installer::new(assets.clone(), places, problem.filter(|_| opts.reimport))))
    };
    let game = CardulApp { mode, assets, saves, art: opts.art, quit: false };

    let window = Window {
        title: "Cardul".into(),
        resolution: opts.size.into(),
        resizable: true,
        mode: if opts.fullscreen {
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        } else {
            WindowMode::Windowed
        },
        ..default()
    };
    App::new()
        .add_plugins(
            DefaultPlugins
                .set(WindowPlugin { primary_window: Some(window), ..default() })
                .set(ImagePlugin::default_nearest())
                .set(LogPlugin { level: Level::WARN, ..default() }),
        )
        .insert_resource(Time::<Fixed>::from_hz(cardul_ui::app::TICK_HZ as f64))
        .insert_non_send(game)
        .add_plugins((input::ControlsPlugin, display::DisplayPlugin, audio::SfxPlugin))
        .add_systems(FixedUpdate, tick)
        .add_systems(Update, dropped)
        .add_systems(Last, save_on_exit.after(ExitSystems))
        .run()
}

/// One 60 Hz game tick.
fn tick(
    mut game: NonSendMut<CardulApp>,
    mut pending: ResMut<input::PendingInput>,
    mut sfx: ResMut<audio::SfxQueue>,
    mut exit: MessageWriter<AppExit>,
) {
    let app = &mut *game;
    if app.wants_quit() {
        return;
    }
    app.tick(&pending.next_tick());
    sfx.0.extend(app.take_sfx());
    if app.wants_quit() {
        exit.write(AppExit::Success);
    }
}

/// Save the current driver when the window is closed (or the app is stopped some other way).
/// When the game itself asked to quit it has already saved.
fn save_on_exit(
    mut game: NonSendMut<CardulApp>,
    mut closes: MessageReader<WindowCloseRequested>,
    mut exits: MessageReader<AppExit>,
    mut saved: Local<bool>,
) {
    let closing = closes.read().count() + exits.read().count() > 0;
    if !closing || *saved || game.wants_quit() {
        return;
    }
    *saved = true;
    game.save();
}

/// The Autoduel folder dragged onto the window (setup screen).
fn dropped(mut game: NonSendMut<CardulApp>, mut drops: MessageReader<FileDragAndDrop>) {
    for d in drops.read() {
        if let FileDragAndDrop::DroppedFile { path_buf, .. } = d {
            game.dropped(path_buf);
        }
    }
}
