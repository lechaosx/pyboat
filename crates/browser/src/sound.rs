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
            0,
            true,
            rapier2d_f64::prelude::Vector::ZERO,
            &[],
            &[],
        );
        let second = sampler.sample(
            geometry,
            0x5eed,
            100,
            0.5,
            0.0,
            0,
            true,
            rapier2d_f64::prelude::Vector::ZERO,
            &[],
            &[],
        );
        assert_eq!(first[..56], second[..56]);
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
    fn returning_to_realtime_starts_thunder_at_the_present() {
        let geometry = WorldGeometry::default();
        let listener = [48_126.919_571_367_784, 463.973_433_833_122_5];
        let event =
            crate::StormField::default().events(geometry, 0x5eed, listener, 846.0, 849.0)[0];
        let arrival = event.arrival(geometry, listener);
        let mut sampler = SoundSampler::default();
        let mut sample = |time: f64, wall, realtime| {
            let ticks = time / simulation::DT_SECONDS;
            sampler.sample(
                geometry,
                0x5eed,
                ticks.floor() as u64,
                ticks.fract() as f32,
                wall,
                1,
                realtime,
                listener.into(),
                &[],
                &[],
            )
        };
        sample(arrival - 0.1, 0.0, false);
        let resumed = sample(arrival + 0.1, 0.2, true);
        assert_eq!(
            resumed.len(),
            59,
            "realtime resume must not replay an arrival from accelerated playback"
        );
        assert_eq!(resumed[56], 1.0, "resume cancels already-playing channels");
    }
    #[test]
    fn shore_crossings_are_found_from_either_side_and_absent_in_open_water() {
        let coast = find_shores(Vec2::ZERO, 0.0, |p| p.x - 25.0);
        assert!(coast.iter().any(|p| (p.x - 25.0).abs() < 2.0));
        assert!(find_shores(Vec2::ZERO, 0.0, |_| -100.0).is_empty());
        assert!(!find_shores(Vec2::new(30.0, 0.0), 0.0, |p| p.x - 25.0).is_empty());
    }

    #[test]
    fn impacts_are_once_only_and_do_not_replay_on_join_resume_or_rewind() {
        let mut sampler = SoundSampler::default();
        let impact = protocol::HullImpact {
            id: 1,
            tick: 101,
            position_m: [0.0, 0.0],
            impulse_n_s: 12.0,
        };
        let mut sample = |tick, epoch, impacts: &[protocol::HullImpact]| {
            sampler.sample(
                WorldGeometry::default(),
                42,
                tick,
                0.0,
                tick as f64 / 60.0,
                epoch,
                true,
                rapier2d_f64::prelude::Vector::ZERO,
                &[],
                impacts,
            )
        };
        assert_eq!(sample(100, 1, &[])[58], 0.0);
        assert_eq!(sample(102, 1, &[impact.clone(), impact.clone()])[58], 1.0);
        assert_eq!(sample(103, 1, std::slice::from_ref(&impact))[58], 0.0);
        assert_eq!(sample(100, 1, std::slice::from_ref(&impact))[58], 0.0);
        assert_eq!(sample(105, 1, std::slice::from_ref(&impact))[58], 0.0);
        assert_eq!(sample(104, 2, std::slice::from_ref(&impact))[58], 0.0);
        assert_eq!(sample(90, 2, &[])[58], 0.0);
        assert_eq!(sample(140, 2, &[impact])[58], 0.0);
    }

    #[test]
    fn a_new_world_resets_impact_time_and_ids() {
        let mut sampler = SoundSampler::default();
        let geometry = WorldGeometry::default();
        let listener = rapier2d_f64::prelude::Vector::ZERO;
        sampler.sample(geometry, 1, 1000, 0.0, 0.0, 1, true, listener, &[], &[]);
        sampler.sample(geometry, 2, 100, 0.0, 1.0, 1, true, listener, &[], &[]);
        let impact = protocol::HullImpact {
            id: 1,
            tick: 101,
            position_m: [0.0, 0.0],
            impulse_n_s: 12.0,
        };
        assert_eq!(
            sampler.sample(
                geometry,
                2,
                102,
                0.0,
                1.05,
                1,
                true,
                listener,
                &[],
                std::slice::from_ref(&impact)
            )[58],
            1.0
        );
        assert_eq!(
            sampler.sample(
                geometry,
                2,
                103,
                0.0,
                1.10,
                1,
                true,
                listener,
                &[],
                &[impact]
            )[58],
            0.0
        );
    }

    #[test]
    fn listener_boat_velocity_and_contacts_use_the_nearest_longitude_image() {
        let geometry = WorldGeometry::default();
        let velocity = rapier2d_f64::prelude::Vector::new(4.0, -2.0);
        let motion = BoatMotion {
            position_m: [geometry.width_m(), 0.0].into(),
            velocity_m_s: velocity,
            angle_rad: 0.0,
            angular_velocity_rad_s: 0.0,
            sail_angle_rad: 0.0,
            sail_angular_velocity_rad_s: 0.0,
            rudder_angle_rad: 0.0,
            rudder_angular_velocity_rad_s: 0.0,
        };
        let mut sampler = SoundSampler::default();
        let expected =
            procedural_world_environment_interpolated(geometry, 42, 100, 0.0, Vec2::ZERO)
                .environment
                .true_wind_velocity_m_s
                - velocity;
        let first = sampler.sample(
            geometry,
            42,
            100,
            0.0,
            0.0,
            1,
            true,
            [0.0, 0.0].into(),
            &[(1, motion)],
            &[],
        );
        assert!((first[0] - expected.length() as f32).abs() < 1e-5);
        let impact = protocol::HullImpact {
            id: 1,
            tick: 101,
            position_m: [geometry.width_m() + 3.0, 0.0],
            impulse_n_s: 12.0,
        };
        let next = sampler.sample(
            geometry,
            42,
            102,
            0.0,
            0.05,
            1,
            true,
            [0.0, 0.0].into(),
            &[(1, motion)],
            &[impact],
        );
        assert_eq!(next[58], 1.0);
        assert_eq!(next[next.len() - 3], 3.0);
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
    storms: crate::StormField,
    thunder: crate::storm::ThunderCursor,
    playback_epoch: u32,
    impact_ids: std::collections::HashMap<u64, u64>,
    impact_start_tick: Option<u64>,
    impact_last_tick: u64,
}

