use mod_api::{BoatControl, BoatMedium, BoatPhysics};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub struct Settings {
    active: bool,
    pub keep_world: bool,
    drag: Option<f32>,
    friction: BTreeMap<String, f32>,
    overrides: BTreeMap<(String, i16), f32>,
    cleared: bool,
    step: f32,
    gravity: Option<f64>,
    values: [Option<f32>; 13],
    air: bool,
    underwater: bool,
    surface: bool,
    water_jump: bool,
    stacking: bool,
    falling_step: bool,
    swim: f32,
    nocollide: bool,
}

#[derive(Default)]
pub struct Ride {
    entity: Option<u32>,
    jumps: i32,
    grace: i32,
    held: bool,
}

fn take<const N: usize>(b: &mut &[u8]) -> Option<[u8; N]> {
    let (n, rest) = b.split_at_checked(N)?;
    *b = rest;
    n.try_into().ok()
}
fn short(b: &mut &[u8]) -> Option<i16> {
    Some(i16::from_be_bytes(take(b)?))
}
fn int(b: &mut &[u8]) -> Option<i32> {
    Some(i32::from_be_bytes(take(b)?))
}
fn float(b: &mut &[u8]) -> Option<f32> {
    let f = f32::from_be_bytes(take(b)?);
    f.is_finite().then_some(f)
}
fn flag(b: &mut &[u8]) -> Option<bool> {
    Some(take::<1>(b)?[0] != 0)
}
pub fn string<'a>(b: &mut &'a [u8]) -> Option<&'a str> {
    let mut len = 0usize;
    for shift in [0, 7, 14] {
        let c = take::<1>(b)?[0];
        len |= usize::from(c & 127) << shift;
        if c < 128 {
            let (s, rest) = b.split_at_checked(len)?;
            *b = rest;
            return std::str::from_utf8(s).ok();
        }
    }
    None
}
fn key(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

impl Settings {
    fn mode(&mut self, mode: i16) -> Option<()> {
        match mode {
            0..=15 | 22..=23 => {
                let n = if mode >= 22 { mode - 8 } else { mode % 8 };
                let drag = if matches!(n, 1 | 4 | 5 | 7) {
                    0.989
                } else {
                    0.98
                };
                self.air = true;
                self.step = if matches!(n, 3 | 5) { 0.5 } else { 1.25 };
                if matches!(n, 0 | 1 | 3 | 5) {
                    self.drag = Some(drag)
                } else {
                    self.friction.insert(key("air"), drag);
                }
                if matches!(n, 3 | 5) {
                    self.values[0] = Some(0.36)
                }
                if mode >= 22 {
                    self.falling_step = true
                }
                if mode < 8 {
                    self.friction.insert(key("slime_block"), -1.);
                }
            }
            16 => {
                for (name, v) in [
                    ("orange_concrete", 0.36),
                    ("black_concrete", 0.),
                    ("green_concrete", 0.5),
                    ("yellow_concrete", 0.18),
                ] {
                    self.overrides.insert((key(name), 0), v);
                }
            }
            17 => {
                for (name, i) in [
                    ("magenta_glazed_terracotta", 1),
                    ("light_gray_glazed_terracotta", 3),
                ] {
                    self.overrides.insert((key(name), i), 0.08);
                }
            }
            18 | 19 | 24 => {
                self.drag = Some(match mode {
                    18 => 0.98,
                    19 => 0.985,
                    _ => 0.989,
                })
            }
            20 | 21 => self.nocollide = mode == 21,
            _ => return None,
        }
        Some(())
    }
    pub fn packet(&mut self, b: &mut &[u8], depth: u8, version: &mut bool) -> Option<()> {
        if depth > 8 {
            return None;
        }
        let id = short(b)?;
        if !matches!(id, 0 | 15) {
            self.active = true;
        }
        match id {
            0 => *self = Self::default(),
            1 => self.step = float(b)?,
            2 => self.drag = Some(float(b)?),
            3 | 22 | 26 => {
                let index = if id == 26 { short(b)? } else { -1 };
                let v = if id != 22 { float(b)? } else { 0. };
                for name in string(b)?.split(',') {
                    let name = key(name);
                    if name.len() > 256 || self.overrides.len() + self.friction.len() >= 512 {
                        return None;
                    }
                    match id {
                        3 => {
                            self.friction.insert(name, v);
                        }
                        22 => {
                            self.friction.insert(name, -1.);
                        }
                        _ => {
                            self.overrides.insert((name, index), v);
                        }
                    }
                }
            }
            6 => self.air = flag(b)?,
            7 | 10..=13 | 34 | 37 | 40 | 41 | 45 | 46 => {
                let i = match id {
                    7 => 0,
                    10 => 3,
                    11 => 1,
                    12 => 2,
                    13 => 4,
                    34 => 5,
                    37 => 8,
                    40 => 9,
                    41 => 10,
                    45 => 11,
                    _ => 12,
                };
                self.values[i] = Some(float(b)?);
            }
            8 | 18 | 24 | 25 => {
                if matches!(id, 18 | 25) {
                    *self = Self {
                        active: true,
                        ..Default::default()
                    };
                }
                let n = if matches!(id, 24 | 25) { short(b)? } else { 1 };
                if !(0..=4096).contains(&n) {
                    return None;
                }
                for _ in 0..n {
                    self.mode(short(b)?)?;
                }
            }
            9 => {
                let f = f64::from_be_bytes(take(b)?);
                if !f.is_finite() {
                    return None;
                }
                self.gravity = Some(-f);
            }
            14 => self.stacking = flag(b)?,
            15 => *version = true,
            16 => self.underwater = flag(b)?,
            17 => self.surface = flag(b)?,
            19 | 35 => self.values[if id == 19 { 7 } else { 6 }] = Some(int(b)? as f32),
            20 => self.water_jump = flag(b)?,
            21 => self.swim = float(b)?,
            23 => {
                self.friction.clear();
                self.cleared = true;
            }
            27 => self.nocollide = short(b)? == 2,
            28 => self.falling_step = flag(b)?,
            33 => {
                let count = int(b)?;
                if !(0..=4096).contains(&count) {
                    return None;
                }
                for _ in 0..count {
                    self.packet(b, depth + 1, version)?;
                }
            }
            // Consume fields which the Nova boat hook cannot apply so the next
            // setting in a compound packet remains correctly aligned.
            38 => self.keep_world = !flag(b)?,
            4 | 5 | 29 | 30 | 39 | 44 | 47 => {
                take::<1>(b)?;
            }
            31 => {
                string(b)?;
            }
            32 => {}
            36 => {
                float(b)?;
            }
            42 | 43 => {
                take::<24>(b)?;
            }
            _ => return None,
        }
        Some(())
    }

    pub fn needs_block(&self) -> bool {
        self.active && (!self.overrides.is_empty() || !self.friction.is_empty())
    }

    pub fn apply(&self, c: &BoatControl, p: &mut BoatPhysics, r: &mut Ride, block: &str) {
        if !self.active {
            *r = Ride::default();
            return;
        }
        if r.entity != Some(c.entity) {
            *r = Ride {
                entity: Some(c.entity),
                ..Default::default()
            };
        }
        let value = |i: usize, base: f32| {
            self.overrides
                .get(&(block.to_owned(), i as i16))
                .copied()
                .or(self.values[i])
                .unwrap_or(base)
        };
        let land = c.medium == BoatMedium::Land;
        let water = c.medium == BoatMedium::Water;
        let submerged = matches!(
            c.medium,
            BoatMedium::UnderWater | BoatMedium::UnderFlowingWater
        );
        if land
            || (self.air && c.medium == BoatMedium::Air)
            || (water && self.surface)
            || (submerged && self.underwater)
        {
            let vanilla = if land { p.drag } else { 0.6 };
            // Nova sums f32 values under the hull. Six ordinary blocks
            // average one ULP below the friction of a single block.
            let replace = self.cleared || (vanilla - 0.6).abs() <= f32::EPSILON;
            let base = if replace {
                self.drag.unwrap_or(0.6)
            } else {
                vanilla
            };
            p.drag = self
                .friction
                .get(block)
                .map(|v| {
                    if *v < 0. {
                        self.drag.unwrap_or(0.6)
                    } else {
                        *v
                    }
                })
                .unwrap_or(base);
        }
        if let Some(g) = self.gravity
            && c.medium != BoatMedium::UnderFlowingWater
        {
            p.gravity = g;
        }
        if c.on_ground || land || self.falling_step {
            p.step_height = self.step;
        }
        p.forward = value(1, p.forward);
        p.backward = value(2, p.backward);
        p.turn = value(3, p.turn);
        p.paddle_turn = value(4, p.paddle_turn);
        let [left, right, up, down] = c.paddles;
        if self.stacking && left != right && (up || down) {
            if up {
                p.forward += p.paddle_turn;
            } else {
                p.backward -= p.paddle_turn;
            }
        }
        let limit = value(11, -1.);
        if limit >= 0. {
            let speed = c.velocity[0].hypot(c.velocity[2]) as f32 * p.drag;
            p.drag *= (1.0 - (speed - limit).max(0.) * value(12, 0.)).clamp(0., 1.);
        }
        if down {
            p.drag *= value(10, 1.);
        }
        if self.nocollide {
            p.collide_entities = false;
        }
        if land || (water && self.water_jump) {
            r.jumps = value(6, 1.).round() as i32;
            r.grace = value(7, 0.).round() as i32;
            r.held = false;
        } else {
            r.grace = r.grace.saturating_sub(1);
            if r.grace == -1 {
                r.jumps = r.jumps.saturating_sub(1);
            }
        }
        if !c.jump {
            r.held = false;
        }
        if submerged && c.jump {
            p.vertical_impulse += f64::from(self.swim);
        }
        let jump = value(0, 0.);
        if c.jump && !r.held && r.jumps > 0 && jump != 0. {
            p.vertical_impulse = f64::from(jump) - c.velocity[1];
            r.held = true;
            r.jumps -= 1;
            r.grace = -1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &mut Settings, mut body: &[u8]) {
        s.packet(&mut body, 0, &mut false).unwrap();
        assert!(body.is_empty());
    }

    #[test]
    fn rally_applies_across_a_flat_dirt_lane() {
        let mut settings = Settings::default();
        parse(&mut settings, &[0, 18, 0, 8]);
        let control = BoatControl {
            medium: BoatMedium::Land,
            ..Default::default()
        };
        for contacts in [1, 2, 4, 6, 9] {
            let average = (0..contacts).map(|_| 0.6_f32).sum::<f32>() / contacts as f32;
            let mut physics = BoatPhysics {
                drag: average,
                ..Default::default()
            };
            settings.apply(
                &control,
                &mut physics,
                &mut Ride::default(),
                "minecraft:dirt",
            );
            assert_eq!(physics.drag, 0.98, "{contacts} dirt blocks under hull");
        }
    }

    #[test]
    fn gravity_on_wire_is_signed_vertical_acceleration() {
        let mut s = Settings::default();
        let mut body = vec![0, 9];
        body.extend((-0.0125f64).to_be_bytes());
        parse(&mut s, &body);
        let mut physics = BoatPhysics::default();
        s.apply(
            &BoatControl::default(),
            &mut physics,
            &mut Ride::default(),
            "minecraft:air",
        );
        assert_eq!(physics.gravity, 0.0125);
    }

    #[test]
    fn parkour_jump_and_per_block_booster() {
        let mut s = Settings::default();
        parse(&mut s, &[0, 25, 0, 2, 0, 11, 0, 17]);
        let c = BoatControl {
            entity: 7,
            medium: BoatMedium::Land,
            on_ground: true,
            jump: true,
            ..BoatControl::default()
        };
        let mut p = BoatPhysics {
            drag: 0.6,
            ..Default::default()
        };
        let mut ride = Ride::default();
        s.apply(&c, &mut p, &mut ride, "minecraft:magenta_glazed_terracotta");
        assert_eq!(p.forward, 0.08);
        assert_eq!(p.step_height, 0.5);
        assert_eq!(p.vertical_impulse, f64::from(0.36f32));
        let mut air = BoatPhysics::default();
        s.apply(
            &BoatControl {
                medium: BoatMedium::Air,
                on_ground: false,
                ..c
            },
            &mut air,
            &mut ride,
            "minecraft:air",
        );
        assert_eq!(air.vertical_impulse, 0.0);
        assert_eq!(air.drag, 0.98);
    }
}
