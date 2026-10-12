//! Truck Stop (`tr.prg`): bus, rumors, batteries, rooms, body armor — plus the Watertown
//! password for the mob evidence and the brain-tape ambush.
use super::talk::{self, Talk, LONG, MEDIUM, SHORT};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::fb::Frame;
use crate::input::Input;
use crate::screens::city::{after_death_in_town, Arrival, CityScreen};
use crate::term::{Answer, Ask, Asker, Term};
use cardul_sim::shops::rumors::{self, TOLD_ALREADY};
use cardul_sim::shops::truckstop::{self as ts, Ambush, Armor, Battery, CarOnBus, Room};
use cardul_sim::story::password_matches;
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(TruckStop::new(cx))
}

const GREETING: &str = "Hi! Whatcha need?";
const ANYTHING_ELSE: &str = "Anything else?";
const GOOD_MORNING: &str = "Good morning! What would you like?";
const NIGHTS: &str = "How many nights? (Esc for none)";
const MORE_MONEY: &str = "You'll need some more money before\nyou can do that.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Menu(&'static str),
    Password,
    Bus,
    Nights,
    /// the bus has arrived (the driver is already in the new city)
    Arrive,
    Leave,
    Death,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Menu,
    Password,
    Bus,
    /// "leave your car on the streets?" for this destination
    Street(u8),
    Nights,
}

struct TruckStop {
    term: Term,
    asker: Asker,
    talk: Talk,
    then: Option<Step>,
    mode: Mode,
    /// rumor heard this visit (reset by a night in a room)
    rumor_told: bool,
    bus_to: Vec<u8>,
}

impl TruckStop {
    fn new(cx: &mut Cx) -> TruckStop {
        // the ambush comes before anything else, deliveries included
        let ambush = ts::tape_ambush(cx.g());
        let died = matches!(ambush, Some(Ambush::Hit { died: true }));
        let (term, rep) = if died {
            let city = cx.game().driver.city;
            (Term::new(&format!("Truck Stop - {}", cx.data.city_name(city))), Default::default())
        } else {
            enter(cx, Building::TruckStop)
        };
        cx.g().free_bus_ticket = false;
        let mut s = TruckStop {
            term,
            asker: Asker::default(),
            talk: Talk::default(),
            then: None,
            mode: Mode::Menu,
            rumor_told: false,
            bus_to: vec![],
        };
        if ambush.is_none() {
            s.draw_menu();
        }
        if let Some(a) = ambush {
            s.talk.say("Before you enter, someone recognizes\nyou and takes a shot at you.", LONG);
            match a {
                Ambush::Missed => s.talk.say("Fortunately, the shot misses.", LONG),
                Ambush::Hit { died } => {
                    cx.sfx(Sfx::PlayerHit);
                    s.talk.say("Unfortunately, you are hit.", LONG);
                    if died {
                        s.talk.say(
                            "Falling to the ground, you realize\nthat you finally got in over your\nhead, and that you are going to die.",
                            LONG,
                        );
                        s.then = Some(Step::Death);
                        return s;
                    }
                }
            }
            cx.save();
        }
        talk::deliveries(cx, &mut s.talk, &rep);
        s.then = Some(if rumors::mob_password_due(cx.game()) { Step::Password } else { Step::Menu(GREETING) });
        s
    }

    fn draw_menu(&mut self) {
        self.term.clear_main();
        talk::menu(
            &mut self.term,
            4,
            3,
            &[
                "Catch a bus $100",
                "Listen for rumors",
                "Get batteries charged $50",
                "Get a room $10",
                "Buy body armor $250",
                "Leave the truck stop",
            ],
        );
    }

    fn draw_bus(&mut self, cx: &Cx) {
        let here = cx.game().driver.city;
        self.bus_to = ts::bus_destinations(&cx.data, here);
        self.term.clear_main();
        talk::center(&mut self.term, 0, "Bus Schedule");
        self.term.at(6, 3, "From");
        self.term.at(22, 3, "To");
        for (i, d) in self.bus_to.iter().enumerate() {
            let r = 5 + 2 * i;
            self.term.at(2, r, &format!("{} - {}", i + 1, cx.data.city_name(here)));
            self.term.at(22, r, cx.data.city_name(*d));
        }
        talk::center(&mut self.term, 15, "All routes are driven daily");
        talk::center(&mut self.term, 16, "Press Esc for none");
    }

