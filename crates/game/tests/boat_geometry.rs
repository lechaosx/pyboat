use game::dinghy_parameters;
use rapier2d::prelude::Vector;

#[test]
fn transom_hung_rudder_pivots_on_the_hull_centerline_at_the_stern() {
    let parameters = dinghy_parameters();
    assert_eq!(
        parameters.rudder_position_m,
        Vector::new(-parameters.hull.length_m / 2.0, 0.0)
    );
}

#[test]
fn passive_boats_cannot_gain_kinetic_energy_in_still_water() {
    let environment = simulation::Environment {
        true_wind_velocity_m_s: Vector::ZERO,
        true_water_velocity_m_s: Vector::ZERO,
        water_depth_m: f32::INFINITY,
    };
    for (speed_m_s, rudder_speed_rad_s) in [(5.0, 0.0), (20.0, 0.0), (700.0, 0.0), (0.0, 20.0)] {
        let motion = simulation::BoatMotion {
            position_m: Vector::ZERO,
            velocity_m_s: Vector::new(speed_m_s, 0.0),
            angle_rad: 0.0,
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: 0.0,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.03,
            rudder_angular_velocity_rad_s: rudder_speed_rad_s,
        };
        let mut isolated = simulation::Simulation::new(dinghy_parameters(), motion);
        let mut shared = simulation::SharedSimulation::new();
        shared.add_boat(dinghy_parameters(), motion);
        let energy = |world: &rapier2d::prelude::PhysicsWorld| {
            world
                .bodies
                .iter()
                .map(|(_, body)| body.kinetic_energy())
                .sum::<f32>()
        };
        let initial_energy = energy(&isolated.world);
        for _ in 0..120 {
            isolated.advance(environment, simulation::ActuatorTorques::default());
            shared.advance(environment, &[simulation::ActuatorTorques::default()]);
            assert!(
                energy(&isolated.world) <= initial_energy * 1.001,
                "isolated {speed_m_s}/{rudder_speed_rad_s}"
            );
            assert!(
                energy(&shared.world) <= initial_energy * 1.001,
                "shared {speed_m_s}/{rudder_speed_rad_s}"
            );
        }
    }
}

#[test]
fn rudder_actuator_startup_cannot_leave_a_passive_boat_gaining_energy() {
    let parameters = dinghy_parameters();
    let motion = simulation::BoatMotion {
        position_m: Vector::ZERO,
        velocity_m_s: Vector::ZERO,
        angle_rad: 0.0,
        angular_velocity_rad_s: 0.0,
        sail_angle_rad: 0.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: 0.0,
        rudder_angular_velocity_rad_s: 0.0,
    };
    let environment = simulation::Environment {
        true_wind_velocity_m_s: Vector::ZERO,
        true_water_velocity_m_s: Vector::ZERO,
        water_depth_m: f32::INFINITY,
    };
    let torques = simulation::ActuatorTorques {
        sail_n_m: 0.0,
        rudder_n_m: parameters.max_rudder_actuator_torque_n_m,
    };
    let mut isolated = simulation::Simulation::new(parameters, motion);
    let mut shared = simulation::SharedSimulation::new();
    shared.add_boat(parameters, motion);
    isolated.advance(environment, torques);
    shared.advance(environment, &[torques]);
    let energy = |world: &rapier2d::prelude::PhysicsWorld| {
        world
            .bodies
            .iter()
            .map(|(_, body)| body.kinetic_energy())
            .sum::<f32>()
    };
    let isolated_energy = energy(&isolated.world);
    let shared_energy = energy(&shared.world);
    for _ in 0..120 {
        isolated.advance(environment, simulation::ActuatorTorques::default());
        shared.advance(environment, &[simulation::ActuatorTorques::default()]);
        assert!(energy(&isolated.world) <= isolated_energy * 1.001);
        assert!(energy(&shared.world) <= shared_energy * 1.001);
    }
}
