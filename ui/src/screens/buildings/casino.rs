//! The Casino in Atlantic City (`ca.prg`): draw poker and blackjack, drawn with the font and
//! the suit glyphs (`assets.suits`: spade, heart, diamond, club).
use super::talk::{self, Talk, LONG, MEDIUM, SHORT};
use super::{enter, leave};
use crate::app::{Cx, Screen, Trans};
use crate::assets::Assets;
use crate::fb::{col, Frame};
use crate::input::{Input, Key};
use crate::term::{Answer, Ask, Asker, Term};
use crate::text;
use cardul_sim::casino::{self, BjOutcome, Blackjack, Card, Poker, PokerHand};
use cardul_sim::world::Building;
use cardul_sim::Sfx;

pub fn screen(cx: &mut Cx) -> Box<dyn Screen> {
    Box::new(Casino::new(cx))
}

const WHAT: &str = "What would you like to do?";
/// Up to six digits, like the original.
const BET_DIGITS: usize = 6;
/// Screen column of the first card and the pitch between cards (8 px cells).
const CARD_COL: i32 = 20;
const CARD_PITCH: i32 = 2;
/// Screen rows of the player's and the dealer's cards.
const PLAYER_ROW: i32 = 7;
const DEALER_ROW: i32 = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Table {
    Poker,
    Blackjack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Menu,
    Bet,
    Deal,
    DealerTurn,
    /// show the result line, then "Press any key to continue."
    Continue(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Menu,
    Bet,
    Discard,
    Hit,
    Continue,
}

struct Casino {
    term: Term,
    asker: Asker,
    talk: Talk,
    then: Option<Step>,
    mode: Mode,
    table: Option<Table>,
    poker: Option<Poker>,
    bj: Option<Blackjack>,
    bet: u32,
    payoff: Option<u32>,
}

/// Print at a screen cell (the original's coordinates) in the term's main area.
fn at(term: &mut Term, col: usize, row: usize, s: &str) {
    term.at(col - 1, row - 2, s);
}

impl Casino {
    fn new(cx: &mut Cx) -> Casino {
        let (term, rep) = enter(cx, Building::Casino);
        let mut s = Casino {
            term,
            asker: Asker::default(),
            talk: Talk::default(),
            then: Some(Step::Menu),
            mode: Mode::Menu,
            table: None,
            poker: None,
            bj: None,
            bet: 0,
            payoff: None,
        };
        talk::deliveries(cx, &mut s.talk, &rep);
        s
    }

    fn draw_menu(&mut self) {
        self.term.clear_main();
        talk::menu(&mut self.term, 10, 5, &["Play Poker", "Play Black Jack", "Leave the Casino"]);
    }

    fn draw_table(&mut self) {
        self.term.clear_main();
        match self.table {
            Some(Table::Poker) => {
                talk::center(&mut self.term, 0, "Draw Poker");
                at(&mut self.term, 20, 5, "1 2 3 4 5");
                at(&mut self.term, 10, 7, "Your Hand");
            }
            _ => {
                talk::center(&mut self.term, 0, "Blackjack");
                at(&mut self.term, 31, 5, "Total");
                at(&mut self.term, 10, 7, "Your Hand");
                at(&mut self.term, 6, 12, "Dealer's Hand");
            }
        }
        at(&mut self.term, 9, 17, "Cash Remaining");
        at(&mut self.term, 10, 18, "Amount bet is");
        at(&mut self.term, 14, 19, "Payoff is");
    }

    fn run(&mut self, cx: &mut Cx, step: Step) -> Trans {
        match step {
            Step::Menu => {
                self.table = None;
                self.poker = None;
                self.bj = None;
                self.draw_menu();
                self.term.msg(WHAT);
                self.mode = Mode::Menu;
                self.asker.set(Ask::Choice(talk::digits(3)));
            }
            Step::Bet => {
                self.poker = None;
                if let Some(t) = &mut self.bj {
                    t.player.clear();
                    t.dealer.clear();
                }
                self.payoff = None;
                self.bet = 0;
                self.term.msg("How much do you wish to bet?");
                self.mode = Mode::Bet;
                self.asker.set(Ask::Digits(BET_DIGITS));
            }
            Step::Deal => match self.table {
                Some(Table::Poker) => {
                    self.poker = Some(Poker::deal(&mut cx.g().rng));
                    self.term.msg(
                        "Please indicate your discards with\nthe number keys. Press Enter when\ndone or Esc to clear them.",
                    );
                    self.mode = Mode::Discard;
                }
                _ => {
                    let first = self.bj.as_mut().and_then(|t| t.start_hand());
                    match first {
                        Some(o) => self.blackjack_over(cx, o),
                        None => self.ask_hit(),
                    }
                }
            },
            Step::DealerTurn => {
                let t = self.bj.as_mut().expect("blackjack table");
                if t.dealer_must_hit() {
                    let r = t.dealer_hit();
                    self.talk.say("The dealer takes a hit.", MEDIUM);
                    match r {
                        Some(o) => self.blackjack_over(cx, o),
                        None => self.then = Some(Step::DealerTurn),
                    }
                } else {
                    let o = t.settle();
                    self.blackjack_over(cx, o);
                }
            }
            Step::Continue(line) => {
                self.term.msg(&format!("{}\nPress any key to continue.", line));
                self.mode = Mode::Continue;
                self.asker.set(Ask::Key);
            }
        }
        Trans::None
    }

    fn ask_hit(&mut self) {
        self.term.msg("Do you want a hit?");
        self.mode = Mode::Hit;
        self.asker.set(Ask::YesNo);
    }

    /// Pay a win (if any), then show `line`.
    fn settle(&mut self, cx: &mut Cx, mult: u32, line: &'static str) {
        if mult > 0 {
            let p = casino::pay_win(&mut cx.g().driver, self.bet, mult);
            self.payoff = Some(p.payoff);
            cx.sfx(Sfx::Fanfare);
            if p.broke_bank {
                self.talk.say(line, MEDIUM);
                self.talk.say("Sir, you have broken the bank!\nWe cannot pay you any more.", LONG);
            }
        } else {
            self.payoff = Some(0);
            cx.sfx(Sfx::Beep);
        }
        cx.save();
        self.then = Some(Step::Continue(line));
    }

    fn poker_over(&mut self, cx: &mut Cx, hand: PokerHand) {
        let line = match hand {
            PokerHand::Nothing => "You lose!",
            PokerHand::OnePair => "You got One Pair!",
            PokerHand::TwoPair => "You got Two Pair!",
            PokerHand::ThreeOfAKind => "You got Three of a Kind!",
            PokerHand::Straight => "You got a Straight!",
            PokerHand::Flush => "You got a Flush!",
            PokerHand::FullHouse => "You got a Full House!",
            PokerHand::FourOfAKind => "You got Four of a Kind!",
            PokerHand::StraightFlush => "You got a Straight Flush!",
        };
        self.settle(cx, hand.multiplier(), line);
    }

    fn blackjack_over(&mut self, cx: &mut Cx, o: BjOutcome) {
        if let Some(t) = &mut self.bj {
            t.stand();
        }
        let line = match o {
            BjOutcome::DealerNatural | BjOutcome::DealerFiveCards | BjOutcome::DealerHigher => "Dealer wins.",
            BjOutcome::PlayerNatural => "Blackjack!",
            BjOutcome::PlayerBust => "You busted!  Dealer wins.",
            BjOutcome::DealerBust => "The dealer has busted!  You win!",
            BjOutcome::FiveCards | BjOutcome::PlayerHigher => "You win!",
        };
        self.settle(cx, o.multiplier(), line);
    }

    fn bet(&mut self, cx: &mut Cx, n: u32) {
        if n == 0 {
            self.then = Some(Step::Menu);
            return;
        }
        if !casino::place_bet(&mut cx.g().driver, n) {
            cx.sfx(Sfx::Beep);
            self.talk.say(
                "I'm sorry, but since you don't\nhave a credit account with us, I\ncannot accept that bet.",
                LONG,
            );
            self.then = Some(Step::Bet);
            return;
        }
        self.bet = n;
        cx.save();
        match self.table {
            Some(Table::Poker) => self.talk.say("Shuffling...", SHORT),
            _ => {
                let rng = &mut cx.g().rng;
                if let Some(t) = self.bj.as_mut().filter(|t| t.needs_shuffle()) {
                    t.shuffle(rng);
                    self.talk.say("Shuffling...", SHORT);
                }
            }
        }
        self.then = Some(Step::Deal);
    }

    /// Discard marking: keys 1-5 toggle, Enter draws, Esc clears the marks.
    fn discard_keys(&mut self, cx: &mut Cx, inp: &Input) {
        let Some(p) = &mut self.poker else { return };
        for k in &inp.keys {
            match k {
                Key::Char(c @ '1'..='5') => {
                    p.toggle((*c as u8 - b'1') as usize);
                    cx.sfx(Sfx::Click);
                }
                Key::Esc => p.clear_marks(),
                Key::Enter => {
                    let hand = p.draw();
                    self.poker_over(cx, hand);
                    return;
                }
                _ => {}
            }
        }
    }
}

/// A card at screen cell (col, row): rank on top, suit below, in a white box.
fn draw_card(fb: &mut Frame, a: &Assets, col: i32, row: i32, card: Option<Card>) {
    let (x, y) = (col * 8, row * 8);
    fb.fill(x - 3, y - 2, 14, 20, col::WHITE);
    fb.rect(x - 3, y - 2, 14, 20, 1, col::LIGHT_GREY);
    match card {
        Some(c) => {
            let ink = if c.is_red() { col::RED } else { col::BLACK };
            text::draw(fb, a, x, y, &c.rank_char().to_string(), ink);
            fb.blit_part(&a.suits, c.suit as i32 * 8, 0, 8, 8, x, y + 9, Some(ink));
        }
        None => {
            fb.fill(x - 1, y, 10, 16, col::BLUE);
            fb.rect(x - 1, y, 10, 16, 1, col::LIGHT_BLUE);
        }
    }
}

impl Screen for Casino {
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
        if self.mode == Mode::Discard {
            self.discard_keys(cx, inp);
            return Trans::None;
        }
        let Some(ans) = self.asker.feed(inp) else { return Trans::None };
        match (self.mode, ans) {
            (Mode::Menu, Answer::Choice('1')) => {
                self.table = Some(Table::Poker);
                self.draw_table();
                self.then = Some(Step::Bet);
            }
            (Mode::Menu, Answer::Choice('2')) => {
                self.table = Some(Table::Blackjack);
                self.draw_table();
                self.bj = Some(Blackjack::new(&mut cx.g().rng));
                self.talk.say("Shuffling...", SHORT);
                self.then = Some(Step::Bet);
            }
            (Mode::Menu, _) => return leave(cx, Building::Casino),
            (Mode::Bet, Answer::Number(n)) => self.bet(cx, n),
            (Mode::Bet, _) => self.then = Some(Step::Menu),
            (Mode::Hit, Answer::Yes) => {
                let r = self.bj.as_mut().and_then(|t| t.hit());
                match r {
                    Some(o) => self.blackjack_over(cx, o),
                    None => self.ask_hit(),
                }
            }
            (Mode::Hit, _) => {
                if let Some(t) = &mut self.bj {
                    t.stand();
                }
                self.then = Some(Step::DealerTurn);
            }
            (Mode::Continue, _) => self.then = Some(Step::Bet),
            _ => {}
        }
        Trans::None
    }

    fn draw(&self, cx: &Cx, fb: &mut Frame) {
        self.term.draw(fb, cx, self.asker.echo());
        let Some(table) = self.table else { return };
        let a = &cx.assets;
        // money
        text::at(fb, a, 25, 17, &text::money(cx.game().driver.money), col::WHITE);
        if self.bet > 0 {
            text::at(fb, a, 25, 18, &text::money(self.bet), col::WHITE);
        }
        if let Some(p) = self.payoff {
            text::at(fb, a, 25, 19, &text::money(p), col::WHITE);
        }
        match table {
            Table::Poker => {
                if let Some(p) = &self.poker {
                    for (i, c) in p.hand.iter().enumerate() {
                        let x = CARD_COL + CARD_PITCH * i as i32;
                        draw_card(fb, a, x, PLAYER_ROW, Some(*c));
                        if p.discard[i] {
                            text::at(fb, a, x, PLAYER_ROW + 3, "X", col::YELLOW);
                        }
                    }
                }
            }
            Table::Blackjack => {
                let Some(t) = &self.bj else { return };
                for (i, c) in t.player.iter().enumerate() {
                    draw_card(fb, a, CARD_COL + CARD_PITCH * i as i32, PLAYER_ROW, Some(*c));
                }
                for (i, c) in t.dealer.iter().enumerate() {
                    let shown = if i == 1 && t.hole_hidden { None } else { Some(*c) };
                    draw_card(fb, a, CARD_COL + CARD_PITCH * i as i32, DEALER_ROW, shown);
                }
                if !t.player.is_empty() {
                    text::at(fb, a, 32, PLAYER_ROW, &casino::bj_total(&t.player).to_string(), col::WHITE);
                }
                if !t.dealer.is_empty() && !t.hole_hidden {
                    text::at(fb, a, 32, DEALER_ROW, &casino::bj_total(&t.dealer).to_string(), col::WHITE);
                }
            }
        }
    }
}
