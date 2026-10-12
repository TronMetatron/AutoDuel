//! Building screens. Each building module exposes `pub fn screen(cx: &mut Cx) -> Box<dyn Screen>`.
//!
//! Conventions (shared by all buildings):
//! * build the screen with [`enter`] (title + automatic cargo deliveries, like the original's
//!   `SR_cargo` on entry), print with [`Term`], ask with [`Asker`];
//! * leave with [`leave`] (back to the city, walker placed outside this building's door);
//! * call [`crate::screens::hotkeys`] first in `update` so F1-F6 work everywhere;
//! * a big transaction sets `cx.g().driver.day_used = true` (closes several buildings for the day);
//! * if the driver dies in a building, return [`crate::screens::city::after_death_in_town`].
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::screens::city::{Arrival, CityScreen};
use crate::term::{Answer, Ask, Asker, Term};
use cardul_sim::game::DeliveryReport;
use cardul_sim::world::Building;

pub mod goldcross;
pub mod truckstop;
pub mod assembly;
pub mod salvage;
pub mod weapons;
pub mod garage;
pub mod arena;
pub mod aada;
pub mod bar;
pub mod fbi;
pub mod outlaw;
pub mod abandoned;
pub mod origin;
pub mod petstore;
pub mod casino;
pub mod hotel;
/// helpers for the vehicle shops (assembly, garage, weapons, salvage, goldcross)
mod shopkit;
/// helpers shared by the world buildings (message pages, menus)
pub mod talk;

/// Open a building's screen (called by the city when the walker reaches its door).
pub fn open(cx: &mut Cx, kind: Building) -> Box<dyn Screen> {
    match kind {
        Building::GoldCross => goldcross::screen(cx),
        Building::TruckStop => truckstop::screen(cx),
        Building::Assembly => assembly::screen(cx),
        Building::Salvage => salvage::screen(cx),
        Building::Weapons => weapons::screen(cx),
        Building::Garage => garage::screen(cx),
        Building::Arena => arena::screen(cx),
        Building::Aada => aada::screen(cx),
        Building::Bar => bar::screen(cx),
        Building::Fbi => fbi::screen(cx),
        Building::OutlawHq => outlaw::screen(cx),
        Building::Abandoned => abandoned::screen(cx),
        Building::Origin => origin::screen(cx),
        Building::PetStore => petstore::screen(cx),
        Building::Casino => casino::screen(cx),
        Building::Hotel => hotel::screen(cx),
    }
}

/// Standard entry: a text screen titled "<building>" with the city name, plus the automatic
/// deliveries for this building (their messages are already printed in the message area).
pub fn enter(cx: &mut Cx, kind: Building) -> (Term, DeliveryReport) {
    let city = cx.game().driver.city;
    let title = format!("{} - {}", kind.name(), cx.data.city_name(city));
    let mut term = Term::new(&title);
    let rep = {
        let (data, game) = cx.dg();
        game.deliver_cargo(kind, data)
    };
    for l in &rep.lines {
        term.msg_add(l);
    }
    if !rep.lines.is_empty() {
        cx.save();
    }
    (term, rep)
}

/// Leave the building: back to the city outside its door.
pub fn leave(cx: &mut Cx, kind: Building) -> Trans {
    cx.save();
    Trans::Replace(Box::new(CityScreen::arrive(cx, Arrival::Building(kind))))
}

/// A building that hasn't been written yet: deliveries work, then "Press any key".
pub struct Placeholder {
    kind: Building,
    term: Term,
    asker: Asker,
    won: bool,
}

impl Placeholder {
    pub fn new(cx: &mut Cx, kind: Building) -> Placeholder {
        let (mut term, rep) = enter(cx, kind);
        term.at(2, 4, "(This building is not open yet.)");
        term.msg_add("Press any key to leave.");
        let mut asker = Asker::default();
        asker.set(Ask::Key);
        Placeholder { kind, term, asker, won: rep.won }
    }
}

impl Screen for Placeholder {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if let Some(Answer::Key(_)) = self.asker.feed(inp) {
            if self.won {
                self.won = false;
                self.asker.set(Ask::Key);
                return Trans::Push(Box::new(crate::screens::ending::EndingScreen::new(cx)));
            }
            return leave(cx, self.kind);
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
    }
    fn resume(&mut self, _cx: &mut Cx) {
        self.asker.set(Ask::Key);
    }
}
