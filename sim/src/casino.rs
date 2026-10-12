//! Casino games in Atlantic City (`ca.prg`): draw poker and blackjack.
//!
//! Money rules shared by both games (`ca $A275`, `pay_win`):
//! * the bet (up to all your cash) is taken when it is placed;
//! * a win pays `bet × multiplier` back (so a multiplier of 1 only returns the bet);
//! * a payoff of $200,000 or more earns prestige +1;
//! * cash is capped at $999,999 ("Sir, you have broken the bank!").
//!   FIX: the original did not check the cap when adding, so cash could overflow.
use crate::driver::Driver;
use crate::rng::Rng;

/// A playing card. `rank` 1 = ace .. 13 = king; `suit` 0 spade, 1 heart, 2 diamond, 3 club
/// (the order of the suit glyphs in `assets/gfx/suits.png`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    pub rank: u8,
    pub suit: u8,
}

impl Card {
    pub fn new(rank: u8, suit: u8) -> Card {
        Card { rank, suit }
    }
    /// Rank as shown on the card: A 2..9 T J Q K (the original's `D_A9EF`).
    pub fn rank_char(self) -> char {
        b"?A23456789TJQK"[self.rank.min(13) as usize] as char
    }
    pub fn is_red(self) -> bool {
        self.suit == 1 || self.suit == 2
    }
}

/// A single 52-card deck; cards are dealt from the end.
#[derive(Clone, Debug)]
pub struct Deck {
    cards: Vec<Card>,
}

impl Deck {
    /// A fresh, ordered deck.
    pub fn new() -> Deck {
        let mut cards = Vec::with_capacity(52);
        for suit in 0..4 {
            for rank in 1..=13 {
                cards.push(Card::new(rank, suit));
            }
        }
        Deck { cards }
    }
    /// A fresh deck shuffled like the original (`ca $A18A`): 256 swaps of two random cards.
    pub fn shuffled(rng: &mut Rng) -> Deck {
        let mut d = Deck::new();
        for _ in 0..256 {
            let a = rng.upto(51) as usize;
            let b = rng.upto(51) as usize;
            d.cards.swap(a, b);
        }
        d
    }
    pub fn deal(&mut self) -> Card {
        // A hand never needs more than the reshuffle threshold, so the deck can't run dry;
        // refill defensively anyway.
        if self.cards.is_empty() {
            *self = Deck::new();
        }
        self.cards.pop().unwrap()
    }
    pub fn len(&self) -> usize {
        self.cards.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
}

impl Default for Deck {
    fn default() -> Self {
        Deck::new()
    }
}

// ------------------------------------------------------------------------------------- money

/// Result of paying a win.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Payout {
    pub payoff: u32,
    /// payoff >= $200,000: prestige +1
    pub prestige: bool,
    /// cash hit $999,999
    pub broke_bank: bool,
}

/// Payoffs of this size or more earn a point of prestige.
pub const BIG_WIN: u32 = 200_000;

/// Take the bet. Returns false (and takes nothing) if it is more than your cash — "I'm sorry,
/// but since you don't have a credit account with us, I cannot accept that bet."
pub fn place_bet(driver: &mut Driver, bet: u32) -> bool {
    driver.pay(bet)
}

/// Pay `bet × mult` back to the player.
pub fn pay_win(driver: &mut Driver, bet: u32, mult: u32) -> Payout {
    let payoff = bet.saturating_mul(mult);
    let prestige = payoff >= BIG_WIN;
    if prestige {
        driver.add_prestige(1);
    }
    let broke_bank = driver.add_money(payoff as i64);
    Payout { payoff, prestige, broke_bank }
}

// ------------------------------------------------------------------------------- draw poker

/// Poker hands, worst to best (`ca $A49E`). Any pair counts; there is no "jacks or better".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PokerHand {
    Nothing,
    OnePair,
    TwoPair,
    ThreeOfAKind,
    Straight,
    Flush,
    FullHouse,
    FourOfAKind,
    StraightFlush,
}