    fn run(&mut self, cx: &mut Cx, step: Step) -> Trans {
        match step {
            Step::Menu(msg) => {
                self.draw_menu();
                self.term.prompt(msg);
                self.mode = Mode::Menu;
                self.asker.set(Ask::Choice(talk::digits(6)));
            }
            Step::Password => {
                self.term.msg(talk::PASSWORD);
                self.mode = Mode::Password;
                self.asker.set(Ask::Text(talk::PASSWORD_LEN));
            }
            Step::Bus => {
                self.draw_bus(cx);
                self.term.prompt("Where d'ya want to go?");
                self.mode = Mode::Bus;
                self.asker.set(Ask::Choice(talk::digits(self.bus_to.len())));
            }
            Step::Nights => {
                self.term.prompt(NIGHTS);
                self.mode = Mode::Nights;
                self.asker.set(Ask::Choice(talk::digits(ts::MAX_NIGHTS as usize)));
            }
            Step::Arrive => return Trans::Replace(Box::new(CityScreen::arrive(cx, Arrival::TruckStop))),
            Step::Leave => return leave(cx, Building::TruckStop),
            Step::Death => return after_death_in_town(cx),
        }
        Trans::None
    }

    fn password(&mut self, cx: &mut Cx, typed: &str) {
        if !password_matches(typed, 2) {
            self.then = Some(Step::Menu(GREETING));
            return;
        }
        let h = rumors::hand_over(cx.g(), 2);
        cx.save();
        self.talk.say(format!("The mob's done if you take this to\nthe AADA in Philly, {}", h.text()), LONG);
        self.then = Some(Step::Menu(GREETING));
    }

    /// Destination chosen: check the fare, then decide what happens to the car.
    fn bus_to(&mut self, cx: &mut Cx, dest: u8) {
        if !ts::can_pay_bus(cx.game()) {
            cx.sfx(Sfx::Beep);
            self.talk.say(MORE_MONEY, MEDIUM);
            self.then = Some(Step::Menu(GREETING));
            return;
        }
        match ts::car_on_bus(cx.game(), &cx.data) {
            CarOnBus::Street => {
                self.term.msg("Do ya really want to leave your car\non the streets?");
                self.mode = Mode::Street(dest);
                self.asker.set(Ask::YesNo);
            }
            CarOnBus::Garage => {
                self.talk.say("We'll put your car in storage at the\nGARAGE.", MEDIUM);
                self.ride(cx, dest);
            }
            CarOnBus::NoCar => self.ride(cx, dest),
        }
    }

    fn ride(&mut self, cx: &mut Cx, dest: u8) {
        let ok = {
            let (data, game) = cx.dg();
            ts::take_bus(game, data, dest)
        };
        if !ok {
            self.talk.say(MORE_MONEY, MEDIUM);
            self.then = Some(Step::Menu(GREETING));
            return;
        }
        let adj = ts::trip_adjective(cx.g());
        cx.sfx(Sfx::Horn);
        cx.save();
        self.talk.say("Step right onto the bus, please.", MEDIUM);
        self.talk.say(format!(".....(after a{} trip).....", adj), MEDIUM);
        self.talk.say(format!("You arrive in {}.", cx.data.city_name(dest)), MEDIUM);
        self.then = Some(Step::Arrive);
    }

    fn room(&mut self, cx: &mut Cx, nights: u32) {
        match ts::rent_room(cx.g(), nights) {
            Room::Slept { .. } => {
                cx.sfx(Sfx::Cash);
                self.rumor_told = false;
                self.talk.say("Have a good sleep!", MEDIUM);
                self.talk.say("...(after a needed break)...", MEDIUM);
                self.then = Some(Step::Menu(GOOD_MORNING));
            }
            Room::NeedMoreMoney => {
                cx.sfx(Sfx::Beep);
                self.talk.say(MORE_MONEY, MEDIUM);
                self.then = Some(Step::Nights);
            }
            Room::FreeBusTicket => {
                self.talk.say(
                    "You don't have enough money, but I'll\ngive you a bus ticket, so you can\nget out of town.",
                    LONG,
                );
                self.then = Some(Step::Bus);
            }
            Room::Lobby { lost_health } => {
                self.talk.say("You don't have enough money, but I'll\nlet you stay in the lobby overnight.", LONG);
                if lost_health {
                    self.talk.say("The sleep you lost cost you your\nhealth.", LONG);
                }
                self.then = Some(Step::Menu(GOOD_MORNING));
            }
        }
        cx.save();
    }

