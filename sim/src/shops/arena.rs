//! The Arena building (`ar.prg`): practice, the schedule, sign-up and results.
//!
//! Schedule (`ar $A62E`/`$A781`): Sunday..Friday the event is `(city + weekday) mod 6`;
//! Saturday is the city Championship on the city's championship day, else Amateur Night.
//! Championships fall on (Jan 1 2030) + 7·(city mod 12) + 1 days, then every 84 days; that
//! first date is always a Saturday.
//! FIX: the original date arithmetic skipped November and December; this uses the normal
//! calendar ([`crate::date`]).
use crate::car::Car;
use crate::combat::ArenaResult;
use crate::date::Date;
use crate::game::Game;
use crate::tables::{ARENA_EVENT_NAMES, ARENA_PRIZES};

pub const AMATEUR_NIGHT: u8 = 0;
pub const UNLIMITED: u8 = 5;
pub const CHAMPIONSHIP: u8 = 6;
pub const PRACTICE: u8 = 7;
pub const PRACTICE_FEE: u32 = 20;
/// Amateur Night: below this prestige anyone without a car may enter...
pub const AMATEUR_PRESTIGE: u8 = 6;
/// ...and above it only drivers with less cash than this ("down and out").
pub const AMATEUR_CASH: u32 = 5_000;
/// Division N (events 1-4) admits cars worth at most N × $5,000.
pub const DIVISION_STEP: u32 = 5_000;
pub const CHAMPIONSHIP_EVERY: i64 = 84;
pub const SATURDAY: u8 = 6;

/// Event name as on the schedule ("Amateur Night", "Division 5", ...).
pub fn event_name(event: u8) -> &'static str {
    ARENA_EVENT_NAMES[event.min(PRACTICE) as usize]
}

/// The weekday event (Sunday..Friday).
pub fn weekday_event(city: u8, weekday: u8) -> u8 {
    (city + weekday) % 6
}

/// The next championship date of `city` on or after `today`.
pub fn championship_date(city: u8, today: Date) -> Date {
    let start = Date::start();
    let first = 7 * (city as i64 % 12) + 1;
    let n = today.ordinal() - start.ordinal();
    let k = if n <= first { 0 } else { (n - first + CHAMPIONSHIP_EVERY - 1) / CHAMPIONSHIP_EVERY };
    start.plus_days((first + k * CHAMPIONSHIP_EVERY) as u32)
}

/// Today's event in `city`.
pub fn todays_event(city: u8, today: Date) -> u8 {
    if today.weekday != SATURDAY {
        weekday_event(city, today.weekday)
    } else if championship_date(city, today) == today {
        CHAMPIONSHIP
    } else {
        AMATEUR_NIGHT
    }
}

/// Why the arena turns you away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// "The arena is closed for the night!"
    Closed,
    /// "I don't mean to annoy you, but you can't practice without a car."
    NoCarToPractice,
    /// "I'm sorry, but you don't have enough money."
    NoMoney,
    /// "You're no amateur! You can't compete in this event."
    NoAmateur,
    /// "Sorry, but you can't enter this event without your own car."
    NeedOwnCar,
    /// "I know ya don't wanta hear this, but your car doesn't qualify for today's event."
    NotQualified,
}

impl Refusal {
    pub fn message(self) -> &'static str {
        match self {
            Refusal::Closed => "The arena is closed for the night!",
            Refusal::NoCarToPractice => "I don't mean to annoy you, but you\ncan't practice without a car.",
            Refusal::NoMoney => "I'm sorry, but you don't have\nenough money.",
            Refusal::NoAmateur => "You're no amateur! You can't compete\nin this event.",
            Refusal::NeedOwnCar => "Sorry, but you can't enter this\nevent without your own car.",
            Refusal::NotQualified => {
                "I know ya don't wanta hear this,\nbut your car doesn't qualify for\ntoday's event."
            }
        }
    }
}

fn own_car_here(game: &Game) -> Option<&Car> {
    if game.driver.loaner { None } else { game.car_here() }
}

/// Practice ($20, your own car with you).
pub fn check_practice(game: &Game) -> Result<(), Refusal> {
    if game.driver.day_used {
        return Err(Refusal::Closed);
    }
    if own_car_here(game).is_none() {
        return Err(Refusal::NoCarToPractice);
    }
    if game.driver.money < PRACTICE_FEE {
        return Err(Refusal::NoMoney);
    }
    Ok(())
}

