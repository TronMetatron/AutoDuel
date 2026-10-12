//! The tile map of a road or the arena, and the solid scenery on it.
//!
//! A map is `w x h` tiles of 128 x 64 world units. Tile (col, row) holds up to 32 scenery
//! objects with offsets x -62..61, y -32..30 around its grid corner (col * 128, row * 64),
//! so a tile's content is a 128 x 64 cell centred on that corner. Every object is solid
//! with the box +-(xrad, yrad) from `data.scenery` (`MODSHAPS $EA5C/$EA73`).
use crate::data::{GameData, Scenery, TileObject};

pub const TILE_W: i32 = 128;
pub const TILE_H: i32 = 64;

/// A placed scenery object in world coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Obstacle {
    pub shape: u8,
    pub x: i32,
    pub y: i32,
}

/// Borrowed view of a map (the battle owns no world data; it is passed in each frame).
pub struct MapView<'a> {
    pub w: i32,
    pub h: i32,
    tiles: &'a [Vec<u8>],
    set: &'a [Vec<TileObject>],
    scenery: &'a [Scenery],
}

impl<'a> MapView<'a> {
    pub fn road(data: &'a GameData, road: u8) -> MapView<'a> {
        let r = &data.roads[road as usize];
        MapView { w: r.w, h: r.h, tiles: &r.tiles, set: &data.road_tiles, scenery: &data.scenery }
    }
    pub fn arena(data: &'a GameData) -> MapView<'a> {
        let a = &data.arena;
        MapView { w: a.w, h: a.h, tiles: &a.tiles, set: &data.arena_tiles, scenery: &data.scenery }
    }

    /// Objects of tile (col, row), or nothing outside the map.
    pub fn tile_objects(&self, col: i32, row: i32) -> &'a [TileObject] {
        if col < 0 || row < 0 || col >= self.w || row >= self.h {
            return &[];
        }
        let Some(t) = self.tiles.get(row as usize).and_then(|r| r.get(col as usize)) else { return &[] };
        self.set.get(*t as usize).map(|v| v.as_slice()).unwrap_or(&[])
    }

    /// The tile whose content cell contains (x, y).
    pub fn tile_at(x: i32, y: i32) -> (i32, i32) {
        ((x + TILE_W / 2).div_euclid(TILE_W), (y + TILE_H / 2).div_euclid(TILE_H))
    }

    /// All objects of the tiles whose content may overlap the rectangle x0..=x1, y0..=y1.
    pub fn objects_in(&self, x0: i32, y0: i32, x1: i32, y1: i32) -> Vec<Obstacle> {
        // objects stick out of their cell by at most a few units; one extra tile covers it
        let (c0, r0) = Self::tile_at(x0, y0);
        let (c1, r1) = Self::tile_at(x1, y1);
        let mut out = vec![];
        for row in (r0 - 1)..=(r1 + 1) {
            for col in (c0 - 1)..=(c1 + 1) {
                for o in self.tile_objects(col, row) {
                    out.push(Obstacle { shape: o.shape, x: col * TILE_W + o.x, y: row * TILE_H + o.y });
                }
            }
        }
        out
    }

    /// Collision half-size of a scenery shape.
    pub fn radius(&self, shape: u8) -> (i32, i32) {
        self.scenery.get(shape as usize).map(|s| (s.xrad, s.yrad)).unwrap_or((0, 0))
    }

    /// Is the point (x, y) inside any object's box (`r2 $84C7`, `|dx| <= xrad && |dy| <= yrad`)?
    pub fn blocked(&self, x: i32, y: i32) -> bool {
        let (c, r) = Self::tile_at(x, y);
        for row in (r - 1)..=(r + 1) {
            for col in (c - 1)..=(c + 1) {
                for o in self.tile_objects(col, row) {
                    let (xr, yr) = self.radius(o.shape);
                    if (x - (col * TILE_W + o.x)).abs() <= xr && (y - (row * TILE_H + o.y)).abs() <= yr {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// The player may move inside x 0..(w-1)*128, y 0..(h-1)*64 (`r1 $C6D4`).
    pub fn player_x_ok(&self, x: i32) -> bool {
        x >= 0 && x < (self.w - 1) * TILE_W
    }
    pub fn player_y_ok(&self, y: i32) -> bool {
        y >= 0 && y < (self.h - 1) * TILE_H
    }
    /// Enemies and projectiles may move inside x -64..w*128+64, y -32..h*64+32 (`r3 $1634`).
    pub fn object_x_ok(&self, x: i32) -> bool {
        x >= -TILE_W / 2 && x < self.w * TILE_W + TILE_W / 2
    }
    pub fn object_y_ok(&self, y: i32) -> bool {
        y >= -TILE_H / 2 && y < self.h * TILE_H + TILE_H / 2
    }
}

/// The window around the player's tile in which enemies think and move (`r2 $8940`):
/// x in [tileX - 64, tileX + 192), y in [tileY - 32, tileY + 96), tileX = px & !127,
/// tileY = py & !63.
pub fn in_active_window(player: (i32, i32), x: i32, y: i32) -> bool {
    let tx = player.0 & !(TILE_W - 1);
    let ty = player.1 & !(TILE_H - 1);
    x >= tx - 64 && x < tx + 192 && y >= ty - 32 && y < ty + 96
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenery_is_solid_and_the_start_points_are_free() {
        let d = crate::need_data!();
        for (i, r) in d.roads.iter().enumerate() {
            let m = MapView::road(&d, i as u8);
            for e in &r.exits {
                assert!(!m.blocked(e.start_x, e.start_y), "road {} start {:?} blocked", i, (e.start_x, e.start_y));
            }
            // somewhere on the map there is an obstacle we can hit
            let mut any = false;
            for row in 0..r.h {
                for col in 0..r.w {
                    for o in m.tile_objects(col, row) {
                        let (x, y) = (col * TILE_W + o.x, row * TILE_H + o.y);
                        assert!(m.blocked(x, y));
                        any = true;
                    }
                }
            }
            assert!(any);
        }
        let m = MapView::arena(&d);
        let e = &d.arena.events[1].exits[1];
        assert!(!m.blocked(e.start_x, e.start_y));
    }
}
