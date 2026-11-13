use rapier2d_f64::prelude::Vector;
use simulation::{BoatMotion, MotionIntent};

#[derive(Default)]
pub struct Outcomes {
    pub turn_rad: f64,
    pub opposite_progress_m: f64,
    pub forward_m: f64,
    pub lateral_m: f64,
    pub signed_turn_rad: f64,
    pub wrong_way_turn_rad: f64,
    pub neutral_turn_rad: f64,
    pub absolute_lateral_m: f64,
    pub elapsed_s: f64,
    pub first_signed_response_s: Option<f64>,
}

impl Outcomes {
    pub fn observe(
        &mut self,
        before: BoatMotion,
        after: BoatMotion,
        current: Vector,
        intent: MotionIntent,
        dt: f64,
    ) {
        let raw = after.angle_rad - before.angle_rad;
        let turn = raw.sin().atan2(raw.cos());
        let heading = before.angle_rad + turn * 0.5;
        let forward = Vector::new(heading.cos(), heading.sin());
        let displacement = after.position_m - before.position_m - current * dt;
        let lateral = forward.perp_dot(displacement);
        self.forward_m += forward.dot(displacement);
        self.lateral_m += lateral;
        self.absolute_lateral_m += lateral.abs();
        self.elapsed_s += dt;
        self.turn_rad += turn;
        if intent.forward != 0.0 {
            self.opposite_progress_m +=
                (-intent.forward.signum() * forward.dot(displacement)).max(0.0);
        }
        if intent.turn == 0.0 {
            self.neutral_turn_rad += turn.abs();
        } else {
            let signed = intent.turn.signum() * turn;
            self.signed_turn_rad += signed;
            self.wrong_way_turn_rad += (-signed).max(0.0);
            if self.first_signed_response_s.is_none()
                && intent.turn * after.angular_velocity_rad_s > 0.0
            {
                self.first_signed_response_s = Some(self.elapsed_s);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pose(position: Vector, angle: f64) -> BoatMotion {
        BoatMotion {
            position_m: position,
            velocity_m_s: Vector::ZERO,
            angle_rad: angle,
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: 0.0,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.0,
            rudder_angular_velocity_rad_s: 0.0,
        }
    }
    #[test]
    fn motion_measures_remove_current_and_use_boat_axes() {
        let mut result = Outcomes::default();
        result.observe(
            pose(Vector::ZERO, std::f64::consts::FRAC_PI_2),
            pose(Vector::new(2.0, 3.0), std::f64::consts::FRAC_PI_2),
            Vector::X,
            MotionIntent {
                forward: 0.5,
                turn: 0.0,
            },
            2.0,
        );
        assert!((result.forward_m - 3.0).abs() < 1e-12);
        assert!(result.lateral_m.abs() < 1e-12);
    }
    #[test]
    fn turning_tracks_sign_reversal_and_wrapped_angles_without_reward_tradeoff() {
        let mut result = Outcomes::default();
        let a = pose(Vector::ZERO, std::f64::consts::PI - 0.1);
        let b = pose(Vector::ZERO, -std::f64::consts::PI + 0.1);
        result.observe(
            a,
            b,
            Vector::ZERO,
            MotionIntent {
                forward: 1.0,
                turn: -0.25,
            },
            1.0,
        );
        assert!((result.signed_turn_rad + 0.2).abs() < 1e-12);
        assert!((result.wrong_way_turn_rad - 0.2).abs() < 1e-12);
        result.observe(
            b,
            a,
            Vector::ZERO,
            MotionIntent {
                forward: 0.0,
                turn: 0.0,
            },
            1.0,
        );
        assert!((result.neutral_turn_rad - 0.2).abs() < 1e-12);
    }
}
