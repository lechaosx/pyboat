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

/// Brightness of an adapted view grows as this power of the light it is
/// adapted to: CIECAM02's brightness of an adapted white follows the fourth
/// root of its luminance-level adaptation factor, which grows as the cube root
/// of photopic adapting luminance (Moroney et al. 2002, Proc. IS&T/SID Color
/// Imaging Conf. 10). Its photopic branch is kept below photopic light, where
/// the model's own and real dim vision would leave night unreadable on a
/// display; this compromise keeps night views readable. display.wgsl mirrors it.
pub const ADAPTED_BRIGHTNESS_EXPONENT: f32 = 1.0 / 12.0;

/// Exposure that shows a view adapted to clear-sky `illumination`, in scene
/// light, at its brightness. A zenith Sun's light keeps its calibrated
/// brightness, and adaptation stops at a clear moonless sky's light, which
/// fully dark-adapted vision is matched to.
pub fn exposure_target(illumination: f32) -> f32 {
    illumination
        .clamp(game::NIGHT_SKY_LIGHT, 1.0)
        .powf(ADAPTED_BRIGHTNESS_EXPONENT - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn equator_light(
        season_phase: f32,
        day_phase: f32,
        lunar_longitude_phase: f32,
    ) -> game::CelestialLighting {
        game::celestial_lighting(game::CelestialConditions {
            geographic_latitude_degrees: 0.0,
            celestial_latitude_degrees: 0.0,
            season_phase,
            day_phase,
            lunar_longitude_phase,
        })
    }

    fn adapted_brightness(light: game::CelestialLighting) -> f32 {
        light.ambient_intensity * exposure_target(light.ambient_intensity)
    }

    #[test]
    fn a_view_adapted_to_a_high_sun_keeps_its_calibrated_brightness() {
        let zenith = equator_light(0.25, 0.5, 0.5);
        assert!((adapted_brightness(zenith) - 1.0).abs() < 1e-6);
        assert_eq!(exposure_target(zenith.ambient_intensity), 1.0);
        let afternoon = equator_light(0.25, 0.5 + 45.0 / 360.0, 0.5);
        assert!(
            adapted_brightness(afternoon) > 0.95,
            "{}",
            adapted_brightness(afternoon)
        );
    }

    /// Full Moon overhead, first quarter Moon at 65 degrees, and a moonless
    /// sky, each with the Sun 18 degrees below the horizon or lower.
    #[test]
    fn adapted_night_views_keep_the_order_of_moon_phases() {
        let full = equator_light(0.5, 0.0, 0.75);
        let quarter = equator_light(0.5, 0.82, 0.5);
        let moonless = equator_light(0.5, 0.82, 0.25);
        assert!(quarter.moon_direction[2] > 0.85 && quarter.sun_direction[2] < -0.3);
        let [full, quarter, moonless] = [full, quarter, moonless].map(adapted_brightness);
        // Each step is several times a just-noticeable brightness difference.
        assert!(full > 1.1 * quarter, "full {full}, quarter {quarter}");
        assert!(
            quarter > 1.1 * moonless,
            "quarter {quarter}, moonless {moonless}"
        );
        assert!(moonless > 0.1, "{moonless}");
    }

    #[test]
    fn exposure_is_bounded_by_full_dark_adaptation_under_starlight() {
        let night = equator_light(0.25, 0.0, 0.0).ambient_intensity;
        assert!(exposure_target(night).is_finite());
        assert_eq!(exposure_target(0.0), exposure_target(night));
        assert_eq!(exposure_target(0.1 * night), exposure_target(night));
    }

    #[test]
    fn compensation_preserves_illumination_order_through_the_full_range() {
        let mut previous = 0.0;
        for stop in -28..=0 {
            let illumination = 2.0_f32.powi(stop);
            let compensated = illumination * exposure_target(illumination);
            assert!(
                compensated >= previous,
                "brightness reversed at illumination {illumination}"
            );
            previous = compensated;
        }
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
