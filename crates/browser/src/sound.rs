use rapier2d_f64::glamx::Vec2;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn packed_world_sound_is_finite_and_repeatable() {
        let mut sampler = SoundSampler::default();
        let geometry = WorldGeometry::default();
        let first = sampler.sample(
            geometry,
            0x5eed,
            100,
            0.5,
            0.0,
            rapier2d_f64::prelude::Vector::ZERO,
            &[],
        );
        let second = sampler.sample(
            geometry,
            0x5eed,
            100,
            0.5,
            0.0,
            rapier2d_f64::prelude::Vector::ZERO,
            &[],
        );
        assert_eq!(first, second);
        assert!(first.iter().all(|value| value.is_finite()));
    }
    #[test]
    fn snow_and_solid_water_do_not_make_rain_or_wave_sounds() {
        assert_eq!(liquid_rain(1.0, -2.0), 0.0);
        assert_eq!(liquid_rain(1.0, 3.0), 1.0);
        assert_eq!(open_water(4.0, 1.0), 0.0);
        assert_eq!(open_water(-1.0, 0.0), 0.0);
        assert_eq!(open_water(4.0, 0.0), 1.0);
    }
    #[test]
    fn shore_crossings_are_found_from_either_side_and_absent_in_open_water() {
        let coast = find_shores(Vec2::ZERO, 0.0, |p| p.x - 25.0);
        assert!(coast.iter().any(|p| (p.x - 25.0).abs() < 2.0));
        assert!(find_shores(Vec2::ZERO, 0.0, |_| -100.0).is_empty());
        assert!(!find_shores(Vec2::new(30.0, 0.0), 0.0, |p| p.x - 25.0).is_empty());
    }
}

use game::{procedural_terrain_elevation_m, procedural_world_environment_interpolated};
use simulation::{BoatMotion, WorldGeometry};

