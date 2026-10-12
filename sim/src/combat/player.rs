//! The player: driving model (`r1 $C494` player_move), handling (`r2 $7A69`) and walking
//! on foot (`r1 $C61A`).
use super::motion::{vehicle_velocity, COS16, SIN16};
use crate::rng::Rng;

/// Walk cycles (shape 16 + frame), by 8-way direction E, NE, N, NW, W, SW, S, SE
/// (`r1 $C7E0`): the road uses the E cycle for NE/SE and the W cycle for NW/SW.
const WALK_E: [u8; 4] = [7, 8, 9, 8];
const WALK_N: [u8; 4] = [0, 4, 0, 2];
const WALK_W: [u8; 4] = [10, 5, 6, 5];
const WALK_S: [u8; 4] = [0, 2, 0, 4];
const WALK_CYCLE: [&[u8; 4]; 8] = [&WALK_E, &WALK_E, &WALK_N, &WALK_W, &WALK_W, &WALK_W, &WALK_S, &WALK_E];

/// Per-car constants for the driving model (`r1 $C1F9` init_drive_params).
#[derive(Clone, Copy, Debug)]
pub struct DriveParams {
    /// sub-speed added per frame while accelerating: 10 / 8 / 6 by accel class
    pub accel: i32,
    /// top speed level: 2 / 3 / 4 / 4 by power plant
    pub top: u8,
}

impl DriveParams {
    pub fn for_car(car: &crate::car::Car) -> DriveParams {
        let accel = match car.accel_class() {
            0 => 10,
            1 => 8,
            2 => 6,
            _ => 0, // overloaded: the car cannot move (the city refuses to let it leave)
        };
        DriveParams { accel, top: car.plant_info().top_level }
    }
    /// Braking removes four times the acceleration (`$10F7`).
    pub fn brake(&self) -> i32 {
        self.accel * 4
    }
}

#[derive(Clone, Debug)]
pub struct Player {
    /// world position of the player (the car, or the walker when on foot)
    pub x: i32,
    pub y: i32,
    /// position at the start of this frame (`$0F70`): shots leave from here
    pub frame_origin: (i32, i32),
    /// heading of the car (16 steps, 0 = east, counter-clockwise), `$0FF2`
    pub heading: u8,
    /// heading chosen this frame, committed by [`Player::update_handling`] (`$0FF1`)
    pub new_heading: u8,
    /// speed level 0..top (`$0EF4`) and sub-speed 0..31 (`$0EF6`)
    pub speed: u8,
    pub sub: u8,
    /// speed at the start of the frame (`$0EF5`): "player stopped" for to-hit, crash damage
    pub speed0: u8,
    /// reversing at speed 1 (`$0EF7`)
    pub reverse: bool,
    /// current handling H and its base H0 (`$10F0`, `$1077`)
    pub handling: i32,
    pub handling_base: i32,
    /// loss of control flags for the next frame (`$10F4` skid, `$10F5` spin)
    pub skid: bool,
    pub spin: bool,
    /// out of the car (`$67` bit 4)
    pub on_foot: bool,
    /// there is a car to get back into
    pub has_car: bool,
    /// where the car was left (`$0E70`)
    pub car_x: i32,
    pub car_y: i32,
    /// walker animation frame 0..10 (shape 16 + frame), `$0F74`
    pub walk_frame: u8,
    walk_step: usize,
    walk_dir: u8,
    /// frames the player has been standing still (`$1072`; enemies attack after 4)
    pub stopped_frames: u32,
    /// velocity attempted this frame (power use and the odometer count it)
    pub vx: i32,
    pub vy: i32,
    /// touched a cloud this frame (`$10F3` smoke, `$1074` paint, `$1073` oil)
    pub in_smoke: bool,
    pub in_paint: bool,
    pub in_oil: bool,
}

impl Player {
    pub fn new(x: i32, y: i32, heading: u8, handling_base: i32, has_car: bool) -> Player {
        Player {
            x,
            y,
            frame_origin: (x, y),
            heading,
            new_heading: heading,
            speed: 0,
            sub: 0,
            speed0: 0,
            reverse: false,
            handling: handling_base,
            handling_base,
            skid: false,
            spin: false,
            on_foot: !has_car,
            has_car,
            car_x: x,
            car_y: y,
            walk_frame: 0,
            walk_step: 0,
            walk_dir: 8,
            stopped_frames: 0,
            vx: 0,
            vy: 0,
            in_smoke: false,
            in_paint: false,
            in_oil: false,
        }
    }

