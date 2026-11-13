use rapier2d_f64::prelude::{Rotation, Vector};
use simulation::{BoatMotion, Environment, MotionIntent};

pub const SEGMENT_TICKS: usize = 240;
pub const SEEDS: [(u64, &str); 4] = [
    (11, "training"),
    (29, "training"),
    (1009, "heldout"),
    (1021, "heldout"),
];

pub struct Case {
    pub name: &'static str,
    pub seed: u64,
    pub split: &'static str,
    pub initial: BoatMotion,
    pub base: Environment,
    pub intents: [MotionIntent; 3],
    pub changing_flow: bool,
}

impl Case {
    pub fn environment(&self, tick: usize) -> Environment {
        if !self.changing_flow {
            return self.base;
        }
        let seconds = tick as f64 * simulation::DT_SECONDS;
        let phase = self.seed as f64 * 0.17;
        Environment {
            true_wind_velocity_m_s: self.base.true_wind_velocity_m_s
                * (1.0 + 0.3 * (seconds * 0.8 + phase).sin()),
            true_water_velocity_m_s: self.base.true_water_velocity_m_s
                + Vector::new(
                    0.3 * (seconds * 0.5 + phase).sin(),
                    0.2 * (seconds * 0.7).sin(),
                ),
            ..self.base
        }
    }
}

pub fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    for (seed, split) in SEEDS {
        let heading = (seed as f64 * 0.37) % std::f64::consts::TAU;
        let rotate = |v| Rotation::new(heading).transform_vector(v);
        let sign = if seed == 11 || seed == 1009 {
            1.0
        } else {
            -1.0
        };
        let wind = rotate(Vector::new(
            0.3 * (seed as f64).sin(),
            sign * (5.0 + 1.5 * (seed as f64).cos()),
        ));
        let current = rotate(Vector::new(0.2, -sign * 0.1));
        let intent = |forward, turn| MotionIntent {
            forward,
            turn: turn * sign,
        };
        for (name, speed, yaw, intents) in [
            (
                "fractional_drive",
                2.0,
                0.0,
                [intent(0.1, 0.0), intent(0.5, 0.0), intent(1.0, 0.0)],
            ),
            (
                "near_zero_crossing",
                0.0,
                0.001 * sign,
                [intent(-1e-6, 0.0), intent(0.0, 0.0), intent(1e-6, 0.0)],
            ),
            (
                "startup",
                0.0,
                0.0,
                [intent(0.25, 0.0), intent(1.0, 0.0), intent(0.0, 0.0)],
            ),
            (
                "underway_axes",
                2.0,
                0.0,
                [intent(0.5, 0.25), intent(0.5, -0.5), intent(0.5, 0.0)],
            ),
            (
                "turn_only",
                2.0,
                0.0,
                [intent(0.0, 1.0), intent(0.0, -0.25), intent(0.0, 0.0)],
            ),
            (
                "stalled_upwind",
                0.0,
                0.0,
                [intent(1.0, 0.0), intent(1.0, 0.5), intent(1.0, -1.0)],
            ),
            (
                "reverse",
                1.0,
                0.0,
                [intent(-0.5, 0.0), intent(-1.0, 0.5), intent(-0.25, -0.5)],
            ),
            (
                "intent_reversal",
                2.0,
                0.0,
                [intent(1.0, 0.5), intent(-0.5, -0.5), intent(0.0, 0.0)],
            ),
            (
                "flow_change",
                2.0,
                0.0,
                [intent(0.5, 0.0), intent(0.5, 0.5), intent(0.5, 0.0)],
            ),
            (
                "equal_state_release",
                2.0,
                0.2 * sign,
                [intent(0.0, 0.0); 3],
            ),
        ] {
            let base = Environment {
                true_wind_velocity_m_s: if name == "stalled_upwind" {
                    rotate(Vector::new(-6.0, 0.0))
                } else if name == "near_zero_crossing" {
                    rotate(Vector::new(6.0, 0.3 * sign))
                } else {
                    wind
                },
                true_water_velocity_m_s: current,
                ..Environment::default()
            };
            result.push(Case {
                name,
                seed,
                split,
                initial: BoatMotion {
                    position_m: Vector::ZERO,
                    velocity_m_s: current + rotate(Vector::new(speed, 0.0)),
                    angle_rad: heading,
                    angular_velocity_rad_s: yaw,
                    sail_angle_rad: 0.3 * sign,
                    sail_angular_velocity_rad_s: 0.0,
                    rudder_angle_rad: if name == "equal_state_release" {
                        0.2 * sign
                    } else {
                        0.0
                    },
                    rudder_angular_velocity_rad_s: 0.0,
                },
                base,
                intents,
                changing_flow: name == "flow_change",
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_split_is_disjoint_reproducible_and_has_independent_fractional_axes() {
        let first = cases();
        let second = cases();
        assert_eq!(first.len(), second.len());
        assert!(!first.is_empty());
        for (a, b) in first.iter().zip(&second) {
            assert_eq!(a.initial, b.initial);
            assert_eq!(a.intents, b.intents);
            for tick in [0, 239, 240, 719] {
                assert_eq!(a.environment(tick), b.environment(tick));
            }
        }
        for split in ["training", "heldout"] {
            let intents = first
                .iter()
                .filter(|c| c.split == split)
                .flat_map(|c| c.intents)
                .collect::<Vec<_>>();
            for forward in [-1.0, -0.5, -0.25, 0.0, 0.25, 0.5, 1.0] {
                assert!(intents.iter().any(|i| i.forward == forward));
            }
            for turn in [-1.0, -0.5, -0.25, 0.0, 0.25, 0.5, 1.0] {
                assert!(intents.iter().any(|i| i.turn == turn));
            }
            assert!(intents.iter().any(|i| i.forward != 0.0 && i.turn == 0.0));
            assert!(intents.iter().any(|i| i.forward == 0.0 && i.turn != 0.0));
        }
        assert!(
            SEEDS
                .iter()
                .filter(|(_, s)| *s == "training")
                .all(|(seed, _)| !SEEDS.contains(&(*seed, "heldout")))
        );
    }
}
