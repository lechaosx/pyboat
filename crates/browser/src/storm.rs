#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lightning_has_irregular_repeated_strokes_without_preflash() {
        let event = LightningEvent {
            id: (0, 0, 0),
            seconds: 10.0,
            position_m: [0.0; 2],
            strength: 1.0,
            temperature_c: 15.0,
        };
        let profile = |event: LightningEvent| {
            (0..801)
                .map(|millisecond| {
                    lightning_rows(
                        WorldGeometry::default(),
                        event.seconds + f64::from(millisecond) / 1000.0,
                        [0.0; 2],
                        vec![event],
                    )[0][3]
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            lightning_rows(WorldGeometry::default(), 9.999, [0.0; 2], vec![event])[0][3],
            0.0,
        );
        let profiles: Vec<_> = (0..16)
            .map(|id| {
                profile(LightningEvent {
                    id: (id, 0, 0),
                    ..event
                })
            })
            .collect();
        assert!(profiles.windows(2).any(|pair| pair[0] != pair[1]));
        assert!(
            profiles
                .iter()
                .all(|profile| profile.iter().all(|v| v.is_finite() && *v >= 0.0))
        );
        assert!(profiles.iter().all(|profile| profile[800] == 0.0));
        let peaks = |profile: &[f32]| {
            profile
                .windows(3)
                .filter(|v| v[1] > v[0] && v[1] >= v[2])
                .count()
        };
        assert!(profiles.iter().any(|profile| peaks(profile) >= 2));
        assert!(
            profiles
                .iter()
                .any(|profile| peaks(profile) != peaks(&profiles[0]))
        );
    }
    #[test]
    fn candidates_wrap_and_do_not_depend_on_query_order() {
        let g = WorldGeometry::default();
        let mut a = StormField::default();
        let mut b = StormField::default();
        let p = [48_126.919_571_367_784, 463.973_433_833_122_5];
        let first = a.events(g, 0x5eed, p, 846.0, 849.0);
        b.events(g, 0x5eed, [0.0, 1_000_000.0], 12.0, 40.0);
        let wrapped = b.events(g, 0x5eed, [p[0] - g.width_m(), p[1]], 846.0, 849.0);
        assert!(
            !first.is_empty(),
            "natural convective-weather fixture must produce a channel"
        );
        assert_eq!(first, wrapped);
        assert_eq!(first, a.events(g, 0x5eed, p, 846.0, 849.0));
    }
    #[test]
    fn low_frame_rate_does_not_skip_arriving_thunder() {
        let e = LightningEvent {
            id: (0, 0, 0),
            seconds: 10.0,
            position_m: [0.0; 2],
            strength: 1.0,
            temperature_c: 15.0,
        };
        let g = WorldGeometry::default();
        let t = e.arrival(g, [0.0; 2]);
        let mut cursor = ThunderCursor::default();
        cursor.advance(t - 0.25, 0.0, &[e], g, [0.0; 2]);
        assert_eq!(cursor.advance(t + 0.25, 0.5, &[e], g, [0.0; 2]).len(), 1);
    }
    #[test]
    fn rain_without_convection_is_not_electrified() {
        assert_eq!(electrification(1.0, 0.0, 1.0), 0.0);
        assert_eq!(electrification(1.0, 1.0, 0.0), 0.0);
        assert!(electrification(1.0, 1.0, 1.0) > 0.0);
    }
    #[test]
    fn capped_sources_keep_fresh_light_over_faded_nearby_channels() {
        let old = LightningEvent {
            id: (0, 0, 0),
            seconds: 10.0,
            position_m: [0.0; 2],
            strength: 1.0,
            temperature_c: 15.0,
        };
        let fresh = LightningEvent {
            id: (1, 0, 0),
            seconds: 10.79,
            position_m: [1000.0, 0.0],
            ..old
        };
        let rows = lightning_rows(
            WorldGeometry::default(),
            10.8,
            [0.0; 2],
            vec![old, old, old, old, fresh],
        );
        assert!(
            rows.iter().any(|row| row[0] == 1000.0),
            "fresh light must survive the four-source cap"
        );
    }
    #[test]
    fn thunder_crosses_intervals_once_and_resets_on_seek() {
        let e = LightningEvent {
            id: (0, 0, 0),
            seconds: 10.0,
            position_m: [0.0, 0.0],
            strength: 1.0,
            temperature_c: 15.0,
        };
        let g = WorldGeometry::default();
        let arrival = e.arrival(g, [3430.0, 0.0]);
        let mut cursor = ThunderCursor::default();
        assert!(
            cursor
                .advance(arrival - 0.1, 0.0, &[e], g, [3430.0, 0.0])
                .is_empty()
        );
        assert_eq!(
            cursor
                .advance(arrival + 0.1, 0.2, &[e], g, [3430.0, 0.0])
                .len(),
            1
        );
        assert!(
            cursor
                .advance(arrival + 0.2, 0.3, &[e], g, [3430.0, 0.0])
                .is_empty()
        );
        assert!(
            cursor
                .advance(arrival - 2.0, 0.4, &[e], g, [3430.0, 0.0])
                .is_empty()
        );
        assert!(
            cursor
                .advance(arrival + 1.0, 0.5, &[e], g, [3430.0, 0.0])
                .is_empty()
        );
    }
}

