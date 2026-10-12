//! Directions, velocities and bearings shared by every moving thing in the driving engine.
//!
//! Headings are 16-step compass directions, counter-clockwise: 0 = east, 4 = north,
//! 8 = west, 12 = south. World y grows downwards (south).

/// `r1 $C7CF`: x component per heading (scaled so that 2 = "full").
pub const COS16: [i32; 16] = [2, 2, 2, 1, 0, -1, -2, -2, -2, -2, -2, -1, 0, 1, 2, 2];
/// `r1 $C7BE`: y component per heading (north positive; negate for screen y).
pub const SIN16: [i32; 16] = [0, 1, 2, 2, 2, 2, 2, 1, 0, -1, -2, -2, -2, -2, -2, -1];

/// Heading offset of each weapon mount (`r2 $7332`): front +0, back +8, left +4, right +12.
pub const MOUNT_OFFSET: [u8; 4] = [0, 8, 4, 12];

/// 16 headings -> the 8-direction frame order N, NE, E, SE, S, SW, W, NW (`r2 $733D`).
/// Used for rockets, flames and (on even headings) enemy cars.
pub const DIR16_TO_FRAME8: [u8; 16] = [2, 1, 1, 1, 0, 7, 7, 7, 6, 5, 5, 5, 4, 3, 3, 3];

/// Bearing on the 3 x 3 grid around a point (`r2 $8BEF`), index = x + 3 * y with
/// x 0 west / 1 centre / 2 east and y 0 north / 1 centre / 2 south. 16 = centred.
pub const GRID_BEARING: [u8; 9] = [6, 4, 2, 8, 16, 0, 10, 12, 14];

/// Vehicle velocity for one frame (`r1 $C5C6`, `r3 $14BE`): `cos16 * speed` and
/// `-sin16 * speed`, each halved (arithmetic shift, rounding down) unless it is +1.
/// This reproduces the original's exact table (see the unit test).
pub fn vehicle_velocity(heading: u8, speed: u8) -> (i32, i32) {
    let h = (heading & 15) as usize;
    let s = speed as i32;
    let half = |v: i32| if v == 1 { 1 } else { v >> 1 };
    (half(COS16[h] * s), half(-SIN16[h] * s))
}

/// Projectile step (`r2 $7BC4`): `cos16 * mult`, `-sin16 * mult`, not halved.
/// Traces use mult 1 (up to 2 units per step), rockets and droppers mult 3.
pub fn step_vector(heading: u8, mult: i32) -> (i32, i32) {
    let h = (heading & 15) as usize;
    (COS16[h] * mult, -SIN16[h] * mult)
}

/// Turn a joystick deflection into a 16-step heading (`r1 $CC0C`): right 0, up 4,
/// left 8, down 12, diagonals in between. None when centred.
pub fn stick_heading(dx: i8, dy: i8) -> Option<u8> {
    Some(match (dx.signum(), dy.signum()) {
        (1, 0) => 0,
        (1, -1) => 2,
        (0, -1) => 4,
        (-1, -1) => 6,
        (-1, 0) => 8,
        (-1, 1) => 10,
        (0, 1) => 12,
        (1, 1) => 14,
        _ => return None,
    })
}

/// Bearing (16-step, even) from `from` to `to` on the original's 3 x 3 grid: an axis
/// counts as "centred" when the difference is within `dz` units. None when both are.
pub fn grid_bearing(from: (i32, i32), to: (i32, i32), dz: i32) -> Option<u8> {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    let gx = if dx > dz { 2 } else if dx < -dz { 0 } else { 1 };
    let gy = if dy > dz { 2 } else if dy < -dz { 0 } else { 1 };
    let b = GRID_BEARING[gx + 3 * gy];
    if b == 16 { None } else { Some(b) }
}

/// Signed heading difference `to - from`, wrapped to -8..=7.
pub fn heading_diff(from: u8, to: u8) -> i32 {
    let d = (to as i32 - from as i32) & 15;
    if d >= 8 { d - 16 } else { d }
}

/// Screen frame of the player car for a heading: `(4 - d) & 15` (`r1 $C484`).
pub fn car_frame(heading: u8) -> usize {
    ((4 - heading as i32) & 15) as usize
}

/// Enemy body frame for an (even) heading: `(2 - d/2) & 7` (`r2 $8772`).
pub fn enemy_frame(heading: u8) -> usize {
    ((2 - (heading as i32 >> 1)) & 7) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The velocity table from the spec (§4), speeds 1..4 by heading 0..15.
    const TABLE: [[(i32, i32); 16]; 4] = [
        [(1, 0), (1, -1), (1, -1), (1, -1), (0, -1), (-1, -1), (-1, -1), (-1, -1), (-1, 0), (-1, 1), (-1, 1), (-1, 1), (0, 1), (1, 1), (1, 1), (1, 1)],
        [(2, 0), (2, -1), (2, -2), (1, -2), (0, -2), (-1, -2), (-2, -2), (-2, -1), (-2, 0), (-2, 1), (-2, 2), (-1, 2), (0, 2), (1, 2), (2, 2), (2, 1)],
        [(3, 0), (3, -2), (3, -3), (1, -3), (0, -3), (-2, -3), (-3, -3), (-3, -2), (-3, 0), (-3, 1), (-3, 3), (-2, 3), (0, 3), (1, 3), (3, 3), (3, 1)],
        [(4, 0), (4, -2), (4, -4), (2, -4), (0, -4), (-2, -4), (-4, -4), (-4, -2), (-4, 0), (-4, 2), (-4, 4), (-2, 4), (0, 4), (2, 4), (4, 4), (4, 2)],
    ];

    #[test]
    fn velocity_table_matches_the_original() {
        for s in 1..=4u8 {
            for h in 0..16u8 {
                assert_eq!(vehicle_velocity(h, s), TABLE[s as usize - 1][h as usize], "speed {} heading {}", s, h);
            }
        }
        for h in 0..16 {
            assert_eq!(vehicle_velocity(h, 0), (0, 0));
        }
    }

    #[test]
    fn frames_and_bearings() {
        assert_eq!(car_frame(0), 4); // east
        assert_eq!(car_frame(4), 0); // north
        assert_eq!(car_frame(5), 15);
        assert_eq!(enemy_frame(0), 2);
        assert_eq!(enemy_frame(4), 0);
        assert_eq!(enemy_frame(14), 3);
        assert_eq!(grid_bearing((0, 0), (10, 0), 3), Some(0));
        assert_eq!(grid_bearing((0, 0), (0, -10), 3), Some(4));
        assert_eq!(grid_bearing((0, 0), (-10, 10), 3), Some(10));
        assert_eq!(grid_bearing((0, 0), (1, 1), 3), None);
        assert_eq!(stick_heading(1, -1), Some(2));
        assert_eq!(stick_heading(0, 0), None);
        assert_eq!(heading_diff(14, 2), 4);
        assert_eq!(heading_diff(2, 14), -4);
    }
}