impl PokerHand {
    /// "You got <name>" (`ca $A6F6..`).
    pub fn name(self) -> &'static str {
        match self {
            PokerHand::Nothing => "nothing",
            PokerHand::OnePair => "One Pair",
            PokerHand::TwoPair => "Two Pair",
            PokerHand::ThreeOfAKind => "Three of a Kind",
            PokerHand::Straight => "a Straight",
            PokerHand::Flush => "a Flush",
            PokerHand::FullHouse => "a Full House",
            PokerHand::FourOfAKind => "Four of a Kind",
            PokerHand::StraightFlush => "a Straight Flush",
        }
    }
    /// Payoff in multiples of the bet (`ca $A4FD` + 1); the bet was already taken.
    pub fn multiplier(self) -> u32 {
        match self {
            PokerHand::Nothing => 0,
            PokerHand::OnePair => 1,
            PokerHand::TwoPair => 2,
            PokerHand::ThreeOfAKind => 3,
            PokerHand::Straight => 4,
            PokerHand::Flush => 5,
            PokerHand::FullHouse => 7,
            PokerHand::FourOfAKind => 25,
            PokerHand::StraightFlush => 70,
        }
    }
}

/// Rank a five-card hand. Straights may be A-2-3-4-5 or 10-J-Q-K-A (no wrap-around).
pub fn evaluate_poker(hand: &[Card; 5]) -> PokerHand {
    let mut counts = [0u8; 14];
    for c in hand {
        counts[c.rank as usize] += 1;
    }
    let mut groups: Vec<u8> = counts.iter().copied().filter(|&n| n > 0).collect();
    groups.sort_unstable_by(|a, b| b.cmp(a));
    let flush = hand.iter().all(|c| c.suit == hand[0].suit);
    let straight = groups.len() == 5 && {
        let mut r: Vec<u8> = hand.iter().map(|c| c.rank).collect();
        r.sort_unstable();
        r[4] - r[0] == 4 || r == [1, 10, 11, 12, 13]
    };
    match (straight, flush, groups.as_slice()) {
        (true, true, _) => PokerHand::StraightFlush,
        (_, _, [4, ..]) => PokerHand::FourOfAKind,
        (_, _, [3, 2]) => PokerHand::FullHouse,
        (_, true, _) => PokerHand::Flush,
        (true, _, _) => PokerHand::Straight,
        (_, _, [3, ..]) => PokerHand::ThreeOfAKind,
        (_, _, [2, 2, ..]) => PokerHand::TwoPair,
        (_, _, [2, ..]) => PokerHand::OnePair,
        _ => PokerHand::Nothing,
    }
}

/// One hand of draw poker: a freshly shuffled deck, five cards, one draw.
#[derive(Clone, Debug)]
pub struct Poker {
    pub deck: Deck,
    pub hand: [Card; 5],
    /// cards marked for discarding (keys 1-5)
    pub discard: [bool; 5],
}

impl Poker {
    /// Shuffle a new deck and deal five cards.
    pub fn deal(rng: &mut Rng) -> Poker {
        let mut deck = Deck::shuffled(rng);
        let hand = [deck.deal(), deck.deal(), deck.deal(), deck.deal(), deck.deal()];
        Poker { deck, hand, discard: [false; 5] }
    }
    pub fn toggle(&mut self, i: usize) {
        if i < 5 {
            self.discard[i] = !self.discard[i];
        }
    }
    pub fn clear_marks(&mut self) {
        self.discard = [false; 5];
    }
    /// Replace the marked cards and rank the final hand.
    pub fn draw(&mut self) -> PokerHand {
        for i in 0..5 {
            if self.discard[i] {
                self.hand[i] = self.deck.deal();
                self.discard[i] = false;
            }
        }
        evaluate_poker(&self.hand)
    }
}

// --------------------------------------------------------------------------------- blackjack

/// The dealer draws below this total and stands on any 17.
pub const DEALER_STANDS: u32 = 17;
/// A new deck is shuffled when fewer cards than this remain at the start of a hand.
pub const RESHUFFLE_BELOW: usize = 10;
/// Five cards without busting ends the hand (a win for the player, a loss for the dealer).
pub const FIVE_CARDS: usize = 5;

/// Blackjack total: J/Q/K = 10; aces count 11, reduced to 1 while the total is over 21.
pub fn bj_total(cards: &[Card]) -> u32 {
    let mut total = 0;
    let mut aces = 0;
    for c in cards {
        total += match c.rank {
            1 => {
                aces += 1;
                11
            }
            r if r >= 10 => 10,
            r => r as u32,
        };
    }
    while total > 21 && aces > 0 {
        total -= 10;
        aces -= 1;
    }
    total
}

/// A two-card 21.
pub fn bj_natural(cards: &[Card]) -> bool {
    cards.len() == 2 && bj_total(cards) == 21
}

/// How a blackjack hand ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BjOutcome {
    /// checked first: the dealer wins at once
    DealerNatural,
    /// "Blackjack!" pays 3x
    PlayerNatural,
    /// "You busted!"
    PlayerBust,
    /// five cards without busting: an automatic win
    FiveCards,
    /// "The dealer has busted!"
    DealerBust,
    /// the dealer reached five cards: the dealer wins
    DealerFiveCards,
    /// higher total than the dealer
    PlayerHigher,
    /// dealer total higher or equal (ties go to the dealer)
    DealerHigher,
}