use simulation::WorldGeometry;
use std::collections::HashMap;

const CELL_M: f64 = 8_000.0;
const EPOCH_SECONDS: f64 = 16.0;
const RADIUS_M: f64 = 20_000.0;
const HEIGHT_M: f64 = 2_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightningEvent {
    pub id: (i64, i64, u64),
    pub seconds: f64,
    pub position_m: [f64; 2],
    pub strength: f32,
    pub temperature_c: f32,
}
impl LightningEvent {
    fn displacement(self, geometry: WorldGeometry, listener: [f64; 2]) -> [f64; 2] {
        geometry.minimum_image_displacement_m([
            self.position_m[0] - listener[0],
            self.position_m[1] - listener[1],
        ])
    }
    pub fn arrival(self, geometry: WorldGeometry, listener: [f64; 2]) -> f64 {
        let d = self.displacement(geometry, listener);
        self.seconds
            + d[0].hypot(d[1]).hypot(HEIGHT_M) / (331.0 + 0.6 * f64::from(self.temperature_c))
    }
}
fn electrification(rain: f32, instability: f32, moisture: f32) -> f32 {
    rain * instability * moisture
}
#[derive(Default)]
pub struct StormField {
    identity: Option<(WorldGeometry, u64)>,
    accepted: HashMap<(i64, i64, u64), Option<LightningEvent>>,
}
impl StormField {
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn identity(&self) -> Option<(WorldGeometry, u64)> {
        self.identity
    }
    pub fn events(
        &mut self,
        geometry: WorldGeometry,
        seed: u64,
        listener: [f64; 2],
        start: f64,
        end: f64,
    ) -> Vec<LightningEvent> {
        if self.identity != Some((geometry, seed)) {
            self.identity = Some((geometry, seed));
            self.accepted.clear();
        }
        let first = (start.max(0.0) / EPOCH_SECONDS).floor() as u64;
        let last = (end.max(0.0) / EPOCH_SECONDS).floor() as u64;

        let columns = (geometry.width_m() / CELL_M).ceil().max(1.0) as i64;
        let spacing = geometry.width_m() / columns as f64;
        let cx = (listener[0] / spacing).floor() as i64;
        let cy = (listener[1] / CELL_M).floor() as i64;
        let reach = (RADIUS_M / spacing).ceil() as i64;
        self.accepted.retain(|key, _| {
            key.2 >= first && key.2 <= last && (key.1 - cy).abs() <= 3 && {
                let d = (key.0 - cx.rem_euclid(columns)).rem_euclid(columns);
                d.min(columns - d) <= reach
            }
        });
        let mut result = Vec::new();
        for epoch in first..=last {
            for y in cy - 3..=cy + 3 {
                for offset in 0..columns.min(2 * reach + 1) {
                    let x = (cx - reach + offset).rem_euclid(columns);
                    let id = (x, y, epoch);
                    let h = hash(
                        seed ^ hash(x as u64)
                            ^ hash(y as u64).rotate_left(19)
                            ^ hash(epoch).rotate_left(37),
                    );
                    // Sparse candidate channels; weather then controls acceptance.
                    if h & 3 != 0 {
                        continue;
                    }
                    let seconds = (epoch as f64 + unit(hash(h))) * EPOCH_SECONDS;
                    if seconds < start || seconds > end {
                        continue;
                    }
                    let position_m = [
                        (x as f64 + unit(hash(h ^ 1))) * spacing,
                        (y as f64 + unit(hash(h ^ 2))) * CELL_M,
                    ];
                    let delta = geometry.minimum_image_displacement_m([
                        position_m[0] - listener[0],
                        position_m[1] - listener[1],
                    ]);
                    if delta[0].hypot(delta[1]) > RADIUS_M {
                        continue;
                    }
                    let event = self.accepted.entry(id).or_insert_with(|| {
                        let weather = game::procedural_weather(
                            geometry,
                            seed,
                            (seconds / simulation::DT_SECONDS) as u64,
                            rapier2d_f64::glamx::Vec2::new(
                                position_m[0] as f32,
                                position_m[1] as f32,
                            ),
                        );
                        // This weather model has no charge/microphysics. Rain,
                        // instability and moist ascent stand in for electrification.
                        let moisture = (weather.ascent_m / 1_000.0).clamp(0.0, 1.0)
                            * weather.relative_humidity;
                        let activity =
                            electrification(weather.rain_intensity, weather.instability, moisture);
                        (unit(hash(h ^ 3)) < f64::from(activity)).then_some(LightningEvent {
                            id,
                            seconds,
                            position_m,
                            strength: 0.7 + unit(hash(h ^ 4)) as f32 * 0.6,
                            temperature_c: weather.surface_air_temperature_c,
                        })
                    });
                    if let Some(event) = event {
                        result.push(*event);
                    }
                }
            }
        }
        result.sort_by(|a, b| a.seconds.total_cmp(&b.seconds).then(a.id.cmp(&b.id)));
        result
    }
}
#[cfg(any(target_arch = "wasm32", test))]
#[derive(Default)]
pub(crate) struct ThunderCursor {
    pub(crate) reset: bool,
    previous: Option<(f64, f64)>,
    heard: HashMap<(i64, i64, u64), f64>,
}
#[cfg(any(target_arch = "wasm32", test))]
impl ThunderCursor {
    pub(crate) fn advance(
        &mut self,
        seconds: f64,
        wall: f64,
        events: &[LightningEvent],
        geometry: WorldGeometry,
        listener: [f64; 2],
    ) -> Vec<[f32; 4]> {
        self.reset = false;
        let previous = self.previous.replace((seconds, wall));
        let Some((before, wall_before)) = previous else {
            self.reset = true;
            return Vec::new();
        };
        let elapsed = seconds - before;
        // Accelerated lab playback, seeks and resumed audio establish a new
        // cursor instead of replaying stale thunder.
        if elapsed < 0.0 || (elapsed - (wall - wall_before)).abs() > 0.15 {
            self.heard.clear();
            self.reset = true;
            return Vec::new();
        }
        self.heard.retain(|_, time| *time > seconds - 100.0);
        let mut result = Vec::new();
        for event in events {
            let arrival = event.arrival(geometry, listener);
            if arrival > before && arrival <= seconds && !self.heard.contains_key(&event.id) {
                self.heard.insert(event.id, seconds);
                let d = event.displacement(geometry, listener);
                result.push([
                    d[0] as f32,
                    d[1] as f32,
                    event.strength,
                    d[0].hypot(d[1]).hypot(HEIGHT_M) as f32,
                ]);
            }
        }
        result
    }
}

