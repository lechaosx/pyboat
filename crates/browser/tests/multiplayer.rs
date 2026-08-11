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
fn multiplayer_client_discards_boats_omitted_from_interest() {
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
    assert!(client.boat("player-2").is_some());
    assert_eq!(client.scenes().len(), 2);

    client.accept(ServerMessage::Snapshot {
        tick: 2,
        boats: vec![boat("player-1", 1.0)],
        removed_player_ids: Vec::new(),
    });

    assert!(client.boat("player-2").is_none());
    assert_eq!(client.scenes().len(), 1);
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
fn multiplayer_client_freezes_when_the_connection_ends() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    let mut local = boat("player-1", 0.0);
    local.acknowledged_input = Some(0);
    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![local],
        removed_player_ids: Vec::new(),
    });
    client.set_motion_intent(1.0, 1.0);
    client.take_inputs(DT_SECONDS);
    let final_position_m = client.camera_m().unwrap();

    client.disconnect();

    assert!(client.take_inputs(1.0).is_empty());
    assert_eq!(client.camera_m(), Some(final_position_m));
    assert_eq!(client.player_id(), Some("player-1"));
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

#[test]
fn multiplayer_client_predicts_local_input_before_server_acknowledgement() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    let mut local = boat("player-1", 0.0);
    local.y = 0.0;
    local.vx = 4.0;
    local.vy = 0.0;
    local.angle = 0.0;
    local.angular_velocity = 0.0;
    local.acknowledged_input = Some(0);
    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![local],
        removed_player_ids: Vec::new(),
    });

    client.set_motion_intent(1.0, 1.0);
    client.take_inputs(DT_SECONDS);

    assert!(client.camera_m().unwrap().x > 0.05);
}

#[test]
fn multiplayer_client_replays_input_emitted_before_the_first_snapshot() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    client.set_motion_intent(1.0, 1.0);
    client.take_inputs(DT_SECONDS);
    let mut local = boat("player-1", 0.0);
    local.y = 0.0;
    local.vx = 4.0;
    local.vy = 0.0;
    local.angle = 0.0;
    local.angular_velocity = 0.0;
    local.acknowledged_input = Some(0);

    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![local],
        removed_player_ids: Vec::new(),
    });

    assert!(client.camera_m().unwrap().x > 0.05);
}

#[test]
fn multiplayer_client_predicts_nearby_collision_in_one_physics_world() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    let mut local = boat("player-1", -3.0);
    local.y = 0.0;
    local.vx = 5.0;
    local.vy = 0.0;
    local.angle = 0.0;
    local.angular_velocity = 0.0;
    local.forward = Some(0.0);
    local.turn = Some(0.0);
    local.acknowledged_input = Some(0);
    let mut other = boat("player-2", 3.0);
    other.y = 0.0;
    other.vx = -5.0;
    other.vy = 0.0;
    other.angle = std::f32::consts::PI;
    other.angular_velocity = 0.0;
    other.forward = Some(0.0);
    other.turn = Some(0.0);
    other.acknowledged_input = None;
    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![local, other],
        removed_player_ids: Vec::new(),
    });

    client.take_inputs(0.25);
    client.take_inputs(0.25);

    assert!(client.view().unwrap().speed_m_s < 3.0);
}

#[test]
fn multiplayer_client_extrapolates_distant_boats_for_at_most_quarter_second() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    let mut local = boat("player-1", 0.0);
    local.y = 0.0;
    local.vx = 0.0;
    local.vy = 0.0;
    local.angle = 0.0;
    local.angular_velocity = 0.0;
    local.acknowledged_input = Some(0);
    let mut distant = boat("player-2", 10.0);
    distant.y = 0.0;
    distant.vx = 2.0;
    distant.vy = 0.0;
    distant.angle = 0.0;
    distant.angular_velocity = 0.0;
    distant.forward = None;
    distant.turn = None;
    distant.acknowledged_input = None;
    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![local, distant],
        removed_player_ids: Vec::new(),
    });
    let initial_bow_x = client.scenes()[1].hull_m[0].x;

    client.take_inputs(1.0);

    let extrapolated_bow_x = client.scenes()[1].hull_m[0].x;
    assert!((extrapolated_bow_x - initial_bow_x - 0.5).abs() < 1.0e-4);
}