impl SoundSampler {
    // Packed presentation boundary: 8 environment values, four 9-value boats,
    // then four 3-value shores, a reset flag, thunder/impact counts,
    // 4-value thunder arrivals and 3-value impacts. Positions are listener-relative.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn sample(
        &mut self,
        geometry: WorldGeometry,
        seed: u64,
        tick: u64,
        fraction: f32,
        now_seconds: f64,
        playback_epoch: u32,
        realtime: bool,
        listener: rapier2d_f64::prelude::Vector,
        boats: &[(u32, BoatMotion)],
        impacts: &[protocol::HullImpact],
    ) -> Vec<f32> {
        let center = listener.as_vec2();
        let environment =
            procedural_world_environment_interpolated(geometry, seed, tick, fraction, center);
        let weather = environment.weather;
        let velocity = boats
            .iter()
            .find(|(_, motion)| {
                let delta =
                    geometry.minimum_image_displacement_m((motion.position_m - listener).into());
                delta[0].hypot(delta[1]) < 0.1
            })
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
            let load = across / (across * across + 25.0).sqrt();
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
                (motion.angular_velocity_rad_s.abs() + motion.sail_angular_velocity_rad_s.abs())
                    as f32,
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
        let reset = playback_epoch != self.playback_epoch
            || !realtime
            || self.storms.identity() != Some((geometry, seed));
        if reset || self.impact_start_tick.is_none() {
            self.impact_ids.clear();
            self.impact_start_tick = Some(tick);
            self.impact_last_tick = tick;
        }
        self.impact_last_tick = self.impact_last_tick.max(tick);
        if reset {
            self.thunder = crate::storm::ThunderCursor::default();
            self.playback_epoch = playback_epoch;
        }
        if !realtime {
            result.extend([1.0, 0.0, 0.0]);
            return result;
        }
        let events = self.storms.events(
            geometry,
            seed,
            listener.into(),
            (seconds - 96.0).max(0.0),
            seconds,
        );
        let arrivals =
            self.thunder
                .advance(seconds, now_seconds, &events, geometry, listener.into());
        result.extend([f32::from(self.thunder.reset), arrivals.len() as f32, 0.0]);
        for thunder in arrivals {
            result.extend(thunder);
        }
        self.impact_ids
            .retain(|_, event_tick| self.impact_last_tick.saturating_sub(*event_tick) <= 60);
        for impact in impacts {
            if impact.tick <= self.impact_start_tick.unwrap_or(tick)
                || impact.tick > tick
                || tick.saturating_sub(impact.tick) > 30
                || self.impact_ids.insert(impact.id, impact.tick).is_some()
            {
                continue;
            }
            let delta = geometry.minimum_image_displacement_m([
                impact.position_m[0] - listener.x,
                impact.position_m[1] - listener.y,
            ]);
            if delta[0].hypot(delta[1]) > 180.0 {
                continue;
            }
            result[58] += 1.0;
            result.extend([
                delta[0] as f32,
                delta[1] as f32,
                (impact.impulse_n_s / (impact.impulse_n_s + 100.0)).sqrt(),
            ]);
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
        playback_epoch: u32,
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
            .filter(|_| {
                self.spectator_target.is_none() || self.health().is_some_and(|health| health > 0.0)
            })
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
            playback_epoch,
            true,
            listener,
            &boats,
            &self.impacts,
        )
    }
}
