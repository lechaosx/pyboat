use game::dinghy_parameters;
use physics::prelude::Vector;

#[test]
fn centering_a_small_rudder_error_dissipates_energy_without_fluid_loads() {
    let mut parameters = dinghy_parameters();
    parameters.air_density_kg_m3 = 0.0;
    parameters.water_density_kg_m3 = 0.0;
    let motion = simulation::BoatMotion {
        position_m: Vector::ZERO,
        velocity_m_s: Vector::ZERO,
        angle_rad: 0.0,
        angular_velocity_rad_s: 0.0,
        sail_angle_rad: 0.0,
        sail_angular_velocity_rad_s: 0.0,
        rudder_angle_rad: 0.01,
        rudder_angular_velocity_rad_s: 0.0,
    };
    let mut simulation = simulation::Simulation::new(parameters, motion);
    let environment = simulation::Environment {
        true_wind_velocity_m_s: Vector::ZERO,
        true_water_velocity_m_s: Vector::ZERO,
        water_depth_m: f64::INFINITY,
        ..simulation::Environment::default()
    };
    let stiffness =
        parameters.max_rudder_actuator_torque_n_m() / (parameters.max_rudder_angle_rad / 10.0);
    let energy = |simulation: &simulation::Simulation| {
        simulation
            .world
            .bodies
            .iter()
            .map(|(_, b)| b.kinetic_energy())
            .sum::<f64>()
            + 0.5 * stiffness * simulation.motion().rudder_angle_rad.powi(2)
    };
    let mut previous_energy = energy(&simulation);
    for tick in 0..60 {
        simulation.advance_targets(
            environment,
            simulation::SurfaceTargets {
                sail_target_angle_rad: Some(0.0),
                sheet_target_angle_rad: 0.0,
                rudder_target_angle_rad: 0.0,
            },
        );
        let current_energy = energy(&simulation);
        assert!(
            current_energy <= previous_energy + 1.0e-6,
            "tick {tick}: centering energy grew from {previous_energy} to {current_energy}"
        );
        previous_energy = current_energy;
    }
    assert!(simulation.motion().rudder_angle_rad.abs() < 1.0e-4);
}

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
        water_depth_m: f64::INFINITY,
        ..simulation::Environment::default()
    };
    for (speed_m_s, yaw_speed, sail_speed, rudder_speed_rad_s) in [
        (5.0, 0.0, 0.0, 0.0),
        (20.0, 0.0, 0.0, 0.0),
        (700.0, 0.0, 0.0, 0.0),
        (0.0, 0.0, 0.0, 20.0),
        (0.0, 20.0, 0.0, 0.0),
        (0.0, 0.0, 20.0, 0.0),
    ] {
        let motion = simulation::BoatMotion {
            position_m: Vector::ZERO,
            velocity_m_s: Vector::new(speed_m_s, 0.0),
            angle_rad: 0.0,
            angular_velocity_rad_s: yaw_speed,
            sail_angle_rad: 0.0,
            sail_angular_velocity_rad_s: sail_speed,
            rudder_angle_rad: 0.03,
            rudder_angular_velocity_rad_s: rudder_speed_rad_s,
        };
        let mut isolated = simulation::Simulation::new(dinghy_parameters(), motion);
        let mut shared = simulation::SharedSimulation::new();
        shared.add_boat(dinghy_parameters(), motion);
        let energy = |world: &physics::prelude::PhysicsWorld| {
            world
                .bodies
                .iter()
                .map(|(_, body)| body.kinetic_energy())
                .sum::<f64>()
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
        water_depth_m: f64::INFINITY,
        ..simulation::Environment::default()
    };
    let torques = simulation::ActuatorTorques {
        sail_n_m: 0.0,
        rudder_n_m: parameters.max_rudder_actuator_torque_n_m(),
    };
    let mut isolated = simulation::Simulation::new(parameters, motion);
    let mut shared = simulation::SharedSimulation::new();
    shared.add_boat(parameters, motion);
    isolated.advance(environment, torques);
    shared.advance(environment, &[torques]);
    let energy = |world: &physics::prelude::PhysicsWorld| {
        world
            .bodies
            .iter()
            .map(|(_, body)| body.kinetic_energy())
            .sum::<f64>()
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

#[test]
fn the_sailor_holds_the_boom_clear_of_the_shrouds_at_full_travel() {
    let mut parameters = dinghy_parameters();
    parameters.air_density_kg_m3 = 0.0;
    parameters.water_density_kg_m3 = 0.0;
    let shroud =
        game::dinghy_stay_anchors_m(parameters.hull.length_m)[1] - parameters.mast_position_m;
    let luff = 2.0 * parameters.sail_area_m2 / parameters.boom_length_m;
    let at_boom = shroud * (luff / (luff + f64::from(game::DINGHY_BOOM_ABOVE_SHEER_M)));
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
    for side in [-1.0, 1.0] {
        let mut boat = simulation::Simulation::new(parameters, motion);
        let mut maximum_angle = 0.0_f64;
        for _ in 0..240 {
            boat.advance_targets(
                simulation::Environment::default(),
                simulation::SurfaceTargets {
                    sail_target_angle_rad: Some(side * parameters.max_sail_angle_rad),
                    sheet_target_angle_rad: side * parameters.max_sail_angle_rad,
                    rudder_target_angle_rad: 0.0,
                },
            );
            maximum_angle = maximum_angle.max(boat.motion().sail_angle_rad.abs());
        }
        let direction = physics::prelude::Rotation::new(-maximum_angle)
            .transform_vector(Vector::new(-1.0, 0.0));
        let along = at_boom.dot(direction).clamp(0.0, parameters.boom_length_m);
        let separation_m = (at_boom - direction * along).length();
        // Rapier's finite positional tolerance permits millimetre-scale penetration.
        assert!(
            separation_m >= 0.025 + f64::from(game::DINGHY_STAY_RADIUS_M) - 0.003,
            "boom/stay separation {separation_m}m at {} degrees",
            maximum_angle.to_degrees()
        );
        assert!((maximum_angle - parameters.max_sail_angle_rad).abs() < 0.005);
    }
}
