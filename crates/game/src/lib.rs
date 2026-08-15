//! Procedural transcendental functions use `libm` so native and WebAssembly
//! builds do not silently diverge through platform math implementations.

use simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, DT_SECONDS, Environment, HullParameters,
    MotionIntent, computed_maximum_yaw_rate_rad_s, surface_control_input,
    surface_target_control_torques, yaw_rate_move_control,
};
use rapier2d::prelude::Vector;

const WIND_WEIBULL_SCALE_M_S: f64 = 9.0;
const WIND_MODES_PER_BAND: usize = 16;
const PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M: f32 = 6000.0;
const SURFACE_CROSS_ISOBAR_FRACTION: f32 = 0.12;
const UPPER_WIND_SCALE: f32 = 1.12;
const THERMAL_WIND_SCALE_M2_S_C: f32 = 12_000.0;
const LAND_WIND_SAMPLE_DISTANCES_M: [f32; 6] = [16.0, 32.0, 64.0, 128.0, 256.0, 512.0];
const LAND_WIND_FULL_OBSTRUCTION_HEIGHT_M: f32 = 1.5;
const LAND_WIND_RECOVERY_DISTANCE_M: f32 = 256.0;
const LAND_WIND_MAX_SPEED_REDUCTION: f32 = 0.7;
const TERRAIN_CLUSTER_OCTAVE_COUNT: usize = 3;
const TERRAIN_ISLAND_OCTAVE_COUNT: usize = 5;
const TERRAIN_REEF_OCTAVE_COUNT: usize = 4;
const WORLD_DAY_SECONDS: f64 = 86_400.0;
const WORLD_YEAR_DAYS: f64 = 360.0;
const WORLD_LUNAR_CYCLE_DAYS: f64 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmospherePrimary {
    pub pressure_hpa: f32,
    pub pressure_gradient_hpa_per_m: Vector,
    pub synoptic_pressure_gradient_hpa_per_m: Vector,
    pub low_temperature_c: f32,
    pub upper_temperature_c: f32,
    pub temperature_gradient_c_per_m: Vector,
    pub water_vapor_g_kg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialConditions {
    pub solar_latitude_degrees: f32,
    pub season_phase: f32,
    pub day_phase: f32,
    pub moon_phase: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialLighting {
    pub sun_direction: [f32; 3],
    pub sun_intensity: f32,
    pub moon_direction: [f32; 3],
    pub moon_intensity: f32,
    pub ambient_intensity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimateConditions {
    pub annual_mean_low_temperature_c: f32,
    pub seasonal_temperature_range_c: f32,
    pub diurnal_temperature_range_c: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TidalConditions {
    pub reference_range_m: f32,
    pub phase_offset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceClimate {
    pub air_temperature_c: f32,
    pub ground_temperature_c: f32,
    pub water_temperature_c: f32,
    pub seasonal_snow_cover: f32,
    pub sea_ice_cover: f32,
    pub glacier_cover: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProceduralWeather {
    pub primary: AtmospherePrimary,
    pub wind_velocity_m_s: Vector,
    pub upper_wind_velocity_m_s: Vector,
    pub wind_shear_m_s: Vector,
    pub relative_humidity: f32,
    pub lift: f32,
    pub cloudiness: f32,
    pub instability: f32,
    pub upper_cloudiness: f32,
    pub rain_intensity: f32,
}

pub fn celestial_lighting(conditions: CelestialConditions) -> CelestialLighting {
    let latitude = f64::from(conditions.solar_latitude_degrees).to_radians();
    let solar_longitude = std::f64::consts::TAU * f64::from(conditions.season_phase);
    let obliquity = 23.44_f64.to_radians();
    let solar_declination = libm::asin(libm::sin(obliquity) * libm::sin(solar_longitude));
    let solar_hour_angle = std::f64::consts::TAU * (f64::from(conditions.day_phase) - 0.5);
    let sun_direction = celestial_direction(latitude, solar_declination, solar_hour_angle);
    let sun_horizon = smoothstep_range(0.0, 0.08, f64::from(sun_direction[2])) as f32;
    let sun_elevation = libm::sqrt(f64::from(sun_direction[2].max(0.0))) as f32;
    let sun_intensity = sun_horizon * (0.35 + 0.65 * sun_elevation);

    let phase_angle = std::f64::consts::TAU * f64::from(conditions.moon_phase.rem_euclid(1.0));
    let lunar_longitude = solar_longitude + phase_angle;
    let lunar_declination = libm::asin(libm::sin(obliquity) * libm::sin(lunar_longitude));
    let moon_direction =
        celestial_direction(latitude, lunar_declination, solar_hour_angle + phase_angle);
    let illuminated_fraction = 0.5 - 0.5 * libm::cos(phase_angle);
    let moon_intensity = 0.12
        * illuminated_fraction as f32
        * smoothstep_range(0.0, 0.08, f64::from(moon_direction[2])) as f32;
    let twilight = smoothstep_range(-0.18, 0.12, f64::from(sun_direction[2])) as f32;
    let ambient_intensity =
        (0.035 + twilight * (0.45 + 0.515 * sun_elevation) + 0.85 * moon_intensity).min(1.0);

    CelestialLighting {
        sun_direction,
        sun_intensity,
        moon_direction,
        moon_intensity,
        ambient_intensity,
    }
}

pub fn current_low_temperature_c(
    climate: ClimateConditions,
    conditions: CelestialConditions,
) -> f32 {
    let hemisphere = if conditions.solar_latitude_degrees < 0.0 {
        -1.0
    } else {
        1.0
    };
    let seasonal_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.09);
    let diurnal_phase = std::f64::consts::TAU * (f64::from(conditions.day_phase) - 0.625);
    let latitude_seasonality = 0.15
        + 0.85
            * smoothstep_range(
                0.0,
                45.0,
                f64::from(conditions.solar_latitude_degrees.abs()),
            ) as f32;
    climate.annual_mean_low_temperature_c
        + 0.5
            * climate.seasonal_temperature_range_c
            * latitude_seasonality
            * hemisphere
            * libm::sin(seasonal_phase) as f32
        + 0.5 * climate.diurnal_temperature_range_c * libm::cos(diurnal_phase) as f32
}

pub fn tide_height_m(tide: TidalConditions, conditions: CelestialConditions) -> f32 {
    let solar_hour_angle = std::f64::consts::TAU * (f64::from(conditions.day_phase) - 0.5);
    let lunar_hour_angle =
        solar_hour_angle + std::f64::consts::TAU * f64::from(conditions.moon_phase);
    let phase_offset = std::f64::consts::TAU * f64::from(tide.phase_offset);
    let lunar_tide = libm::cos(2.0 * (lunar_hour_angle + phase_offset));
    let solar_tide = 0.46 * libm::cos(2.0 * (solar_hour_angle + phase_offset));
    0.5 * tide.reference_range_m * ((lunar_tide + solar_tide) / 1.46) as f32
}

pub fn world_celestial_conditions(world_time_seconds: f64) -> CelestialConditions {
    CelestialConditions {
        solar_latitude_degrees: 45.0,
        season_phase: (0.2521 + world_time_seconds / (WORLD_DAY_SECONDS * WORLD_YEAR_DAYS))
            .rem_euclid(1.0) as f32,
        day_phase: (0.5833 + world_time_seconds / WORLD_DAY_SECONDS).rem_euclid(1.0) as f32,
        moon_phase: (0.25 + world_time_seconds / (WORLD_DAY_SECONDS * WORLD_LUNAR_CYCLE_DAYS))
            .rem_euclid(1.0) as f32,
    }
}

pub fn procedural_tidal_conditions(match_seed: u64, position_m: Vector) -> TidalConditions {
    let response =
        terrain_fractal_noise(match_seed ^ 0x94d0_49bb_1331_11eb, position_m, 131_072.0, 3) as f32;
    let lag =
        terrain_fractal_noise(match_seed ^ 0xbf58_476d_1ce4_e5b9, position_m, 131_072.0, 3) as f32;
    TidalConditions {
        reference_range_m: 1.25 + 0.75 * response,
        phase_offset: 0.08 * lag,
    }
}

pub fn procedural_water_height_m(
    match_seed: u64,
    world_time_seconds: f64,
    position_m: Vector,
) -> f32 {
    tide_height_m(
        procedural_tidal_conditions(match_seed, position_m),
        world_celestial_conditions(world_time_seconds),
    )
}

pub fn procedural_water_depth_m(
    match_seed: u64,
    world_time_seconds: f64,
    position_m: Vector,
) -> f32 {
    (procedural_water_height_m(match_seed, world_time_seconds, position_m)
        - procedural_terrain_elevation_m(match_seed, position_m))
    .max(0.0)
}

pub fn climate_surface(
    climate: ClimateConditions,
    conditions: CelestialConditions,
    water_vapor_g_kg: f32,
    elevation_m: f32,
) -> SurfaceClimate {
    let air_temperature_c = current_low_temperature_c(climate, conditions);
    let hemisphere = if conditions.solar_latitude_degrees < 0.0 {
        -1.0
    } else {
        1.0
    };
    let latitude_seasonality = 0.15
        + 0.85
            * smoothstep_range(
                0.0,
                45.0,
                f64::from(conditions.solar_latitude_degrees.abs()),
            ) as f32;
    let seasonal_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.13);
    let ground_temperature_c = climate.annual_mean_low_temperature_c
        + 0.42
            * climate.seasonal_temperature_range_c
            * latitude_seasonality
            * hemisphere
            * libm::sin(seasonal_phase) as f32;
    let water_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.175);
    let water_temperature_c = climate.annual_mean_low_temperature_c
        + 0.28
            * climate.seasonal_temperature_range_c
            * latitude_seasonality
            * hemisphere
            * libm::sin(water_phase) as f32;
    let ground_temperature_at_elevation_c = ground_temperature_c - 0.0065 * elevation_m.max(0.0);
    let moisture_support = smoothstep_range(2.0, 10.0, f64::from(water_vapor_g_kg)) as f32;
    let seasonal_snow_cover = (1.0
        - smoothstep_range(-6.0, 2.0, f64::from(ground_temperature_at_elevation_c)) as f32)
        * (0.35 + 0.65 * moisture_support);
    let sea_ice_cover = 1.0 - smoothstep_range(-2.2, 1.0, f64::from(water_temperature_c)) as f32;
    let annual_temperature_at_elevation_c =
        climate.annual_mean_low_temperature_c - 0.0065 * elevation_m.max(0.0);
    let glacier_cover = (1.0
        - smoothstep_range(-5.0, 1.0, f64::from(annual_temperature_at_elevation_c)) as f32)
        * moisture_support;

    SurfaceClimate {
        air_temperature_c,
        ground_temperature_c,
        water_temperature_c,
        seasonal_snow_cover,
        sea_ice_cover,
        glacier_cover,
    }
}

fn celestial_direction(latitude: f64, declination: f64, hour_angle: f64) -> [f32; 3] {
    let east = -libm::cos(declination) * libm::sin(hour_angle);
    let north = libm::sin(declination) * libm::cos(latitude)
        - libm::cos(declination) * libm::sin(latitude) * libm::cos(hour_angle);
    let up = libm::sin(latitude) * libm::sin(declination)
        + libm::cos(latitude) * libm::cos(declination) * libm::cos(hour_angle);
    [east as f32, north as f32, up as f32]
}

#[derive(Clone, Copy)]
struct PressureSample {
    potential_m2_s: f64,
    gradient_m_s: Vector,
    synoptic_gradient_m_s: Vector,
}

#[derive(Clone, Copy)]
struct WindBand {
    minimum_wavelength_m: f64,
    maximum_wavelength_m: f64,
    variance_fraction: f64,
}

const WIND_BANDS: [WindBand; 3] = [
    WindBand {
        minimum_wavelength_m: 32_768.0,
        maximum_wavelength_m: 131_072.0,
        variance_fraction: 0.90,
    },
    WindBand {
        minimum_wavelength_m: 4096.0,
        maximum_wavelength_m: 32_768.0,
        variance_fraction: 0.08,
    },
    WindBand {
        minimum_wavelength_m: 64.0,
        maximum_wavelength_m: 512.0,
        variance_fraction: 0.02,
    },
];

pub fn dinghy_parameters() -> BoatParameters {
    BoatParameters {
        hull: HullParameters {
            mass_kg: 222.0,
            length_m: 4.0,
            beam_m: 1.5,
            bow_length_m: 0.7,
        },
        sail_mass_kg: 12.0,
        boom_mass_kg: 10.0,
        boom_length_m: 2.0,
        mast_position_m: Vector::new(0.4, 0.0),
        max_sail_angle_rad: std::f32::consts::FRAC_PI_2,
        air_density_kg_m3: 1.225,
        sail_area_m2: 8.0,
        sail_center_of_effort_from_mast_m: 0.7,
        sail_base_drag_coefficient: 0.02,
        water_density_kg_m3: 1025.0,
        keel_area_m2: 0.6,
        keel_position_m: Vector::ZERO,
        keel_base_drag_coefficient: 0.02,
        keel_stall_angle_rad: 15.0_f32.to_radians(),
        keel_stall_lift_coefficient: 0.8,
        hull_reference_area_m2: 0.3,
        hull_drag_coefficient: 0.08,
        hull_lateral_drag_coefficient: 0.02,
        rudder_mass_kg: 4.0,
        tiller_mass_kg: 2.0,
        rudder_draw_length_m: 0.5,
        tiller_length_m: 1.0,
        rudder_position_m: Vector::new(-1.8, 0.0),
        max_rudder_angle_rad: 30.0_f32.to_radians(),
        rudder_area_m2: 0.12,
        rudder_base_drag_coefficient: 0.02,
        rudder_stall_angle_rad: 15.0_f32.to_radians(),
        rudder_stall_lift_coefficient: 0.8,
        max_sail_actuator_torque_n_m: 375.0,
        max_rudder_actuator_torque_n_m: 60.0,
        sail_bearing_damping_n_m_s_rad: 20.0,
        rudder_bearing_damping_n_m_s_rad: 2.0,
        draft_m: 0.6,
        ground_static_friction_coefficient: 1.0,
        ground_kinetic_friction_coefficient: 0.6,
    }
}

pub fn match_environment() -> Environment {
    Environment {
        true_wind_velocity_m_s: Vector::new(6.0, 0.0),
        true_water_velocity_m_s: Vector::ZERO,
        water_depth_m: f32::INFINITY,
    }
}

pub fn procedural_match_environment(match_seed: u64, tick: u64, position_m: Vector) -> Environment {
    let open_wind = procedural_unsheltered_wind_velocity_m_s(match_seed, tick, position_m);
    Environment {
        true_wind_velocity_m_s: land_sheltered_wind(match_seed, position_m, open_wind),
        true_water_velocity_m_s: Vector::ZERO,
        water_depth_m: procedural_water_depth_m(match_seed, tick as f64 / 60.0, position_m),
    }
}

pub fn procedural_weather(match_seed: u64, tick: u64, position_m: Vector) -> ProceduralWeather {
    derive_weather(procedural_atmosphere_primary(match_seed, tick, position_m))
}

pub fn procedural_atmosphere_primary(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> AtmospherePrimary {
    let pressure = procedural_pressure_sample(match_seed, tick, position_m);
    let low_temperature_c = sample_low_temperature_c(match_seed, tick, position_m);
    let upper_temperature_c = -20.0
        + 9.0
            * procedural_weather_noise(
                match_seed,
                tick,
                position_m,
                0xbe54_66cf_34e9_0c6c,
                65_536.0,
                4,
            ) as f32;
    let sample_distance_m = 256.0;
    let temperature_gradient_c_per_m = Vector::new(
        sample_low_temperature_c(
            match_seed,
            tick,
            position_m + Vector::new(sample_distance_m, 0.0),
        ) - sample_low_temperature_c(
            match_seed,
            tick,
            position_m - Vector::new(sample_distance_m, 0.0),
        ),
        sample_low_temperature_c(
            match_seed,
            tick,
            position_m + Vector::new(0.0, sample_distance_m),
        ) - sample_low_temperature_c(
            match_seed,
            tick,
            position_m - Vector::new(0.0, sample_distance_m),
        ),
    ) / (2.0 * sample_distance_m);
    let water_vapor_g_kg = (7.0
        + 7.0
            * procedural_weather_noise(
                match_seed,
                tick,
                position_m,
                0x082e_fa98_ec4e_6c89,
                3584.0,
                5,
            ) as f32)
        .clamp(1.0, 14.0);

    AtmospherePrimary {
        pressure_hpa: 1013.25
            + pressure.potential_m2_s as f32 / PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M,
        pressure_gradient_hpa_per_m: pressure.gradient_m_s / PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M,
        synoptic_pressure_gradient_hpa_per_m: pressure.synoptic_gradient_m_s
            / PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M,
        low_temperature_c,
        upper_temperature_c,
        temperature_gradient_c_per_m,
        water_vapor_g_kg,
    }
}

pub fn derive_weather(primary: AtmospherePrimary) -> ProceduralWeather {
    let wind_velocity_m_s = wind_from_pressure_gradient(primary.pressure_gradient_hpa_per_m);
    let upper_pressure_wind_m_s = UPPER_WIND_SCALE
        * wind_from_pressure_gradient(primary.synoptic_pressure_gradient_hpa_per_m);
    let thermal_wind_m_s = THERMAL_WIND_SCALE_M2_S_C
        * Vector::new(
            -primary.temperature_gradient_c_per_m.y,
            primary.temperature_gradient_c_per_m.x,
        );
    let upper_wind_velocity_m_s = upper_pressure_wind_m_s + thermal_wind_m_s;
    let saturation_capacity_g_kg =
        3.5 * libm::exp(0.055 * f64::from(primary.low_temperature_c + 5.0)) as f32;
    let relative_humidity = (primary.water_vapor_g_kg / saturation_capacity_g_kg).clamp(0.0, 1.5);
    let instability = smoothstep_range(
        28.0,
        48.0,
        f64::from(primary.low_temperature_c - primary.upper_temperature_c),
    ) as f32;
    let pressure_lift = smoothstep_range(1012.0, 990.0, f64::from(primary.pressure_hpa)) as f32;
    let gradient_lift = smoothstep_range(
        0.0002,
        0.0015,
        f64::from(primary.pressure_gradient_hpa_per_m.length()),
    ) as f32;
    let frontal_lift = smoothstep_range(
        0.00005,
        0.0005,
        f64::from(primary.temperature_gradient_c_per_m.length()),
    ) as f32;
    let lift = (0.45 * pressure_lift + 0.35 * gradient_lift + 0.20 * frontal_lift).clamp(0.0, 1.0);
    let condensation = smoothstep_range(0.45, 0.95, f64::from(relative_humidity)) as f32;
    let cloud_signal = 0.68 * condensation + 0.22 * lift + 0.10 * instability * condensation;
    let cloudiness = smoothstep_range(0.50, 0.98, f64::from(cloud_signal)) as f32;
    let upper_cloud_signal = 0.55 * condensation + 0.30 * lift + 0.15 * frontal_lift;
    let upper_cloudiness = smoothstep_range(0.68, 0.90, f64::from(upper_cloud_signal)) as f32;
    let rain_signal = cloudiness * (0.45 + 0.20 * instability + 0.45 * lift);

    ProceduralWeather {
        primary,
        wind_velocity_m_s,
        upper_wind_velocity_m_s,
        wind_shear_m_s: upper_wind_velocity_m_s - wind_velocity_m_s,
        relative_humidity,
        lift,
        cloudiness,
        instability,
        upper_cloudiness,
        rain_intensity: smoothstep_range(0.55, 0.90, f64::from(rain_signal)) as f32,
    }
}

fn sample_low_temperature_c(match_seed: u64, tick: u64, position_m: Vector) -> f32 {
    16.0 + 13.0
        * procedural_weather_noise(
            match_seed,
            tick,
            position_m,
            0x4528_21e6_38d0_1377,
            65_536.0,
            4,
        ) as f32
}

fn wind_from_pressure_gradient(pressure_gradient_hpa_per_m: Vector) -> Vector {
    let along_isobar = Vector::new(
        -pressure_gradient_hpa_per_m.y,
        pressure_gradient_hpa_per_m.x,
    );
    let normalization = (1.0 + SURFACE_CROSS_ISOBAR_FRACTION.powi(2)).sqrt();
    PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M
        * (along_isobar - SURFACE_CROSS_ISOBAR_FRACTION * pressure_gradient_hpa_per_m)
        / normalization
}

pub fn procedural_terrain_elevation_m(match_seed: u64, position_m: Vector) -> f32 {
    // Keep this composition synchronized with `procedural_terrain_sample` in
    // `crates/browser/src/scene.wgsl`: Rust supplies physical depth while WGSL
    // renders the same shoreline.
    let cluster = terrain_fractal_noise(
        match_seed ^ 0xa076_1d64_78bd_642f,
        position_m,
        8192.0,
        TERRAIN_CLUSTER_OCTAVE_COUNT,
    );
    let island = terrain_fractal_noise(
        match_seed ^ 0xe703_7ed1_a0b4_28db,
        position_m,
        1024.0,
        TERRAIN_ISLAND_OCTAVE_COUNT,
    );
    let local_density = 0.68 * cluster + 0.32 * island;
    let unscaled_elevation_m = 18.0 * (local_density - 0.35);
    let island_ruggedness = smoothstep_range(0.32, 0.58, cluster);
    let trench_depth = smoothstep_range(0.50, 0.74, -cluster);
    let elevation_scale = 3.0 * (1.0 + 19.0 * island_ruggedness + 29.0 * trench_depth);
    let base_elevation_m = unscaled_elevation_m * elevation_scale;
    let base_depth_m = -base_elevation_m;
    // Preserve measured coastal relief before deepening the open ocean; applying
    // the remap at the shore would turn navigable coastal slopes into abyss walls.
    let offshore_amount = smoothstep_range(4.0, 24.0, base_depth_m);
    let ocean_floor_depth_m = 3_000.0 + 3_000.0 * trench_depth;
    let terrain_elevation_m =
        -(base_depth_m + offshore_amount * (ocean_floor_depth_m - base_depth_m));
    // Reef gates use the pre-remap coast so open-ocean depth does not change the
    // distribution of channels and emergent rocks.
    let seabed_mask = smoothstep_range(-4.0, -2.5, base_elevation_m)
        * (1.0 - smoothstep_range(-0.5, -0.1, base_elevation_m));
    let rock_mask = smoothstep_range(-2.0, -0.8, base_elevation_m)
        * (1.0 - smoothstep_range(-0.4, -0.1, base_elevation_m));
    if seabed_mask == 0.0 && rock_mask == 0.0 {
        return terrain_elevation_m as f32;
    }
    let reef = terrain_fractal_noise(
        match_seed ^ 0x5899_65cc_7537_4cc3,
        position_m,
        32.0,
        TERRAIN_REEF_OCTAVE_COUNT,
    );
    let reef_peak = smoothstep_range(0.65, 0.85, reef);
    (terrain_elevation_m + 0.6 * seabed_mask * reef + 2.0 * rock_mask * reef_peak) as f32
}

pub fn procedural_cloud_cover(match_seed: u64, tick: u64, position_m: Vector) -> f32 {
    procedural_weather(match_seed, tick, position_m).cloudiness
}

fn procedural_weather_noise(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
    salt: u64,
    wavelength_m: f64,
    octave_count: usize,
) -> f64 {
    let (transported_x_m, transported_y_m) =
        transported_weather_position_m(match_seed, tick, position_m);
    terrain_fractal_noise(
        match_seed ^ salt,
        Vector::new(transported_x_m as f32, transported_y_m as f32),
        wavelength_m,
        octave_count,
    )
}

fn terrain_fractal_noise(
    field_seed: u64,
    position_m: Vector,
    mut wavelength_m: f64,
    octave_count: usize,
) -> f64 {
    let mut noise = 0.0;
    let mut amplitude = 1.0;
    let mut amplitude_sum = 0.0;
    for octave in 0..octave_count {
        noise += amplitude
            * terrain_value_noise(
                field_seed ^ (octave as u64).wrapping_mul(0xd1b5_4a32_d192_ed03),
                position_m,
                wavelength_m,
            );
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    noise / amplitude_sum
}

fn terrain_value_noise(match_seed: u64, position_m: Vector, wavelength_m: f64) -> f64 {
    let x = f64::from(position_m.x) / wavelength_m;
    let y = f64::from(position_m.y) / wavelength_m;
    let x0 = libm::floor(x) as i64;
    let y0 = libm::floor(y) as i64;
    let blend_x = smoothstep(x - x0 as f64);
    let blend_y = smoothstep(y - y0 as f64);
    let bottom = lerp(
        terrain_lattice_value(match_seed, x0, y0),
        terrain_lattice_value(match_seed, x0 + 1, y0),
        blend_x,
    );
    let top = lerp(
        terrain_lattice_value(match_seed, x0, y0 + 1),
        terrain_lattice_value(match_seed, x0 + 1, y0 + 1),
        blend_x,
    );
    lerp(bottom, top, blend_y)
}

fn terrain_lattice_value(match_seed: u64, x: i64, y: i64) -> f64 {
    let key = match_seed
        ^ (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
    2.0 * hash_unit(key) - 1.0
}

fn smoothstep(value: f64) -> f64 {
    value * value * (3.0 - 2.0 * value)
}

fn smoothstep_range(lower: f64, upper: f64, value: f64) -> f64 {
    smoothstep(((value - lower) / (upper - lower)).clamp(0.0, 1.0))
}

fn lerp(first: f64, second: f64, amount: f64) -> f64 {
    first + amount * (second - first)
}

pub fn procedural_unsheltered_wind_velocity_m_s(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> Vector {
    let pressure = procedural_pressure_sample(match_seed, tick, position_m);
    wind_from_pressure_gradient(pressure.gradient_m_s / PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M)
}

fn procedural_pressure_sample(match_seed: u64, tick: u64, position_m: Vector) -> PressureSample {
    // Rotating the analytic gradient of one scalar potential gives coherent flow
    // without sources or sinks; `wind_from_pressure_gradient` then adds the small
    // cross-isobar component used by the surface model.
    let (transported_x_m, transported_y_m) =
        transported_weather_position_m(match_seed, tick, position_m);
    let mut potential_m2_s = 0.0;
    let mut gradient_x_m_s = 0.0;
    let mut gradient_y_m_s = 0.0;
    let mut synoptic_gradient_x_m_s = 0.0;
    let mut synoptic_gradient_y_m_s = 0.0;
    for (band_index, band) in WIND_BANDS.iter().enumerate() {
        let band_key = match_seed ^ (band_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let rotation = hash_unit(band_key ^ 0xa076_1d64_78bd_642f);
        let amplitude_m_s = WIND_WEIBULL_SCALE_M_S
            * libm::sqrt(band.variance_fraction / WIND_MODES_PER_BAND as f64);
        for mode in 0..WIND_MODES_PER_BAND {
            let mode_key = band_key ^ (mode as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
            let direction_rad =
                std::f64::consts::TAU * (mode as f64 / WIND_MODES_PER_BAND as f64 + rotation);
            let direction_x = libm::cos(direction_rad);
            let direction_y = libm::sin(direction_rad);
            let wavelength_fraction = hash_unit(mode_key ^ 0x8ebc_6af0_9c88_c6e3);
            let wavelength_m = band.minimum_wavelength_m
                * libm::exp(
                    libm::log(band.maximum_wavelength_m / band.minimum_wavelength_m)
                        * wavelength_fraction,
                );
            let phase_rad = std::f64::consts::TAU
                * ((transported_x_m * direction_x + transported_y_m * direction_y) / wavelength_m);
            let (cosine_coefficient, sine_coefficient) = normal_pair(mode_key);
            let wave =
                cosine_coefficient * libm::cos(phase_rad) + sine_coefficient * libm::sin(phase_rad);
            let gradient_x = amplitude_m_s * direction_x * wave;
            let gradient_y = amplitude_m_s * direction_y * wave;
            potential_m2_s += amplitude_m_s * wavelength_m / std::f64::consts::TAU
                * (cosine_coefficient * libm::sin(phase_rad)
                    - sine_coefficient * libm::cos(phase_rad));
            gradient_x_m_s += gradient_x;
            gradient_y_m_s += gradient_y;
            if band_index == 0 {
                synoptic_gradient_x_m_s += gradient_x;
                synoptic_gradient_y_m_s += gradient_y;
            }
        }
    }
    PressureSample {
        potential_m2_s,
        gradient_m_s: Vector::new(gradient_x_m_s as f32, gradient_y_m_s as f32),
        synoptic_gradient_m_s: Vector::new(
            synoptic_gradient_x_m_s as f32,
            synoptic_gradient_y_m_s as f32,
        ),
    }
}

fn transported_weather_position_m(match_seed: u64, tick: u64, position_m: Vector) -> (f64, f64) {
    let time_seconds = tick as f64 * f64::from(DT_SECONDS);
    let transport_direction_rad =
        std::f64::consts::TAU * hash_unit(match_seed ^ 0x243f_6a88_85a3_08d3);
    (
        f64::from(position_m.x) - 6.0 * libm::cos(transport_direction_rad) * time_seconds,
        f64::from(position_m.y) - 6.0 * libm::sin(transport_direction_rad) * time_seconds,
    )
}

fn land_sheltered_wind(match_seed: u64, position_m: Vector, open_wind: Vector) -> Vector {
    let speed_m_s = open_wind.length();
    if speed_m_s == 0.0 {
        return open_wind;
    }
    let upwind = -open_wind / speed_m_s;
    let mut shelter: f32 = 0.0;
    for distance_m in LAND_WIND_SAMPLE_DISTANCES_M {
        let elevation_m =
            procedural_terrain_elevation_m(match_seed, position_m + distance_m * upwind);
        let obstruction = (elevation_m / LAND_WIND_FULL_OBSTRUCTION_HEIGHT_M).clamp(0.0, 1.0);
        let recovery = 1.0 / (1.0 + distance_m / LAND_WIND_RECOVERY_DISTANCE_M);
        shelter = shelter.max(obstruction * recovery);
    }
    open_wind * (1.0 - LAND_WIND_MAX_SPEED_REDUCTION * shelter)
}

fn normal_pair(key: u64) -> (f64, f64) {
    let radius = libm::sqrt(-2.0 * libm::log(hash_unit(key ^ 0x5899_65cc_7537_4cc3)));
    let angle = std::f64::consts::TAU * hash_unit(key ^ 0x1d8e_4e27_c47d_124f);
    (radius * libm::cos(angle), radius * libm::sin(angle))
}

fn hash_unit(value: u64) -> f64 {
    let hash = wind_hash(value);
    ((hash >> 11) as f64 + 0.5) / (1_u64 << 53) as f64
}

fn wind_hash(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^= value >> 31;
    value
}

pub fn computed_yaw_rate_torques(motion: BoatMotion, intent: MotionIntent) -> ActuatorTorques {
    computed_yaw_rate_torques_in_environment(motion, intent, match_environment())
}

pub fn computed_yaw_rate_torques_in_environment(
    motion: BoatMotion,
    intent: MotionIntent,
    environment: Environment,
) -> ActuatorTorques {
    let parameters = dinghy_parameters();
    let input = surface_control_input(parameters, motion, environment, 1, 10.0_f32.to_radians());
    let maximum_yaw_rate_rad_s = computed_maximum_yaw_rate_rad_s(parameters, motion, environment);
    let targets = yaw_rate_move_control(&input, intent, maximum_yaw_rate_rad_s);
    surface_target_control_torques(parameters, motion, targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_wind_has_uniform_direction_and_weibull_speed() {
        const SAMPLE_COUNT: usize = 16_384;
        let mut quadrant_counts = [0; 4];
        let mut speed_bins = [0; 31];
        let mut speed_sum = 0.0;
        let mut x_square_sum = 0.0;
        let mut y_square_sum = 0.0;
        let mut cross_sum = 0.0;
        let mut maximum_speed: f32 = 0.0;

        for seed in 0..SAMPLE_COUNT as u64 {
            let velocity =
                procedural_unsheltered_wind_velocity_m_s(seed, 12_345, Vector::new(12.0, -7.0));
            let quadrant = match (velocity.x >= 0.0, velocity.y >= 0.0) {
                (true, true) => 0,
                (false, true) => 1,
                (false, false) => 2,
                (true, false) => 3,
            };
            quadrant_counts[quadrant] += 1;
            let speed = velocity.length();
            speed_bins[(speed.floor() as usize).min(speed_bins.len() - 1)] += 1;
            speed_sum += speed;
            x_square_sum += velocity.x * velocity.x;
            y_square_sum += velocity.y * velocity.y;
            cross_sum += velocity.x * velocity.y;
            maximum_speed = maximum_speed.max(speed);
        }

        for count in quadrant_counts {
            assert!(count > SAMPLE_COUNT / 8);
        }
        let mean_speed = speed_sum / SAMPLE_COUNT as f32;
        assert!((7.6..=8.4).contains(&mean_speed));
        assert!((38.0..=43.0).contains(&(x_square_sum / SAMPLE_COUNT as f32)));
        assert!((38.0..=43.0).contains(&(y_square_sum / SAMPLE_COUNT as f32)));
        assert!((cross_sum / SAMPLE_COUNT as f32).abs() < 1.0);
        let mode_bin = speed_bins
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| *count)
            .unwrap()
            .0;
        assert!((5..=7).contains(&mode_bin));
        assert!(speed_bins[0] > 0);
        assert!(speed_bins[6] > speed_bins[12]);
        assert!(speed_bins[12] > speed_bins[18]);
        assert!(maximum_speed > 25.0);
    }

    #[test]
    fn upwind_land_shelters_nearby_water() {
        let match_seed = 0x5eed;
        let tick = 12_345;
        let mut strongest_reduction: f32 = 0.0;
        let mut sheltered_water_found = false;

        for x in -32..=32 {
            for y in -32..=32 {
                let position_m = Vector::new(x as f32 * 128.0, y as f32 * 128.0);
                if procedural_terrain_elevation_m(match_seed, position_m) >= 0.0 {
                    continue;
                }
                let open_wind =
                    procedural_unsheltered_wind_velocity_m_s(match_seed, tick, position_m);
                let open_speed_m_s = open_wind.length();
                if open_speed_m_s < 2.0 {
                    continue;
                }
                let upwind = -open_wind / open_speed_m_s;
                let obstruction_m = [64.0, 128.0, 256.0, 512.0]
                    .into_iter()
                    .map(|distance_m| {
                        procedural_terrain_elevation_m(match_seed, position_m + distance_m * upwind)
                    })
                    .fold(0.0_f32, f32::max);
                if obstruction_m < 1.0 {
                    continue;
                }

                sheltered_water_found = true;
                let local_speed_m_s = procedural_match_environment(match_seed, tick, position_m)
                    .true_wind_velocity_m_s
                    .length();
                strongest_reduction =
                    strongest_reduction.max(1.0 - local_speed_m_s / open_speed_m_s);
            }
        }

        assert!(sheltered_water_found);
        assert!(strongest_reduction > 0.15, "{strongest_reduction}");
    }
}
