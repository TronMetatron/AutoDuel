//! Title, main menu, driver roster and driver creation (the original SETUP module).
use crate::app::{Cx, Screen, Trans};
use crate::fb::{col, Frame};
use crate::input::{Input, Key};
use crate::screens::city::{Arrival, CityScreen};
use crate::term::{Answer, Ask, Asker, Term};
use crate::text;
use cardul_sim::game::list_saves;
use cardul_sim::Game;

pub struct TitleScreen {
    t: u32,
}

impl Default for TitleScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl TitleScreen {
    pub fn new() -> TitleScreen {
        TitleScreen { t: 0 }
    }
}

impl Screen for TitleScreen {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        self.t += 1;
        if self.t == 1 {
            cx.sfx(cardul_sim::Sfx::Title);
        }
        if inp.any_key() && self.t > 10 {
            return Trans::Replace(Box::new(MainMenu::new()));
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        fb.clear(col::BLACK);
        // the intro reveals the title over the background (like the original)
        fb.blit(&cx.assets.title_bg, 0, 0);
        let reveal = (self.t as i32 * 4).min(200);
        fb.set_clip(0, 0, 320, reveal);
        fb.blit(&cx.assets.title, 0, 0);
        fb.reset_clip();
        if self.t > 60 && (self.t / 30).is_multiple_of(2) {
            text::draw_bg(fb, &cx.assets, 88, 190, " Press any key ", col::BLACK, col::WHITE);
        }
    }
}

pub struct MainMenu {
    term: Term,
    asker: Asker,
    /// "Quit Cardul? (Y/N)" is up (after Esc)
    quitting: bool,
    /// the art set named on the G line
    art_shown: Option<crate::assets::ArtSet>,
}

const MENU_KEYS: [char; 6] = ['1', '2', '3', '4', 'g', 'q'];
const START_PROMPT: &str = "How would you like to start the game?";

impl Default for MainMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl MainMenu {
    pub fn new() -> MainMenu {
        let mut term = Term::new("CARDUL");
        term.at(4, 2, "Welcome to the World of CARDUL");
        term.at(4, 3, "(a remake of Origin's Autoduel)");
        term.at(4, 6, "1 - Continue with current driver.");
        term.at(4, 7, "2 - Activate another driver.");
        term.at(4, 8, "3 - Create a new driver.");
        term.at(4, 9, "4 - Create a cheat driver.");
        term.at(4, 12, "Q - Quit.");
        term.at(4, 15, "F1 shows the keys at any time.");
        term.msg(START_PROMPT);
        let mut asker = Asker::default();
        asker.set(Ask::Choice(MENU_KEYS.to_vec()));
        MainMenu { term, asker, quitting: false, art_shown: None }
    }
}

/// Load a driver and put him in his city (the original "Continue" places you at the Truck Stop).
/// `file` is the save he was loaded from.
pub fn continue_game(cx: &mut Cx, g: Game, file: std::path::PathBuf) -> Result<Trans, String> {
    if !g.driver.alive {
        return Err("The driver is dead! You must create a new one!".into());
    }
    cx.start_game(g, Some(file));
    let s = CityScreen::arrive(cx, Arrival::TruckStop);
    Ok(Trans::Reset(Box::new(s)))
}

