use game::procedural_weather;
use rapier2d::prelude::Vector;
use simulation::{DT_SECONDS, WorldGeometry};

pub const MAX_LOW_CLOUD_KERNELS: usize = 256;
pub const LOW_CLOUD_KERNEL_VECTORS: usize = 2 * MAX_LOW_CLOUD_KERNELS;
pub const MAX_UPPER_CLOUD_KERNELS: usize = 96;
pub const UPPER_CLOUD_KERNEL_VECTORS: usize = 2 * MAX_UPPER_CLOUD_KERNELS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LowCloudKernel {
    pub center_m: Vector,
    pub radius_m: f32,
    pub amplitude: f32,
    pub axis: Vector,
    pub aspect_ratio: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LowCloudKernelTuning {
    pub body_size_scale: f32,
    pub occurrence_scale: f32,
}

impl Default for LowCloudKernelTuning {
    fn default() -> Self {
        Self {
            body_size_scale: 1.0,
            occurrence_scale: 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LowCloudKernelUniforms {
    pub count_and_padding: [f32; 4],
    pub vectors: [[f32; 4]; LOW_CLOUD_KERNEL_VECTORS],
}

#[derive(Clone, Debug, PartialEq)]
pub struct UpperCloudKernelUniforms {
    pub count_and_padding: [f32; 4],
    pub vectors: [[f32; 4]; UPPER_CLOUD_KERNEL_VECTORS],
}

pub fn low_cloud_kernel_uniforms(kernels: &[LowCloudKernel]) -> LowCloudKernelUniforms {
    let count = kernels.len().min(MAX_LOW_CLOUD_KERNELS);
    let mut vectors = [[0.0; 4]; LOW_CLOUD_KERNEL_VECTORS];
    for (index, kernel) in kernels.iter().take(count).enumerate() {
        vectors[2 * index] = [
            kernel.center_m.x,
            kernel.center_m.y,
            kernel.radius_m,
            kernel.amplitude,
        ];
        vectors[2 * index + 1] = [kernel.axis.x, kernel.axis.y, kernel.aspect_ratio, 0.0];
    }
    LowCloudKernelUniforms {
        count_and_padding: [count as f32, 0.0, 0.0, 0.0],
        vectors,
    }
}

pub fn upper_cloud_kernel_uniforms(kernels: &[LowCloudKernel]) -> UpperCloudKernelUniforms {
    let count = kernels.len().min(MAX_UPPER_CLOUD_KERNELS);
    let mut vectors = [[0.0; 4]; UPPER_CLOUD_KERNEL_VECTORS];
    for (index, kernel) in kernels.iter().take(count).enumerate() {
        vectors[2 * index] = [
            kernel.center_m.x,
            kernel.center_m.y,
            kernel.radius_m,
            kernel.amplitude,
        ];
        vectors[2 * index + 1] = [kernel.axis.x, kernel.axis.y, kernel.aspect_ratio, 0.0];
    }
    UpperCloudKernelUniforms {
        count_and_padding: [count as f32, 0.0, 0.0, 0.0],
        vectors,
    }
}

pub fn low_cloud_kernels(
    world_geometry: WorldGeometry,
    match_seed: u64,
    cloud_tick: u64,
    wind_tick: u64,
    camera_m: Vector,
    view_size_m: Vector,
) -> Vec<LowCloudKernel> {
    low_cloud_kernels_with_tuning(
        world_geometry,
        match_seed,
        cloud_tick,
        wind_tick,
        camera_m,
        view_size_m,
        LowCloudKernelTuning::default(),
    )
}

pub fn low_cloud_kernels_with_tuning(
    world_geometry: WorldGeometry,
    match_seed: u64,
    cloud_tick: u64,
    wind_tick: u64,
    camera_m: Vector,
    view_size_m: Vector,
    tuning: LowCloudKernelTuning,
) -> Vec<LowCloudKernel> {
    const TARGET_CELL_SIZE_M: f32 = 640.0;
    const VIEW_MARGIN_M: f32 = 2_500.0;
    const LOBES_PER_CLUSTER: u64 = 4;

    let body_size_scale = tuning.body_size_scale.clamp(0.25, 4.0);
    let spawn_probability = (0.08 * tuning.occurrence_scale).clamp(0.0, 1.0);

    let camera_m: Vector = world_geometry.wrap_position_m(camera_m.into()).into();
    let horizontal_cells = (world_geometry.width_m() / TARGET_CELL_SIZE_M)
        .round()
        .max(1.0) as i64;
    let vertical_cells = (world_geometry.height_m() / TARGET_CELL_SIZE_M)
        .round()
        .max(1.0) as i64;
    let cell_size_m = Vector::new(
        world_geometry.width_m() / horizontal_cells as f32,
        world_geometry.height_m() / vertical_cells as f32,
    );
    let cloud_tick = cloud_tick.min(i64::MAX as u64) as i64;
    let wind_tick = wind_tick.min(i64::MAX as u64) as i64;
    let cohort_ticks = (300.0 / DT_SECONDS) as i64;
    let current_cohort = cloud_tick.div_euclid(cohort_ticks);
    let mut kernels = Vec::new();

    for cohort in [current_cohort - 2, current_cohort - 1, current_cohort] {
        let birth_tick = cohort * cohort_ticks;
        let age_seconds = (cloud_tick - birth_tick) as f32 * DT_SECONDS;
        let temporal_weight = cohort_weight(age_seconds);
        if temporal_weight <= 0.0 {
            continue;
        }
        let birth_camera_m = integrate_path(camera_m, age_seconds, |position_m, elapsed| {
            let elapsed_ticks = (elapsed / DT_SECONDS).round() as i64;
            let sample_wind_tick = (wind_tick - elapsed_ticks).max(0);
            let weather = procedural_weather(
                world_geometry,
                match_seed,
                sample_wind_tick as u64,
                position_m,
            );
            -visual_cloud_wind(weather.wind_velocity_m_s - weather.surface_gust_velocity_m_s)
        });
        let (cell_x_values, minimum_cell_y, maximum_cell_y) = candidate_cells(
            world_geometry,
            birth_camera_m,
            0.5 * view_size_m + Vector::splat(VIEW_MARGIN_M + 310.0 * body_size_scale),
            cell_size_m,
            horizontal_cells,
        );
        for cell_y in minimum_cell_y..=maximum_cell_y {
            for &cell_x in &cell_x_values {
                let cluster_seed = kernel_hash(match_seed, cohort, cell_x, cell_y, 0);
                if hash_unit(cluster_seed) >= spawn_probability {
                    continue;
                }
                let birth_center_m = Vector::new(
                    -0.5 * world_geometry.width_m()
                        + (cell_x as f32
                            + 0.15
                            + 0.70 * hash_unit(kernel_hash(match_seed, cohort, cell_x, cell_y, 1)))
                            * cell_size_m.x,
                    -0.5 * world_geometry.height_m()
                        + (cell_y as f32
                            + 0.15
                            + 0.70 * hash_unit(kernel_hash(match_seed, cohort, cell_x, cell_y, 2)))
                            * cell_size_m.y,
                );
                let birth_weather = procedural_weather(
                    world_geometry,
                    match_seed,
                    birth_tick.max(0) as u64,
                    birth_center_m,
                );
                let formation = visible_low_cloud_coverage(birth_weather);
                if formation <= 0.02
                    || hash_unit(kernel_hash(match_seed, cohort, cell_x, cell_y, 3))
                        > (0.20 + 0.80 * formation)
                {
                    continue;
                }
                let center_m =
                    integrate_path(birth_center_m, age_seconds, |position_m, elapsed| {
                        let elapsed_ticks = (elapsed / DT_SECONDS).round() as i64;
                        let sample_cloud_tick = birth_tick + elapsed_ticks;
                        let sample_wind_tick =
                            (wind_tick - (cloud_tick - sample_cloud_tick)).max(0);
                        let weather = procedural_weather(
                            world_geometry,
                            match_seed,
                            sample_wind_tick as u64,
                            position_m,
                        );
                        visual_cloud_wind(
                            weather.wind_velocity_m_s - weather.surface_gust_velocity_m_s,
                        )
                    });
                let angle = std::f32::consts::TAU
                    * hash_unit(kernel_hash(match_seed, cohort, cell_x, cell_y, 4));
                let axis = Vector::new(angle.cos(), angle.sin());
                let perpendicular = Vector::new(-axis.y, axis.x);
                for lobe in 0..LOBES_PER_CLUSTER {
                    let lobe_seed = kernel_hash(match_seed, cohort, cell_x, cell_y, 5 + lobe);
                    let main_lobe = lobe == 0;
                    let longitudinal_offset_m = if main_lobe {
                        0.0
                    } else {
                        body_size_scale * (2.0 * hash_unit(lobe_seed) - 1.0) * 155.0
                    };
                    let lateral_offset_m = if main_lobe {
                        0.0
                    } else {
                        body_size_scale * (2.0 * hash_unit(mix_hash(lobe_seed)) - 1.0) * 105.0
                    };
                    let radius_m = if main_lobe {
                        body_size_scale * (220.0 + 90.0 * hash_unit(mix_hash(mix_hash(lobe_seed))))
                    } else {
                        body_size_scale
                            * (110.0 + 75.0 * hash_unit(mix_hash(mix_hash(mix_hash(lobe_seed)))))
                    };
                    let center_m: Vector = world_geometry
                        .wrap_position_m(
                            (center_m
                                + longitudinal_offset_m * axis
                                + lateral_offset_m * perpendicular)
                                .into(),
                        )
                        .into();
                    let mut view_offset_m = center_m - camera_m;
                    view_offset_m.x -= world_geometry.width_m()
                        * (view_offset_m.x / world_geometry.width_m()).round();
                    if view_offset_m.x.abs() > 0.5 * view_size_m.x + VIEW_MARGIN_M + radius_m
                        || view_offset_m.y.abs() > 0.5 * view_size_m.y + VIEW_MARGIN_M + radius_m
                    {
                        continue;
                    }
                    kernels.push(LowCloudKernel {
                        center_m,
                        radius_m,
                        amplitude: temporal_weight * if main_lobe { 1.0 } else { 0.72 },
                        axis,
                        aspect_ratio: 0.88 + 0.24 * hash_unit(mix_hash(lobe_seed ^ 0xa076_1d64)),
                    });
                }
            }
        }
    }
    kernels.sort_by(|left, right| {
        periodic_distance_squared(world_geometry, left.center_m, camera_m).total_cmp(
            &periodic_distance_squared(world_geometry, right.center_m, camera_m),
        )
    });
    kernels.truncate(MAX_LOW_CLOUD_KERNELS);
    kernels
}

pub fn upper_cloud_kernels(
    world_geometry: WorldGeometry,
    match_seed: u64,
    cloud_tick: u64,
    wind_tick: u64,
    camera_m: Vector,
    view_size_m: Vector,
) -> Vec<LowCloudKernel> {
    const TARGET_CELL_SIZE_M: f32 = 6_000.0;
    const VIEW_MARGIN_M: f32 = 12_000.0;
    const LOBES_PER_CLUSTER: u64 = 3;

    let camera_m: Vector = world_geometry.wrap_position_m(camera_m.into()).into();
    let horizontal_cells = (world_geometry.width_m() / TARGET_CELL_SIZE_M)
        .round()
        .max(1.0) as i64;
    let vertical_cells = (world_geometry.height_m() / TARGET_CELL_SIZE_M)
        .round()
        .max(1.0) as i64;
    let cell_size_m = Vector::new(
        world_geometry.width_m() / horizontal_cells as f32,
        world_geometry.height_m() / vertical_cells as f32,
    );
    let cloud_tick = cloud_tick.min(i64::MAX as u64) as i64;
    let wind_tick = wind_tick.min(i64::MAX as u64) as i64;
    let cohort_ticks = (900.0 / DT_SECONDS) as i64;
    let current_cohort = cloud_tick.div_euclid(cohort_ticks);
    let mut kernels = Vec::new();

    for cohort in [current_cohort - 2, current_cohort - 1, current_cohort] {
        let birth_tick = cohort * cohort_ticks;
        let age_seconds = (cloud_tick - birth_tick) as f32 * DT_SECONDS;
        let temporal_weight = upper_cohort_weight(age_seconds);
        if temporal_weight <= 0.0 {
            continue;
        }
        let birth_camera_m =
            integrate_path_with_step(camera_m, age_seconds, 300.0, |position_m, elapsed| {
                let elapsed_ticks = (elapsed / DT_SECONDS).round() as i64;
                let sample_wind_tick = (wind_tick - elapsed_ticks).max(0);
                -procedural_weather(
                    world_geometry,
                    match_seed,
                    sample_wind_tick as u64,
                    position_m,
                )
                .upper_wind_velocity_m_s
            });
        let (cell_x_values, minimum_cell_y, maximum_cell_y) = candidate_cells(
            world_geometry,
            birth_camera_m,
            0.5 * view_size_m + Vector::splat(VIEW_MARGIN_M + 28_000.0),
            cell_size_m,
            horizontal_cells,
        );
        for cell_y in minimum_cell_y..=maximum_cell_y {
            for &cell_x in &cell_x_values {
                let cluster_seed = kernel_hash(match_seed, cohort, cell_x, cell_y, 100);
                if hash_unit(cluster_seed) >= 0.58 {
                    continue;
                }
                let birth_center_m = Vector::new(
                    -0.5 * world_geometry.width_m()
                        + (cell_x as f32
                            + 0.10
                            + 0.80
                                * hash_unit(kernel_hash(match_seed, cohort, cell_x, cell_y, 101)))
                            * cell_size_m.x,
                    -0.5 * world_geometry.height_m()
                        + (cell_y as f32
                            + 0.10
                            + 0.80
                                * hash_unit(kernel_hash(match_seed, cohort, cell_x, cell_y, 102)))
                            * cell_size_m.y,
                );
                let birth_weather = procedural_weather(
                    world_geometry,
                    match_seed,
                    birth_tick.max(0) as u64,
                    birth_center_m,
                );
                let formation = upper_cloud_coverage(birth_weather);
                if formation <= 0.03
                    || hash_unit(kernel_hash(match_seed, cohort, cell_x, cell_y, 103))
                        > (0.12 + 0.88 * formation)
                {
                    continue;
                }
                let center_m = integrate_path_with_step(
                    birth_center_m,
                    age_seconds,
                    300.0,
                    |position_m, elapsed| {
                        let elapsed_ticks = (elapsed / DT_SECONDS).round() as i64;
                        let sample_cloud_tick = birth_tick + elapsed_ticks;
                        let sample_wind_tick =
                            (wind_tick - (cloud_tick - sample_cloud_tick)).max(0);
                        procedural_weather(
                            world_geometry,
                            match_seed,
                            sample_wind_tick as u64,
                            position_m,
                        )
                        .upper_wind_velocity_m_s
                    },
                );
                let angle = std::f32::consts::TAU
                    * hash_unit(kernel_hash(match_seed, cohort, cell_x, cell_y, 104));
                let axis = Vector::new(angle.cos(), angle.sin());
                let perpendicular = Vector::new(-axis.y, axis.x);
                let optical_thickness = 0.35
                    + 0.75 * (1.0 - birth_weather.instability)
                    + 0.95 * birth_weather.instability
                    + 1.35 * birth_weather.rain_intensity;
                for lobe in 0..LOBES_PER_CLUSTER {
                    let lobe_seed = kernel_hash(match_seed, cohort, cell_x, cell_y, 105 + lobe);
                    let main_lobe = lobe == 0;
                    let longitudinal_offset_m = if main_lobe {
                        0.0
                    } else {
                        (2.0 * hash_unit(lobe_seed) - 1.0) * 4_200.0
                    };
                    let lateral_offset_m = if main_lobe {
                        0.0
                    } else {
                        (2.0 * hash_unit(mix_hash(lobe_seed)) - 1.0) * 2_400.0
                    };
                    let radius_m = if main_lobe {
                        6_000.0 + 3_200.0 * hash_unit(mix_hash(mix_hash(lobe_seed)))
                    } else {
                        3_000.0 + 2_200.0 * hash_unit(mix_hash(mix_hash(mix_hash(lobe_seed))))
                    };
                    let aspect_ratio = if main_lobe {
                        1.5 + 1.3 * hash_unit(mix_hash(lobe_seed ^ 0xe703_7ed1))
                    } else {
                        0.9 + hash_unit(mix_hash(lobe_seed ^ 0xe703_7ed1))
                    };
                    let support_radius_m = radius_m * aspect_ratio.max(1.0);
                    let center_m: Vector = world_geometry
                        .wrap_position_m(
                            (center_m
                                + longitudinal_offset_m * axis
                                + lateral_offset_m * perpendicular)
                                .into(),
                        )
                        .into();
                    let mut view_offset_m = center_m - camera_m;
                    view_offset_m.x -= world_geometry.width_m()
                        * (view_offset_m.x / world_geometry.width_m()).round();
                    if view_offset_m.x.abs()
                        > 0.5 * view_size_m.x + VIEW_MARGIN_M + support_radius_m
                        || view_offset_m.y.abs()
                            > 0.5 * view_size_m.y + VIEW_MARGIN_M + support_radius_m
                    {
                        continue;
                    }
                    kernels.push(LowCloudKernel {
                        center_m,
                        radius_m,
                        amplitude: temporal_weight
                            * formation
                            * optical_thickness
                            * if main_lobe { 1.0 } else { 0.68 },
                        axis,
                        aspect_ratio,
                    });
                }
            }
        }
    }
    kernels.sort_by(|left, right| {
        periodic_distance_squared(world_geometry, left.center_m, camera_m).total_cmp(
            &periodic_distance_squared(world_geometry, right.center_m, camera_m),
        )
    });
    kernels.truncate(MAX_UPPER_CLOUD_KERNELS);
    kernels
}

fn visible_low_cloud_coverage(weather: game::ProceduralWeather) -> f32 {
    let stratiform = smoothstep(0.78, 1.05, weather.relative_humidity)
        * (1.0 - weather.instability)
        * smoothstep(0.18, 0.75, weather.lift);
    let fair_weather_lift = smoothstep(0.12, 0.68, weather.instability)
        * (1.0 - smoothstep(0.08, 0.42, weather.rain_intensity))
        * (1.0 - 0.90 * stratiform);
    weather.cloudiness * (0.06 + 0.94 * fair_weather_lift)
}

fn upper_cloud_coverage(weather: game::ProceduralWeather) -> f32 {
    (weather
        .upper_cloudiness
        .max(0.30 * weather.cloudiness + 0.70 * weather.rain_intensity))
    .clamp(0.0, 1.0)
}

fn visual_cloud_wind(wind_m_s: Vector) -> Vector {
    wind_m_s
}

fn smoothstep(lower: f32, upper: f32, value: f32) -> f32 {
    let value = ((value - lower) / (upper - lower)).clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn kernel_hash(seed: u64, cohort: i64, cell_x: i64, cell_y: i64, salt: u64) -> u64 {
    mix_hash(
        seed ^ (cohort as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (cell_x as u64).wrapping_mul(0xd1b5_4a32_d192_ed03)
            ^ (cell_y as u64).wrapping_mul(0x94d0_49bb_1331_11eb)
            ^ salt.wrapping_mul(0x369d_ea0f_31a5_3f85),
    )
}

fn mix_hash(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn hash_unit(value: u64) -> f32 {
    (value >> 40) as f32 / (1_u32 << 24) as f32
}

fn candidate_cells(
    world_geometry: WorldGeometry,
    center_m: Vector,
    half_size_m: Vector,
    cell_size_m: Vector,
    horizontal_cells: i64,
) -> (Vec<i64>, i64, i64) {
    let center_m: Vector = world_geometry.wrap_position_m(center_m.into()).into();
    let minimum_cell_x = ((center_m.x - half_size_m.x + 0.5 * world_geometry.width_m())
        / cell_size_m.x)
        .floor() as i64;
    let maximum_cell_x = ((center_m.x + half_size_m.x + 0.5 * world_geometry.width_m())
        / cell_size_m.x)
        .floor() as i64;
    let cell_x_values = if maximum_cell_x - minimum_cell_x + 1 >= horizontal_cells {
        (0..horizontal_cells).collect()
    } else {
        (minimum_cell_x..=maximum_cell_x)
            .map(|cell_x| cell_x.rem_euclid(horizontal_cells))
            .collect()
    };
    let minimum_cell_y = ((center_m.y - half_size_m.y + 0.5 * world_geometry.height_m())
        / cell_size_m.y)
        .floor() as i64;
    let maximum_cell_y = ((center_m.y + half_size_m.y + 0.5 * world_geometry.height_m())
        / cell_size_m.y)
        .floor() as i64;
    (cell_x_values, minimum_cell_y, maximum_cell_y)
}

fn periodic_distance_squared(
    world_geometry: WorldGeometry,
    position_m: Vector,
    camera_m: Vector,
) -> f32 {
    let mut difference = position_m - camera_m;
    difference.x -= world_geometry.width_m() * (difference.x / world_geometry.width_m()).round();
    difference.length_squared()
}

const KERNEL_LIFETIME_SECONDS: f32 = 900.0;
fn cohort_weight(age_seconds: f32) -> f32 {
    if !(0.0..KERNEL_LIFETIME_SECONDS).contains(&age_seconds) {
        return 0.0;
    }
    let phase = std::f32::consts::PI * age_seconds / KERNEL_LIFETIME_SECONDS;
    (2.0 / 3.0) * phase.sin().powi(2)
}

fn upper_cohort_weight(age_seconds: f32) -> f32 {
    const UPPER_KERNEL_LIFETIME_SECONDS: f32 = 2_700.0;
    if !(0.0..UPPER_KERNEL_LIFETIME_SECONDS).contains(&age_seconds) {
        return 0.0;
    }
    let phase = std::f32::consts::PI * age_seconds / UPPER_KERNEL_LIFETIME_SECONDS;
    (2.0 / 3.0) * phase.sin().powi(2)
}

fn integrate_path<F>(position_m: Vector, age_seconds: f32, wind_at: F) -> Vector
where
    F: FnMut(Vector, f32) -> Vector,
{
    integrate_path_with_step(position_m, age_seconds, 60.0, wind_at)
}

fn integrate_path_with_step<F>(
    mut position_m: Vector,
    age_seconds: f32,
    step_limit_seconds: f32,
    mut wind_at: F,
) -> Vector
where
    F: FnMut(Vector, f32) -> Vector,
{
    let mut elapsed_seconds = 0.0;
    while elapsed_seconds < age_seconds {
        let step_seconds = (age_seconds - elapsed_seconds).min(step_limit_seconds);
        let initial_wind = wind_at(position_m, elapsed_seconds);
        let midpoint_seconds = elapsed_seconds + 0.5 * step_seconds;
        let midpoint_m = position_m + 0.5 * step_seconds * initial_wind;
        position_m += step_seconds * wind_at(midpoint_m, midpoint_seconds);
        elapsed_seconds += step_seconds;
    }
    position_m
}

#[cfg(test)]
mod tests {
    use super::{
        LowCloudKernelTuning, cohort_weight, integrate_path, low_cloud_kernels,
        low_cloud_kernels_with_tuning, upper_cloud_kernels, upper_cohort_weight, visual_cloud_wind,
    };
    use rapier2d::prelude::Vector;
    use simulation::WorldGeometry;

    #[test]
    fn staggered_cohorts_have_continuous_unit_weight() {
        for age_seconds in [0.0, 75.0, 150.0, 225.0, 299.99] {
            let weight = cohort_weight(age_seconds)
                + cohort_weight(age_seconds + 300.0)
                + cohort_weight(age_seconds + 600.0);
            assert!((weight - 1.0).abs() < 1e-5, "{age_seconds}: {weight}");
        }
    }

    #[test]
    fn staggered_upper_cohorts_have_continuous_unit_weight() {
        for age_seconds in [0.0, 225.0, 450.0, 675.0, 899.99] {
            let weight = upper_cohort_weight(age_seconds)
                + upper_cohort_weight(age_seconds + 900.0)
                + upper_cohort_weight(age_seconds + 1_800.0);
            assert!((weight - 1.0).abs() < 1e-5, "{age_seconds}: {weight}");
        }
    }

    #[test]
    fn path_integration_samples_a_time_varying_wind() {
        let position = integrate_path(Vector::ZERO, 300.0, |_position, seconds| {
            Vector::new(2.0, 0.01 * seconds)
        });

        assert!((position.x - 600.0).abs() < 1e-3, "{position:?}");
        assert!((position.y - 450.0).abs() < 1e-3, "{position:?}");
    }

    #[test]
    fn cloud_motion_uses_physical_wind_speed() {
        for wind_m_s in [Vector::new(1.0, 2.0), Vector::new(-18.0, 7.0)] {
            assert_eq!(visual_cloud_wind(wind_m_s), wind_m_s);
        }
    }

    #[test]
    fn zero_occurrence_produces_no_lower_clouds() {
        let geometry = WorldGeometry::default();
        assert!(
            low_cloud_kernels_with_tuning(
                geometry,
                24_301,
                54_000,
                54_000,
                Vector::ZERO,
                Vector::new(8_000.0, 8_000.0),
                LowCloudKernelTuning {
                    occurrence_scale: 0.0,
                    ..LowCloudKernelTuning::default()
                },
            )
            .is_empty()
        );
    }

    #[test]
    fn generated_field_is_deterministic_and_periodic() {
        let geometry = WorldGeometry::default();
        let cloud_tick = 54_000;
        let view_size_m = Vector::new(8_000.0, 8_000.0);
        let (seed, kernels) = (0..64)
            .find_map(|seed| {
                let kernels = low_cloud_kernels(
                    geometry,
                    seed,
                    cloud_tick,
                    cloud_tick,
                    Vector::ZERO,
                    view_size_m,
                );
                (!kernels.is_empty()).then_some((seed, kernels))
            })
            .expect("representative field should contain lower-cloud kernels");
        assert_eq!(
            kernels,
            low_cloud_kernels(
                geometry,
                seed,
                cloud_tick,
                cloud_tick,
                Vector::ZERO,
                view_size_m,
            )
        );
        assert_eq!(
            kernels,
            low_cloud_kernels(
                geometry,
                seed,
                cloud_tick,
                cloud_tick,
                Vector::new(geometry.width_m(), 0.0),
                view_size_m,
            )
        );
    }

    #[test]
    fn upper_field_is_deterministic_and_periodic() {
        let geometry = WorldGeometry::default();
        let cloud_tick = 162_000;
        let view_size_m = Vector::new(8_000.0, 8_000.0);
        let (seed, kernels) = (0..64)
            .find_map(|seed| {
                let kernels = upper_cloud_kernels(
                    geometry,
                    seed,
                    cloud_tick,
                    cloud_tick,
                    Vector::ZERO,
                    view_size_m,
                );
                (!kernels.is_empty()).then_some((seed, kernels))
            })
            .expect("representative field should contain upper-cloud kernels");
        assert_eq!(
            kernels,
            upper_cloud_kernels(
                geometry,
                seed,
                cloud_tick,
                cloud_tick,
                Vector::ZERO,
                view_size_m,
            )
        );
        assert_eq!(
            kernels,
            upper_cloud_kernels(
                geometry,
                seed,
                cloud_tick,
                cloud_tick,
                Vector::new(geometry.width_m(), 0.0),
                view_size_m,
            )
        );
    }

}
