//! The DOS game's picture libraries (`*.tlb`) and their compression.
//!
//! A library starts with `TLBR` and comes in two forms:
//!
//! * plain: `u32` file size and `u16` entry count (little-endian), then `count + 1` `u32`
//!   offsets (relative to the end of the table), then the entries;
//! * compressed: `u32` file size (big-endian), `FF FF`, `u16` count and `count` `u16`
//!   compressed sizes (big-endian), then the entries. Each entry is a big-endian `u16`
//!   unpacked size and a Huffman bit stream decoded with the game's `table.dec`. The three
//!   `u16` header fields of a compressed picture are stored big-endian too; [`read`] turns them
//!   round so every entry it returns has the plain layout.

/// The game's static Huffman decoder (`table.dec`): four levels of 256-entry lookup tables
/// indexed by the next 8 bits of input, each a symbol table followed by a length table. A
/// length with bit 7 set means "skip that many bits and look up again one level down".
pub struct Huffman {
    table: Vec<u8>,
}

impl Huffman {
    pub fn new(table: &[u8]) -> Result<Huffman, String> {
        if table.len() != 2048 {
            return Err(format!("table.dec is {} bytes, expected 2048", table.len()));
        }
        Ok(Huffman { table: table.to_vec() })
    }

    /// Unpack one entry: a big-endian `u16` length, then the bit stream (most significant bit
    /// first).
    pub fn decode(&self, packed: &[u8]) -> Result<Vec<u8>, String> {
        if packed.len() < 2 {
            return Err("compressed entry too short".into());
        }
        let n = (packed[0] as usize) << 8 | packed[1] as usize;
        let bits = &packed[2..];
        let total = bits.len() * 8;
        let peek = |pos: usize| -> usize {
            let mut v = 0usize;
            for k in 0..8 {
                let q = pos + k;
                let b = if q < total { (bits[q >> 3] >> (7 - (q & 7))) & 1 } else { 0 };
                v = v << 1 | b as usize;
            }
            v
        };
        let mut out = Vec::with_capacity(n);
        let mut pos = 0usize;
        while out.len() < n {
            if pos > total + 16 {
                return Err("compressed entry ends early".into());
            }
            let mut level = 0;
            loop {
                if level >= 4 {
                    return Err("bad code in compressed entry".into());
                }
                let i = peek(pos);
                let len = self.table[level * 512 + 256 + i];
                if len & 0x80 != 0 {
                    pos += (len & 0x7f) as usize;
                    level += 1;
                    continue;
                }
                if len == 0 {
                    return Err("bad code in compressed entry".into());
                }
                out.push(self.table[level * 512 + i]);
                pos += len as usize;
                break;
            }
        }
        Ok(out)
    }
}

fn be16(d: &[u8], i: usize) -> usize {
    (d[i] as usize) << 8 | d[i + 1] as usize
}
fn le32(d: &[u8], i: usize) -> usize {
    d[i] as usize | (d[i + 1] as usize) << 8 | (d[i + 2] as usize) << 16 | (d[i + 3] as usize) << 24
}

/// All entries of a library, unpacked, with picture headers in the plain (little-endian)
/// layout.
pub fn read(name: &str, d: &[u8], huff: &Huffman) -> Result<Vec<Vec<u8>>, String> {
    let err = |m: &str| format!("{}: {}", name, m);
    if d.len() < 12 || &d[0..4] != b"TLBR" {
        return Err(err("not a picture library"));
    }
    if d[8] == 0xFF && d[9] == 0xFF {
        let n = be16(d, 10);
        let mut p = 12 + 2 * n;
        if p > d.len() {
            return Err(err("truncated"));
        }
        let mut out = Vec::with_capacity(n);
        for k in 0..n {
            let size = be16(d, 12 + 2 * k);
            let e = d.get(p..p + size).ok_or_else(|| err("truncated"))?;
            let mut u = huff.decode(e).map_err(|m| err(&m))?;
            // compressed pictures keep their u16 header fields big-endian
            if u.len() >= 8 {
                u.swap(0, 1);
                u.swap(2, 3);
                u.swap(4, 5);
            }
            out.push(u);
            p += size;
        }
        return Ok(out);
    }
    let n = d[8] as usize | (d[9] as usize) << 8;
    let base = 10 + 4 * (n + 1);
    if base > d.len() {
        return Err(err("truncated"));
    }
    let offs: Vec<usize> = (0..=n).map(|k| le32(d, 10 + 4 * k)).collect();
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let (a, b) = (base + offs[k], base + offs[k + 1]);
        out.push(d.get(a..b).ok_or_else(|| err("truncated"))?.to_vec());
    }
    Ok(out)
}