impl Screen for MainMenu {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if inp.pressed(Key::F(1)) {
            return Trans::Push(Box::new(crate::screens::info::InfoScreen::help()));
        }
        if self.art_shown != Some(cx.settings.art) {
            self.art_shown = Some(cx.settings.art);
            self.term.at(4, 11, &format!("G - Graphics: {:<14}", format!("{} (F10)", cx.settings.art.name())));
        }
        if self.quitting {
            match self.asker.feed(inp) {
                Some(Answer::Yes) => return Trans::Quit,
                Some(_) => {
                    self.quitting = false;
                    self.term.msg(START_PROMPT);
                    self.asker.set(Ask::Choice(MENU_KEYS.to_vec()));
                }
                None => {}
            }
            return Trans::None;
        }
        match self.asker.feed(inp) {
            Some(Answer::Choice('1')) => {
                let res = match cx.last_save() {
                    None => Err("There is no driver yet. Create one!".to_string()),
                    Some(p) => Game::load_from(&p).and_then(|g| continue_game(cx, g, p.clone())),
                };
                match res {
                    Ok(t) => return t,
                    Err(e) => self.term.msg(&e),
                }
            }
            Some(Answer::Choice('2')) => return Trans::Push(Box::new(RosterScreen::new(cx))),
            Some(Answer::Choice('3')) => return Trans::Push(Box::new(CreateScreen::new(false))),
            Some(Answer::Choice('4')) => return Trans::Push(Box::new(CreateScreen::new(true))),
            Some(Answer::Choice('q')) => return Trans::Quit,
            Some(Answer::Choice('g')) => cx.switch_art = true,
            // Esc (also the key that leaves the title picture) asks first (play-test)
            Some(Answer::Cancel) => {
                self.quitting = true;
                self.term.msg("Quit Cardul? (Y/N)");
                self.asker.set(Ask::YesNo);
                return Trans::None;
            }
            _ => {}
        }
        if !self.asker.waiting() {
            self.asker.set(Ask::Choice(MENU_KEYS.to_vec()));
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
    }
    fn resume(&mut self, _cx: &mut Cx) {
        if self.quitting {
            self.quitting = false;
            self.term.msg(START_PROMPT);
        }
        self.asker.set(Ask::Choice(MENU_KEYS.to_vec()));
    }
}

/// "Activate another driver": pick a saved driver.
pub struct RosterScreen {
    term: Term,
    asker: Asker,
    saves: Vec<(std::path::PathBuf, Game)>,
}

impl RosterScreen {
    pub fn new(cx: &Cx) -> RosterScreen {
        let saves: Vec<_> = list_saves(&cx.save_dir).into_iter().take(9).collect();
        let mut term = Term::new("Drivers");
        let mut keys = vec![];
        if saves.is_empty() {
            term.at(2, 2, "No saved drivers.");
        }
        for (i, (_, g)) in saves.iter().enumerate() {
            let d = &g.driver;
            let status = if d.alive { cx.data.city_name(d.city).to_string() } else { "dead".to_string() };
            term.at(1, 2 + 2 * i, &format!("{} {:<16} {}", i + 1, d.name, status));
            term.at(3, 3 + 2 * i, &format!("{}  prestige {}", text::money(d.money), d.prestige));
            keys.push(char::from_digit(i as u32 + 1, 10).unwrap());
        }
        term.msg("Press a number, or Esc to go back.");
        let mut asker = Asker::default();
        asker.set(Ask::Choice(keys));
        RosterScreen { term, asker, saves }
    }
}

impl Screen for RosterScreen {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        match self.asker.feed(inp) {
            Some(Answer::Choice(c)) => {
                let i = c.to_digit(10).unwrap() as usize - 1;
                let (p, g) = self.saves[i].clone();
                match continue_game(cx, g, p) {
                    Ok(t) => {
                        cx.save();
                        return t;
                    }
                    Err(e) => {
                        self.term.msg(&e);
                        let n = self.saves.len();
                        self.asker.set(Ask::Choice((1..=n as u32).map(|i| char::from_digit(i, 10).unwrap()).collect()));
                    }
                }
            }
            Some(Answer::Cancel) => return Trans::Pop,
            _ => {}
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, None);
    }
}

/// Longest driver name.
const NAME_LEN: usize = 16;

/// Driver creation: name, then 50 points over the three skills (`setup $836A`).
pub struct CreateScreen {
    cheat: bool,
    term: Term,
    asker: Asker,
    name: Option<String>,
    skills: [u8; 3],
    points: u8,
    cursor: usize,
    /// D pressed with points left: "Start anyway? (Y/N)" is up
    confirm: bool,
}