/// Pay for practice; the caller then runs arena event [`PRACTICE`].
pub fn start_practice(game: &mut Game) -> Result<(), Refusal> {
    check_practice(game)?;
    game.driver.pay(PRACTICE_FEE);
    Ok(())
}

/// How you get in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Ok,
    /// "You're no amateur, but you look down and out, so we'll let you compete anyway."
    DownAndOut,
}

/// Highest event class a car of this value is too expensive for (`ar $A9B9`).
pub fn car_class(value: u32) -> u8 {
    (value.saturating_sub(1) / DIVISION_STEP).min(6) as u8
}

/// Sign-up checks for `event` (0..6).
pub fn check_entry(game: &Game, event: u8) -> Result<Entry, Refusal> {
    if game.driver.day_used {
        return Err(Refusal::Closed);
    }
    if event == AMATEUR_NIGHT {
        // you must arrive on foot; the arena lends you its KILLER KART
        if game.car_here().is_some() {
            return Err(Refusal::NoAmateur);
        }
        if game.driver.prestige < AMATEUR_PRESTIGE {
            return Ok(Entry::Ok);
        }
        if game.driver.money >= AMATEUR_CASH {
            return Err(Refusal::NoAmateur);
        }
        return Ok(Entry::DownAndOut);
    }
    let Some(car) = own_car_here(game) else { return Err(Refusal::NeedOwnCar) };
    if event < UNLIMITED && car_class(car.value()) >= event {
        return Err(Refusal::NotQualified);
    }
    Ok(Entry::Ok)
}

/// Enter `event` after [`check_entry`]: Amateur Night puts you in the loaner.
pub fn start_event(game: &mut Game, event: u8) {
    if event == AMATEUR_NIGHT {
        game.loaner = Some(Car::killer_kart(game.driver.city));
        game.driver.loaner = true;
        game.driver.car_with_you = true;
    }
}

/// What the arena says and does after an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// "Very good! You oughta come back for the next championship." + "Your prize is $X!"
    Won { prize: u32 },
    /// "Too bad. Maybe ya need a little more experience."
    Lost,
    /// "That's too bad about your car. Come back for amateur night if ya need another one."
    CarDestroyed,
    /// "You'll be able to pick up your car at the garage."
    CarTowed,
}

impl Outcome {
    pub fn message(self) -> &'static str {
        match self {
            Outcome::Won { .. } => "Very good! You oughta come back\nfor the next championship.",
            Outcome::Lost => "Too bad. Maybe ya need a little\nmore experience.",
            Outcome::CarDestroyed => {
                "That's too bad about your car. Come\nback for amateur night if ya need\nanother one."
            }
            Outcome::CarTowed => "You'll be able to pick up your car\nat the garage.",
        }
    }
}

/// Give the loaner back after an event: you arrived on foot, so you leave on foot.
pub fn return_loaner(game: &mut Game) {
    if game.driver.loaner {
        game.driver.loaner = false;
        game.loaner = None;
        game.driver.car_with_you = false;
    }
}

/// Apply an event's result (`ar $A147`, docs/combat.md §7): win = prize, prestige +2
/// (Championship +3), driving +1; lost −1; car destroyed −2 (the car is gone); a car out of
/// power is towed to this city's garage. The arena is then closed for the day.
pub fn finish_event(game: &mut Game, event: u8, result: ArenaResult) -> Outcome {
    let out = match result {
        ArenaResult::Won => {
            let prize = ARENA_PRIZES[event.min(CHAMPIONSHIP) as usize];
            game.driver.add_money(prize as i64);
            game.driver.add_prestige(if event == CHAMPIONSHIP { 3 } else { 2 });
            crate::driver::Driver::raise(&mut game.driver.driving, 1);
            Outcome::Won { prize }
        }
        ArenaResult::Fled => {
            game.driver.add_prestige(-1);
            Outcome::Lost
        }
        ArenaResult::WonOnFoot => {
            game.driver.add_prestige(-2);
            game.lose_current_car();
            Outcome::CarDestroyed
        }
        ArenaResult::CarTowed | ArenaResult::Practice { on_foot: true, .. } => {
            if !game.driver.loaner {
                game.store_current_car();
            }
            Outcome::CarTowed
        }
        ArenaResult::Practice { .. } => Outcome::Lost,
    };
    return_loaner(game);
    game.driver.day_used = true;
    out
}