    /// Throttle and steering for one frame. `stick` is the joystick heading (None when
    /// centred or when the car has no power). Sets `new_heading`, the speed and returns
    /// the velocity to try.
    pub fn drive(&mut self, stick: Option<u8>, p: DriveParams, rng: &mut Rng) -> (i32, i32) {
        let s0 = self.speed;
        self.new_heading = self.heading;
        // loss of control: spin turns every frame, skid about one frame in four
        let random_turn = self.spin || (self.skid && rng.upto(3) == 0);
        if random_turn {
            let d: i32 = if rng.coin() { 1 } else { -1 };
            self.new_heading = ((self.heading as i32 + d) & 15) as u8;
            if stick.is_some() {
                self.accelerate(s0, p);
            } else {
                self.coast(s0);
            }
        } else if let Some(want) = stick {
            let diff = (want as i32 - self.heading as i32) & 15;
            match diff {
                0 => self.accelerate(s0, p),
                1..=6 => {
                    self.new_heading = (self.heading + 1) & 15;
                    self.accelerate(s0, p);
                }
                7..=9 => self.brake_or_reverse(s0, p),
                _ => {
                    self.new_heading = (self.heading + 15) & 15;
                    self.accelerate(s0, p);
                }
            }
        } else {
            self.coast(s0);
        }
        let dir = if self.reverse { (self.new_heading + 8) & 15 } else { self.new_heading };
        vehicle_velocity(dir, self.speed)
    }

    fn accelerate(&mut self, s0: u8, p: DriveParams) {
        self.reverse = false;
        let sub = self.sub as i32 + p.accel;
        if sub < 32 {
            self.sub = sub as u8;
            return;
        }
        self.sub = (sub & 31) as u8;
        if s0 + 1 > p.top {
            // at top speed the sub-speed stays full (`r1 $CB34` clamp_speed)
            self.sub = 31;
            self.speed = p.top;
        } else {
            self.speed = s0 + 1;
        }
    }

    fn brake_or_reverse(&mut self, s0: u8, p: DriveParams) {
        if self.reverse {
            return; // keep backing up at speed 1
        }
        let sub = self.sub as i32 - p.brake();
        if sub >= 0 {
            self.sub = sub as u8;
            return;
        }
        self.sub = (sub & 31) as u8;
        if s0 == 0 {
            self.sub = 0;
            self.reverse = true;
            self.speed = 1;
        } else {
            self.speed = s0 - 1;
        }
    }

    fn coast(&mut self, s0: u8) {
        self.new_heading = self.heading;
        if self.reverse {
            self.reverse = false;
            self.speed = 0;
            return;
        }
        let sub = self.sub as i32 - 16;
        if sub >= 0 {
            self.sub = sub as u8;
            return;
        }
        self.sub = (sub & 31) as u8;
        if s0 == 0 {
            self.sub = 0;
            self.speed = 0;
        } else {
            self.speed = s0 - 1;
        }
    }

    /// Walking: 8 directions, 1 unit per axis per frame, with the E/W walk cycles.
    pub fn walk(&mut self, stick: Option<u8>) -> (i32, i32) {
        let Some(h) = stick else { return (0, 0) };
        let dir = h >> 1;
        if dir != self.walk_dir {
            self.walk_step = 0;
        }
        self.walk_dir = dir;
        self.walk_frame = WALK_CYCLE[dir as usize][self.walk_step % 4];
        self.walk_step = (self.walk_step + 1) % 4;
        let h = (dir * 2) as usize;
        (COS16[h] >> 1, -(SIN16[h] >> 1))
    }

    /// The car hit something: it stops dead.
    pub fn crash(&mut self) {
        self.speed = 0;
        self.sub = 0;
    }

    /// Handling for this frame (`r2 $7A69`), then commit the new heading. Turning costs
    /// the speed level, going straight recovers 4 up to H0, oil costs 20. H < 10 spins,
    /// H < 20 skids (both act on the next frame).
    pub fn update_handling(&mut self, stick_pushed: bool) {
        if stick_pushed && self.new_heading != self.heading {
            self.handling = (self.handling - self.speed as i32).max(0);
        } else if self.handling < self.handling_base {
            self.handling = (self.handling + 4).min(self.handling_base);
        }
        if self.in_oil {
            self.handling = (self.handling - 20).max(0);
        }
        self.spin = self.handling < 10;
        self.skid = !self.spin && self.handling < 20;
        if !self.on_foot {
            self.heading = self.new_heading;
        }
    }

