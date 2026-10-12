//! Calendar. The original stores day/month/year/weekday and advances one day at a time.
//! FIX: the original month table was broken for November/December; this is a normal
//! Gregorian calendar. The weekday is kept as its own counter (the original starts on
//! "Fri 1-1-2030" although that date was really a Tuesday).
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Date {
    pub day: u8,     // 1..=31
    pub month: u8,   // 1..=12
    pub year: u16,   // 2030...
    pub weekday: u8, // 0 = Sun .. 6 = Sat
}

pub const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
pub const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

impl Date {
    pub fn start() -> Date {
        Date { day: 1, month: 1, year: 2030, weekday: 5 }
    }
    pub fn is_leap(year: u16) -> bool {
        (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
    }
    pub fn days_in_month(month: u8, year: u16) -> u8 {
        match month {
            2 => {
                if Date::is_leap(year) { 29 } else { 28 }
            }
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        }
    }
    pub fn next_day(&self) -> Date {
        let mut d = *self;
        d.weekday = (d.weekday + 1) % 7;
        d.day += 1;
        if d.day > Date::days_in_month(d.month, d.year) {
            d.day = 1;
            d.month += 1;
            if d.month > 12 {
                d.month = 1;
                d.year += 1;
            }
        }
        d
    }
    pub fn plus_days(&self, n: u32) -> Date {
        let mut d = *self;
        for _ in 0..n {
            d = d.next_day();
        }
        d
    }
    /// Days since 1-1-2000, for differences.
    pub fn ordinal(&self) -> i64 {
        let mut n: i64 = 0;
        for y in 2000..self.year {
            n += if Date::is_leap(y) { 366 } else { 365 };
        }
        for m in 1..self.month {
            n += Date::days_in_month(m, self.year) as i64;
        }
        n + self.day as i64 - 1
    }
    /// "Fri 1-1-2030" (the original format: weekday, month-day-year).
    pub fn long(&self) -> String {
        format!("{} {}-{}-{}", WEEKDAYS[self.weekday as usize], self.month, self.day, self.year)
    }
    /// "Sat, Jan 12".
    pub fn short(&self) -> String {
        format!("{}, {} {}", WEEKDAYS[self.weekday as usize], MONTHS[self.month as usize - 1], self.day)
    }
    /// "1-12-30" style used on cargo due dates.
    pub fn mdy(&self) -> String {
        format!("{}-{}-{:02}", self.month, self.day, self.year % 100)
    }
}

impl PartialOrd for Date {
    fn partial_cmp(&self, other: &Date) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Date {
    fn cmp(&self, other: &Date) -> Ordering {
        (self.year, self.month, self.day).cmp(&(other.year, other.month, other.day))
    }
}
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.long())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn calendar_rolls_over() {
        let d = Date { day: 31, month: 12, year: 2030, weekday: 2 };
        let n = d.next_day();
        assert_eq!((n.day, n.month, n.year, n.weekday), (1, 1, 2031, 3));
        let f = Date { day: 28, month: 2, year: 2032, weekday: 0 };
        assert_eq!(f.next_day().day, 29);
        assert_eq!(Date::start().long(), "Fri 1-1-2030");
        assert!(Date::start().plus_days(10) > Date::start());
        assert_eq!(Date::start().plus_days(365).ordinal() - Date::start().ordinal(), 365);
    }
}
