use pyboat_protocol::{
    BoatSnapshot, ClientMessage, ProtocolError, ServerMessage, decode_client_message,
    decode_server_message, encode_client_message, encode_server_message,
};

#[test]
fn welcome_message_carries_match_seed() {
    let expected = ServerMessage::Welcome {
        player_id: "player-3".to_owned(),
        match_seed: 42,
    };
    let encoded = encode_server_message(&ServerMessage::Welcome {
        player_id: "player-3".to_owned(),
        match_seed: 42,
    })
    .unwrap();
    let decoded = decode_server_message(&encoded).unwrap();

    assert_eq!(
        encoded,
        r#"{"type":"welcome","playerId":"player-3","matchSeed":42}"#
    );
    assert_eq!(decoded, expected);
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

    assert_eq!(
        encoded,
        r#"{"type":"input","sequence":12,"forward":0.75,"turn":-0.25}"#
    );
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

    assert_eq!(encoded, r#"{"type":"viewport","radius":17.5}"#);
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

    assert_eq!(
        encoded,
        r#"{"type":"snapshot","tick":7,"boats":[],"removedPlayerIds":[]}"#
    );
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

    assert!(encoded.contains(r#""health":75.0,"lastHitTick":11,"wreck":false"#));
    assert_eq!(decoded, message);
}
