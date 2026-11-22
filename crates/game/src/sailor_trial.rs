use std::f64::consts::{FRAC_PI_2, PI, TAU};

use physics::prelude::{Rotation, Vector};
use simulation::{
    BoatMotion, BoatParameters, Environment, MotionIntent, SurfaceTargets, rudder_foil_force,
    sail_static_motor_stiffness_n_m_rad,
};

pub fn targets(
    parameters: BoatParameters,
    motion: BoatMotion,
    environment: Environment,
    intent: MotionIntent,
    force_trim: bool,
    load_aware_helm: bool,
) -> SurfaceTargets {
    let mut targets = crate::sailor_targets(parameters, motion, environment, intent);
    let flow_at_hinge = |fluid: Vector, hinge: Vector| {
        Rotation::new(-motion.angle_rad).transform_vector(fluid - motion.velocity_m_s)
            - Vector::new(-hinge.y, hinge.x) * motion.angular_velocity_rad_s
    };
    if force_trim {
        let wind = flow_at_hinge(
            environment.true_wind_velocity_m_s,
            parameters.mast_position_m,
        );
        let angle = force_trim_target(
            parameters,
            motion.sail_angle_rad,
            wind,
            intent.forward.clamp(-1.0, 1.0),
            targets.sheet_target_angle_rad,
        );
        targets.sheet_target_angle_rad = angle;
        targets.sail_target_angle_rad = Some(angle);
    }
    if load_aware_helm {
        let water = flow_at_hinge(
            environment.true_water_velocity_m_s,
            parameters.rudder_position_m,
        );
        targets.rudder_target_angle_rad = loaded_helm_target(
            parameters,
            motion.rudder_angle_rad,
            water,
            intent.turn.clamp(-1.0, 1.0),
        );
    }
    targets
}

fn force_trim_target(
    parameters: BoatParameters,
    sail_angle: f64,
    wind: Vector,
    power: f64,
    simple_target: f64,
) -> f64 {
    let limit = parameters.max_sail_angle_rad;
    let capacity = parameters.max_sail_actuator_torque_n_m;
    // In uniform wind the triangular sail's pressure center is one third of the boom from its mast.
    let peak_moment = parameters.air_density_kg_m3
        * parameters.sail_area_m2
        * wind.length_squared()
        * parameters.boom_length_m
        / 3.0;
    if peak_moment == 0.0 {
        return 0.0;
    }
    let stiffness = sail_static_motor_stiffness_n_m_rad(parameters);
    let from = libm::atan2(-wind.y, -wind.x);
    let current = sail_angle.clamp(-limit, limit);
    let motor_target = |trim: f64| {
        let moment = -peak_moment * libm::sin(trim - from);
        trim - moment.clamp(-capacity, capacity) / stiffness
    };
    // Boat-axis pressure varies as cos(2θ − φ) − cos(φ). Base drag is constant across trim.
    let surge = |angle: f64| libm::cos(2.0 * angle - from);
    if peak_moment <= capacity {
        let endpoint = if power == 0.0 {
            0.0
        } else if power < 0.0 && FRAC_PI_2 - from.abs() * 0.5 >= limit && current != 0.0 {
            // Both backing angles fall outside the shrouds. Keep the existing side through headwind.
            limit.copysign(current)
        } else {
            simple_target / power.abs()
        };
        let want = surge(0.0) + power.abs() * (surge(endpoint) - surge(0.0));
        return motor_target(trim_for_surge(from, 0.0, endpoint, want));
    }

    let inverted = from - from.signum() * PI;
    let hold_width = libm::asin(capacity / peak_moment);
    let from_inverted = (current - inverted).abs();
    let holds_inverted = peak_moment < stiffness && from_inverted < hold_width;
    let pinned_away_from_wind = from_inverted < FRAC_PI_2
        && from_inverted > hold_width
        && current * (current - inverted) > 0.0;
    if pinned_away_from_wind {
        let stop = limit.copysign(current - inverted);
        return if power > 0.0 {
            // Sustained drive can pull the boom across once the load falls below the hand-force limit.
            stop * (1.0 - power) + simple_target
        } else {
            motor_target(stop)
        };
    }

    let (feather, width) = if holds_inverted {
        // Wind pushing the boom away from an inverted feather can run it out to the shrouds.
        // Use half the effort for steady trim and keep the other half to recover from disturbances.
        (inverted, libm::asin(0.5 * capacity / peak_moment))
    } else {
        (from, hold_width)
    };
    let low = (feather - width).clamp(-limit, limit);
    let high = (feather + width).clamp(-limit, limit);
    let neutral = feather.clamp(low, high);
    let phase = (2.0 * neutral - from) / PI;
    let before = ((from + (phase.ceil() - 1.0) * PI) * 0.5).clamp(low, neutral);
    let after = ((from + (phase.floor() + 1.0) * PI) * 0.5).clamp(neutral, high);
    let neutral_surge = surge(neutral);
    let benefit = |angle| (surge(angle) - neutral_surge) * power.signum();
    let left = benefit(before);
    let right = benefit(after);
    let endpoint = if left > right && left > 0.0 {
        before
    } else if right > 0.0 {
        after
    } else {
        neutral
    };
    let local_surge = surge(endpoint);
    let mut fraction = power.abs();
    if holds_inverted && power > 0.0 {
        let drive = simple_target / power;
        let drive_surge = surge(drive);
        if drive_surge > local_surge && drive * current < 0.0 {
            // Keep at least the first half of the stick on the current side. Stronger drive
            // can pull the boom across to make use of the next point of sail.
            let handoff = 0.5 + 0.5 * (local_surge - neutral_surge) / (drive_surge - neutral_surge);
            if power > handoff {
                let crossing = (power - handoff) / (1.0 - handoff);
                let local = motor_target(endpoint);
                return local + crossing * (drive - local);
            }
            fraction /= handoff;
        }
    }
    let want = neutral_surge + fraction * (local_surge - neutral_surge);
    motor_target(trim_for_surge(from, neutral, endpoint, want))
}

