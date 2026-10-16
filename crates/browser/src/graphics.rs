/// Memory for the baked surface's textures by device class; above it the bake
/// coarsens. At 28 bytes per texel and two levels on the √2 ladder, 256 MiB
/// holds a 1080p scene at one texel per pixel (223 MiB) and 512 MiB a 1440p
/// one (396 MiB). The raised cap needs `navigator.deviceMemory`'s largest
/// report, 8 GB; browsers without it keep 256 MiB.
pub fn baked_surface_memory_bytes(touch_screen: bool, device_memory_gb: Option<f64>) -> u64 {
    if touch_screen {
        64 << 20
    } else if device_memory_gb.is_some_and(|memory| memory >= 8.0) {
        512 << 20
    } else {
        256 << 20
    }
}

/// Lowest scene scale, matching the 2× per-axis upscale EASU is designed for.
/// The half-scale bench scenarios render images for inspecting shorelines,
/// shallow hazards, and the ice edge at this scale.
pub const MINIMUM_RENDER_SCALE: f32 = 0.5;

pub fn scene_render_size(output_width: u32, output_height: u32, render_scale: f32) -> (u32, u32) {
    let scale = render_scale.clamp(MINIMUM_RENDER_SCALE, 1.0);
    let scaled = |dimension: u32| ((dimension as f32 * scale).round() as u32).clamp(1, dimension);
    (scaled(output_width), scaled(output_height))
}

/// Paces rendering below the display refresh rate while callers keep advancing
/// input and simulation on every display frame. A due frame stays due until
/// `frame_rendered` reports that one was submitted.
#[derive(Default)]
pub struct FramePacer {
    budget_seconds: f64,
}

impl FramePacer {
    pub fn frame_due(&mut self, elapsed_seconds: f64, frame_rate_cap: Option<f64>) -> bool {
        let Some(frame_rate_cap) = frame_rate_cap else {
            self.budget_seconds = 0.0;
            return true;
        };
        let interval_seconds = frame_rate_cap.recip();
        let elapsed_seconds = elapsed_seconds.max(0.0);
        self.budget_seconds += elapsed_seconds;
        // Render on the display frame nearest each due time, so timestamp
        // jitter cannot push a frame a whole refresh late.
        self.budget_seconds + 0.5 * elapsed_seconds >= interval_seconds
    }

    pub fn frame_rendered(&mut self, frame_rate_cap: Option<f64>) {
        if let Some(frame_rate_cap) = frame_rate_cap {
            let interval_seconds = frame_rate_cap.recip();
            // Steady pacing carries less than half a refresh; the bound stops a
            // pause from becoming a burst of catch-up frames.
            self.budget_seconds =
                (self.budget_seconds - interval_seconds).min(0.5 * interval_seconds);
        }
    }
}

const RESOLUTION_WINDOW_FRAMES: usize = 30;
const RESOLUTION_STEP: f32 = 0.05;
/// Share of the frame-time target that the measured window may use after a change.
const RESOLUTION_HEADROOM: f64 = 0.85;
const UNCAPPED_TARGET_FRAME_RATE: f64 = 60.0;

/// Chooses a scene scale from per-frame GPU times. Falling fits the
/// measurement at once; rising takes one step per window and only when the
/// raised scale is predicted to fit, so a load whose cost grows no faster than
/// pixel count settles instead of oscillating. The target is the frame-rate
/// cap's interval; observed frame intervals never set it, because browsers slow
/// animation callbacks to match an overloaded GPU.
pub struct ResolutionController {
    scale: f32,
    frame_ms: Vec<f64>,
}

impl ResolutionController {
    pub fn new(scale: f32) -> Self {
        Self {
            scale,
            frame_ms: Vec::with_capacity(RESOLUTION_WINDOW_FRAMES),
        }
    }

