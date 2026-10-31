use std::f32::consts::PI;

use rapier2d::prelude::{
    ColliderSet, ImpulseJointSet, MotorModel, RigidBody, RigidBodySet, Vector,
};
use simulation::{
    BoatMotion, BoatParameters, HullParameters, create_boat, read_motion, restore_motion,
};

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
        air_density_kg_m3: 1.225,
        sail_area_m2: 8.0,
        sail_center_of_effort_from_mast_m: 0.7,
        sail_base_drag_coefficient: 0.02,
        water_density_kg_m3: 1025.0,
        keel_area_m2: 0.6,
        keel_position_m: Vector::ZERO,
        keel_base_drag_coefficient: 0.02,
        keel_stall_angle_rad: 15.0_f32.to_radians(),
        keel_stall_lift_coefficient: 0.8,
        hull_reference_area_m2: 0.3,
        hull_drag_coefficient: 0.08,
        hull_lateral_drag_coefficient: 0.02,
        rudder_mass_kg: 4.0,
        tiller_mass_kg: 2.0,
        rudder_draw_length_m: 0.5,
        tiller_length_m: 1.0,
        rudder_position_m: Vector::new(-1.8, 0.0),
        max_rudder_angle_rad: PI / 6.0,
        rudder_area_m2: 0.12,
        rudder_base_drag_coefficient: 0.02,
        rudder_stall_angle_rad: 15.0_f32.to_radians(),
        rudder_stall_lift_coefficient: 0.8,
        max_sail_actuator_torque_n_m: 375.0,
        max_tiller_hand_force_n: 60.0,
        sail_bearing_damping_n_m_s_rad: 20.0,
        rudder_bearing_damping_n_m_s_rad: 2.0,
        draft_m: 0.6,
        ground_static_friction_coefficient: 1.0,
        ground_kinetic_friction_coefficient: 0.6,
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
    let sail_motor = sail_joint.motor().unwrap();
    assert_eq!(sail_motor.model, MotorModel::ForceBased);
    assert_eq!(sail_motor.target_vel, 0.0);
    assert_eq!(sail_motor.stiffness, 0.0);
    assert_eq!(sail_motor.damping, 20.0);

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
    let rudder_motor = rudder_joint.motor().unwrap();
    assert_eq!(rudder_motor.model, MotorModel::ForceBased);
    assert_eq!(rudder_motor.target_vel, 0.0);
    assert_eq!(rudder_motor.stiffness, 0.0);
    assert_eq!(rudder_motor.damping, 2.0);
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