#[test]
fn multiplayer_client_smooths_authoritative_position_corrections() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    let mut local = boat("player-1", 0.0);
    local.y = 0.0;
    local.vx = 0.0;
    local.vy = 0.0;
    local.angle = 0.0;
    local.angular_velocity = 0.0;
    local.acknowledged_input = Some(0);
    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![local.clone()],
        removed_player_ids: Vec::new(),
    });
    local.x = 1.0;

    client.accept(ServerMessage::Snapshot {
        tick: 2,
        boats: vec![local],
        removed_player_ids: Vec::new(),
    });

    assert!(client.camera_m().unwrap().x.abs() < 1.0e-4);
    client.take_inputs(0.1);
    assert!(client.camera_m().unwrap().x > 0.9);
}

#[test]
fn multiplayer_client_smooths_rotation_across_the_angle_wrap() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    let mut local = boat("player-1", 0.0);
    local.y = 0.0;
    local.vx = 0.0;
    local.vy = 0.0;
    local.angle = std::f32::consts::PI - 0.05;
    local.angular_velocity = 0.0;
    local.acknowledged_input = Some(0);
    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![local.clone()],
        removed_player_ids: Vec::new(),
    });
    local.angle = -std::f32::consts::PI + 0.05;
    client.accept(ServerMessage::Snapshot {
        tick: 2,
        boats: vec![local],
        removed_player_ids: Vec::new(),
    });

    client.take_inputs(0.05);

    assert!(client.scenes()[0].hull_m[0].x < -1.0);
}

#[test]
fn multiplayer_client_preserves_pose_across_physics_bubble_transitions() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    let mut local = boat("player-1", 0.0);
    local.y = 0.0;
    local.vx = 0.0;
    local.vy = 0.0;
    local.angle = 0.0;
    local.angular_velocity = 0.0;
    local.acknowledged_input = Some(0);
    let mut remote = boat("player-2", 10.0);
    remote.y = 0.0;
    remote.vx = 2.0;
    remote.vy = 0.0;
    remote.angle = 0.0;
    remote.angular_velocity = 0.0;
    remote.forward = None;
    remote.turn = None;
    remote.acknowledged_input = None;
    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![local.clone(), remote.clone()],
        removed_player_ids: Vec::new(),
    });
    client.take_inputs(0.05);
    let before_enter = client.scenes()[1].hull_m[0];

    remote.x = 7.5;
    remote.forward = Some(0.0);
    remote.turn = Some(0.0);
    client.accept(ServerMessage::Snapshot {
        tick: 2,
        boats: vec![local.clone(), remote.clone()],
        removed_player_ids: Vec::new(),
    });
    let at_enter = client.scenes()[1].hull_m[0];
    assert!((at_enter - before_enter).length() < 1.0e-4);

    client.take_inputs(0.05);
    let before_leave = client.scenes()[1].hull_m[0];
    remote.x = 8.5;
    remote.forward = None;
    remote.turn = None;
    client.accept(ServerMessage::Snapshot {
        tick: 3,
        boats: vec![local, remote],
        removed_player_ids: Vec::new(),
    });
    let at_leave = client.scenes()[1].hull_m[0];
    assert!((at_leave - before_leave).length() < 1.0e-4);
}

#[test]
fn multiplayer_client_preserves_visual_velocity_across_kinematic_corrections() {
    let mut client = MultiplayerClient::new();
    client.accept(ServerMessage::Welcome {
        player_id: "player-1".to_owned(),
    });
    let mut local = boat("player-1", 0.0);
    local.y = 0.0;
    local.vx = 0.0;
    local.vy = 0.0;
    local.angle = 0.0;
    local.angular_velocity = 0.0;
    local.acknowledged_input = Some(0);
    let mut remote = boat("player-2", 10.0);
    remote.y = 0.0;
    remote.vx = 2.0;
    remote.vy = 0.0;
    remote.angle = 0.0;
    remote.angular_velocity = 0.0;
    remote.forward = None;
    remote.turn = None;
    remote.acknowledged_input = None;
    client.accept(ServerMessage::Snapshot {
        tick: 1,
        boats: vec![local.clone(), remote.clone()],
        removed_player_ids: Vec::new(),
    });
    client.take_inputs(0.05);
    let before = client.scenes()[1].hull_m[0].x;

    remote.x = 10.05;
    remote.vx = -2.0;
    client.accept(ServerMessage::Snapshot {
        tick: 2,
        boats: vec![local, remote],
        removed_player_ids: Vec::new(),
    });
    client.take_inputs(0.001);

    assert!(client.scenes()[1].hull_m[0].x > before);
}