    pub fn record_frame(
        &mut self,
        frame_ms: f64,
        frame_rate_cap: Option<f64>,
        maximum_scale: f32,
    ) -> f32 {
        let maximum_scale = maximum_scale.clamp(MINIMUM_RENDER_SCALE, 1.0);
        self.scale = self.scale.min(maximum_scale);
        self.frame_ms.push(frame_ms);
        if self.frame_ms.len() < RESOLUTION_WINDOW_FRAMES {
            return self.scale;
        }
        self.frame_ms.sort_by(f64::total_cmp);
        let median_ms = self.frame_ms[RESOLUTION_WINDOW_FRAMES / 2];
        self.frame_ms.clear();
        let target_ms = 1000.0 / frame_rate_cap.unwrap_or(UNCAPPED_TARGET_FRAME_RATE);
        let scale = f64::from(self.scale);
        if median_ms > target_ms {
            // Scene cost follows pixel count, the square of the scale.
            let fitted = (scale * (RESOLUTION_HEADROOM * target_ms / median_ms).sqrt()) as f32;
            self.scale = fitted
                .min(self.scale - RESOLUTION_STEP)
                .max(MINIMUM_RENDER_SCALE);
        } else if self.scale < maximum_scale {
            let raised = (self.scale + RESOLUTION_STEP).min(maximum_scale);
            if median_ms * (f64::from(raised) / scale).powi(2) < RESOLUTION_HEADROOM * target_ms {
                self.scale = raised;
            }
        }
        self.scale
    }
}

/// Graphics tiers change only appearance-only surface detail;
/// `tools/playground-capture/tests/graphics_quality.rs` checks that shorelines,
/// shallow water, the sea-ice edge, and darkening match `High`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphicsQuality {
    Low,
    Medium,
    High,
}

