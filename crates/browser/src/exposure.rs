#[derive(Default)]
pub struct ViewExposure {
    stops: Option<f64>,
}

impl ViewExposure {
    pub fn advance(&mut self, target: f32, elapsed_seconds: f64) -> f32 {
        let target = f64::from(target).log2();
        let stops = self.stops.get_or_insert(target);
        // Adapt in stops so equal brightness ratios take equal time. Darkness
        // takes longer to adapt to than bright light. Gameplay time constants
        // make adaptation perceptible within seconds.
        let time_constant_seconds = if target > *stops { 3.0 } else { 1.0 };
        let weight = -(-elapsed_seconds.max(0.0) / time_constant_seconds).exp_m1();
        *stops += (target - *stops) * weight;
        stops.exp2() as f32
    }
}

pub fn exposure_target(illumination: f32) -> f32 {
    // Linear material reflectance needs several stops at night; partial
    // compensation retains the difference between moonlit and moonless scenes.
    (0.45 / illumination.max(0.001)).powf(0.95).clamp(1.0, 16.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposure_target_is_bounded_and_preserves_night_contrast() {
        let day = exposure_target(1.0);
        let moonlight = exposure_target(0.12);
        let night = exposure_target(0.035);
        assert!(day < moonlight && moonlight < night);
        let maximum = exposure_target(0.0);
        assert!(maximum.is_finite());
        for illumination in [0.0, 0.035, 0.12, 1.0, 10.0] {
            assert!((day..=maximum).contains(&exposure_target(illumination)));
        }
        assert!(0.035 * night < 0.12 * moonlight);
        assert!(0.12 * moonlight < day);
    }

    #[test]
    fn exposure_only_compensates_darkness_and_plateaus_at_the_dark_limit() {
        for illumination in [0.45, 0.6, 0.8, 1.0, 10.0] {
            assert_eq!(exposure_target(illumination), 1.0);
        }
        assert_eq!(exposure_target(0.0), exposure_target(0.0001));
    }

    #[test]
    fn compensation_preserves_illumination_order_through_the_full_range() {
        let mut previous = 0.0;
        for sample in 1..=512 {
            let illumination = sample as f32 / 256.0;
            let compensated = illumination * exposure_target(illumination);
            assert!(
                compensated > previous,
                "brightness reversed at illumination {illumination}"
            );
            previous = compensated;
        }
    }

    #[test]
    fn moonlight_reduces_compensation_through_combined_celestial_illumination() {
        let world = simulation::WorldGeometry::default();
        let position = rapier2d::prelude::Vector::ZERO;
        let night_samples: Vec<_> = (0..28 * 24)
            .map(|hour| {
                game::celestial_lighting(game::world_celestial_conditions(
                    world,
                    f64::from(hour) * 3600.0,
                    position,
                ))
            })
            .filter(|light| light.sun_direction[2] < -0.3)
            .collect();
        let moonless = night_samples
            .iter()
            .min_by(|a, b| {
                a.moon_ambient_intensity
                    .total_cmp(&b.moon_ambient_intensity)
            })
            .unwrap();
        let moonlit = night_samples
            .iter()
            .max_by(|a, b| {
                a.moon_ambient_intensity
                    .total_cmp(&b.moon_ambient_intensity)
            })
            .unwrap();
        assert!(moonlit.moon_ambient_intensity > moonless.moon_ambient_intensity);
        assert!(moonlit.ambient_intensity > moonless.ambient_intensity);
        assert!(
            exposure_target(moonlit.ambient_intensity)
                < exposure_target(moonless.ambient_intensity)
        );
    }

    #[test]
    fn exposure_initializes_to_the_first_scene() {
        let mut exposure = ViewExposure::default();
        assert_eq!(exposure.advance(4.0, 0.0), 4.0);
    }

    #[test]
    fn exposure_adapts_gradually_in_both_directions() {
        let mut exposure = ViewExposure::default();
        exposure.advance(1.0, 0.0);
        let darkening = exposure.advance(4.0, 0.5);
        assert!(darkening > 1.0 && darkening < 4.0);
        let more_darkening = exposure.advance(4.0, 0.5);
        assert!(more_darkening > darkening && more_darkening < 4.0);
        let brightening = exposure.advance(1.0, 0.5);
        assert!(brightening > 1.0 && brightening < more_darkening);
        assert!((exposure.advance(1.0, 60.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn exposure_does_not_adapt_without_elapsed_time() {
        let mut exposure = ViewExposure::default();
        exposure.advance(1.0, 0.0);
        assert_eq!(exposure.advance(4.0, 0.0), 1.0);
        assert_eq!(exposure.advance(4.0, -1.0), 1.0);
    }

    #[test]
    fn exposure_adaptation_is_independent_of_frame_rate() {
        let adapt = |frames: u32| {
            let mut exposure = ViewExposure::default();
            exposure.advance(1.0, 0.0);
            let mut result = 1.0;
            for _ in 0..frames {
                result = exposure.advance(4.0, 2.0 / f64::from(frames));
            }
            result
        };
        assert!((adapt(60) - adapt(288)).abs() < 1e-6);
    }

    #[test]
    fn exposure_adapts_to_bright_light_faster_than_darkness() {
        let mut darkening = ViewExposure::default();
        let mut brightening = ViewExposure::default();
        darkening.advance(1.0, 0.0);
        brightening.advance(4.0, 0.0);
        let dark_progress = darkening.advance(4.0, 1.0).log2() / 2.0;
        let bright_progress = (2.0 - brightening.advance(1.0, 1.0).log2()) / 2.0;
        assert!(bright_progress > dark_progress);
    }
}
