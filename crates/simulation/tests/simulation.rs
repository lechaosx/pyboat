use std::f32::consts::PI;

use pyboat_simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, DT_SECONDS, HullParameters, Simulation,
};
use rapier2d::prelude::{RigidBody, Vector};

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

fn motion() -> BoatMotion {
    BoatMotion {
        position_m: Vector::ZERO,
        velocity_m_s: Vector::new(2.0, -1.0),
        angle_rad: 0.7,
        angular_velocity_rad_s: 0.0,
        sail_angle_rad: 0.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: 0.0,
        rudder_angular_velocity_rad_s: 0.0,
    }
}

fn assert_close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1.0e-4, "{actual} != {expected}");
}

fn assert_vector_close(actual: Vector, expected: Vector) {
    assert_close(actual.x, expected.x);
    assert_close(actual.y, expected.y);
}

fn world_point(body: &RigidBody, local_point: Vector) -> Vector {
    body.translation() + body.rotation().transform_vector(local_point)
}

fn angular_momentum(simulation: &Simulation) -> f32 {
    simulation
        .world
        .bodies
        .iter()
        .map(|(_, body)| {
            body.mass_properties().local_mprops.principal_inertia() * body.angvel()
                + body.translation().perp_dot(body.linvel() * body.mass())
        })
        .sum()
}

#[test]
fn fixed_ticks_preserve_uniform_motion_and_connected_hinges() {
    let parameters = reference_parameters();
    let mut simulation = Simulation::new(parameters, motion());

    assert_eq!(simulation.world.integration_parameters.dt, DT_SECONDS);
    assert_eq!(simulation.world.gravity, Vector::ZERO);
    for _ in 0..60 {
        simulation.advance(ActuatorTorques::default());
    }

    let result = simulation.motion();
    assert_vector_close(result.position_m, Vector::new(2.0, -1.0));
    assert_vector_close(result.velocity_m_s, Vector::new(2.0, -1.0));
    assert_close(result.angle_rad, 0.7);
    assert_close(result.angular_velocity_rad_s, 0.0);
    assert_close(result.sail_angle_rad, 0.0);
    assert_close(result.rudder_angle_rad, 0.0);

    let hull = &simulation.world.bodies[simulation.boat.hull.body];
    let sail = &simulation.world.bodies[simulation.boat.sail];
    let rudder = &simulation.world.bodies[simulation.boat.rudder];
    assert_vector_close(
        world_point(hull, parameters.mast_position_m),
        world_point(sail, Vector::new(1.0, 0.0)),
    );
    assert_vector_close(
        world_point(hull, parameters.rudder_position_m),
        rudder.translation(),
    );
}

#[test]
fn sail_joint_stops_outward_motion_at_its_limit() {
    let parameters = reference_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    initial.sail_angular_velocity_rad_s = 4.0;
    let mut simulation = Simulation::new(parameters, initial);

    for _ in 0..120 {
        simulation.advance(ActuatorTorques::default());
    }

    let sail_angle_rad = simulation.motion().sail_angle_rad;
    assert!(sail_angle_rad > 0.5);
    assert!(sail_angle_rad <= parameters.max_sail_angle_rad + 1.0e-3);
}

#[test]
fn actuator_torques_rotate_surfaces_and_conserve_angular_momentum() {
    let parameters = reference_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    let mut simulation = Simulation::new(parameters, initial);

    simulation.advance(ActuatorTorques {
        sail_n_m: 100.0,
        rudder_n_m: 20.0,
    });

    let result = simulation.motion();
    assert!(result.sail_angular_velocity_rad_s > 0.0);
    assert!(result.rudder_angular_velocity_rad_s > 0.0);
    assert!(result.angular_velocity_rad_s < 0.0);
    assert!(angular_momentum(&simulation).abs() < 1.0e-4);
}

#[test]
fn actuator_torques_cannot_drive_surfaces_past_joint_limits() {
    let loaded_limit_tolerance_rad = 1.0e-3;
    let parameters = reference_parameters();
    let mut initial = motion();
    initial.velocity_m_s = Vector::ZERO;
    initial.angle_rad = 0.0;
    let mut simulation = Simulation::new(parameters, initial);

    for _ in 0..240 {
        simulation.advance(ActuatorTorques {
            sail_n_m: 100.0,
            rudder_n_m: 20.0,
        });
    }

    let result = simulation.motion();
    assert!(result.sail_angle_rad > 0.5);
    assert!(
        result.sail_angle_rad <= parameters.max_sail_angle_rad + loaded_limit_tolerance_rad,
        "{} > {}",
        result.sail_angle_rad,
        parameters.max_sail_angle_rad
    );
    assert!(result.rudder_angle_rad > 0.1);
    assert!(
        result.rudder_angle_rad <= parameters.max_rudder_angle_rad + loaded_limit_tolerance_rad,
        "{} > {}",
        result.rudder_angle_rad,
        parameters.max_rudder_angle_rad
    );
}
