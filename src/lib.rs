use mod_api::*;
use std::sync::Mutex;

mod session;
mod settings;
use session::Session;
use settings::Ride;

const SETTINGS: &str = "openboatutils:settings";
const CONTEXT: &str = "openboatutils:context";
const VERSION: [u8; 7] = [0, 0, 0, 0, 0, 22, 0];

#[derive(Default)]
pub struct Mod {
    state: Mutex<Session>,
    ride: Mutex<Ride>,
}

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, registrar: &mut dyn Registrar, _: &RegistrationContext) {
        registrar
            .payload_handlers()
            .registrar("1")
            .configuration_raw(
                PayloadId("openboatutils:configuration".into()),
                Some(PayloadBytes(VERSION.to_vec())),
            );
    }

    fn register_client_commands(
        &self,
        _: &mut dyn ClientHost,
        event: &mut RegisterClientCommandsEvent<'_>,
    ) {
        for usage in ["obutest reset", "obutest rally", "obutest dirt"] {
            event.commands.push(Command::new(usage));
        }
    }

    fn command(&self, host: &mut dyn ClientHost, event: &mut CommandEvent<'_>) -> Flow {
        let mut words = event.command.split_whitespace();
        if words.next() != Some("obutest") {
            return Flow::Continue;
        }
        let message = if !host
            .client_session()
            .client_server()
            .is_some_and(|s| s.integrated_local)
        {
            "Open an offline world to use /obutest."
        } else if let (Some(profile), None) = (words.next(), words.next())
            && let Some(message) = self.state.lock().unwrap().test_profile(profile)
        {
            *self.ride.lock().unwrap() = Ride::default();
            message
        } else {
            "Use /obutest reset, /obutest rally, or /obutest dirt."
        };
        let _ = host.chat_line(Text::literal(message));
        Flow::Cancel
    }

    fn client_disconnecting(&self, _: &dyn ClientHost, _: &mut ClientDisconnectingEvent) {
        *self.state.lock().unwrap() = Session::default();
        *self.ride.lock().unwrap() = Ride::default();
    }

    fn client_player_logging_in(
        &self,
        host: &mut dyn ClientHost,
        _: &mut ClientPlayerLoggingInEvent,
    ) {
        *self.state.lock().unwrap() = Session::default();
        *self.ride.lock().unwrap() = Ride::default();
        send(
            host,
            "minecraft:register",
            b"openboatutils:settings\0openboatutils:context",
        );
        send(host, SETTINGS, &VERSION);
    }

    fn client_player_clone(&self, _: &mut dyn ClientHost, event: &mut ClientPlayerCloneEvent) {
        if event.dimension_changed {
            let mut state = self.state.lock().unwrap();
            if !state.current().keep_world {
                *state = Session::default();
            }
            *self.ride.lock().unwrap() = Ride::default();
        }
    }

    fn client_packet(&self, host: &mut dyn ClientHost, event: &mut ClientPacketEvent<'_>) {
        let Some(payload) = &event.packet.custom_payload else {
            return;
        };
        if payload.channel != SETTINGS && payload.channel != CONTEXT {
            return;
        }
        let mut bytes = payload.data.as_slice();
        let transaction = if bytes.starts_with(&[0x7f, 0xff]) {
            let Some((header, rest)) = bytes.split_at_checked(6) else {
                return;
            };
            bytes = rest;
            Some(header)
        } else {
            None
        };
        let mut current = self.state.lock().unwrap();

        let mut version = false;
        if current
            .receive(&payload.channel, bytes, &mut version)
            .is_none()
        {
            return;
        }
        drop(current);
        if version {
            send(host, SETTINGS, &VERSION);
        }
        if let Some(header) = transaction {
            send(host, &payload.channel, header);
        }
    }

    fn controlled_boat_physics(
        &self,
        host: &dyn ClientHost,
        event: &mut ControlledBoatPhysicsEvent<'_>,
    ) {
        let session = self.state.lock().unwrap();
        let settings = session.current();
        let c = event.control;
        let block = match c.medium {
            BoatMedium::Land if settings.needs_block() => {
                let [x, y, z] = c.position;
                host.level(host.dimension())
                    .and_then(|l| {
                        l.block(
                            host,
                            [
                                x.floor() as i32,
                                (y - 0.001).floor() as i32,
                                z.floor() as i32,
                            ],
                        )
                    })
                    .map(|b| b.id(host))
                    .unwrap_or_default()
            }
            BoatMedium::Air => "minecraft:air".into(),
            BoatMedium::Land => String::new(),
            _ => "minecraft:water".into(),
        };
        settings.apply(c, event.physics, &mut self.ride.lock().unwrap(), &block);
    }
}

// All three play channel identifiers fit in a one-byte Minecraft VarInt.
fn send(host: &mut dyn ClientHost, channel: &str, bytes: &[u8]) {
    let mut body = Vec::with_capacity(1 + channel.len() + bytes.len());
    body.push(channel.len() as u8);
    body.extend_from_slice(channel.as_bytes());
    body.extend_from_slice(bytes);
    let _ = host.send_packet(ServerboundPlayPacket::new(
        Identifier::parse("minecraft:custom_payload").unwrap(),
        body,
    ));
}

#[cfg(test)]
mod hook_tests {
    use super::*;

    #[derive(Default)]
    struct Client {
        sent: Vec<ServerboundPlayPacket>,
    }
    impl Host for Client {}
    mod_api::impl_host_areas!(Client);
    impl ClientHost for Client {}
    mod_api::impl_client_host_areas!(Client, except(network));
    impl mod_api::prelude::NetworkClientHost for Client {
        fn send_packet(&mut self, packet: ServerboundPlayPacket) -> Result<(), SendPacketError> {
            self.sent.push(packet);
            Ok(())
        }
    }

    #[test]
    fn paper_handshake_transaction_and_ridden_boat_tick() {
        let m = Mod::default();
        let mut host = Client::default();
        m.client_player_logging_in(
            &mut host,
            &mut ClientPlayerLoggingInEvent {
                player_id: 1,
                uuid: 1,
            },
        );
        assert_eq!(host.sent.len(), 2);
        assert_eq!(
            host.sent[0].body,
            b"\x12minecraft:registeropenboatutils:settings\0openboatutils:context"
        );
        assert_eq!(
            host.sent[1].body,
            b"\x16openboatutils:settings\x00\x00\x00\x00\x00\x16\x00"
        );
        let packet = ClientPacket::new(
            Identifier::parse("minecraft:custom_payload").unwrap(),
            Some(ClientPayload::new(
                SETTINGS.into(),
                vec![0x7f, 0xff, 0xab, 0xcd, 0xef, 1, 0, 18, 0, 8],
            )),
        );
        m.client_packet(&mut host, &mut ClientPacketEvent { packet: &packet });
        assert_eq!(
            host.sent[2].body,
            b"\x16openboatutils:settings\x7f\xff\xab\xcd\xef\x01"
        );
        let control = BoatControl {
            on_ground: true,
            medium: BoatMedium::Land,
            ..Default::default()
        };
        let mut physics = BoatPhysics {
            drag: 0.6,
            ..Default::default()
        };
        m.controlled_boat_physics(
            &host,
            &mut ControlledBoatPhysicsEvent {
                control: &control,
                physics: &mut physics,
            },
        );
        assert_eq!((physics.drag, physics.step_height), (0.98, 1.25));
    }
}
