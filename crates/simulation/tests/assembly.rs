use std::f32::consts::PI;

use pyboat_simulation::{
    BoatMotion, BoatParameters, HullParameters, create_boat, read_motion, restore_motion,
};
use rapier2d::prelude::{ColliderSet, ImpulseJointSet, RigidBody, RigidBodySet, Vector};

fn reference_parameters() -> BoatParameters {
    BoatParameters {
        hull: HullParameters {
            mass_kg: 222.0,
            length_m: 4.0,
            beam_m: 1.5,
            bow_length_m: 0.7,
        },
        sail_mass_kg: 12.0,
        boom_mass_kg: 10.0,
        boom_length_m: 2.0,
        mast_position_m: Vector::new(0.4, 0.0),
        max_sail_angle_rad: PI / 2.0,
        rudder_mass_kg: 4.0,
        tiller_mass_kg: 2.0,
        rudder_draw_length_m: 0.5,
        tiller_length_m: 1.0,
        rudder_position_m: Vector::new(-1.8, 0.0),
        max_rudder_angle_rad: PI / 6.0,
    }
}

fn zero_motion() -> BoatMotion {
    BoatMotion {
        position_m: Vector::ZERO,
        velocity_m_s: Vector::ZERO,
        angle_rad: 0.0,
        angular_velocity_rad_s: 0.0,
        sail_angle_rad: 0.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: 0.0,
        rudder_angular_velocity_rad_s: 0.0,
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
}

fn assert_vector_close(actual: Vector, expected: Vector) {
    assert_close(actual.x, expected.x);
    assert_close(actual.y, expected.y);
}

fn velocity_at_local_point(body: &RigidBody, point: Vector) -> Vector {
    let offset = body.rotation().transform_vector(point);
    body.linvel() + Vector::new(-offset.y, offset.x) * body.angvel()
}

#[test]
fn creates_the_reference_articulated_assembly() {
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();
    let mut joints = ImpulseJointSet::new();

    let boat = create_boat(
        reference_parameters(),
        zero_motion(),
        &mut bodies,
        &mut colliders,
        &mut joints,
    );

    assert_eq!(bodies.len(), 3);
    assert_eq!(colliders.len(), 1);
    assert_eq!(joints.len(), 2);
    assert_eq!(bodies[boat.hull.body].mass(), 222.0);
    assert_eq!(bodies[boat.sail].mass(), 22.0);
    assert_eq!(bodies[boat.rudder].mass(), 6.0);
    assert_eq!(
        bodies.iter().map(|(_, body)| body.mass()).sum::<f32>(),
        250.0
    );

    for handle in [boat.hull.body, boat.sail, boat.rudder] {
        assert_eq!(bodies[handle].local_center_of_mass(), Vector::ZERO);
        assert!(
            bodies[handle]
                .mass_properties()
                .local_mprops
                .principal_inertia()
                > 0.0
        );
    }

    let sail_joint = joints
        .get(boat.sail_joint)
        .unwrap()
        .data
        .as_revolute()
        .unwrap();
    assert_eq!(sail_joint.local_anchor1(), Vector::new(0.4, 0.0));
    assert_eq!(sail_joint.local_anchor2(), Vector::new(1.0, 0.0));
    let sail_limits = sail_joint.limits().unwrap();
    assert_close(sail_limits.min, -PI / 2.0);
    assert_close(sail_limits.max, PI / 2.0);

    let rudder_joint = joints
        .get(boat.rudder_joint)
        .unwrap()
        .data
        .as_revolute()
        .unwrap();
    assert_eq!(rudder_joint.local_anchor1(), Vector::new(-1.8, 0.0));
    assert_eq!(rudder_joint.local_anchor2(), Vector::ZERO);
    let rudder_limits = rudder_joint.limits().unwrap();
    assert_close(rudder_limits.min, -PI / 6.0);
    assert_close(rudder_limits.max, PI / 6.0);
}

#[test]
fn authoritative_motion_round_trips_and_keeps_hinges_connected() {
    let parameters = reference_parameters();
    let mut bodies = RigidBodySet::new();
    let mut colliders = ColliderSet::new();
    let mut joints = ImpulseJointSet::new();
    let boat = create_boat(
        parameters,
        zero_motion(),
        &mut bodies,
        &mut colliders,
        &mut joints,
    );
    let motion = BoatMotion {
        position_m: Vector::new(3.0, 4.0),
        velocity_m_s: Vector::new(2.0, -1.0),
        angle_rad: 0.7,
        angular_velocity_rad_s: 0.2,
        sail_angle_rad: 0.3,
        sail_angular_velocity_rad_s: -0.1,
        rudder_angle_rad: -0.2,
        rudder_angular_velocity_rad_s: 0.4,
    };

    restore_motion(&boat, parameters, motion, &mut bodies);

    let restored = read_motion(&boat, &bodies);
    assert_vector_close(restored.position_m, motion.position_m);
    assert_vector_close(restored.velocity_m_s, motion.velocity_m_s);
    assert_close(restored.angle_rad, motion.angle_rad);
    assert_close(
        restored.angular_velocity_rad_s,
        motion.angular_velocity_rad_s,
    );
    assert_close(restored.sail_angle_rad, motion.sail_angle_rad);
    assert_close(
        restored.sail_angular_velocity_rad_s,
        motion.sail_angular_velocity_rad_s,
    );
    assert_close(restored.rudder_angle_rad, motion.rudder_angle_rad);
    assert_close(
        restored.rudder_angular_velocity_rad_s,
        motion.rudder_angular_velocity_rad_s,
    );

    let hull = &bodies[boat.hull.body];
    let sail = &bodies[boat.sail];
    let rudder = &bodies[boat.rudder];
    assert_vector_close(
        hull.translation() + hull.rotation().transform_vector(parameters.mast_position_m),
        sail.translation() + sail.rotation().transform_vector(Vector::new(1.0, 0.0)),
    );
    assert_vector_close(
        hull.translation()
            + hull
                .rotation()
                .transform_vector(parameters.rudder_position_m),
        rudder.translation(),
    );
    assert_vector_close(
        velocity_at_local_point(hull, parameters.mast_position_m),
        velocity_at_local_point(sail, Vector::new(1.0, 0.0)),
    );
    assert_vector_close(
        velocity_at_local_point(hull, parameters.rudder_position_m),
        rudder.linvel(),
    );
}