/// After practice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PracticeOutcome {
    /// "Great run! You should try one of our competitive events."
    GreatRun,
    /// "Looks like ya needed the practice!"
    NeededIt,
    /// on foot: the car is towed to the garage
    CarTowed,
}

impl PracticeOutcome {
    pub fn message(self) -> &'static str {
        match self {
            PracticeOutcome::GreatRun => "Great run! You should try one of\nour competitive events.",
            PracticeOutcome::NeededIt => "Looks like ya needed the practice!",
            PracticeOutcome::CarTowed => Outcome::CarTowed.message(),
        }
    }
}

/// Apply a practice result. Practice changes no stats and leaves the arena open.
pub fn finish_practice(game: &mut Game, result: ArenaResult) -> PracticeOutcome {
    match result {
        ArenaResult::Practice { on_foot: true, .. } | ArenaResult::CarTowed => {
            game.store_current_car();
            PracticeOutcome::CarTowed
        }
        ArenaResult::Practice { hit: true, .. } => PracticeOutcome::NeededIt,
        _ => PracticeOutcome::GreatRun,
    }
}

/// The Harrisburg arena asks for "SAN ANTONIO ROSE" (`ar $A316`).
pub fn decoy_password_due(game: &Game) -> bool {
    let d = &game.driver;
    d.city == 10 && d.prestige >= 20 && !d.quest.taken[0] && d.quest.free()
}

/// "We need you to take a decoy prize to the Manchester Arena," + [`super::rumors::Handover`].
pub const DECOY_GIVE: &str = "We need you to take a decoy prize to\nthe Manchester Arena,";

#[cfg(test)]
mod tests {
    use super::*;

    fn game() -> Option<Game> {
        let d = crate::data::test_data()?;
        Some(Game::new("T", false, &d, 1))
    }

    #[test]
    fn championship_dates() {
        let start = Date::start();
        // Watertown (0): day 7*0+1 = Sat Jan 2
        let w = championship_date(0, start);
        assert_eq!((w.month, w.day, w.weekday), (1, 2, SATURDAY));
        // New York (7): 50 days after Jan 1 = Feb 20
        let ny = championship_date(7, start);
        assert_eq!((ny.month, ny.day, ny.weekday), (2, 20, SATURDAY));
        // on the day itself it is today; the day after, 84 days later
        assert_eq!(championship_date(7, ny), ny);
        let next = championship_date(7, ny.next_day());
        assert_eq!(next.ordinal() - ny.ordinal(), 84);
        assert_eq!(next.weekday, SATURDAY);
        // cities 12..15 share with 0..3
        assert_eq!(championship_date(12, start), championship_date(0, start));
        assert_eq!(championship_date(15, start), championship_date(3, start));
        // a normal calendar through November and December, always on a Saturday
        let mut d = start;
        for _ in 0..800 {
            let c = championship_date(10, d);
            assert!(c >= d && c.weekday == SATURDAY);
            assert!(c.ordinal() - d.ordinal() < 84);
            d = d.next_day();
        }
    }

    #[test]
    fn schedule() {
        let start = Date::start(); // Friday
        assert_eq!(todays_event(7, start), (7 + 5) % 6);
        let ny = championship_date(7, start);
        assert_eq!(todays_event(7, ny), CHAMPIONSHIP);
        assert_eq!(todays_event(7, ny.plus_days(7)), AMATEUR_NIGHT);
        assert_eq!(weekday_event(0, 0), 0);
        assert_eq!(weekday_event(4, 3), 1);
        assert_eq!(event_name(CHAMPIONSHIP), "Championship");
    }

