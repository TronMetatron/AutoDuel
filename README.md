# AutoDuel 

An unofficial, open-source remake of Origin Systems' *Autoduel* (1985) in Rust: drive the
highways of 2030, run courier jobs, fight in the arena and build your own armed car. The game
logic and screens are engine-free Rust; a thin [Bevy](https://bevy.org) shell provides the
window, input and sound.

**You need your own copy of Autoduel for DOS.** This repository contains none of the original
game's files: no graphics, maps, text or code. On first start Cardul reads your copy and builds
its classic graphics and world data from it, the way Exult or ScummVM work.

## Getting started

1. Install Rust 1.95 or newer (<https://rustup.rs>). On Linux you also need the ALSA and udev
   development packages (`libasound2-dev libudev-dev` on Debian/Ubuntu).
2. Copy all the files of your Autoduel (DOS) into a folder called `original` in the repository
   root (a subfolder such as `original/AUTODUEL/` is fine too).
3. Run, from the repository root:

```
cargo run --release
```

The first build takes a while. On first start the game checks your files and builds
`assets/gfx`, `assets/data` and `assets/art.json` from them (well under a second). If it can't
find them it shows a setup screen: put the files where it says and press Enter, or drag your
Autoduel folder onto the window.

The import needs these files from the DOS game: `TABLE.DEC`, `CARS.TLB`, `CARSTOO.TLB`,
`TERRAIN.TLB`, `FRAME.TLB`, `INTRO.TLB`, `WIN.TLB`, `CASINO.TLB`, `CITY00.TLB`-`CITY0F.TLB`,
`CITYDAT`, `ROADMAP`, `ROADMOD`, `ARENSTUF`, `DRIVERS`, `AUTODUEL.OVL` and `INIT.OVL`. It can
also be run on its own: `cargo run -p cardul-import -- <Autoduel folder> assets`.

Options (after `--` when using `cargo run`):

```
--original DIR   your copy of Autoduel (default: the original folder)
--reimport       build the graphics and data again from it
--assets DIR     game data folder (default: found automatically)
--saves DIR      where saved drivers go (default: <assets>/../saves)
--windowed WxH   start in a window of this size (default 1280x800)
--fullscreen     start in borderless full screen
--art SET        graphics for this run: classic or enhanced
```

## Graphics: classic or enhanced

- **Classic**: the DOS game's own 16-colour EGA pictures, taken from your copy: the towns,
  cars, roadside objects, title, HUD, font and the newspaper ending. The DOS game has no road
  map picture, so Cardul draws one (F5).
- **Enhanced**: a modern 1280x800 set included in `assets/gfx_enhanced/`.

F10 switches between them at any time, and so does G on the main menu; the choice is kept in
`saves/settings.json`. The classic screen is scaled by the largest whole number that fits the
window; the enhanced one fills the window, smoothly scaled.

## Controls

```
Arrows/WASD/pad  walk, drive, menus
Ctrl, Z, pad A   fire
Space, pad B     next weapon
1..0             select weapon slot
G                get out of / into car
P                pause (driving)

F1  this help      F2  driver status
F3  car sheet      F4  your cars
F5  road map       F6  save game
F7  sound on/off   F8  cargo (where to, due)
F9  driving speed  F10 graphics (classic / enhanced)
F11 full screen    Esc back / cancel
```

Gamepad: A also confirms menus, Y gets out of the car, Start is Enter and Back/Select is Esc.
In town, doors are the short yellow bars on the building walls: walk into one to go in.

Sound is synthesised by the game (there are no sound files): cues for keys, shops, footsteps,
doors, weapons, hits and crashes, a title tune, and an engine hum that follows your speed.

## Your first hour

1. **Start.** You begin in New York on Friday 1-1-2030 with $2,000 and no car. Fridays in New
   York are Amateur Night: sign up at the Arena and fight five weak cars in the loaner car.
2. **Get a car.** Assembly Line (Boston, New York, Pittsburgh): design one you can afford.
3. **Earn.** AADA offices hand out courier jobs. Leave town by driving into a road exit,
   survive the highway, and deliver to the destination building. F8 shows what you carry.
4. **Grow.** Road trips, arena wins and deliveries raise prestige and skills. Repair at the
   Garage, reload at the Weapon Shop, buy a clone at Gold Cross before it gets dangerous.
5. **The story.** At prestige 20, 40, 60, 80 and 95 the Truck Stops and Joe's Bars start to
   talk. Follow the clues to the FBI in New York and Mr. Big.

## Repairs to the original

Original bugs are fixed and marked `FIX:` in the code: a working calendar, the brain-tape
soft-lock, the arena AI, hits landing on the side they come from, one horizontal scale for all
objects, correct salvage prices, the AADA pay cap, the circular quest clues, and the Assembly
Line telling you why a design fails.

## Saves

Drivers are saved as `saves/<name>.json` next to the `assets` folder, or in the folder given
with `--saves`. The game saves on every city entry and building visit, on F6, and when you
close the window.

## Code layout

| crate | folder | what |
|---|---|---|
| `cardul-sim` | `sim/` | game rules: driver, cars, economy, story, road/arena combat, save files |
| `cardul-import` | `import/` | reads your copy of Autoduel (DOS) and builds the classic graphics and world data |
| `cardul-ui` | `ui/` | screens, the software renderer (320x200, or 1280x800 with the enhanced art), the setup screen |
| `cardul` | `game/` | the Bevy shell: window, keyboard/gamepad, display scaling, sound |

`ui/examples/play.rs` plays a script against the game without a window and saves screenshots
(`cargo run -p cardul-ui --example play -- script.txt`); the commands are listed at the top of
the file.

## Tests

`cargo test --workspace`. Tests that need the game's data run only after an import (start the
game once); without it they pass without checking anything. The import's own tests look for
your copy in `original/` or in the folder named by `CARDUL_ORIGINAL`.

## Legal

Cardul is a fan project, not affiliated with or endorsed by Origin Systems, Electronic Arts or
Steve Jackson Games. *Autoduel* and *Car Wars* are trademarks of Steve Jackson Games. The code
is under the MIT License (see `LICENSE`). The enhanced pictures' text is set in Saira
(SIL Open Font License 1.1, `assets/gfx_enhanced/OFL.txt`). The setup screen's font is
font8x8 by Daniel Hepper (public domain).
