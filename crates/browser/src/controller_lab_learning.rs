use super::{LabObservation, physical_targets};
use simulation::SurfaceTargets;

const INPUTS: usize = 18;
const HIDDEN1: usize = 32;
const HIDDEN2: usize = 16;
const WEIGHTS: usize = 1170;
const RATE_SCALE: f64 = 0.4;

fn softsign(x: f64) -> f64 {
    x / (1.0 + x.abs())
}

fn features_with_seed(o: &LabObservation, physical: SurfaceTargets) -> [f64; INPUTS] {
    let rotation = rapier2d_f64::prelude::Rotation::new(-o.motion.angle_rad);
    let water = o.environment.true_water_velocity_m_s - o.motion.velocity_m_s;
    let wind = o.environment.true_wind_velocity_m_s - o.motion.velocity_m_s;
    let body = |x: f64, y: f64| {
        let local = rotation.transform_vector(rapier2d_f64::prelude::Vector::new(x, y));
        [local.x, local.y]
    };
    let water = body(water.x, water.y);
    let wind = body(wind.x, wind.y);
    [
        o.intent.forward,
        o.intent.turn,
        water[0] / 4.0,
        water[1] / 4.0,
        wind[0] / 8.0,
        wind[1] / 8.0,
        o.motion.angular_velocity_rad_s / RATE_SCALE,
        o.motion.sail_angle_rad / o.parameters.max_sail_angle_rad,
        o.motion.sail_angular_velocity_rad_s / 2.0,
        o.motion.rudder_angle_rad / o.parameters.max_rudder_angle_rad,
        o.motion.rudder_angular_velocity_rad_s / 4.0,
        physical.rudder_target_angle_rad / o.parameters.max_rudder_angle_rad,
        physical
            .sail_target_angle_rad
            .unwrap_or(physical.sheet_target_angle_rad)
            / o.parameters.max_sail_angle_rad,
        o.fluid_hinge_torques_n_m[1] / o.parameters.max_rudder_actuator_torque_n_m(),
        o.fluid_hinge_torques_n_m[0] / o.parameters.max_sail_actuator_torque_n_m,
        o.maximum_yaw_rate_rad_s / 0.8,
        o.rudder_hinge_inertia_kg_m2 / 10.0,
        o.locked_inertia_kg_m2 / 500.0,
    ]
    .map(|x| x.clamp(-4.0, 4.0))
}

fn network_output(x: &[f64; INPUTS], weights: &[f64; WEIGHTS]) -> [f64; 2] {
    let mut offset = 0;
    let mut first = [0.0; HIDDEN1];
    for out in &mut first {
        let mut sum = weights[offset + INPUTS];
        for j in 0..INPUTS {
            sum += x[j] * weights[offset + j];
        }
        *out = softsign(sum);
        offset += INPUTS + 1;
    }
    let mut second = [0.0; HIDDEN2];
    for out in &mut second {
        let mut sum = weights[offset + HIDDEN1];
        for j in 0..HIDDEN1 {
            sum += first[j] * weights[offset + j];
        }
        *out = softsign(sum);
        offset += HIDDEN1 + 1;
    }
    let mut result = [0.0; 2];
    for out in &mut result {
        let mut sum = weights[offset + HIDDEN2];
        for j in 0..HIDDEN2 {
            sum += second[j] * weights[offset + j];
        }
        *out = softsign(sum);
        offset += HIDDEN2 + 1;
    }
    result
}

fn learned_with_weights(o: &LabObservation, weights: &[f64; WEIGHTS]) -> SurfaceTargets {
    let mut targets = physical_targets(o);
    let output = network_output(&features_with_seed(o, targets), weights);
    let sail = targets
        .sail_target_angle_rad
        .unwrap_or(targets.sheet_target_angle_rad)
        + output[0] * 0.5 * o.parameters.max_sail_angle_rad;
    targets.sheet_target_angle_rad = sail.clamp(
        -o.parameters.max_sail_angle_rad,
        o.parameters.max_sail_angle_rad,
    );
    targets.sail_target_angle_rad = Some(targets.sheet_target_angle_rad);
    targets.rudder_target_angle_rad = (targets.rudder_target_angle_rad
        + output[1] * 0.5 * o.parameters.max_rudder_angle_rad)
        .clamp(
            -o.parameters.max_rudder_angle_rad,
            o.parameters.max_rudder_angle_rad,
        );
    targets
}

mod trained {
    include!("controller_lab_weights.rs");
}
pub(super) fn learned_targets(o: &LabObservation) -> SurfaceTargets {
    learned_with_weights(o, &trained::TRAINED_WEIGHTS)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inference_respects_known_bias_and_is_bounded() {
        let mut weights = [0.0; WEIGHTS];
        weights[WEIGHTS - 1] = 2.0;
        assert_eq!(network_output(&[0.0; INPUTS], &weights), [0.0, 2.0 / 3.0]);
        for sign in [-1.0, 1.0] {
            weights[WEIGHTS - 1] = sign * 1e100;
            let output = network_output(&[4.0; INPUTS], &weights);
            assert!(output.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        }
    }
}
