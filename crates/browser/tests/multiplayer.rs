use pyboat_browser::MultiplayerClient;
use pyboat_protocol::{BoatSnapshot, ClientMessage, ServerMessage};
use pyboat_simulation::DT_SECONDS;

fn boat(id: &str, x: f32) -> BoatSnapshot {
    BoatSnapshot {
        id: id.to_owned(),
        x,
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
        acknowledged_input: (id == "player-1").then_some(4),
    }
}

#[test]
fn multiplayer_client_retains_omitted_boats_until_a_tombstone() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![boat("player-1", 0.0), boat("player-2", 10.0)],
        removed_player_ids: Vec::new(),
    });

    assert_eq!(client.player_id(), Some("player-1"));
    assert!(!client.boat("player-2").unwrap().stale);
    assert_eq!(client.scenes().len(), 2);

    client.accept(ServerMessage::Snapshot {
        tick: 2,
        boats: vec![boat("player-1", 1.0)],
        removed_player_ids: Vec::new(),
    });

    assert!(client.boat("player-2").unwrap().stale);
    assert_eq!(client.scenes().len(), 1);

    client.accept(ServerMessage::Snapshot {
        tick: 3,
        boats: vec![boat("player-1", 2.0)],
        removed_player_ids: vec!["player-2".to_owned()],
    });

    assert!(client.boat("player-2").is_none());
}

#[test]
fn multiplayer_client_emits_clamped_sequenced_input_at_fixed_ticks() {
    let mut client = MultiplayerClient::new();
    client.set_motion_intent(2.0, -2.0);
    assert!(client.take_inputs(DT_SECONDS).is_empty());
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });

    assert!(client.take_inputs(DT_SECONDS / 2.0).is_empty());
    assert_eq!(
        client.take_inputs(DT_SECONDS / 2.0),
        vec![ClientMessage::Input {
            sequence: 1,
            forward: 1.0,
            turn: -1.0,
        }]
    );
    assert_eq!(
        client.take_inputs(2.0 * DT_SECONDS),
        vec![
            ClientMessage::Input {
                sequence: 2,
                forward: 1.0,
                turn: -1.0,
            },
            ClientMessage::Input {
                sequence: 3,
                forward: 1.0,
                turn: -1.0,
            },
        ]
    );
}

#[test]
fn multiplayer_client_derives_camera_and_view_from_authoritative_state() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    client.accept(ServerMessage::Snapshot {
        tick: 9,
        boats: vec![boat("player-1", 7.0), boat("player-2", 10.0)],
        removed_player_ids: Vec::new(),
    });

    assert_eq!(client.tick(), Some(9));
    assert_eq!(client.camera_m().unwrap().x, 7.0);
    assert_eq!(client.camera_m().unwrap().y, 2.0);
    assert_eq!(client.view().unwrap().speed_m_s, 5.0);
    assert_eq!(client.scenes().len(), 2);
}