    fn menu_choice(&mut self, cx: &mut Cx, c: char) {
        match c {
            '1' => self.then = Some(Step::Bus),
            '2' => {
                let r = if self.rumor_told { TOLD_ALREADY } else { rumors::truck_stop(cx.g()) };
                self.rumor_told = true;
                self.talk.say(r, LONG);
                self.then = Some(Step::Menu(ANYTHING_ELSE));
            }
            '3' => {
                let r = ts::charge_battery(cx.g());
                let m = match r {
                    Battery::NoCar => "You don't even have a car with you!\nHow can I charge the batteries?",
                    Battery::Full => "They don't need charging!",
                    Battery::NoMoney => MORE_MONEY,
                    Battery::Charged => "There you go. All charged up!",
                };
                cx.sfx(if r == Battery::Charged { Sfx::Cash } else { Sfx::Beep });
                self.talk.say(m, MEDIUM);
                self.then = Some(Step::Menu(ANYTHING_ELSE));
            }
            '4' => self.then = Some(Step::Nights),
            '5' => {
                let r = ts::buy_armor(cx.g());
                let m = match r {
                    Armor::AlreadyHave => "The set you have has nothing wrong\nwith it!",
                    Armor::NoMoney => MORE_MONEY,
                    Armor::Bought => "You should always wear body armor!",
                };
                cx.sfx(if r == Armor::Bought { Sfx::Cash } else { Sfx::Beep });
                self.talk.say(m, MEDIUM);
                self.then = Some(Step::Menu(ANYTHING_ELSE));
            }
            _ => {
                self.talk.say("See ya later!", SHORT);
                self.then = Some(Step::Leave);
            }
        }
        cx.save();
    }
}

impl Screen for TruckStop {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(t) = crate::screens::hotkeys(cx, inp, true) {
            return t;
        }
        if self.talk.update(&mut self.term, inp) {
            return Trans::None;
        }
        if let Some(step) = self.then.take() {
            let t = self.run(cx, step);
            // the key that skipped the last message also answers the new prompt
            if !matches!(t, Trans::None) || self.then.is_some() || !self.talk.type_ahead(inp, &self.asker) {
                return t;
            }
        }
        let Some(ans) = self.asker.feed(inp) else { return Trans::None };
        match (self.mode, ans) {
            (Mode::Menu, Answer::Choice(c)) => self.menu_choice(cx, c),
            (Mode::Menu, Answer::Cancel) => self.menu_choice(cx, '6'),
            (Mode::Password, Answer::Text(t)) => self.password(cx, &t),
            (Mode::Password, _) => self.then = Some(Step::Menu(GREETING)),
            (Mode::Bus, Answer::Choice(c)) => {
                let dest = self.bus_to[(c as u8 - b'1') as usize];
                self.bus_to(cx, dest);
            }
            (Mode::Bus, _) => {
                self.talk.say("Better make up your mind. The bus\nis starting to leave.", MEDIUM);
                self.then = Some(Step::Menu(ANYTHING_ELSE));
            }
            (Mode::Street(dest), Answer::Yes) => self.ride(cx, dest),
            (Mode::Street(_), _) => {
                self.talk.say("That's a smart decision, I think.", MEDIUM);
                self.then = Some(Step::Menu(GREETING));
            }
            (Mode::Nights, Answer::Choice(c)) => self.room(cx, (c as u8 - b'0') as u32),
            (Mode::Nights, _) => self.then = Some(Step::Menu(ANYTHING_ELSE)),
            _ => {}
        }
        Trans::None
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
    }
}
