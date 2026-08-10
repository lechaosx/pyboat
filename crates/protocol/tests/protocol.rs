use pyboat_protocol::{
    ClientMessage, ProtocolError, ServerMessage, decode_client_message, decode_server_message,
    encode_client_message, encode_server_message,
};

#[test]
fn welcome_message_matches_the_python_protocol() {
    let expected = ServerMessage::Welcome {
        player_id: "player-3".to_owned(),
    };
    let encoded = encode_server_message(&ServerMessage::Welcome {
        player_id: "player-3".to_owned(),
    })
    .unwrap();
    let decoded = decode_server_message(&encoded).unwrap();

    assert_eq!(encoded, r#"{"type":"welcome","playerId":"player-3"}"#);
    assert_eq!(decoded, expected);
}

#[test]
fn input_message_matches_the_python_protocol() {
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
fn empty_snapshot_matches_the_python_protocol() {
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
