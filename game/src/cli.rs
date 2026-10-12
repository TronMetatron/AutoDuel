//! Command line options and finding the assets folder.
use std::path::{Path, PathBuf};

pub const USAGE: &str = "\
Cardul - a remake of Origin's Autoduel

Usage: cardul [options]

  --original DIR   your copy of Autoduel for DOS, to build the classic
                   graphics and the world data from on first start
                   (default: the original folder next to the assets)
  --reimport       build them again from your copy of the game
  --assets DIR     game data folder (default: found automatically)
  --saves DIR      where saved drivers go (default: <assets>/../saves)
  --windowed WxH   start in a window of this size (default 1280x800)
  --fullscreen     start in borderless full screen (F11 toggles)
  --art SET        graphics for this run: classic (from your copy of the
                   game) or enhanced (default: the last choice, F10 switches)
  -h, --help       show this help";

pub struct Options {
    pub original: Option<PathBuf>,
    pub reimport: bool,
    pub assets: Option<PathBuf>,
    pub saves: Option<PathBuf>,
    /// initial window size (logical pixels)
    pub size: (u32, u32),
    pub fullscreen: bool,
    /// graphics for this run (None: the saved choice)
    pub art: Option<cardul_ui::assets::ArtSet>,
}

pub enum Parsed {
    Run(Options),
    Help,
}

/// Parse the arguments (without the program name). Accepts `--opt value` and `--opt=value`.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Parsed, String> {
    let mut o = Options { original: None, reimport: false, assets: None, saves: None, size: (1280, 800), fullscreen: false, art: None };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
            _ => (arg.clone(), None),
        };
        let mut value = || inline.clone().or_else(|| args.next()).ok_or(format!("{name} needs a value"));
        match name.as_str() {
            "-h" | "--help" => return Ok(Parsed::Help),
            "--original" => o.original = Some(PathBuf::from(value()?)),
            "--reimport" => o.reimport = true,
            "--assets" => o.assets = Some(PathBuf::from(value()?)),
            "--saves" => o.saves = Some(PathBuf::from(value()?)),
            "--windowed" => {
                let v = value()?;
                o.size = parse_size(&v).ok_or(format!("--windowed wants a size like 1280x800, not '{v}'"))?;
                o.fullscreen = false;
            }
            "--fullscreen" => o.fullscreen = true,
            "--art" => {
                let v = value()?;
                o.art = Some(cardul_ui::assets::ArtSet::parse(&v).ok_or(format!("--art wants classic or enhanced, not '{v}'"))?);
            }
            _ => return Err(format!("unknown option '{arg}'")),
        }
    }
    Ok(Parsed::Run(o))
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    let (w, h) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// An assets folder: the enhanced graphics, or an earlier import of the original game.
fn is_assets_dir(p: &Path) -> bool {
    p.join("data").join("cities.json").exists() || p.join("gfx_enhanced").join("art.json").exists()
}

/// The assets folder to use: `--assets DIR` if given (checked), else the automatic search.
pub fn resolve_assets(given: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(dir) = given {
        return if is_assets_dir(&dir) {
            Ok(dir)
        } else {
            Err(format!(
                "{} is not a Cardul assets folder (it has neither gfx_enhanced nor data).\n\n{}",
                dir.display(),
                BUILD_HINT
            ))
        };
    }
    searched().into_iter().find(|p| is_assets_dir(p)).ok_or_else(|| {
        let mut m = String::from("Cardul could not find its assets folder (one containing gfx_enhanced).\nLooked in:\n");
        for p in searched() {
            m += &format!("  {}\n", p.display());
        }
        m + "\n" + BUILD_HINT
    })
}

const BUILD_HINT: &str = "\
Run Cardul from the repository root (or put the assets folder next to the program),
or pass --assets DIR.";

/// Where the assets folder is looked for: next to the program, in the working directory, and
/// up to three folders above either, then the source tree's.
fn searched() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];
    let mut walk = |start: Option<PathBuf>| {
        let mut d = start;
        for _ in 0..4 {
            let Some(dd) = d else { break };
            let p = dd.join("assets");
            if !out.contains(&p) {
                out.push(p);
            }
            d = dd.parent().map(Path::to_path_buf);
        }
    };
    walk(std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)));
    walk(std::env::current_dir().ok());
    // the source tree's assets folder (sim/ and game/ are siblings)
    out.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("assets"));
    out
}

/// Default saves folder: `saves/` next to the assets folder.
pub fn default_saves(assets: &Path) -> PathBuf {
    let abs = std::path::absolute(assets).unwrap_or_else(|_| assets.to_path_buf());
    abs.parent().unwrap_or(&abs).join("saves")
}

/// Where to look for the player's copy of Autoduel: `--original DIR` if given, then the
/// `original` folder next to the assets folder, next to the program and in the working
/// directory. The first is where the setup screen asks the player to put it.
pub fn original_places(given: Option<PathBuf>, assets: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = given.into_iter().collect();
    let abs = std::path::absolute(assets).unwrap_or_else(|_| assets.to_path_buf());
    v.push(abs.parent().unwrap_or(&abs).join("original"));
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        v.push(d.join("original"));
    }
    if let Ok(d) = std::env::current_dir() {
        v.push(d.join("original"));
    }
    let mut out: Vec<PathBuf> = vec![];
    for p in v {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Options, String> {
        match parse(args.iter().map(|s| s.to_string()))? {
            Parsed::Run(o) => Ok(o),
            Parsed::Help => Err("help".into()),
        }
    }

    #[test]
    fn options() {
        let o = run(&["--assets", "a", "--saves=s", "--windowed", "640x400", "--original", "ad"]).unwrap();
        assert_eq!(o.original, Some(PathBuf::from("ad")));
        assert!(!o.reimport);
        assert!(run(&["--reimport"]).unwrap().reimport);
        assert_eq!(o.assets, Some(PathBuf::from("a")));
        assert_eq!(o.saves, Some(PathBuf::from("s")));
        assert_eq!(o.size, (640, 400));
        assert!(!o.fullscreen);
        assert!(run(&["--fullscreen"]).unwrap().fullscreen);
        assert_eq!(run(&["--art", "classic"]).unwrap().art, Some(cardul_ui::assets::ArtSet::Classic));
        assert_eq!(run(&["--art=new"]).unwrap().art, Some(cardul_ui::assets::ArtSet::Enhanced));
        assert!(run(&["--art", "pastel"]).is_err());
        assert!(run(&["--windowed", "0x5"]).is_err());
        assert!(run(&["--saves"]).is_err());
        assert!(run(&["--bogus"]).is_err());
        assert!(matches!(parse(["-h".to_string()]), Ok(Parsed::Help)));
    }
}
