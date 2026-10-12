//! Text-mode screens (buildings, status screens): a 40 x 25 character screen with the original
//! layout — a frame, the main area (rows 2-19) and a three-line message area (rows 21-23) —
//! plus [`Asker`], which collects answers to prompts (any key, Y/N, numbers, text, menu keys).
use crate::app::Cx;
use crate::fb::{col, Frame};
use crate::input::{Input, Key};
use crate::text;

pub const COLS: usize = 38; // inside the frame
pub const MAIN_ROWS: usize = 18; // rows 2..19
pub const MSG_ROWS: usize = 3; // rows 21..23

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    ch: char,
    fg: u32,
    inv: bool,
}

const BLANK: Cell = Cell { ch: ' ', fg: col::WHITE, inv: false };

pub struct Term {
    pub title: String,
    main: Vec<[Cell; COLS]>,
    /// cursor in the main area
    pub row: usize,
    pub col: usize,
    msg: Vec<String>,
    /// the message area holds a reply that should stay above the next [`Term::prompt`]
    msg_reply: bool,
    pub fg: u32,
}

impl Default for Term {
    fn default() -> Self {
        Term::new("")
    }
}

impl Term {
    pub fn new(title: &str) -> Term {
        Term { title: title.to_string(), main: vec![[BLANK; COLS]; MAIN_ROWS], row: 0, col: 0, msg: vec![], msg_reply: false, fg: col::WHITE }
    }
    /// Clear main and message areas.
    pub fn cls(&mut self) {
        self.clear_main();
        self.msg_clear();
    }
    pub fn clear_main(&mut self) {
        for r in self.main.iter_mut() {
            *r = [BLANK; COLS];
        }
        self.row = 0;
        self.col = 0;
    }
    fn scroll(&mut self) {
        self.main.remove(0);
        self.main.push([BLANK; COLS]);
        self.row = MAIN_ROWS - 1;
    }
    fn newline(&mut self) {
        self.col = 0;
        self.row += 1;
        if self.row >= MAIN_ROWS {
            self.scroll();
        }
    }
    fn put(&mut self, ch: char, inv: bool) {
        if self.col >= COLS {
            self.newline();
        }
        let fg = self.fg;
        self.main[self.row][self.col] = Cell { ch, fg, inv };
        self.col += 1;
    }
    /// Print with word wrap at the cursor (no newline at the end).
    pub fn print(&mut self, s: &str) {
        let lines = text::wrap(s, COLS);
        // continue the current line if it fits, else wrap by words
        for (i, l) in lines.iter().enumerate() {
            if i > 0 {
                self.newline();
            }
            if self.col > 0 && self.col + l.chars().count() > COLS {
                self.newline();
            }
            for ch in l.chars() {
                self.put(ch, false);
            }
        }
    }
    pub fn println(&mut self, s: &str) {
        self.print(s);
        self.newline();
    }
    /// Inverse-video text at the cursor.
    pub fn print_inv(&mut self, s: &str) {
        for ch in s.chars() {
            self.put(ch, true);
        }
    }
    /// Print at a position in the main area (col 0..37, row 0..17), no wrapping.
    pub fn at(&mut self, col: usize, row: usize, s: &str) {
        self.at_style(col, row, s, false);
    }
    pub fn at_inv(&mut self, col: usize, row: usize, s: &str) {
        self.at_style(col, row, s, true);
    }
    fn at_style(&mut self, col: usize, row: usize, s: &str, inv: bool) {
        if row >= MAIN_ROWS {
            return;
        }
        let fg = self.fg;
        if col + s.trim_end().chars().count() > COLS {
            text::report_overflow(s);
        }
        for (i, ch) in s.chars().enumerate() {
            if col + i < COLS {
                self.main[row][col + i] = Cell { ch, fg, inv };
            }
        }
        self.row = row;
        self.col = (col + s.chars().count()).min(COLS);
    }
    /// Highlight (invert) a whole main-area row.
    pub fn invert_row(&mut self, row: usize) {
        if row < MAIN_ROWS {
            for c in self.main[row].iter_mut() {
                c.inv = !c.inv;
            }
        }
    }
    pub fn goto(&mut self, col: usize, row: usize) {
        self.col = col.min(COLS);
        self.row = row.min(MAIN_ROWS - 1);
    }

    /// Replace the message area text (word-wrapped, last three lines kept).
    pub fn msg(&mut self, s: &str) {
        self.msg = text::wrap(s, COLS);
        while self.msg.len() > MSG_ROWS {
            self.msg.remove(0);
        }
        self.msg_reply = false;
    }
    /// A reply to the player's last request: replaces the message area like [`Term::msg`],
    /// and stays above the next [`Term::prompt`] if both fit, so the answer can still be read
    /// once the shop asks for the next thing.
    pub fn reply(&mut self, s: &str) {
        self.msg(s);
        self.msg_reply = true;
    }
    /// A prompt: under the reply that is up, or alone if there is none or both don't fit.
    pub fn prompt(&mut self, s: &str) {
        let lines = text::wrap(s, COLS);
        if self.msg_reply && self.msg.len() + lines.len() <= MSG_ROWS {
            self.msg.extend(lines);
            self.msg_reply = false;
        } else {
            self.msg(s);
        }
    }
    /// Append to the message area.
    pub fn msg_add(&mut self, s: &str) {
        for l in text::wrap(s, COLS) {
            self.msg.push(l);
        }
        while self.msg.len() > MSG_ROWS {
            self.msg.remove(0);
        }
    }
    pub fn msg_clear(&mut self) {
        self.msg.clear();
        self.msg_reply = false;
    }

