#![cfg(feature = "playground")]
use browser::{PlaygroundBoat, controller_lab::LabControllerKind, player_motion_intent};
use game::{
    computed_yaw_rate_targets_in_environment, dinghy_parameters, procedural_match_environment,
};
use rapier2d_f64::prelude::Vector;
use simulation::{SharedSimulation, WorldGeometry};

#[test]
fn switching_and_world_rebase_preserve_physical_state() {
    let mut boat = PlaygroundBoat::spawn(WorldGeometry::default(), 7, 3600, Vector::ZERO).unwrap();
    boat.set_player_input([1.0, 0.5, 0.0, 0.0]);
    for _ in 0..12 {
        boat.step();
    }
    let motion = boat.motion();
    let tick = boat.tick();
    for kind in LabControllerKind::ALL {
        boat.select(kind);
        assert_eq!(boat.motion(), motion);
        assert_eq!(boat.tick(), tick);
    }
    boat.rebase(WorldGeometry::new(32_000.0, 48_000.0).unwrap(), 8, 7200);
    assert_eq!(boat.motion(), motion);
    assert_eq!(boat.tick(), 7200);
}

#[test]
fn every_tick_uses_production_environment_and_current_heading_intent() {
    let geometry = WorldGeometry::default();
    let mut boat = PlaygroundBoat::spawn(geometry, 7, 3600, Vector::ZERO).unwrap();
    let input = [0.0, 0.0, 0.25, 0.4];
    boat.set_player_input(input);
    let mut reference = SharedSimulation::new_with_world_geometry(geometry);
    reference.add_boat(dinghy_parameters(), boat.motion());
    for _ in 0..6 {
        let motion = boat.motion();
        let expected = player_motion_intent(
            Some(motion.angle_rad),
            input[0],
            input[1],
            input[2],
            input[3],
        );
        let environment = procedural_match_environment(geometry, 7, boat.tick(), motion.position_m);
        assert_eq!(boat.environment(), environment);
        let targets =
            computed_yaw_rate_targets_in_environment(reference.motion(0), expected, environment);
        reference.advance_controlled_with_environments(&[environment], &[Some(targets)]);
        boat.step();
        assert_eq!(boat.intent(), expected);
        assert_eq!(boat.motion(), reference.motion(0));
    }
}

#[test]
fn spawning_finds_safe_water_and_does_not_advance_clock() {
    let boat = PlaygroundBoat::spawn(WorldGeometry::default(), 7, 3600, Vector::ZERO).unwrap();
    assert_eq!(boat.tick(), 3600);
    assert!(boat.environment().water_depth_m > game::dinghy_parameters().draft_m);
}

#[test]
fn presentation_uses_nearest_longitude_image() {
    let geometry = WorldGeometry::default();
    let boat = PlaygroundBoat::spawn(geometry, 7, 3600, Vector::ZERO).unwrap();
    let motion = boat.motion();
    let near = boat.scene_near_camera(motion.position_m);
    let lap = boat.scene_near_camera(motion.position_m + Vector::new(geometry.width_m(), 0.0));
    for (near, lap) in near.hull_m.iter().zip(lap.hull_m) {
        assert!((f64::from(lap.x - near.x) - geometry.width_m()).abs() < 0.01);
        assert_eq!(lap.y, near.y);
    }
    assert_eq!(boat.motion(), motion);
}