    #[test]
    fn entry_rules() {
        let Some(mut g) = game() else { return };
        // Amateur Night: on foot, low prestige
        assert_eq!(check_entry(&g, 0), Ok(Entry::Ok));
        g.driver.prestige = 6;
        assert_eq!(check_entry(&g, 0), Ok(Entry::DownAndOut));
        g.driver.money = 5000;
        assert_eq!(check_entry(&g, 0), Err(Refusal::NoAmateur));
        // divisions need your own car with you, worth at most N x $1,000
        assert_eq!(check_entry(&g, 1), Err(Refusal::NeedOwnCar));
        g.add_car(Car::default_design("C", 7)); // $940
        assert_eq!(check_entry(&g, 0), Err(Refusal::NoAmateur));
        g.driver.prestige = 0;
        assert_eq!(check_entry(&g, 0), Err(Refusal::NoAmateur));
        for e in 1..=6 {
            assert_eq!(check_entry(&g, e), Ok(Entry::Ok));
        }
        assert_eq!(car_class(5000), 0);
        assert_eq!(car_class(5001), 1);
        assert_eq!(car_class(20_000), 3);
        assert_eq!(car_class(20_001), 4);
        assert_eq!(car_class(1_000_000), 6);
        let mut k = Car::killer_kart(7);
        k.set_plant(2);
        k.set_tires(3);
        k.weapons[1] = Some(crate::car::Weapon::new(crate::tables::w::LA, 0));
        let v = k.value();
        assert!(v > 10_000 && v <= 15_000, "{}", v);
        g.cars[0] = Some(k);
        assert_eq!(check_entry(&g, 1), Err(Refusal::NotQualified));
        assert_eq!(check_entry(&g, 2), Err(Refusal::NotQualified));
        assert_eq!(check_entry(&g, 3), Ok(Entry::Ok));
        g.driver.day_used = true;
        assert_eq!(check_entry(&g, 3), Err(Refusal::Closed));
        assert_eq!(check_practice(&g), Err(Refusal::Closed));
    }

    #[test]
    fn amateur_night_win_returns_the_loaner() {
        let Some(mut g) = game() else { return };
        check_entry(&g, 0).unwrap();
        start_event(&mut g, 0);
        assert_eq!(g.car().unwrap().name, "KILLER KART");
        let out = finish_event(&mut g, 0, ArenaResult::Won);
        assert_eq!(out, Outcome::Won { prize: 1500 });
        assert_eq!((g.driver.money, g.driver.prestige, g.driver.driving), (3500, 2, 1));
        assert!(g.loaner.is_none() && !g.driver.loaner && !g.driver.car_with_you);
        assert!(g.driver.day_used);
    }

    #[test]
    fn results() {
        let Some(mut g) = game() else { return };
        g.add_car(Car::default_design("C", 7));
        g.driver.prestige = 10;
        assert_eq!(finish_event(&mut g, 6, ArenaResult::Won), Outcome::Won { prize: 9500 });
        assert_eq!(g.driver.prestige, 13);
        assert_eq!(finish_event(&mut g, 1, ArenaResult::Fled), Outcome::Lost);
        assert_eq!(g.driver.prestige, 12);
        assert_eq!(finish_event(&mut g, 1, ArenaResult::CarTowed), Outcome::CarTowed);
        assert!(!g.driver.car_with_you && g.car().is_some());
        g.driver.car_with_you = true;
        assert_eq!(finish_event(&mut g, 1, ArenaResult::WonOnFoot), Outcome::CarDestroyed);
        assert_eq!(g.driver.prestige, 10);
        assert!(g.car().is_none());
        // practice
        g.add_car(Car::default_design("D", 7));
        g.driver.day_used = false;
        g.driver.money = 20;
        assert_eq!(start_practice(&mut g), Ok(()));
        assert_eq!(g.driver.money, 0);
        assert_eq!(start_practice(&mut g), Err(Refusal::NoMoney));
        let p = finish_practice(&mut g, ArenaResult::Practice { hit: true, on_foot: false });
        assert_eq!(p, PracticeOutcome::NeededIt);
        assert!(!g.driver.day_used);
        let p = finish_practice(&mut g, ArenaResult::Practice { hit: false, on_foot: true });
        assert_eq!(p, PracticeOutcome::CarTowed);
        assert!(!g.driver.car_with_you);
    }

    #[test]
    fn decoy_password() {
        let Some(mut g) = game() else { return };
        g.driver.city = 10;
        assert!(!decoy_password_due(&g));
        g.driver.prestige = 20;
        assert!(decoy_password_due(&g));
        g.driver.quest.taken[0] = true;
        assert!(!decoy_password_due(&g));
    }
}
