//! `cardul-import <Autoduel folder> [assets folder]`: build Cardul's classic graphics and world
//! data from a copy of Autoduel for DOS (the game does this itself on first start).
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("usage: cardul-import <Autoduel (DOS) folder> [assets folder, default ./assets]");
        std::process::exit(2);
    }
    let src = std::path::PathBuf::from(&args[0]);
    let dst = std::path::PathBuf::from(args.get(1).map(String::as_str).unwrap_or("assets"));
    let o = match cardul_import::Original::open(&src) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    match cardul_import::import(&o, &dst) {
        Ok(s) => println!("Imported {} pictures and the world data from {} into {}.", s.pictures, s.source.display(), dst.display()),
        Err(e) => {
            eprintln!("import failed: {e}");
            std::process::exit(1);
        }
    }
}