impl GraphicsQuality {
    const ALL: [Self; 3] = [Self::Low, Self::Medium, Self::High];

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|quality| quality.name() == name)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }

    /// Override constants for the scene shader's fragment stage; `High` keeps
    /// the shader defaults. Measured at 1080p on a Radeon 780M against `High`:
    /// - Decorative water normals evaluate four 4D octaves from 18 m. One
    ///   octave saves about 2.5 ms of a 9.8 ms open-water frame, two about
    ///   1.5 ms; 99th-percentile image changes stay at 1 level.
    /// - The metre-scale micro-relief material field takes six octaves. The
    ///   baked surface holds all six at every tier, so the limit applies only
    ///   to positions evaluated directly.
    pub fn scene_constants(self) -> &'static [(&'static str, f64)] {
        match self {
            Self::Low => &[
                ("DECORATIVE_WATER_OCTAVES", 1.0),
                ("MATERIAL_MICRO_OCTAVES", 0.0),
            ],
            Self::Medium => &[
                ("DECORATIVE_WATER_OCTAVES", 2.0),
                ("MATERIAL_MICRO_OCTAVES", 3.0),
            ],
            Self::High => &[],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphicsSettings {
    pub maximum_pixel_ratio: f64,
    pub render_scale: f32,
    pub frame_rate_cap: Option<f64>,
    pub automatic_resolution: bool,
    pub quality: GraphicsQuality,
}

impl GraphicsSettings {
    pub fn from_query(search: &str, touch_screen: bool) -> Self {
        let mut settings = if touch_screen {
            Self {
                maximum_pixel_ratio: 2.0,
                render_scale: 0.75,
                frame_rate_cap: Some(30.0),
                automatic_resolution: true,
                quality: GraphicsQuality::Low,
            }
        } else {
            Self {
                maximum_pixel_ratio: 2.0,
                render_scale: 1.0,
                frame_rate_cap: None,
                automatic_resolution: false,
                quality: GraphicsQuality::High,
            }
        };
        for (key, value) in search
            .trim_start_matches('?')
            .split('&')
            .filter_map(|parameter| parameter.split_once('='))
        {
            let number = value
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite());
            match (key, number) {
                ("renderScale", Some(scale)) => {
                    settings.render_scale = (scale as f32).clamp(MINIMUM_RENDER_SCALE, 1.0);
                }
                ("pixelRatio", Some(ratio)) => settings.maximum_pixel_ratio = ratio.max(1.0),
                ("fps", Some(0.0)) => settings.frame_rate_cap = None,
                ("fps", Some(rate)) if rate > 0.0 => settings.frame_rate_cap = Some(rate),
                ("autoResolution", Some(enabled)) => {
                    settings.automatic_resolution = enabled != 0.0;
                }
                ("quality", _) => {
                    settings.quality = GraphicsQuality::parse(value).unwrap_or(settings.quality);
                }
                _ => {}
            }
        }
        settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_large_reported_device_memory_raises_only_the_desktop_bake_memory() {
        let desktop = baked_surface_memory_bytes(false, None);
        assert_eq!(baked_surface_memory_bytes(false, Some(4.0)), desktop);
        assert!(baked_surface_memory_bytes(false, Some(8.0)) > desktop);
        let touch = baked_surface_memory_bytes(true, None);
        assert!(touch < desktop);
        assert_eq!(baked_surface_memory_bytes(true, Some(8.0)), touch);
    }

    #[test]
    fn scene_size_rounds_the_scaled_output_within_its_bounds() {
        assert_eq!(scene_render_size(1921, 1081, 1.0), (1921, 1081));
        assert_eq!(scene_render_size(1920, 1080, 0.5), (960, 540));
        assert_eq!(scene_render_size(1920, 1080, 0.67), (1286, 724));
        assert_eq!(scene_render_size(1920, 1080, 0.25), (960, 540));
        assert_eq!(scene_render_size(1920, 1080, 2.0), (1920, 1080));
        assert_eq!(scene_render_size(1, 3, 0.5), (1, 2));
    }

    fn rendered_frames(pacer: &mut FramePacer, interval_ms: &[f64], cap: Option<f64>) -> Vec<bool> {
        interval_ms
            .iter()
            .map(|interval| {
                let due = pacer.frame_due(interval / 1000.0, cap);
                if due {
                    pacer.frame_rendered(cap);
                }
                due
            })
            .collect()
    }

    #[test]
    fn a_due_frame_that_could_not_render_stays_due() {
        let mut pacer = FramePacer::default();
        let frame = 1.0 / 60.0;
        assert!(!pacer.frame_due(frame, Some(30.0)));
        assert!(pacer.frame_due(frame, Some(30.0)));
        assert!(pacer.frame_due(frame, Some(30.0)));
        pacer.frame_rendered(Some(30.0));
        let after = rendered_frames(&mut pacer, &[1000.0 * frame; 4], Some(30.0));
        assert_eq!(after.iter().filter(|due| **due).count(), 2, "{after:?}");
    }

    #[test]
    fn uncapped_or_slower_than_cap_display_renders_every_frame() {
        let display = [1000.0 / 60.0; 30];
        for cap in [None, Some(60.0), Some(144.0)] {
            let frames = rendered_frames(&mut FramePacer::default(), &display, cap);
            assert!(frames.iter().all(|due| *due), "{cap:?}");
        }
    }

    #[test]
    fn a_cap_dividing_the_refresh_rate_renders_evenly_despite_jitter() {
        let jittered: Vec<f64> = (0..60)
            .map(|frame| 1000.0 / 60.0 + if frame % 3 == 0 { 0.4 } else { -0.2 })
            .collect();
        let frames = rendered_frames(&mut FramePacer::default(), &jittered, Some(30.0));
        for pair in frames.chunks(2) {
            assert_eq!(pair.iter().filter(|due| **due).count(), 1, "{frames:?}");
        }
    }

    #[test]
    fn a_cap_not_dividing_the_refresh_rate_holds_its_average_rate() {
        let display = [1000.0 / 144.0; 144 * 5];
        let frames = rendered_frames(&mut FramePacer::default(), &display, Some(60.0));
        let rendered = frames.iter().filter(|due| **due).count();
        assert!((299..=301).contains(&rendered), "{rendered}");
        let segments: Vec<usize> = frames.split(|due| *due).map(<[bool]>::len).collect();
        let gaps = &segments[1..segments.len() - 1];
        assert!(gaps.iter().all(|gap| (1..=2).contains(gap)), "{gaps:?}");
    }

    #[test]
    fn a_long_pause_renders_once_without_a_catch_up_burst() {
        let mut pacer = FramePacer::default();
        rendered_frames(&mut pacer, &[1000.0 / 60.0; 10], Some(30.0));
        assert!(pacer.frame_due(5.0, Some(30.0)));
        pacer.frame_rendered(Some(30.0));
        let after = rendered_frames(&mut pacer, &[1000.0 / 60.0; 10], Some(30.0));
        assert!(after.iter().filter(|due| **due).count() <= 6, "{after:?}");
    }

    fn record_window(controller: &mut ResolutionController, frame_ms: f64, maximum: f32) -> f32 {
        let mut scale = 0.0;
        for _ in 0..RESOLUTION_WINDOW_FRAMES {
            scale = controller.record_frame(frame_ms, Some(60.0), maximum);
        }
        scale
    }

    #[test]
    fn resolution_holds_until_a_window_of_frames_is_measured() {
        let mut controller = ResolutionController::new(1.0);
        for _ in 1..RESOLUTION_WINDOW_FRAMES {
            assert_eq!(controller.record_frame(40.0, Some(60.0), 1.0), 1.0);
        }
        assert!(controller.record_frame(40.0, Some(60.0), 1.0) < 1.0);
    }

    #[test]
    fn resolution_falls_with_pixel_cost_to_the_minimum() {
        let mut controller = ResolutionController::new(1.0);
        let fitted = record_window(&mut controller, 1000.0 / 60.0 * 4.0, 1.0);
        assert_eq!(fitted, MINIMUM_RENDER_SCALE);
        assert_eq!(
            record_window(&mut controller, 100.0, 1.0),
            MINIMUM_RENDER_SCALE
        );

        let mut controller = ResolutionController::new(1.0);
        let slightly_over = record_window(&mut controller, 17.0, 1.0);
        assert!(
            (0.85..=0.95).contains(&slightly_over),
            "a small overrun still drops at least one step: {slightly_over}"
        );
    }

    #[test]
    fn resolution_rises_one_step_only_with_predicted_headroom() {
        let mut controller = ResolutionController::new(0.7);
        assert_eq!(record_window(&mut controller, 13.0, 1.0), 0.7);
        let raised = record_window(&mut controller, 8.0, 1.0);
        assert!((raised - 0.75).abs() < 1e-6, "{raised}");
        for _ in 0..10 {
            record_window(&mut controller, 1.0, 0.9);
        }
        assert_eq!(controller.record_frame(1.0, Some(60.0), 0.9), 0.9);
    }

    #[test]
    fn resolution_follows_a_lowered_maximum_immediately() {
        let mut controller = ResolutionController::new(1.0);
        assert_eq!(controller.record_frame(1.0, None, 0.6), 0.6);
    }

    #[test]
    fn uncapped_resolution_targets_sixty_frames_per_second() {
        let mut capped = ResolutionController::new(1.0);
        let mut uncapped = ResolutionController::new(1.0);
        for _ in 0..RESOLUTION_WINDOW_FRAMES {
            capped.record_frame(20.0, Some(30.0), 1.0);
            uncapped.record_frame(20.0, None, 1.0);
        }
        assert_eq!(capped.record_frame(20.0, Some(30.0), 1.0), 1.0);
        assert!(uncapped.record_frame(20.0, None, 1.0) < 1.0);
    }

    #[test]
    fn graphics_settings_default_by_device_and_accept_valid_overrides() {
        let desktop = GraphicsSettings::from_query("", false);
        assert_eq!(desktop.render_scale, 1.0);
        assert_eq!(desktop.frame_rate_cap, None);
        assert!(!desktop.automatic_resolution);
        let touch = GraphicsSettings::from_query("?webtransportPort=1", true);
        assert!(touch.render_scale < 1.0);
        assert!(touch.frame_rate_cap.is_some());
        assert!(touch.automatic_resolution);
        assert!(touch.maximum_pixel_ratio <= desktop.maximum_pixel_ratio);

        assert_eq!(
            GraphicsSettings::from_query(
                "?renderScale=0.6&pixelRatio=3&fps=45&autoResolution=1",
                false
            ),
            GraphicsSettings {
                maximum_pixel_ratio: 3.0,
                render_scale: 0.6,
                frame_rate_cap: Some(45.0),
                automatic_resolution: true,
                quality: GraphicsQuality::High,
            }
        );
        let uncapped = GraphicsSettings::from_query("?fps=0&autoResolution=0", true);
        assert_eq!(uncapped.frame_rate_cap, None);
        assert!(!uncapped.automatic_resolution);
        assert_eq!(
            GraphicsSettings::from_query("?renderScale=0.1&pixelRatio=0.5", false),
            GraphicsSettings {
                maximum_pixel_ratio: 1.0,
                render_scale: MINIMUM_RENDER_SCALE,
                ..desktop
            }
        );
        assert_eq!(
            GraphicsSettings::from_query("?renderScale=NaN&pixelRatio=x&fps=-5", false),
            desktop
        );
    }

    #[test]
    fn graphics_quality_defaults_by_device_and_accepts_named_tiers() {
        assert_eq!(
            GraphicsSettings::from_query("", false).quality,
            GraphicsQuality::High
        );
        assert_eq!(
            GraphicsSettings::from_query("", true).quality,
            GraphicsQuality::Low
        );
        for (name, quality) in [
            ("low", GraphicsQuality::Low),
            ("medium", GraphicsQuality::Medium),
            ("high", GraphicsQuality::High),
        ] {
            assert_eq!(GraphicsQuality::parse(name), Some(quality));
            assert_eq!(
                GraphicsSettings::from_query(&format!("?quality={name}"), true).quality,
                quality
            );
        }
        assert_eq!(GraphicsQuality::parse("ultra"), None);
        assert_eq!(
            GraphicsSettings::from_query("?quality=ultra&quality=", true).quality,
            GraphicsQuality::Low
        );
    }
}
