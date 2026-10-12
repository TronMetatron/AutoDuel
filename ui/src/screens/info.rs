//! Information screens: help, driver status, car sheet, your cars, road map.
use crate::app::{Cx, Screen, Trans};
use crate::assets::rgb;
use crate::fb::{col, Frame};
use crate::input::Input;
use crate::text;
use cardul_sim::car::Car;
use cardul_sim::tables::{self, CHASSIS_NAMES, SUSPENSION_NAMES, TIRES};
use cardul_sim::Game;

/// A framed 40x25 text page of absolutely positioned strings. Any key closes it.
pub struct InfoScreen {
    items: Vec<(i32, i32, String, bool)>,
}

impl Default for InfoScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl InfoScreen {
    pub fn new() -> InfoScreen {
        InfoScreen { items: vec![] }
    }
    pub fn put(&mut self, c: i32, r: i32, s: &str) {
        self.items.push((c, r, s.to_string(), false));
    }
    pub fn put_inv(&mut self, c: i32, r: i32, s: &str) {
        self.items.push((c, r, s.to_string(), true));
    }

    pub fn help() -> InfoScreen {
        let mut p = InfoScreen::new();
        for (i, l) in super::HELP.iter().enumerate() {
            p.put(3, 3 + i as i32, l);
        }
        p.put(3, 22, "Press any key.");
        p
    }

    /// Driver status (`SR_status_screen`, subrout $4347).
    pub fn status(cx: &Cx) -> InfoScreen {
        let g = cx.game();
        let d = &g.driver;
        let mut p = InfoScreen::new();
        p.put(12, 2, &d.name);
        // labels from column 9, so the longest ("Mechanic Ability:") ends before the values
        p.put(9, 5, "Money:");
        p.put(27, 5, &text::money(d.money));
        p.put(9, 6, "Prestige:");
        p.put(27, 6, &d.prestige.to_string());
        p.put(9, 9, "Health:");
        p.put(27, 9, &d.health.to_string());
        p.put(9, 10, "Body Armor:");
        p.put(27, 10, &d.body_armor.to_string());
        p.put(9, 11, "Clone?:");
        p.put(27, 11, if d.clone.is_some() { "Yes" } else { "No" });
        if let Some(t) = &d.clone {
            p.put(9, 12, "Clone at:");
            p.put(20, 12, cx.data.city_name(t.city));
        }
        p.put(9, 15, "Driving Skill:");
        p.put(27, 15, &d.driving.to_string());
        p.put(9, 16, "Marksmanship:");
        p.put(27, 16, &d.marksman.to_string());
        p.put(9, 17, "Mechanic Ability:");
        p.put(27, 17, &d.mechanic.to_string());
        p.put(1, 21, &format!("You are in {}.", cx.data.city_name(d.city)));
        p.put(1, 22, &d.date.long());
        p
    }

    /// Cargo of the current car: what, where to and by when (the remake's F8; the original
    /// only showed the jobs at the AADA counter).
    pub fn cargo(cx: &Cx) -> InfoScreen {
        let g = cx.game();
        let mut p = InfoScreen::new();
        p.put(15, 2, "Your cargo");
        let Some(car) = g.car() else {
            p.put(15, 11, "No car!");
            return p;
        };
        let mut row = 5;
        for (i, c) in car.cargo.iter().enumerate() {
            let Some(c) = c else { continue };
            p.put(2, row, &format!("{} {}", i + 1, c.name));
            match c.dest {
                Some((b, city)) => {
                    p.put(4, row + 1, &format!("to the {} in {}", b.name(), cx.data.city_name(city)));
                    let due = match c.due {
                        Some(d) if c.is_overdue(g.driver.date) => format!("was due {} (late!)", d.mdy()),
                        Some(d) => format!("due {}, pays {}", d.mdy(), text::money(c.pay)),
                        None => format!("no deadline, pays {}", text::money(c.pay)),
                    };
                    p.put(4, row + 2, &due);
                }
                None => p.put(4, row + 1, &format!("sell at a Salvage Yard ({})", text::money(c.value))),
            }
            row += 4;
        }
        if row == 5 {
            p.put(2, 5, &format!("{} carries no cargo.", car.name));
        }
        p.put(1, 21, &format!("Today is {}.", g.driver.date.long()));
        p.put(1, 22, "Press any key.");
        p
    }

