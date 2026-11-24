use crate::{
    CONTEXT,
    settings::{Settings, string},
};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Session {
    current: Settings,
    selected: Option<String>,
    stored: BTreeMap<String, Settings>,
}
impl Session {
    pub fn current(&self) -> &Settings {
        &self.current
    }

    /// Exercise the same play payloads as a track plugin in a fresh local session.
    pub fn test_profile(&mut self, profile: &str) -> Option<&'static str> {
        let mut settings = Session::default();
        let mut version = false;
        let message = match profile {
            "reset" => {
                settings.receive(crate::SETTINGS, &[0, 0], &mut version)?;
                "OpenBoatUtils test reset. Vanilla boat physics."
            }
            "rally" => {
                settings.receive(crate::SETTINGS, &[0, 18, 0, 8], &mut version)?;
                "OpenBoatUtils Rally active. Ordinary ground slipperiness is 0.98."
            }
            "dirt" => {
                let block = b"minecraft:dirt";
                let mut packet = vec![0, 3];
                packet.extend(0.98_f32.to_be_bytes());
                packet.push(block.len() as u8);
                packet.extend(block);
                settings.receive(crate::SETTINGS, &packet, &mut version)?;
                "OpenBoatUtils dirt test active. Dirt slipperiness is 0.98."
            }
            _ => return None,
        };
        *self = settings;
        Some(message)
    }

    pub fn receive(&mut self, channel: &str, mut bytes: &[u8], version: &mut bool) -> Option<()> {
        if bytes.len() > 65536 {
            return None;
        }
        if channel != CONTEXT {
            let mut settings = self.current.clone();
            settings.packet(&mut bytes, 0, version)?;
            if !bytes.is_empty() {
                return None;
            }
            self.current = settings;
            if let Some(name) = &self.selected {
                self.stored.insert(name.clone(), self.current.clone());
            }
            return Some(());
        }
        let (opcode, rest) = bytes.split_at_checked(2)?;
        bytes = rest;
        match opcode {
            [0, 0] if bytes.is_empty() => {
                self.selected = None;
                self.current = Settings::default();
            }
            [0, kind @ (1 | 2 | 3)] => {
                let mut name = string(&mut bytes)?.to_owned();
                if name.len() > 256 {
                    return None;
                }
                if !name.contains(':') {
                    name = format!("minecraft:{name}");
                }
                if *kind == 3 {
                    if self.stored.len() >= 32 && !self.stored.contains_key(&name) {
                        return None;
                    }
                    let mut body = vec![0, 33];
                    body.extend_from_slice(bytes);
                    let mut slice = body.as_slice();
                    let mut settings = Settings::default();
                    settings.packet(&mut slice, 0, version)?;
                    if !slice.is_empty() {
                        return None;
                    }
                    self.stored.insert(name, settings);
                } else if !bytes.is_empty() {
                    return None;
                } else if *kind == 2 {
                    self.stored.remove(&name);
                    if self.selected.as_ref() == Some(&name) {
                        self.selected = None;
                        self.current = Settings::default();
                    }
                } else if name == "openboatutils:default" {
                    self.selected = None;
                    self.current = Settings::default();
                } else if !name.starts_with("openboatutils:") {
                    if self.stored.len() >= 32 && !self.stored.contains_key(&name) {
                        return None;
                    }
                    self.current = self.stored.get(&name).cloned().unwrap_or_default();
                    self.selected = Some(name);
                }
            }
            _ => return None,
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mod_api::{BoatControl, BoatMedium, BoatPhysics};

    fn physics(s: &Session) -> BoatPhysics {
        let control = BoatControl {
            medium: BoatMedium::Land,
            on_ground: true,
            ..Default::default()
        };
        let mut result = BoatPhysics {
            drag: 0.6,
            ..Default::default()
        };
        s.current().apply(
            &control,
            &mut result,
            &mut Default::default(),
            "minecraft:stone",
        );
        result
    }

    #[test]
    fn paper_settings_change_the_next_tick_and_reset() {
        let mut s = Session::default();
        let mut version = false;
        s.receive(crate::SETTINGS, &[0, 18, 0, 8], &mut version)
            .unwrap();
        assert_eq!((physics(&s).drag, physics(&s).step_height), (0.98, 1.25));
        let before = physics(&s);
        assert!(
            s.receive(
                crate::SETTINGS,
                &[0, 33, 0, 0, 0, 2, 0, 0, 0, 11, 0],
                &mut version
            )
            .is_none()
        );
        assert_eq!(before, physics(&s));
        s.receive(crate::SETTINGS, &[0, 15], &mut version).unwrap();
        assert!(version);
        s.receive(crate::SETTINGS, &[0, 0], &mut version).unwrap();
        assert_eq!((physics(&s).drag, physics(&s).step_height), (0.6, 0.0));
    }

    #[test]
    fn singleplayer_profiles_replace_one_another() {
        let mut s = Session::default();
        let control = BoatControl {
            medium: BoatMedium::Land,
            ..Default::default()
        };
        for (profile, dirt, stone) in [
            ("rally", 0.98, 0.98),
            ("dirt", 0.98, 0.6),
            ("reset", 0.6, 0.6),
        ] {
            assert!(s.test_profile(profile).is_some());
            for (block, expected) in [
                ("minecraft:dirt", dirt),
                ("minecraft:stone", stone),
                ("minecraft:blue_ice", 0.989),
            ] {
                let mut p = BoatPhysics {
                    drag: if block == "minecraft:blue_ice" {
                        0.989
                    } else {
                        0.6
                    },
                    ..Default::default()
                };
                s.current()
                    .apply(&control, &mut p, &mut Default::default(), block);
                assert_eq!(p.drag, expected, "{profile} on {block}");
            }
        }
    }

    #[test]
    fn named_context_applies_only_after_switch() {
        let mut s = Session::default();
        let mut v = false;
        let name = b"tracks:ice";
        let mut store = vec![0, 3, name.len() as u8];
        store.extend(name);
        store.extend([0, 0, 0, 1, 0, 11]);
        store.extend(0.25f32.to_be_bytes());
        s.receive(CONTEXT, &store, &mut v).unwrap();
        assert_eq!(physics(&s).forward, 0.04);
        store.truncate(3 + name.len());
        store[1] = 1;
        s.receive(CONTEXT, &store, &mut v).unwrap();
        assert_eq!(physics(&s).forward, 0.25);
        store[1] = 2;
        s.receive(CONTEXT, &store, &mut v).unwrap();
        assert_eq!(physics(&s).forward, 0.04);
    }
}