impl BjOutcome {
    /// Payoff in multiples of the bet (already taken).
    pub fn multiplier(self) -> u32 {
        match self {
            BjOutcome::PlayerNatural => 3,
            BjOutcome::FiveCards | BjOutcome::DealerBust | BjOutcome::PlayerHigher => 2,
            _ => 0,
        }
    }
    pub fn player_wins(self) -> bool {
        self.multiplier() > 0
    }
}

/// A blackjack table: one deck that lasts several hands.
#[derive(Clone, Debug)]
pub struct Blackjack {
    pub deck: Deck,
    pub player: Vec<Card>,
    pub dealer: Vec<Card>,
    /// the dealer's second card is face down
    pub hole_hidden: bool,
}

impl Blackjack {
    pub fn new(rng: &mut Rng) -> Blackjack {
        Blackjack { deck: Deck::shuffled(rng), player: vec![], dealer: vec![], hole_hidden: true }
    }
    /// Does the deck need shuffling before the next hand?
    pub fn needs_shuffle(&self) -> bool {
        self.deck.len() < RESHUFFLE_BELOW
    }
    pub fn shuffle(&mut self, rng: &mut Rng) {
        self.deck = Deck::shuffled(rng);
    }
    /// Deal a new hand (player, dealer up, player, dealer down) and check the naturals.
    pub fn start_hand(&mut self) -> Option<BjOutcome> {
        self.player.clear();
        self.dealer.clear();
        self.hole_hidden = true;
        self.player.push(self.deck.deal());
        self.dealer.push(self.deck.deal());
        self.player.push(self.deck.deal());
        self.dealer.push(self.deck.deal());
        if bj_natural(&self.dealer) {
            self.hole_hidden = false;
            Some(BjOutcome::DealerNatural)
        } else if bj_natural(&self.player) {
            Some(BjOutcome::PlayerNatural)
        } else {
            None
        }
    }
    /// The player takes a card.
    pub fn hit(&mut self) -> Option<BjOutcome> {
        self.player.push(self.deck.deal());
        if bj_total(&self.player) > 21 {
            Some(BjOutcome::PlayerBust)
        } else if self.player.len() >= FIVE_CARDS {
            Some(BjOutcome::FiveCards)
        } else {
            None
        }
    }
    /// The player stands: the hole card is turned over.
    pub fn stand(&mut self) {
        self.hole_hidden = false;
    }
    pub fn dealer_must_hit(&self) -> bool {
        bj_total(&self.dealer) < DEALER_STANDS
    }
    /// "The dealer takes a hit."
    pub fn dealer_hit(&mut self) -> Option<BjOutcome> {
        self.dealer.push(self.deck.deal());
        if bj_total(&self.dealer) > 21 {
            Some(BjOutcome::DealerBust)
        } else if self.dealer.len() >= FIVE_CARDS {
            Some(BjOutcome::DealerFiveCards)
        } else {
            None
        }
    }
    /// Compare totals once the dealer stands; ties go to the dealer.
    pub fn settle(&self) -> BjOutcome {
        if bj_total(&self.player) > bj_total(&self.dealer) {
            BjOutcome::PlayerHigher
        } else {
            BjOutcome::DealerHigher
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(cards: [(u8, u8); 5]) -> [Card; 5] {
        cards.map(|(r, s)| Card::new(r, s))
    }

    #[test]
    fn poker_ranks_every_hand() {
        use PokerHand::*;
        assert_eq!(evaluate_poker(&h([(2, 0), (5, 1), (9, 2), (11, 3), (13, 0)])), Nothing);
        assert_eq!(evaluate_poker(&h([(2, 0), (2, 1), (9, 2), (11, 3), (13, 0)])), OnePair);
        assert_eq!(evaluate_poker(&h([(2, 0), (2, 1), (9, 2), (9, 3), (13, 0)])), TwoPair);
        assert_eq!(evaluate_poker(&h([(7, 0), (7, 1), (7, 2), (9, 3), (13, 0)])), ThreeOfAKind);
        assert_eq!(evaluate_poker(&h([(5, 0), (6, 1), (7, 2), (8, 3), (9, 0)])), Straight);
        assert_eq!(evaluate_poker(&h([(1, 0), (2, 1), (3, 2), (4, 3), (5, 0)])), Straight);
        assert_eq!(evaluate_poker(&h([(10, 0), (11, 1), (12, 2), (13, 3), (1, 0)])), Straight);
        // no wrap-around straight
        assert_eq!(evaluate_poker(&h([(11, 0), (12, 1), (13, 2), (1, 3), (2, 0)])), Nothing);
        assert_eq!(evaluate_poker(&h([(2, 1), (5, 1), (9, 1), (11, 1), (13, 1)])), Flush);
        assert_eq!(evaluate_poker(&h([(3, 0), (3, 1), (3, 2), (12, 3), (12, 0)])), FullHouse);
        assert_eq!(evaluate_poker(&h([(4, 0), (4, 1), (4, 2), (4, 3), (12, 0)])), FourOfAKind);
        assert_eq!(evaluate_poker(&h([(5, 2), (6, 2), (7, 2), (8, 2), (9, 2)])), StraightFlush);
        assert_eq!(evaluate_poker(&h([(10, 3), (11, 3), (12, 3), (13, 3), (1, 3)])), StraightFlush);
        let mults: Vec<u32> = [OnePair, TwoPair, ThreeOfAKind, Straight, Flush, FullHouse, FourOfAKind, StraightFlush]
            .iter()
            .map(|p| p.multiplier())
            .collect();
        assert_eq!(mults, vec![1, 2, 3, 4, 5, 7, 25, 70]);
    }

    #[test]
    fn deck_and_draw() {
        let mut rng = Rng::new(7);
        let d = Deck::shuffled(&mut rng);
        assert_eq!(d.len(), 52);
        let mut all: Vec<(u8, u8)> = d.cards.iter().map(|c| (c.rank, c.suit)).collect();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 52);
        let mut p = Poker::deal(&mut rng);
        assert_eq!(p.deck.len(), 47);
        let kept = p.hand[0];
        p.toggle(1);
        p.toggle(2);
        p.draw();
        assert_eq!(p.deck.len(), 45);
        assert_eq!(p.hand[0], kept);
    }

    #[test]
    fn blackjack_totals() {
        let c = |r| Card::new(r, 0);
        assert_eq!(bj_total(&[c(1), c(13)]), 21);
        assert!(bj_natural(&[c(1), c(12)]));
        assert!(!bj_natural(&[c(7), c(7), c(7)]));
        assert_eq!(bj_total(&[c(1), c(1)]), 12);
        assert_eq!(bj_total(&[c(1), c(1), c(1), c(9)]), 12);
        assert_eq!(bj_total(&[c(1), c(5), c(10)]), 16);
        assert_eq!(bj_total(&[c(11), c(12), c(2)]), 22);
        assert_eq!(bj_total(&[c(2), c(3), c(4), c(5), c(6)]), 20);
    }

    #[test]
    fn blackjack_outcomes() {
        let c = |r| Card::new(r, 0);
        let mut rng = Rng::new(3);
        let mut t = Blackjack::new(&mut rng);
        // ties go to the dealer
        t.player = vec![c(10), c(8)];
        t.dealer = vec![c(9), c(9)];
        assert_eq!(t.settle(), BjOutcome::DealerHigher);
        t.player = vec![c(10), c(9)];
        assert_eq!(t.settle(), BjOutcome::PlayerHigher);
        assert_eq!(BjOutcome::PlayerNatural.multiplier(), 3);
        assert_eq!(BjOutcome::FiveCards.multiplier(), 2);
        assert_eq!(BjOutcome::DealerFiveCards.multiplier(), 0);
        // a full hand from a fresh deck never runs the deck dry
        for _ in 0..40 {
            if t.needs_shuffle() {
                t.shuffle(&mut rng);
            }
            if t.start_hand().is_some() {
                continue;
            }
            while t.player.len() < 5 && t.hit().is_none() {}
            t.stand();
            while t.dealer_must_hit() {
                if t.dealer_hit().is_some() {
                    break;
                }
            }
        }
    }

    #[test]
    fn payouts_and_bank() {
        let mut d = Driver::new("T", false);
        d.money = 1000;
        assert!(!place_bet(&mut d, 1001));
        assert!(place_bet(&mut d, 1000));
        assert_eq!(d.money, 0);
        let p = pay_win(&mut d, 1000, 2);
        assert_eq!((p.payoff, p.prestige, p.broke_bank, d.money), (2000, false, false, 2000));
        d.money = 900_000;
        let p = pay_win(&mut d, 100_000, 2);
        assert!(p.prestige && p.broke_bank);
        assert_eq!(d.money, 999_999);
        assert_eq!(d.prestige, 1);
    }
}
