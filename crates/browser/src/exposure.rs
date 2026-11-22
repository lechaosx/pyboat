/// Seconds of adaptation that settle the eye: adapting-field-blur.wgsl's
/// easing toward the field's target then completes.
pub const SETTLED_ADAPTATION_SECONDS: f32 = f32::MAX;

/// Seconds over which the eye adapts in the next rendered frame: the time
/// since the last rendered frame. Loading time does not advance the initial
/// state. The first visible view starts settled to its local field.
#[derive(Default)]
pub struct AdaptationClock {
    seconds_since_rendered: Option<f64>,
}

impl AdaptationClock {
    pub fn is_initial(&self) -> bool {
        self.seconds_since_rendered.is_none()
    }

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

/// Photopic brightness exponent retained for the diagnostic gain parameter.
pub const ADAPTED_BRIGHTNESS_EXPONENT: f32 = 1.0 / 12.0;

const RESPONSE_EXPONENT: f32 = 0.73;
pub(crate) const DAYLIGHT_BACKGROUND_CD_M2: f32 =
    0.2 * game::ZENITH_DAYLIGHT_LUX / std::f32::consts::PI;

/// Pattanaik et al. (2000), Eq.2–7: neural backgrounds in cd/m² and
/// available rod/cone pigment. Rod luminance is photopic-equivalent in this
/// RGB renderer; spectral metamerism remains outside the model.
#[derive(Clone, Copy)]
pub(crate) struct EyeState(pub [f32; 4]);

impl EyeState {
    pub fn settled(illumination: f32) -> Self {
        let background = illumination.max(game::NIGHT_SKY_LIGHT) * DAYLIGHT_BACKGROUND_CD_M2;
        Self([
            background,
            background,
            0.04 / (0.04 + background),
            2.0e6 / (2.0e6 + background),
        ])
    }

    fn response(self, luminance: f32) -> f32 {
        let [rod, cone, rod_pigment, cone_pigment] = self.0;
        let response =
            |sigma: f32, pigment| pigment / (1.0 + (sigma / luminance).powf(RESPONSE_EXPONENT));
        response(rod_sigma(rod), rod_pigment) + response(cone_sigma(cone), cone_pigment)
    }
}

fn rod_sigma(background: f32) -> f32 {
    if background == 0.0 {
        return 2.5874 / 19000.0;
    }
    let j = 1.0 / (5.0e5 * background + 1.0);
    2.5874 * background
        / (19000.0 * j * j * background
            + 0.2615 * (1.0 - j * j).powi(4) * background.powf(1.0 / 6.0))
}

fn cone_sigma(background: f32) -> f32 {
    if background == 0.0 {
        return 12.9223;
    }
    let k = 1.0 / (5.0 * background + 1.0);
    12.9223 * background
        / (k.powi(4) * background + 0.171 * (1.0 - k.powi(4)).powi(2) * background.cbrt())
}

fn daylight_inverse(response: f32) -> f32 {
    if response <= 0.0 {
        return 0.0;
    }
    let [rod, cone, rod_pigment, cone_pigment] = EyeState::settled(1.0).0;
    let rod = rod_sigma(rod).powf(RESPONSE_EXPONENT);
    let cone = cone_sigma(cone).powf(RESPONSE_EXPONENT);
    // With x=L^n the sum of the two Eq.2 responses is quadratic in x.
    // Select the positive root without subtracting nearly equal numbers.
    let a = rod_pigment + cone_pigment - response;
    let b = rod_pigment * cone + cone_pigment * rod - response * (rod + cone);
    let c = response * rod * cone;
    let discriminant = (b * b + 4.0 * a * c).sqrt();
    let x = if b >= 0.0 {
        2.0 * c / (b + discriminant)
    } else {
        (discriminant - b) / (2.0 * a)
    };
    x.powf(1.0 / RESPONSE_EXPONENT)
}

/// Effective gain of a neutral surface in a settled view. Display conversion
/// uses the virtual daylight observer's inverse response with zero black;
/// the per-surface shader also carries rod/cone color and temporal state.
pub fn exposure_target(illumination: f32) -> f32 {
    let illumination = illumination.clamp(game::NIGHT_SKY_LIGHT, 1.0);
    let background = illumination * DAYLIGHT_BACKGROUND_CD_M2;
    let response = EyeState::settled(illumination).response(background);
    daylight_inverse(response) / background
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

    #[test]
    fn starlight_uses_the_rod_and_cone_response_at_the_virtual_daylight_reference() {
        // Pattanaik et al. (2000), Eq.2–8, with a zero-black reference
        // observer at 20% of the calibrated zenith daylight luminance.
        let light = 4.65e-5 / (0.2 * game::ZENITH_DAYLIGHT_LUX / std::f32::consts::PI);
        let gray = 0.2 * light * exposure_target(light);
        assert!((gray - 0.015_267_765).abs() < 1e-7, "{gray}");
    }

    #[test]
    fn a_view_adapted_to_a_high_sun_keeps_its_calibrated_brightness() {
        let zenith = equator_light(0.25, 0.5, 0.5);
        assert!((exposure_target(zenith.ambient_intensity) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn settled_neutral_brightness_matches_the_published_response_at_night_levels() {
        // Independent double-precision Eq.2–8 reference, inverted through
        // the zero-black daylight observer, before the display shoulder.
        for (background, expected) in [(0.0153, 0.027_503_162), (0.00438, 0.028_675_423)] {
            let illumination = background / DAYLIGHT_BACKGROUND_CD_M2;
            let gray = 0.2 * illumination * exposure_target(illumination);
            assert!((gray - expected).abs() < 1e-7, "{background}: {gray}");
        }
    }

    #[test]
    fn exposure_is_bounded_by_full_dark_adaptation_under_starlight() {
        let night = equator_light(0.25, 0.0, 0.0).ambient_intensity;
        assert!(exposure_target(night).is_finite());
        assert_eq!(exposure_target(0.0), exposure_target(night));
        assert_eq!(exposure_target(0.1 * night), exposure_target(night));
    }

    #[test]
    fn compensation_stays_finite_and_bounded_through_the_light_range() {
        for stop in -28..=0 {
            let illumination = 2.0_f32.powi(stop);
            let compensated = illumination * exposure_target(illumination);
            assert!(
                compensated.is_finite() && (0.0..=1.0 + 1e-6).contains(&compensated),
                "brightness {compensated} at illumination {illumination}"
            );
        }
    }

    #[test]
    fn first_visible_match_starts_with_settled_vision() {
        let mut clock = AdaptationClock::default();
        clock.advance(30.0);
        assert_eq!(clock.seconds(), SETTLED_ADAPTATION_SECONDS);
        assert!(clock.is_initial());
        clock.frame_rendered();
        clock.advance(0.25);
        clock.advance(0.5);
        assert_eq!(clock.seconds(), 0.75);
        assert!(!clock.is_initial());
        clock.frame_rendered();
        assert_eq!(clock.seconds(), 0.0);
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
