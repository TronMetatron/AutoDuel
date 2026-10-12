//! Deterministic random numbers (xorshift32). Serializable so saves reproduce.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rng {
    state: u32,
}

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng { state: if seed == 0 { 0x2545_F491 } else { seed } }
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }
    /// A random byte 0..=255 (the original `SR_random`).
    pub fn byte(&mut self) -> u8 {
        (self.next_u32() >> 24) as u8
    }
    /// 0..=n inclusive, uniform. This is the original `SR_random_n(n)` (which was slightly
    /// non-uniform; the bias is not reproduced).
    pub fn upto(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        self.next_u32() % (n + 1)
    }
    /// 1..=n inclusive.
    pub fn dice(&mut self, n: u32) -> u32 {
        1 + self.upto(n.saturating_sub(1))
    }
    /// true with probability num/den.
    pub fn chance(&mut self, num: u32, den: u32) -> bool {
        self.next_u32() % den < num
    }
    pub fn coin(&mut self) -> bool {
        self.byte() & 0x80 != 0
    }
}