fn trim_for_surge(from: f64, neutral: f64, endpoint: f64, surge: f64) -> f64 {
    if endpoint == neutral {
        return neutral;
    }
    // Select the inverse cosine root on the arc between neutral and the endpoint.
    let halfway_phase = neutral + endpoint - from;
    let revolution = (halfway_phase / TAU).round() * TAU;
    ((from + revolution + libm::acos(surge.clamp(-1.0, 1.0)).copysign(halfway_phase - revolution))
        * 0.5)
        .clamp(neutral.min(endpoint), neutral.max(endpoint))
}

fn loaded_helm_target(
    parameters: BoatParameters,
    rudder_angle: f64,
    water: Vector,
    turn: f64,
) -> f64 {
    let limit = parameters.max_rudder_angle_rad;
    let capacity = parameters.max_rudder_actuator_torque_n_m();
    let broadside_force_per_speed_squared = rudder_foil_force(
        Vector::Y,
        0.0,
        parameters.water_density_kg_m3,
        parameters.rudder_area_m2,
        parameters.rudder_area_m2,
        parameters.rudder_draw_length_m,
        parameters.rudder_base_drag_coefficient,
    )
    .y;
    let peak_moment = broadside_force_per_speed_squared
        * water.length_squared()
        * parameters.rudder_draw_length_m
        * 0.5;
    if peak_moment == 0.0 {
        return -turn * limit;
    }
    let sternway = water.x > 0.0;
    let streamwise = if sternway { water } else { -water };
    let aligned = libm::atan2(streamwise.y, streamwise.x);
    let peak_load_in_travel = peak_moment * libm::sin((aligned.abs() + limit).min(FRAC_PI_2));
    let available = if sternway {
        // Astern, crossing the effort limit can sweep the blade to the stop. Retain
        // more recovery effort as the maximum load in the rudder's travel increases.
        capacity * (capacity / peak_load_in_travel).min(1.0)
    } else {
        capacity
    };
    let width = libm::asin((available / peak_moment).min(1.0));
    let (low, high) = if peak_moment <= capacity {
        (-limit, limit)
    } else {
        (
            (aligned - width).clamp(-limit, limit),
            (aligned + width).clamp(-limit, limit),
        )
    };
    let neutral = 0.0_f64.clamp(low, high);
    let endpoint = if turn > 0.0 { low } else { high };
    let angle = neutral + turn.abs() * (endpoint - neutral);
    let moment =
        if sternway { peak_moment } else { -peak_moment } * libm::sin(rudder_angle - aligned);
    let stiffness = capacity / (limit / 10.0);
    angle - moment.clamp(-capacity, capacity) / stiffness
}