fn hash(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
fn unit(x: u64) -> f64 {
    (x >> 11) as f64 / ((1_u64 << 53) as f64)
}

fn lightning_envelope(event: LightningEvent, seconds: f64) -> f32 {
    let age = seconds - event.seconds;
    if !(0.0..0.8).contains(&age) {
        return 0.0;
    }
    let mut state = hash(
        hash(event.id.0 as u64)
            ^ hash(event.id.1 as u64).rotate_left(19)
            ^ hash(event.id.2).rotate_left(37),
    );
    let strokes = 1 + state % 5;
    let mut onset = 0.0;
    let mut light = 0.0;
    for stroke in 0..strokes {
        state = hash(state.wrapping_add(0x9e3779b97f4a7c15));
        // Natural multi-stroke flashes commonly have 40–80 ms intervals:
        // https://natural-resources.canada.ca/forests-forestry/wildland-fires/lightning-more-information
        // The 18 ms decay broadens the optical impulse for frame-rate visibility;
        // it represents cloud-scattered light on a display, not channel current.
        let elapsed = age - onset;
        if (0.0..0.06).contains(&elapsed) {
            let strength = if stroke == 0 {
                1.0
            } else {
                0.3 + 0.9 * unit(state)
            };
            light += strength * (-elapsed / 0.018).exp();
        }
        onset += 0.04 + 0.04 * unit(hash(state));
    }
    light as f32
}

/// Diffuse cloud-scattered flashes, represented as at most four light sources.
/// The finite height and capped range are a presentation proxy for a channel
/// illuminating its cloud, not a resolved lightning radiation model.
pub fn lightning_sources(
    geometry: WorldGeometry,
    seed: u64,
    seconds: f64,
    camera: [f64; 2],
) -> [[f32; 4]; 4] {
    let events =
        StormField::default().events(geometry, seed, camera, (seconds - 0.8).max(0.0), seconds);
    lightning_rows(geometry, seconds, camera, events)
}

fn lightning_rows(
    geometry: WorldGeometry,
    seconds: f64,
    camera: [f64; 2],
    events: Vec<LightningEvent>,
) -> [[f32; 4]; 4] {
    let mut sources: Vec<_> = events
        .into_iter()
        .map(|event| {
            let envelope = lightning_envelope(event, seconds);
            let d = event.displacement(geometry, camera);
            (
                event.id,
                [
                    d[0] as f32,
                    d[1] as f32,
                    HEIGHT_M as f32,
                    event.strength * envelope * 16.0,
                ],
            )
        })
        .collect();
    sources.sort_by(|a, b| {
        let intensity =
            |row: [f32; 4]| row[3] / (row[0] * row[0] + row[1] * row[1] + row[2] * row[2]);
        intensity(b.1)
            .total_cmp(&intensity(a.1))
            .then(a.0.cmp(&b.0))
    });
    let mut rows = [[0.0; 4]; 4];
    for (row, (_, source)) in rows.iter_mut().zip(sources) {
        *row = source;
    }
    rows
}
