//! The Baltimore Hotel (`ho.prg`): a night costs $200, takes a day and heals one point.
use crate::game::Game;

pub const NIGHT_PRICE: u32 = 200;
pub const HEALTH_MAX: u8 = 3;

/// Spend the night. Returns false if you can't pay ("I'm sorry sir, but you haven't enough
/// money.").
pub fn spend_night(game: &mut Game) -> bool {
    if !game.driver.pay(NIGHT_PRICE) {
        return false;
    }
    game.next_day();
    game.driver.health = (game.driver.health + 1).min(HEALTH_MAX);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_night_heals() {
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 1);
        g.driver.health = 1;
        g.driver.day_used = true;
        let day = g.driver.date;
        assert!(spend_night(&mut g));
        assert_eq!((g.driver.health, g.driver.money, g.driver.date), (2, 1800, day.next_day()));
        assert!(!g.driver.day_used);
        g.driver.health = 3;
        assert!(spend_night(&mut g));
        assert_eq!(g.driver.health, 3);
        g.driver.money = 199;
        assert!(!spend_night(&mut g));
        assert_eq!(g.driver.money, 199);
    }
}
