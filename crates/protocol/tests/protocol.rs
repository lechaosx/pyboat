use protocol::{
    BoatSnapshot, ClientMessage, ProtocolError, ServerMessage, WorldGeometry,
    decode_client_message, decode_server_message, encode_client_message, encode_server_message,
};
use serde::Serialize;

#[test]
fn welcome_message_carries_match_seed_and_world_geometry() {
    let world_geometry = WorldGeometry::new(80_000.0, 40_000.0).unwrap();
    let expected = ServerMessage::Welcome {
        player_id: "player-3".to_owned(),
        match_seed: 42,
        world_geometry,
    };
    let encoded = encode_server_message(&ServerMessage::Welcome {
        player_id: "player-3".to_owned(),
        match_seed: 42,
        world_geometry,
    })
    .unwrap();
    let decoded = decode_server_message(&encoded).unwrap();

    assert_eq!(decoded, expected);
}

#[test]
fn welcome_message_rejects_invalid_world_geometry_from_the_wire() {
    #[derive(Serialize)]
    struct RawWorldGeometry {
        width_m: f32,
        height_m: f32,
    }

    #[derive(Serialize)]
    enum RawServerMessage {
        Welcome {
            player_id: String,
            match_seed: u64,
            world_geometry: RawWorldGeometry,
        },
    }

    let raw = RawServerMessage::Welcome {
        player_id: "player-3".to_owned(),
        match_seed: 42,
        world_geometry: RawWorldGeometry {
            width_m: 0.0,
            height_m: 40_000.0,
        },
    };
    let mut encoded = vec![protocol::PROTOCOL_VERSION];
    encoded.extend(postcard::to_allocvec(&raw).unwrap());

    assert!(matches!(
        decode_server_message(&encoded),
        Err(ProtocolError::Codec(_))
    ));
}

#[test]
fn input_message_is_compact_and_round_trips() {
    let expected = ClientMessage::Input {
        sequence: 12,
        forward: 0.75,
        turn: -0.25,
    };
    let encoded = encode_client_message(&expected).unwrap();
    let decoded = decode_client_message(&encoded).unwrap();

    assert_eq!(encoded.len(), 11);
    assert_eq!(decoded, expected);
}

#[test]
fn input_rejects_values_outside_the_control_range() {
    let error = encode_client_message(&ClientMessage::Input {
        sequence: 1,
        forward: 1.01,
        turn: 0.0,
    })
    .unwrap_err();

    assert_eq!(error, ProtocolError::InvalidMotionIntent);
}

#[test]
fn viewport_message_is_compact_and_rejects_invalid_radius() {
    let expected = ClientMessage::Viewport { radius: 17.5 };
    let encoded = encode_client_message(&expected).unwrap();
    let decoded = decode_client_message(&encoded).unwrap();

    assert_eq!(encoded.len(), 6);
    assert_eq!(decoded, expected);
    assert_eq!(
        encode_client_message(&ClientMessage::Viewport { radius: 0.0 }).unwrap_err(),
        ProtocolError::InvalidViewportRadius
    );
}

#[test]
fn empty_snapshot_is_compact_and_round_trips() {
    let expected = ServerMessage::Snapshot {
        tick: 7,
        boats: Vec::new(),
        removed_player_ids: Vec::new(),
    };
    let encoded = encode_server_message(&expected).unwrap();
    let decoded = decode_server_message(&encoded).unwrap();

    assert_eq!(encoded, [3, 1, 7, 0, 0]);
    assert_eq!(decoded, expected);
}

#[test]
fn boat_damage_state_is_compact_and_round_trips() {
    let message = ServerMessage::Snapshot {
        tick: 12,
        boats: vec![BoatSnapshot {
            id: "player-2".to_owned(),
            health: 75.0,
            last_hit_tick: Some(11),
            wreck: false,
            x: 1.0,
            y: 2.0,
            vx: 3.0,
            vy: 4.0,
            angle: 0.5,
            angular_velocity: 0.1,
            sail: 0.2,
            sail_angular_velocity: -0.1,
            rudder: -0.3,
            rudder_angular_velocity: 0.2,
            forward: Some(0.75),
            turn: Some(-0.25),
            acknowledged_input: None,
        }],
        removed_player_ids: Vec::new(),
    };

    let encoded = encode_server_message(&message).unwrap();
    let decoded = decode_server_message(&encoded).unwrap();

    assert!(encoded.len() < 80);
    assert_eq!(decoded, message);
}

#[test]
fn protocol_rejects_an_unsupported_version() {
    assert_eq!(
        decode_client_message(&[1, 0]),
        Err(ProtocolError::UnsupportedVersion(1))
    );
}
