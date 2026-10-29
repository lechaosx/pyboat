/// Seconds of adaptation that settle the eye: adapting-field-blur.wgsl's
/// easing toward the field's target then completes.
pub const SETTLED_ADAPTATION_SECONDS: f32 = f32::MAX;

/// Seconds over which the eye adapts in the next rendered frame: the time
/// since the last rendered frame, or until one is rendered, long enough to
/// settle.
#[derive(Default)]
pub struct AdaptationClock {
    seconds_since_rendered: Option<f64>,
}

impl AdaptationClock {
    pub fn advance(&mut self, elapsed_seconds: f64) {
        if let Some(seconds) = &mut self.seconds_since_rendered {
            *seconds += elapsed_seconds.max(0.0);
        }
    }

    pub fn seconds(&self) -> f32 {
        self.seconds_since_rendered
            .map_or(SETTLED_ADAPTATION_SECONDS, |seconds| seconds as f32)
    }

    pub fn frame_rendered(&mut self) {
        self.seconds_since_rendered = Some(0.0);
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

/// Exposure that shows a view adapted to `illumination`, in scene light, at
/// its brightness. A zenith Sun's clear-sky light keeps its calibrated
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
    fn adaptation_settles_until_a_frame_is_rendered() {
        let mut clock = AdaptationClock::default();
        clock.advance(0.5);
        assert_eq!(clock.seconds(), SETTLED_ADAPTATION_SECONDS);
        clock.frame_rendered();
        assert_eq!(clock.seconds(), 0.0);
    }

    #[test]
    fn adaptation_spans_frames_left_unrendered() {
        let mut clock = AdaptationClock::default();
        clock.frame_rendered();
        clock.advance(0.25);
        clock.advance(-1.0);
        clock.advance(0.5);
        assert_eq!(clock.seconds(), 0.75);
        clock.frame_rendered();
        clock.advance(0.125);
        assert_eq!(clock.seconds(), 0.125);
    }
}
