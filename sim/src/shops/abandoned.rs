//! Abandoned building (`ab.prg`, "Shack"): nothing but muggers, and one clue in Albany.
use crate::game::Game;

/// Below this much cash the muggers take half; otherwise a flat $1,000.
pub const MUG_THRESHOLD: u32 = 2_000;
pub const MUG_FLAT: u32 = 1_000;

/// What happens when you walk in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Albany at prestige 80+: the "Little" clue (and no mugging)
    Clue(&'static str),
    /// "The building is abandoned."
    Empty,
    /// "You were just mugged. They took $X."
    Mugged(u32),
}

/// How much the muggers take from `money`.
pub fn mugging_amount(money: u32) -> u32 {
    if money < MUG_THRESHOLD { money / 2 } else { MUG_FLAT }
}

/// Enter the building (`ab $A000`): the clue, else 50% empty, 50% mugged.
pub fn visit(game: &mut Game) -> Event {
    if let Some(c) = super::rumors::albany_clue(game) {
        return Event::Clue(c);
    }
    if game.rng.coin() {
        return Event::Empty;
    }
    let took = mugging_amount(game.driver.money);
    game.driver.money -= took;
    Event::Mugged(took)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mugging_rules() {
        assert_eq!(mugging_amount(0), 0);
        assert_eq!(mugging_amount(1999), 999);
        assert_eq!(mugging_amount(2000), 1000);
        assert_eq!(mugging_amount(500_000), 1000);
        let d = crate::need_data!();
        let mut g = Game::new("T", false, &d, 4);
        let (mut empty, mut mugged) = (0, 0);
        for _ in 0..200 {
            g.driver.money = 1500;
            match visit(&mut g) {
                Event::Empty => empty += 1,
                Event::Mugged(n) => {
                    mugged += 1;
                    assert_eq!((n, g.driver.money), (750, 750));
                }
                Event::Clue(_) => panic!(),
            }
        }
        assert!(empty > 60 && mugged > 60);
        g.driver.city = 4;
        g.driver.prestige = 80;
        g.driver.money = 5000;
        assert!(matches!(visit(&mut g), Event::Clue(_)));
        assert_eq!(g.driver.money, 5000);
    }
}