    /// Car sheet (`SR_car_screen`, subrout $45CD). `which` = car slot, None = current car.
    pub fn car_sheet(cx: &Cx, which: Option<usize>) -> InfoScreen {
        let g = cx.game();
        let car = match which {
            Some(i) => g.cars.get(i).and_then(|c| c.as_ref()),
            None => g.car(),
        };
        let mut p = InfoScreen::new();
        let Some(car) = car else {
            p.put(15, 11, "No car!");
            return p;
        };
        car_sheet_into(&mut p, g, car);
        let with_you = g.driver.car_with_you
            && match which {
                None => true,
                Some(i) => g.driver.car == Some(i as u8) && !g.driver.loaner,
            };
        let loc = if with_you { "with you".to_string() } else { format!("in {}", cx.data.city_name(car.city)) };
        p.put(1, 21, &format!("Location: {}", loc));
        p
    }
}

/// Fill the car sheet layout (also used by the Assembly Line and the Garage).
pub fn car_sheet_into(p: &mut InfoScreen, g: &Game, car: &Car) {
    let b = car.body_info();
    p.put(1, 1, "Drvr:");
    p.put(6, 1, &g.driver.name);
    p.put(2, 2, "Car:");
    p.put(6, 2, &car.name);
    p.put(1, 3, "Money:$");
    p.put(8, 3, &g.driver.money.to_string());
    p.put(2, 4, "Cost:$");
    p.put(8, 4, &car.value().to_string());
    p.put(2, 6, "Body:");
    p.put(7, 6, b.name);
    p.put(2, 7, "Chas:");
    p.put(7, 7, CHASSIS_NAMES[car.chassis as usize]);
    p.put(2, 8, "Susp:");
    p.put(7, 8, SUSPENSION_NAMES[car.suspension as usize]);
    p.put(4, 10, "Max Wt:");
    p.put(11, 10, &car.max_load().to_string());
    p.put(4, 11, "Max Sp:");
    p.put(11, 11, &car.spaces().to_string());
    p.put(6, 12, "Top:");
    p.put(10, 12, &car.top_mph().to_string());
    p.put(6, 13, "Hcl:");
    p.put(10, 13, &car.handling_class().to_string());
    p.put(3, 15, "Wt Left:");
    p.put(11, 15, &car.weight_left().to_string());
    p.put(3, 16, "Sp Left:");
    p.put(11, 16, &car.spaces_left().to_string());
    p.put(6, 17, "Acc:");
    p.put(10, 17, &car.accel_mph().map(|v| v.to_string()).unwrap_or_else(|| "--".into()));
    p.put(6, 18, "Bat:");
    p.put(10, 18, &car.battery.to_string());
    p.put(17, 11, "Armr");
    for (i, l) in ["F:", "B:", "L:", "R:", "U:"].iter().enumerate() {
        p.put(17, 12 + i as i32, l);
        p.put(19, 12 + i as i32, &car.armor[i].pts.to_string());
    }
    p.put(24, 1, "Item");
    p.put(30, 1, "Pt");
    p.put(33, 1, "Loc");
    p.put(27, 2, "Plant");
    p.put(23, 3, &format!("{:<6}", car.plant_info().name));
    p.put(30, 3, &car.plant.dp.to_string());
    p.put(33, 3, "Ctr");
    p.put(27, 4, "Tires");
    for (i, t) in car.tires.iter().enumerate() {
        p.put(23, 5 + i as i32, TIRES[t.kind as usize].short);
        p.put(30, 5 + i as i32, &t.dp.to_string());
        p.put(33, 5 + i as i32, tables::LOCATION_NAMES[6 + i]);
    }
    p.put(26, 9, "Weapons");
    p.put(37, 9, "Am");
    for i in 0..10 {
        let r = 10 + i as i32;
        match &car.weapons[i] {
            Some(w) => {
                p.put(23, r, w.info().short);
                p.put(30, r, &w.dp.to_string());
                p.put(33, r, tables::LOCATION_NAMES[w.mount as usize]);
                if w.uses_ammo() {
                    p.put(37, r, &w.ammo.to_string());
                }
            }
            None => p.put(23, r, "...... .. ... .."),
        }
    }
    let cargo: Vec<String> = car.cargo.iter().flatten().map(|c| c.name.clone()).collect();
    if !cargo.is_empty() {
        let s = format!("Cargo: {}", cargo.join(", "));
        let s = if s.chars().count() > 30 { format!("{}...", s.chars().take(27).collect::<String>()) } else { s };
        p.put(1, 20, &format!("{} (F8)", s));
    }
}