    /// Draw the whole text screen. `prompt` = text being typed (shown after the message).
    pub fn draw(&self, fb: &mut Frame, cx: &Cx, prompt: Option<&str>) {
        let a = &cx.assets;
        fb.clear(col::BLACK);
        // frame: outer box and the separator above the message area
        a.draw_text_frame(fb, true);
        if !self.title.is_empty() {
            let x = (40 - self.title.chars().count() as i32) / 2;
            text::at(fb, a, x, 1, &self.title, col::WHITE);
        }
        for (r, line) in self.main.iter().enumerate() {
            for (c, cell) in line.iter().enumerate() {
                let x = (c as i32 + 1) * 8;
                let y = (r as i32 + 2) * 8;
                if cell.inv {
                    fb.fill(x, y, 8, 8, cell.fg);
                    text::draw(fb, a, x, y, &cell.ch.to_string(), col::BLACK);
                } else if cell.ch != ' ' {
                    text::draw(fb, a, x, y, &cell.ch.to_string(), cell.fg);
                }
            }
        }
        let mut lines = self.msg.clone();
        if let Some(p) = prompt {
            let cursor = if (cx.tick / 16).is_multiple_of(2) { "_" } else { " " };
            let s = format!("{}{}", p, cursor);
            match lines.last_mut() {
                Some(l) if l.chars().count() + s.chars().count() <= COLS => l.push_str(&s),
                _ => lines.push(s),
            }
            while lines.len() > MSG_ROWS {
                lines.remove(0);
            }
        }
        for (i, l) in lines.iter().enumerate() {
            text::at(fb, a, 1, 21 + i as i32, l, col::WHITE);
        }
    }
}

/// What to ask for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ask {
    /// any key (Esc included)
    Key,
    YesNo,
    /// up to n digits, Enter to finish (empty = 0)
    Digits(usize),
    /// up to n characters of text, Enter to finish
    Text(usize),
    /// one of these keys (lower-case letters / digits)
    Choice(Vec<char>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Answer {
    Key(Key),
    Yes,
    No,
    Number(u32),
    Text(String),
    Choice(char),
    /// Esc
    Cancel,
}

#[derive(Default)]
pub struct Asker {
    pub ask: Option<Ask>,
    pub buf: String,
}

impl Asker {
    pub fn set(&mut self, a: Ask) {
        self.ask = Some(a);
        self.buf.clear();
    }
    pub fn clear(&mut self) {
        self.ask = None;
        self.buf.clear();
    }
    pub fn waiting(&self) -> bool {
        self.ask.is_some()
    }
    /// The text being typed, for [`Term::draw`].
    pub fn echo(&self) -> Option<&str> {
        match self.ask {
            Some(Ask::Digits(_)) | Some(Ask::Text(_)) => Some(&self.buf),
            _ => None,
        }
    }
    /// Feed one tick of input; returns the answer once complete (and stops asking).
    pub fn feed(&mut self, inp: &Input) -> Option<Answer> {
        let ask = self.ask.clone()?;
        let ans = match ask {
            Ask::Key => inp.keys.first().map(|k| Answer::Key(*k)),
            Ask::YesNo => {
                if inp.pressed(Key::Char('y')) {
                    Some(Answer::Yes)
                } else if inp.pressed(Key::Char('n')) || inp.pressed(Key::Esc) {
                    Some(Answer::No)
                } else {
                    None
                }
            }
            Ask::Digits(n) => {
                let mut out = None;
                for k in &inp.keys {
                    match k {
                        Key::Char(c) if c.is_ascii_digit() && self.buf.len() < n => self.buf.push(*c),
                        Key::Backspace => {
                            self.buf.pop();
                        }
                        Key::Enter => out = Some(Answer::Number(self.buf.parse().unwrap_or(0))),
                        Key::Esc => out = Some(Answer::Cancel),
                        _ => {}
                    }
                }
                out
            }
            Ask::Text(n) => {
                let mut out = None;
                for c in &inp.chars {
                    if (c.is_ascii_alphanumeric() || " .,'-!?&".contains(*c)) && self.buf.chars().count() < n {
                        self.buf.push(*c);
                    }
                }
                for k in &inp.keys {
                    match k {
                        Key::Backspace => {
                            self.buf.pop();
                        }
                        Key::Enter => out = Some(Answer::Text(self.buf.trim().to_string())),
                        Key::Esc => out = Some(Answer::Cancel),
                        _ => {}
                    }
                }
                out
            }
            Ask::Choice(list) => {
                let mut out = None;
                for k in &inp.keys {
                    match k {
                        Key::Char(c) if list.contains(c) => out = Some(Answer::Choice(*c)),
                        Key::Esc => out = Some(Answer::Cancel),
                        _ => {}
                    }
                }
                out
            }
        };
        if ans.is_some() {
            self.clear();
        }
        ans
    }
}