impl CreateScreen {
    pub fn new(cheat: bool) -> CreateScreen {
        let mut term = Term::new(if cheat { "Create a cheat driver" } else { "Create a new driver" });
        term.at(2, 1, &format!("(Up to {} characters.)", NAME_LEN));
        term.msg("What is the character's name? ");
        let mut asker = Asker::default();
        asker.set(Ask::Text(NAME_LEN));
        let base = if cheat { 20 } else { 0 };
        CreateScreen { cheat, term, asker, name: None, skills: [base; 3], points: 50, cursor: 0, confirm: false }
    }
    fn redraw(&mut self) {
        let names = ["Driving Skill", "Marksmanship", "Mechanic Ability"];
        self.term.clear_main();
        self.term.at(2, 1, &format!("Name: {}", self.name.clone().unwrap_or_default()));
        for (i, name) in names.iter().enumerate() {
            let line = format!("{:<20}{:>3}", name, self.skills[i]);
            if i == self.cursor {
                self.term.at_inv(4, 4 + 2 * i, &line);
            } else {
                self.term.at(4, 4 + 2 * i, &line);
            }
        }
        self.term.at(4, 11, &format!("Points left={}", self.points));
        self.term.msg("Create your driver using arrow keys. Up/down choose, left/right change. Press D when done.");
    }
    /// Make the driver and put him in New York.
    fn start(&self, cx: &mut Cx) -> Trans {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u32)
            .unwrap_or(1);
        let mut g = Game::new(self.name.as_deref().unwrap_or("Driver"), self.cheat, &cx.data, seed);
        g.driver.driving = self.skills[0];
        g.driver.marksman = self.skills[1];
        g.driver.mechanic = self.skills[2];
        cx.start_game(g, None);
        cx.save();
        let s = CityScreen::arrive(cx, Arrival::Start);
        Trans::Reset(Box::new(s))
    }
}

impl Screen for CreateScreen {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if self.name.is_none() {
            match self.asker.feed(inp) {
                Some(Answer::Text(s)) if !s.is_empty() => {
                    self.name = Some(s);
                    self.redraw();
                }
                Some(Answer::Text(_)) => self.asker.set(Ask::Text(NAME_LEN)),
                Some(Answer::Cancel) => return Trans::Pop,
                _ => {}
            }
            return Trans::None;
        }
        if self.confirm {
            match self.asker.feed(inp) {
                Some(Answer::Yes) => return self.start(cx),
                Some(_) => {
                    self.confirm = false;
                    self.redraw();
                }
                None => {}
            }
            return Trans::None;
        }
        for k in &inp.keys {
            match k {
                Key::Up => self.cursor = (self.cursor + 2) % 3,
                Key::Down | Key::Enter => self.cursor = (self.cursor + 1) % 3,
                Key::Right => {
                    if self.points > 0 && self.skills[self.cursor] < 99 {
                        self.points -= 1;
                        self.skills[self.cursor] += 1;
                    }
                }
                Key::Left => {
                    let base = if self.cheat { 20 } else { 0 };
                    if self.skills[self.cursor] > base {
                        self.points += 1;
                        self.skills[self.cursor] -= 1;
                    }
                }
                // points left over would be lost: ask first
                Key::Char('d') if self.points > 0 => {
                    let s = if self.points == 1 { "" } else { "s" };
                    self.term.msg(&format!("You still have {} point{} to spend. Start anyway? (Y/N)", self.points, s));
                    self.confirm = true;
                    self.asker.set(Ask::YesNo);
                    return Trans::None;
                }
                Key::Char('d') => return self.start(cx),
                Key::Esc => return Trans::Pop,
                _ => {}
            }
        }
        if !inp.keys.is_empty() {
            self.redraw();
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
    }
}