impl Screen for InfoScreen {
    fn update(&mut self, _cx: &mut Cx, inp: &Input) -> Trans {
        if inp.any_key() { Trans::Pop } else { Trans::None }
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        fb.clear(col::BLACK);
        cx.assets.draw_text_frame(fb, false);
        for (c, r, s, inv) in &self.items {
            if *inv {
                text::draw_bg(fb, &cx.assets, c * 8, r * 8, s, col::BLACK, col::WHITE);
            } else {
                text::at(fb, &cx.assets, *c, *r, s, col::WHITE);
            }
        }
    }
}

/// "Examine cars": list of owned cars; press a number to see one.
pub struct ExamineScreen {
    list: Vec<usize>,
}

impl ExamineScreen {
    pub fn new(cx: &Cx) -> ExamineScreen {
        ExamineScreen { list: cx.game().owned_cars().map(|(i, _)| i).collect() }
    }
}

impl Screen for ExamineScreen {
    fn update(&mut self, cx: &mut Cx, inp: &Input) -> Trans {
        if let Some(c) = inp.char_key()
            && let Some(n) = c.to_digit(10)
            && n >= 1
            && (n as usize) <= self.list.len()
        {
            return Trans::Push(Box::new(InfoScreen::car_sheet(cx, Some(self.list[n as usize - 1]))));
        }
        if inp.any_key() { Trans::Pop } else { Trans::None }
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        let a = &cx.assets;
        let g = cx.game();
        fb.clear(col::BLACK);
        cx.assets.draw_text_frame(fb, false);
        text::at(fb, a, 13, 2, "Your cars", col::WHITE);
        if self.list.is_empty() {
            text::at(fb, a, 3, 6, "You don't own any cars.", col::WHITE);
        }
        for (n, i) in self.list.iter().enumerate() {
            let c = g.cars[*i].as_ref().unwrap();
            let r = 5 + 2 * n as i32;
            text::at(fb, a, 3, r, &format!("{}", n + 1), col::WHITE);
            text::at(fb, a, 6, r, &c.name, col::WHITE);
            let here = g.driver.car == Some(*i as u8) && g.driver.car_with_you;
            let loc = if here { "with you".to_string() } else { cx.data.city_name(c.city).to_string() };
            text::at(fb, a, 24, r, &loc, col::WHITE);
        }
        text::at(fb, a, 1, 21, "Press a number to examine, or other", col::WHITE);
        text::at(fb, a, 1, 22, "to continue.", col::WHITE);
    }
}

/// The road map ('<-' key in Grue's version): Trurl's map with the current city marked.
pub struct MapScreen;

impl Default for MapScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl MapScreen {
    pub fn new() -> MapScreen {
        MapScreen
    }
}

impl Screen for MapScreen {
    fn update(&mut self, _cx: &mut Cx, inp: &Input) -> Trans {
        if inp.any_key() { Trans::Pop } else { Trans::None }
    }
    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        fb.clear(col::BLACK);
        fb.blit(&cx.assets.roadmap, 0, 0);
        if let Some(g) = &cx.game {
            // FIX: the map's title said "FALL 2030" whatever the date; the season and year
            // are drawn under it instead (the map pictures leave them out)
            let d = &g.driver.date;
            let season = match d.month {
                3..=5 => "SPRING",
                6..=8 => "SUMMER",
                9..=11 => "FALL",
                _ => "WINTER",
            };
            let name = cx.data.city_name(g.driver.city);
            let blink = (cx.tick / 20).is_multiple_of(2);
            if let Some(m) = &cx.assets.config.roadmap {
                // art with its own map: the season in its title box, a ring round the city dot
                let [x, y] = m.season;
                text::draw_px(fb, &cx.assets, x, y, &format!("{} {}", season, d.year), 3, rgb(m.season_ink));
                if let (Some([dx, dy]), true) = (m.markers.get(name), blink) {
                    let r = 6 * fb.scale;
                    fb.rect_px([dx - r, dy - r, dx + r, dy + r], fb.scale + 1, rgb(m.marker));
                }
                return;
            }
            let line = format!(" {} {}", season, d.year);
            text::draw_bg(fb, &cx.assets, 0, 16, &format!("{:<12}", line), col::WHITE, col::BLACK);
            if let Some(m) = cx.data.map_markers.iter().find(|m| m.city == name)
                && blink {
                fb.rect(m.col * 8 - 2, m.row * 8 - 2, 12, 12, 2, col::RED);
            }
        }
    }
}