fn liquid_rain(intensity: f32, temperature: f32) -> f32 {
    let t = ((temperature + 1.0) / 3.0).clamp(0.0, 1.0);
    intensity * t * t * (3.0 - 2.0 * t)
}
fn open_water(depth: f32, ice: f32) -> f32 {
    if depth > 0.0 {
        1.0 - ice.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn find_shores(center: Vec2, height: f32, elevation: impl Fn(Vec2) -> f32) -> Vec<Vec2> {
    let mut shores = Vec::with_capacity(8);
    let start = elevation(center) - height;
    for ray in 0..8 {
        let direction = Vec2::from_angle(ray as f32 * std::f32::consts::FRAC_PI_4);
        let mut previous = 0.0;
        let mut previous_height = start;
        for distance in [12.0, 35.0, 85.0, 180.0] {
            let sampled = elevation(center + direction * distance) - height;
            if sampled.signum() != previous_height.signum() {
                let (mut low, mut high) = (previous, distance);
                for _ in 0..5 {
                    let midpoint = (low + high) * 0.5;
                    if (elevation(center + direction * midpoint) - height).signum()
                        == previous_height.signum()
                    {
                        low = midpoint;
                    } else {
                        high = midpoint;
                    }
                }
                shores.push(center + direction * ((low + high) * 0.5));
                break;
            }
            previous = distance;
            previous_height = sampled;
        }
    }
    shores.sort_by(|a, b| {
        a.distance_squared(center)
            .total_cmp(&b.distance_squared(center))
    });
    shores.truncate(4);
    shores
}

struct ShoreCache {
    geometry: WorldGeometry,
    seed: u64,
    center: Vec2,
    world_seconds: f64,
    refreshed_at_seconds: f64,
    shores: Vec<(Vec2, f32)>,
}
#[derive(Default)]
pub(crate) struct SoundSampler {
    shore_cache: Option<ShoreCache>,
}

impl SoundSampler {
    // Packed presentation boundary: 8 environment values, four 9-value boats,
    // then four 3-value shores. Positions are listener-relative before f32.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sample(
        &mut self,
        geometry: WorldGeometry,
        seed: u64,
        tick: u64,
        fraction: f32,
        now_seconds: f64,
        listener: rapier2d_f64::prelude::Vector,
        boats: &[(u32, BoatMotion)],
    ) -> Vec<f32> {
        let center = listener.as_vec2();
        let environment =
            procedural_world_environment_interpolated(geometry, seed, tick, fraction, center);
        let weather = environment.weather;
        let velocity = boats
            .iter()
            .find(|(_, motion)| (motion.position_m - listener).length_squared() < 0.01)
            .map_or(rapier2d_f64::prelude::Vector::ZERO, |(_, motion)| {
                motion.velocity_m_s
            });
        let apparent = environment.environment.true_wind_velocity_m_s - velocity;
        let mut result = vec![0.0; 56];
        result[..8].copy_from_slice(&[
            apparent.length() as f32,
            environment.ocean.significant_wave_height_m,
            liquid_rain(weather.rain_intensity, weather.surface_air_temperature_c),
            open_water(
                environment.environment.water_depth_m as f32,
                environment.surface_climate.polar_ice_closure,
            ),
            weather.instability,
            weather.wind_velocity_m_s.x,
            weather.wind_velocity_m_s.y,
            weather.rain_intensity,
        ]);
        for (index, (id, motion)) in boats.iter().take(4).enumerate() {
            let displacement =
                geometry.minimum_image_displacement_m((motion.position_m - listener).into());
            let local = if displacement[0].hypot(displacement[1]) < 0.01 {
                environment
            } else {
                procedural_world_environment_interpolated(
                    geometry,
                    seed,
                    tick,
                    fraction,
                    motion.position_m.as_vec2(),
                )
            };
            let wind = local.environment.true_wind_velocity_m_s - motion.velocity_m_s;
            let angle = motion.angle_rad + motion.sail_angle_rad;
            let across = wind.dot(rapier2d_f64::prelude::Vector::new(
                -angle.sin(),
                angle.cos(),
            ));
            let load = across / (across * across + 4.0).sqrt();
            let water = open_water(
                local.environment.water_depth_m as f32,
                local.surface_climate.polar_ice_closure,
            );
            let speed =
                (motion.velocity_m_s - local.environment.true_water_velocity_m_s).length() as f32;
            let start = 8 + index * 9;
            result[start..start + 9].copy_from_slice(&[
                *id as f32,
                displacement[0] as f32,
                displacement[1] as f32,
                speed * water,
                motion.angular_velocity_rad_s as f32,
                load as f32,
                (wind.length() / (wind.length() + 2.0) * (1.0 - load.abs())) as f32,
                local.ocean.surface_displacement_m,
                water,
            ]);
        }
        let seconds = (tick as f64 + f64::from(fraction)) * simulation::DT_SECONDS;
        let refresh = self.shore_cache.as_ref().is_none_or(|cached| {
            cached.geometry != geometry
                || cached.seed != seed
                || center.distance(cached.center) > 15.0
                || seconds < cached.world_seconds
                || now_seconds - cached.refreshed_at_seconds >= 1.0
        });
        if refresh {
            // Cheap coast proxy: eight radial transects against the local tide,
            // not an exact coastline-distance field. Tiny coves between rays
            // can be missed; tide variation within 180 m is treated as uniform.
            let shores = find_shores(center, environment.water_height_m, |position| {
                procedural_terrain_elevation_m(geometry, seed, position)
            });
            let shores = shores
                .into_iter()
                .map(|shore| {
                    let direction = (shore - center).normalize_or_zero();
                    let a = shore + direction * 12.0;
                    let b = shore - direction * 12.0;
                    let wet_side = if procedural_terrain_elevation_m(geometry, seed, a)
                        < procedural_terrain_elevation_m(geometry, seed, b)
                    {
                        a
                    } else {
                        b
                    };
                    let sample = procedural_world_environment_interpolated(
                        geometry, seed, tick, fraction, wet_side,
                    );
                    let strength = (sample.ocean.significant_wave_height_m / 1.5).clamp(0.0, 1.0)
                        * open_water(
                            sample.environment.water_depth_m as f32,
                            sample.surface_climate.polar_ice_closure,
                        );
                    (shore, strength)
                })
                .collect();
            self.shore_cache = Some(ShoreCache {
                geometry,
                seed,
                center,
                world_seconds: seconds,
                refreshed_at_seconds: now_seconds,
                shores,
            });
        }
        if let Some(cached) = &self.shore_cache {
            for (index, (shore, strength)) in cached.shores.iter().enumerate() {
                let delta =
                    geometry.minimum_image_displacement_m((shore.as_dvec2() - listener).into());
                result[44 + index * 3..47 + index * 3].copy_from_slice(&[
                    delta[0] as f32,
                    delta[1] as f32,
                    *strength,
                ]);
            }
        }
        result
    }
}

impl crate::MultiplayerClient {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn sound_scene(
        &self,
        sampler: &mut SoundSampler,
        spectator: Vec2,
        now_seconds: f64,
    ) -> Vec<f32> {
        let (Some(geometry), Some(seed), Some((tick, fraction))) = (
            self.world_geometry(),
            self.match_seed(),
            self.render_clock(),
        ) else {
            return Vec::new();
        };
        let listener = self
            .local_motion()
            .map_or(spectator.as_dvec2(), |motion| motion.position_m);
        let mut nearest: Vec<_> = self
            .boats
            .keys()
            .filter_map(|id| {
                self.render_motion(id).map(|motion| {
                    let delta = geometry
                        .minimum_image_displacement_m((motion.position_m - listener).into());
                    let distance = delta[0].hypot(delta[1]);
                    let key = id.bytes().fold(1_u32, |hash, byte| {
                        hash.wrapping_mul(31).wrapping_add(u32::from(byte))
                    }) % 16_777_215
                        + 1;
                    (distance, key, motion)
                })
            })
            .filter(|(distance, _, _)| *distance < 120.0)
            .collect();
        nearest.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let boats: Vec<_> = nearest
            .into_iter()
            .take(4)
            .map(|(_, id, motion)| (id, motion))
            .collect();
        sampler.sample(
            geometry,
            seed,
            tick,
            fraction,
            now_seconds,
            listener,
            &boats,
        )
    }
}
