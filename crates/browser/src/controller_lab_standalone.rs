use rapier2d_f64::prelude::Rotation;
use simulation::{BoatParameters, Environment, MotionIntent, SharedSimulation, SurfaceTargets};

pub const WEIGHTS: usize = 338;
const INPUTS: usize = 18;
const HIDDEN: usize = 16;

fn softsign(value: f64) -> f64 {
    value / (1.0 + value.abs())
}

pub fn targets_with_weights(
    simulation: &SharedSimulation,
    boat: usize,
    parameters: BoatParameters,
    environment: Environment,
    intent: MotionIntent,
    weights: &[f64; WEIGHTS],
) -> SurfaceTargets {
    let motion = simulation.motion(boat);
    let dynamics = simulation.control_dynamics(boat, environment);
    let body = Rotation::new(-motion.angle_rad);
    let water = body.transform_vector(environment.true_water_velocity_m_s - motion.velocity_m_s);
    let wind = body.transform_vector(environment.true_wind_velocity_m_s - motion.velocity_m_s);
    // These scales condition the pilot's inputs; they are not physical limits.
    let input = [
        intent.forward,
        intent.turn,
        water.x / 4.0,
        water.y / 4.0,
        wind.x / 8.0,
        wind.y / 8.0,
        motion.angular_velocity_rad_s / 0.4,
        motion.sail_angle_rad / parameters.max_sail_angle_rad,
        motion.sail_angular_velocity_rad_s / 2.0,
        motion.rudder_angle_rad / parameters.max_rudder_angle_rad,
        motion.rudder_angular_velocity_rad_s / 4.0,
        dynamics.fluid_hinge_torques_n_m[0] / parameters.max_sail_actuator_torque_n_m,
        dynamics.fluid_hinge_torques_n_m[1] / parameters.max_rudder_actuator_torque_n_m(),
        dynamics.sail_hinge_inertia_kg_m2 / 100.0,
        dynamics.rudder_hinge_inertia_kg_m2 / 10.0,
        dynamics.locked_inertia_kg_m2 / 500.0,
        parameters.max_sail_actuator_torque_n_m / 100.0,
        parameters.max_rudder_actuator_torque_n_m() / 100.0,
    ]
    .map(|x| x.clamp(-4.0, 4.0));
    let mut hidden = [0.0; HIDDEN];
    let mut offset = 0;
    for value in &mut hidden {
        let mut sum = weights[offset + INPUTS];
        for (j, input) in input.iter().enumerate() {
            sum += input * weights[offset + j];
        }
        *value = softsign(sum);
        offset += INPUTS + 1;
    }
    let output: [f64; 2] = std::array::from_fn(|_| {
        let mut sum = weights[offset + HIDDEN];
        for (j, hidden) in hidden.iter().enumerate() {
            sum += hidden * weights[offset + j];
        }
        offset += HIDDEN + 1;
        softsign(sum)
    });
    let sail = output[0] * parameters.max_sail_angle_rad;
    SurfaceTargets {
        sheet_target_angle_rad: sail,
        sail_target_angle_rad: Some(sail),
        rudder_target_angle_rad: output[1] * parameters.max_rudder_angle_rad,
    }
}

mod trained {
    include!("controller_lab_standalone_weights.rs");
}

pub(super) fn targets(
    simulation: &SharedSimulation,
    boat: usize,
    parameters: BoatParameters,
    environment: Environment,
    intent: MotionIntent,
) -> SurfaceTargets {
    targets_with_weights(
        simulation,
        boat,
        parameters,
        environment,
        intent,
        &trained::TRAINED_WEIGHTS,
    )
}
