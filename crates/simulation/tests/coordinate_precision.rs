use rapier2d::prelude::{IntegrationParameters, PhysicsWorld, RigidBodyBuilder, Vector};
use simulation::DT_SECONDS;

#[test]
fn small_steps_preserve_motion_at_kilometre_coordinates() {
    for origin_m in [0.0, 4_100.0, 32_000.0] {
        let mut world = PhysicsWorld {
            gravity: Vector::ZERO,
            integration_parameters: IntegrationParameters {
                dt: DT_SECONDS / 240.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let handle = world.bodies.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::new(origin_m, 0.0))
                .linvel(Vector::new(2.0, 0.0))
                .additional_mass(1.0)
                .build(),
        );
        for _ in 0..240 {
            world.step();
        }
        let displacement_m = world.bodies[handle].translation().x - origin_m;
        let expected_m = 2.0 * DT_SECONDS;
        assert!(
            (displacement_m - expected_m).abs() < 1.0e-6,
            "origin={origin_m}, displacement={displacement_m}, expected={expected_m}"
        );
    }
}