    /// A tire burst: -50 handling, now and as the base (`r2 $75B5`).
    pub fn tire_blown(&mut self) {
        self.handling = (self.handling - 50).max(0);
        self.handling_base = (self.handling_base - 50).max(0);
    }

    /// Leave the car where it stands (`r1 $C18A`).
    pub fn get_out(&mut self) {
        self.on_foot = true;
        self.car_x = self.x;
        self.car_y = self.y;
        self.speed = 0;
        self.speed0 = 0;
        self.sub = 0;
        self.reverse = false;
        self.vx = 0;
        self.vy = 0;
    }

    /// Get back in if the car is within 5 units (`r1 $C1AD`).
    pub fn try_get_in(&mut self) -> bool {
        if !self.has_car || (self.car_x - self.x).abs() >= 6 || (self.car_y - self.y).abs() >= 6 {
            return false;
        }
        self.on_foot = false;
        self.x = self.car_x;
        self.y = self.car_y;
        true
    }

    /// Speed bar value 0..9: speed level * 2 + sub-speed / 16 (`r1 $CEF9`).
    pub fn speed_bar(&self) -> u8 {
        if self.on_foot { 0 } else { self.speed * 2 + self.sub / 16 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> DriveParams {
        DriveParams { accel: 10, top: 3 }
    }

    #[test]
    fn accelerate_brake_reverse_coast() {
        let mut rng = Rng::new(1);
        let mut p = Player::new(0, 0, 0, 100, true);
        // 32 / 10 -> +1 level every 3.2 frames, clamped at top
        for _ in 0..4 {
            p.drive(Some(0), params(), &mut rng);
            p.update_handling(true);
        }
        assert_eq!(p.speed, 1);
        for _ in 0..20 {
            p.drive(Some(0), params(), &mut rng);
            p.update_handling(true);
        }
        assert_eq!(p.speed, 3);
        assert_eq!(vehicle_velocity(0, p.speed), (3, 0));
        // brake: about 1.25 levels per frame, then reverse at speed 1
        for _ in 0..6 {
            p.drive(Some(8), params(), &mut rng);
            p.update_handling(true);
        }
        assert!(p.reverse);
        assert_eq!(p.speed, 1);
        assert_eq!(p.drive(Some(8), params(), &mut rng), (-1, 0));
        // centring the stick stops a reversing car at once
        p.drive(None, params(), &mut rng);
        assert_eq!((p.speed, p.reverse), (0, false));
    }

    #[test]
    fn steering_turns_one_step_per_frame() {
        let mut rng = Rng::new(1);
        let mut p = Player::new(0, 0, 0, 100, true);
        for i in 1..=4 {
            p.drive(Some(4), params(), &mut rng);
            p.update_handling(true);
            assert_eq!(p.heading, i);
        }
        // clockwise when the stick is on the right-hand side
        p.drive(Some(0), params(), &mut rng);
        p.update_handling(true);
        assert_eq!(p.heading, 3);
    }

    #[test]
    fn handling_drops_while_turning_fast() {
        let mut rng = Rng::new(7);
        let mut p = Player::new(0, 0, 0, 30, true);
        p.speed = 4;
        p.sub = 31;
        let mut skidded = false;
        for i in 0..40 {
            let want = if i % 2 == 0 { 4 } else { 12 };
            p.drive(Some(want), DriveParams { accel: 10, top: 4 }, &mut rng);
            p.update_handling(true);
            skidded |= p.skid || p.spin;
        }
        assert!(skidded);
        // standing still restores it (done by the battle at speed 0); going straight recovers +4
        let h = p.handling;
        p.new_heading = p.heading;
        p.update_handling(false);
        assert_eq!(p.handling, (h + 4).min(30));
    }

    #[test]
    fn walking_cycles() {
        let mut p = Player::new(0, 0, 0, 100, true);
        p.get_out();
        assert_eq!(p.walk(Some(0)), (1, 0));
        assert_eq!(p.walk_frame, 7);
        assert_eq!(p.walk(Some(0)), (1, 0));
        assert_eq!(p.walk_frame, 8);
        assert_eq!(p.walk(Some(6)), (-1, -1));
        assert_eq!(p.walk_frame, 10);
        assert_eq!(p.walk(Some(12)), (0, 1));
        assert_eq!(p.walk(None), (0, 0));
        p.x = 3;
        assert!(p.try_get_in());
        assert_eq!((p.x, p.on_foot), (0, false));
    }
}
