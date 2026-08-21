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
const AIR_TEMPERATURE_500_HPA_OFFSET_C: f32 = 35.5;
const AIR_TEMPERATURE_500_HPA_SEASONAL_RANGE_FRACTION: f32 = 0.25;
const SURFACE_AIR_MASS_COUPLING_500_HPA: f32 = 0.45;
const INDEPENDENT_AIR_TEMPERATURE_500_HPA_RANGE_C: f32 = 6.0;
const BASE_WEATHER_TRANSPORT_SPEED_M_S: f64 = 6.0;
const WEATHER_TRANSPORT_DIRECTION_SPREAD_RAD: f64 = std::f64::consts::PI / 720.0;
const SURFACE_AIR_TEMPERATURE_NOISE_SALT: u64 = 0x4528_21e6_38d0_1377;
const AIR_TEMPERATURE_500_HPA_NOISE_SALT: u64 = 0xbe54_66cf_34e9_0c6c;
const WATER_VAPOR_NOISE_SALT: u64 = 0x082e_fa98_ec4e_6c89;
const SURFACE_AIR_MASS_TRANSPORT_SPEED_SCALE: f64 = 0.9;
const AIR_TEMPERATURE_500_HPA_TRANSPORT_SPEED_SCALE: f64 = 1.15;
const WATER_VAPOR_TRANSPORT_SPEED_SCALE: f64 = 1.0;
const LAND_WIND_SAMPLE_DISTANCES_M: [f32; 6] = [16.0, 32.0, 64.0, 128.0, 256.0, 512.0];
const LAND_WIND_FULL_OBSTRUCTION_HEIGHT_M: f32 = 1.5;
const LAND_WIND_RECOVERY_DISTANCE_M: f32 = 256.0;
const LAND_WIND_MAX_SPEED_REDUCTION: f32 = 0.7;
const TERRAIN_CLUSTER_OCTAVE_COUNT: usize = 3;
const TERRAIN_ISLAND_OCTAVE_COUNT: usize = 5;
const TERRAIN_SHOAL_OCTAVE_COUNT: usize = 3;
const TERRAIN_REEF_OCTAVE_COUNT: usize = 4;
// OpenSimplex2's CC0 24-direction 2D gradient layout reduces repeated contour
// features when a field is thresholded. Keep values and order synchronized with
// `scene.wgsl` so its hash selects the same gradient.
const SIMPLEX_GRADIENT_DIRECTIONS: [(f32, f32); 24] = [
    (0.9914449, 0.1305262),
    (0.9238795, 0.3826834),
    (0.7933533, 0.6087614),
    (0.6087614, 0.7933533),
    (0.3826834, 0.9238795),
    (0.1305262, 0.9914449),
    (-0.1305262, 0.9914449),
    (-0.3826834, 0.9238795),
    (-0.6087614, 0.7933533),
    (-0.7933533, 0.6087614),
    (-0.9238795, 0.3826834),
    (-0.9914449, 0.1305262),
    (-0.9914449, -0.1305262),
    (-0.9238795, -0.3826834),
    (-0.7933533, -0.6087614),
    (-0.6087614, -0.7933533),
    (-0.3826834, -0.9238795),
    (-0.1305262, -0.9914449),
    (0.1305262, -0.9914449),
    (0.3826834, -0.9238795),
    (0.6087614, -0.7933533),
    (0.7933533, -0.6087614),
    (0.9238795, -0.3826834),
    (0.9914449, -0.1305262),
];
const WORLD_DAY_SECONDS: f64 = 86_400.0;
const WORLD_YEAR_DAYS: f64 = 360.0;
const WORLD_LUNAR_CYCLE_DAYS: f64 = 28.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmospherePrimary {
    pub pressure_hpa: f32,
    pub pressure_gradient_hpa_per_m: Vector,
    pub synoptic_pressure_gradient_hpa_per_m: Vector,
    pub surface_air_temperature_anomaly_c: f32,
    pub air_temperature_500_hpa_c: f32,
    pub temperature_gradient_c_per_m: Vector,
    pub water_vapor_g_kg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialConditions {
    pub geographic_latitude_degrees: f32,
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
    pub annual_mean_air_temperature_c: f32,
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
    pub polar_ice_closure: f32,
    pub solid_ice_cover: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProceduralWeather {
    pub primary: AtmospherePrimary,
    pub surface_air_temperature_c: f32,
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

struct ScalarNoiseSample {
    value: f64,
    gradient_x_per_m: f64,
    gradient_y_per_m: f64,
}

struct TemperatureSample {
    value: f32,
    gradient_per_m: Vector,
}

pub fn celestial_lighting(conditions: CelestialConditions) -> CelestialLighting {
    let latitude = f64::from(conditions.geographic_latitude_degrees).to_radians();
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

pub fn current_air_temperature_c(
    climate: ClimateConditions,
    conditions: CelestialConditions,
) -> f32 {
    let seasonal_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.09);
    let diurnal_phase = std::f64::consts::TAU * (f64::from(conditions.day_phase) - 0.625);
    let latitude_factor = seasonal_latitude_factor(conditions.geographic_latitude_degrees);
    climate.annual_mean_air_temperature_c
        + 0.5
            * climate.seasonal_temperature_range_c
            * latitude_factor
            * libm::sin(seasonal_phase) as f32
        + 0.5 * climate.diurnal_temperature_range_c * libm::cos(diurnal_phase) as f32
}

pub fn current_500_hpa_air_temperature_c(
    climate: ClimateConditions,
    conditions: CelestialConditions,
) -> f32 {
    let seasonal_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.09);
    let latitude_factor = seasonal_latitude_factor(conditions.geographic_latitude_degrees);
    // The offset approximates the standard-atmosphere lapse from sea level to
    // 500 hPa. The mid-troposphere follows half the surface seasonal amplitude.
    climate.annual_mean_air_temperature_c - AIR_TEMPERATURE_500_HPA_OFFSET_C
        + AIR_TEMPERATURE_500_HPA_SEASONAL_RANGE_FRACTION
            * climate.seasonal_temperature_range_c
            * latitude_factor
            * libm::sin(seasonal_phase) as f32
}

fn seasonal_latitude_factor(geographic_latitude_degrees: f32) -> f32 {
    let magnitude = smoothstep_range(
        0.0,
        45.0,
        f64::from(geographic_latitude_degrees.abs()),
    ) as f32;
    if geographic_latitude_degrees < 0.0 {
        -magnitude
    } else {
        magnitude
    }
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
        geographic_latitude_degrees: 45.0,
        season_phase: (0.2521 + world_time_seconds / (WORLD_DAY_SECONDS * WORLD_YEAR_DAYS))
            .rem_euclid(1.0) as f32,
        day_phase: (0.5833 + world_time_seconds / WORLD_DAY_SECONDS).rem_euclid(1.0) as f32,
        moon_phase: (0.25 + world_time_seconds / (WORLD_DAY_SECONDS * WORLD_LUNAR_CYCLE_DAYS))
            .rem_euclid(1.0) as f32,
    }
}

pub fn world_climate_conditions() -> ClimateConditions {
    ClimateConditions {
        annual_mean_air_temperature_c: 14.0,
        seasonal_temperature_range_c: 12.0,
        diurnal_temperature_range_c: 4.0,
    }
}

pub fn procedural_tidal_conditions(match_seed: u64, position_m: Vector) -> TidalConditions {
    // Tide response and lag occupy independent seed domains; sharing terrain or
    // hash-mixer constants would couple unrelated fields at aligned coordinates.
    let response =
        terrain_fractal_noise(match_seed ^ 0x6a09_e667_f3bc_c909, position_m, 131_072.0, 3) as f32;
    let lag =
        terrain_fractal_noise(match_seed ^ 0xbb67_ae85_84ca_a73b, position_m, 131_072.0, 3) as f32;
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
        - procedural_solid_surface_elevation_m(
            match_seed,
            position_m,
            14.0,
            world_celestial_conditions(world_time_seconds).geographic_latitude_degrees,
        ))
    .max(0.0)
}

pub fn climate_surface(
    climate: ClimateConditions,
    conditions: CelestialConditions,
    water_vapor_g_kg: f32,
    elevation_m: f32,
) -> SurfaceClimate {
    let air_temperature_c = current_air_temperature_c(climate, conditions);
    let latitude_factor = seasonal_latitude_factor(conditions.geographic_latitude_degrees);
    let seasonal_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.13);
    let ground_temperature_c = climate.annual_mean_air_temperature_c
        + 0.42
            * climate.seasonal_temperature_range_c
            * latitude_factor
            * libm::sin(seasonal_phase) as f32;
    let water_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.175);
    let water_temperature_c = climate.annual_mean_air_temperature_c
        + 0.28
            * climate.seasonal_temperature_range_c
            * latitude_factor
            * libm::sin(water_phase) as f32;
    let ground_temperature_at_elevation_c = ground_temperature_c - 0.0065 * elevation_m.max(0.0);
    let moisture_support = smoothstep_range(2.0, 10.0, f64::from(water_vapor_g_kg)) as f32;
    let seasonal_snow_cover = (1.0
        - smoothstep_range(-6.0, 2.0, f64::from(ground_temperature_at_elevation_c)) as f32)
        * (0.35 + 0.65 * moisture_support);
    let annual_cold =
        1.0 - smoothstep_range(-5.0, 1.0, f64::from(climate.annual_mean_air_temperature_c)) as f32;
    let polar_latitude = smoothstep_range(
        55.0,
        75.0,
        f64::from(conditions.geographic_latitude_degrees.abs()),
    ) as f32;
    let polar_ice_closure =
        polar_latitude * smoothstep_range(0.97, 1.0, f64::from(annual_cold)) as f32;
    let annual_temperature_at_elevation_c =
        climate.annual_mean_air_temperature_c - 0.0065 * elevation_m.max(0.0);
    let solid_ice_cover =
        1.0 - smoothstep_range(-5.0, 1.0, f64::from(annual_temperature_at_elevation_c)) as f32;

    SurfaceClimate {
        air_temperature_c,
        ground_temperature_c,
        water_temperature_c,
        seasonal_snow_cover,
        polar_ice_closure,
        solid_ice_cover,
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
    transport_salt: u64,
    transport_speed_scale: f64,
}

// Pressure bands remain nearly co-moving because even sub-percent drift evolves
// the 64 m gust modes quickly; scalar layers tolerate wider speed separation.
const WIND_BANDS: [WindBand; 3] = [
    WindBand {
        minimum_wavelength_m: 32_768.0,
        maximum_wavelength_m: 131_072.0,
        variance_fraction: 0.90,
        transport_salt: 0x510e_527f_ade6_82d1,
        transport_speed_scale: 0.995,
    },
    WindBand {
        minimum_wavelength_m: 4096.0,
        maximum_wavelength_m: 32_768.0,
        variance_fraction: 0.08,
        transport_salt: 0x9b05_688c_2b3e_6c1f,
        transport_speed_scale: 1.0,
    },
    WindBand {
        minimum_wavelength_m: 64.0,
        maximum_wavelength_m: 512.0,
        variance_fraction: 0.02,
        transport_salt: 0x1f83_d9ab_fb41_bd6b,
        transport_speed_scale: 1.005,
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
    derive_weather(
        procedural_atmosphere_primary(match_seed, tick, position_m),
        world_climate_conditions(),
        world_celestial_conditions(tick as f64 * f64::from(DT_SECONDS)),
    )
}

pub fn procedural_atmosphere_primary(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> AtmospherePrimary {
    let pressure = procedural_pressure_sample(match_seed, tick, position_m);
    let surface_temperature =
        sample_surface_air_temperature_anomaly_c(match_seed, tick, position_m);
    let surface_air_temperature_anomaly_c = surface_temperature.value;
    let air_temperature_500_hpa_c = current_500_hpa_air_temperature_c(
        world_climate_conditions(),
        world_celestial_conditions(tick as f64 * f64::from(DT_SECONDS)),
    ) + SURFACE_AIR_MASS_COUPLING_500_HPA
        * surface_air_temperature_anomaly_c
        + INDEPENDENT_AIR_TEMPERATURE_500_HPA_RANGE_C
            * procedural_weather_noise(
                match_seed,
                tick,
                position_m,
                AIR_TEMPERATURE_500_HPA_NOISE_SALT,
                AIR_TEMPERATURE_500_HPA_TRANSPORT_SPEED_SCALE,
                65_536.0,
                4,
            ) as f32;
    let water_vapor_g_kg = (8.5
        + 8.5
            * procedural_weather_noise(
                match_seed,
                tick,
                position_m,
                WATER_VAPOR_NOISE_SALT,
                WATER_VAPOR_TRANSPORT_SPEED_SCALE,
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
        surface_air_temperature_anomaly_c,
        air_temperature_500_hpa_c,
        temperature_gradient_c_per_m: surface_temperature.gradient_per_m,
        water_vapor_g_kg,
    }
}

pub fn derive_weather(
    primary: AtmospherePrimary,
    climate: ClimateConditions,
    conditions: CelestialConditions,
) -> ProceduralWeather {
    let surface_air_temperature_c =
        current_air_temperature_c(climate, conditions) + primary.surface_air_temperature_anomaly_c;
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
        3.5 * libm::exp(0.055 * f64::from(surface_air_temperature_c + 5.0)) as f32;
    let relative_humidity = (primary.water_vapor_g_kg / saturation_capacity_g_kg).clamp(0.0, 1.5);
    let instability = smoothstep_range(
        28.0,
        48.0,
        f64::from(surface_air_temperature_c - primary.air_temperature_500_hpa_c),
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
    let rain_signal = smoothstep_range(0.65, 0.75, f64::from(cloudiness)) as f32
        * cloudiness
        * (0.45 + 0.20 * instability + 0.45 * lift);

    ProceduralWeather {
        primary,
        surface_air_temperature_c,
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

fn sample_surface_air_temperature_anomaly_c(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> TemperatureSample {
    let noise = procedural_weather_noise_sample(
        match_seed,
        tick,
        position_m,
        SURFACE_AIR_TEMPERATURE_NOISE_SALT,
        SURFACE_AIR_MASS_TRANSPORT_SPEED_SCALE,
        65_536.0,
        4,
    );
    TemperatureSample {
        value: 13.0 * noise.value as f32,
        gradient_per_m: Vector::new(
            (13.0 * noise.gradient_x_per_m) as f32,
            (13.0 * noise.gradient_y_per_m) as f32,
        ),
    }
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
    let cluster = terrain_simplex_fractal_noise(
        match_seed ^ 0xa076_1d64_78bd_642f,
        position_m,
        8192.0,
        TERRAIN_CLUSTER_OCTAVE_COUNT,
        0.5,
    ) as f64;
    let island = terrain_simplex_fractal_noise(
        match_seed ^ 0xe703_7ed1_a0b4_28db,
        position_m,
        1536.0,
        TERRAIN_ISLAND_OCTAVE_COUNT,
        0.35,
    ) as f64;
    let local_density = 0.78 * cluster + 0.22 * island;
    let unscaled_elevation_m = 18.0 * (local_density - 0.30);
    let island_ruggedness = smoothstep_range(0.32, 0.58, cluster);
    let trench_depth = smoothstep_range(0.50, 0.74, -cluster);
    let elevation_scale = 3.0 * (1.0 + 19.0 * island_ruggedness + 29.0 * trench_depth);
    let base_elevation_m = unscaled_elevation_m * elevation_scale;
    let base_depth_m = -base_elevation_m;
    // Preserve measured coastal relief before deepening the open ocean; applying
    // the remap at the shore would turn navigable coastal slopes into abyss walls.
    let offshore_amount = smoothstep_range(4.0, 24.0, base_depth_m);
    let ocean_floor_depth_m = 3_000.0 + 3_000.0 * trench_depth;
    let mut terrain_elevation_m =
        -(base_depth_m + offshore_amount * (ocean_floor_depth_m - base_depth_m));
    let land_detail_mask = smoothstep_range(4.0, 12.0, base_elevation_m);
    if land_detail_mask > 0.0 {
        let land_ruggedness = smoothstep_range(
            -0.25,
            0.25,
            terrain_fractal_noise(
                match_seed ^ 0xbf58_476d_1ce4_e5b9,
                position_m,
                2048.0,
                3,
            ),
        );
        let land_detail = terrain_simplex_fractal_noise(
            match_seed ^ 0x2545_f491_4f6c_dd1d,
            position_m,
            256.0,
            6,
            0.65,
        ) as f64;
        let ridged_detail = 1.0 - 2.0 * land_detail.abs();
        let ridge_blend = smoothstep_range(0.35, 0.75, land_ruggedness);
        let shaped_land_detail = lerp(land_detail, ridged_detail, ridge_blend);
        terrain_elevation_m +=
            5.0 * land_detail_mask * (0.08 + 0.92 * land_ruggedness) * shaped_land_detail;
    }
    // Reef gates use the pre-remap coast so open-ocean depth does not change the
    // distribution of channels and emergent rocks.
    let seabed_mask = smoothstep_range(-4.0, -2.5, base_elevation_m)
        * (1.0 - smoothstep_range(-0.5, -0.1, base_elevation_m));
    let rock_mask = smoothstep_range(-2.0, -0.8, base_elevation_m)
        * (1.0 - smoothstep_range(1.2, 2.0, base_elevation_m));
    if seabed_mask == 0.0 && rock_mask == 0.0 {
        return terrain_elevation_m as f32;
    }
    let shoal = terrain_fractal_noise(
        match_seed ^ 0xd1b5_4a32_d192_ed03,
        position_m,
        256.0,
        TERRAIN_SHOAL_OCTAVE_COUNT,
    );
    let reef_ruggedness = smoothstep_range(
        -0.25,
        0.25,
        terrain_fractal_noise(match_seed ^ 0xbf58_476d_1ce4_e5b9, position_m, 2048.0, 3),
    );
    let coastal_rock_amount = smoothstep_range(
        -0.25,
        0.35,
        terrain_fractal_noise(match_seed ^ 0x9e37_79b9_7f4a_7c15, position_m, 1536.0, 3),
    );
    let coastal_rock_relief_amount = smoothstep_range(0.45, 0.65, coastal_rock_amount);
    let reef = terrain_fractal_noise(
        match_seed ^ 0x5899_65cc_7537_4cc3,
        position_m,
        32.0,
        TERRAIN_REEF_OCTAVE_COUNT,
    );
    let reef_peak = smoothstep_range(0.72, 0.90, reef);
    let coastal_detail_mask = seabed_mask.max(rock_mask);
    (terrain_elevation_m
        + 0.25 * seabed_mask * shoal
        + 1.5
            * coastal_detail_mask
            * coastal_rock_relief_amount
            * (0.05 + 1.55 * reef_ruggedness)
            * reef
        + 2.0 * rock_mask * reef_peak) as f32
}

pub fn procedural_solid_surface_elevation_m(
    match_seed: u64,
    position_m: Vector,
    annual_mean_air_temperature_c: f32,
    geographic_latitude_degrees: f32,
) -> f32 {
    let bedrock_elevation_m = procedural_terrain_elevation_m(match_seed, position_m);
    let annual_temperature_c =
        annual_mean_air_temperature_c - 0.0065 * bedrock_elevation_m.max(0.0);
    let cold = 1.0 - smoothstep_range(-5.0, 1.0, f64::from(annual_temperature_c)) as f32;
    let polar_latitude =
        smoothstep_range(55.0, 75.0, f64::from(geographic_latitude_degrees.abs())) as f32;
    let coastal_support = smoothstep_range(-4.0, 4.0, f64::from(bedrock_elevation_m)) as f32;
    let ocean_support = smoothstep_range(-6_000.0, 4.0, f64::from(bedrock_elevation_m)) as f32;
    let water_reach = cold * (0.25 + 0.75 * polar_latitude);
    let cold_water_support = coastal_support + water_reach * (ocean_support - coastal_support);
    let expansion_amount = cold * cold_water_support;
    let polar_closure = polar_latitude * smoothstep_range(0.97, 1.0, f64::from(cold)) as f32;
    let ice_amount = expansion_amount + polar_closure * (1.0 - expansion_amount);
    let ice_surface_m =
        (20.0 * cold + 0.25 * bedrock_elevation_m.max(0.0)).max(bedrock_elevation_m + 4.0 * cold);
    bedrock_elevation_m
        .max(bedrock_elevation_m + ice_amount * (ice_surface_m - bedrock_elevation_m))
}

pub fn procedural_cloud_cover(match_seed: u64, tick: u64, position_m: Vector) -> f32 {
    procedural_weather(match_seed, tick, position_m).cloudiness
}

fn procedural_weather_noise(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
    salt: u64,
    transport_speed_scale: f64,
    wavelength_m: f64,
    octave_count: usize,
) -> f64 {
    let (transported_x_m, transported_y_m) =
        transported_weather_position_m(match_seed, tick, position_m, salt, transport_speed_scale);
    terrain_fractal_noise_at(
        match_seed ^ salt,
        transported_x_m,
        transported_y_m,
        wavelength_m,
        octave_count,
    )
}

fn procedural_weather_noise_sample(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
    salt: u64,
    transport_speed_scale: f64,
    wavelength_m: f64,
    octave_count: usize,
) -> ScalarNoiseSample {
    let (transported_x_m, transported_y_m) =
        transported_weather_position_m(match_seed, tick, position_m, salt, transport_speed_scale);
    terrain_fractal_noise_sample(
        match_seed ^ salt,
        transported_x_m,
        transported_y_m,
        wavelength_m,
        octave_count,
    )
}

fn terrain_fractal_noise(
    field_seed: u64,
    position_m: Vector,
    wavelength_m: f64,
    octave_count: usize,
) -> f64 {
    terrain_fractal_noise_at(
        field_seed,
        f64::from(position_m.x),
        f64::from(position_m.y),
        wavelength_m,
        octave_count,
    )
}

fn terrain_simplex_fractal_noise(
    field_seed: u64,
    position_m: Vector,
    mut wavelength_m: f32,
    octave_count: usize,
    persistence: f32,
) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut amplitude_sum = 0.0;
    for octave in 0..octave_count {
        let octave_seed = field_seed ^ (octave as u64).wrapping_mul(0xd1b5_4a32_d192_ed03);
        value += amplitude * terrain_simplex_noise_sample(octave_seed, position_m, wavelength_m).0;
        amplitude_sum += amplitude;
        amplitude *= persistence;
        wavelength_m *= 0.5;
    }
    value / amplitude_sum
}

fn terrain_simplex_noise_sample(
    field_seed: u64,
    position_m: Vector,
    wavelength_m: f32,
) -> (f32, Vector) {
    const SKEW: f32 = 0.366_025_42;
    const UNSKEW: f32 = 0.211_324_87;
    const NORMALIZATION: f32 = 70.0;

    let scaled = position_m / wavelength_m;
    let skew = (scaled.x + scaled.y) * SKEW;
    let cell_x = libm::floorf(scaled.x + skew) as i32;
    let cell_y = libm::floorf(scaled.y + skew) as i32;
    let unskew = (cell_x + cell_y) as f32 * UNSKEW;
    let first_offset = Vector::new(
        scaled.x - (cell_x as f32 - unskew),
        scaled.y - (cell_y as f32 - unskew),
    );
    let (middle_x, middle_y) = if first_offset.x > first_offset.y {
        (1, 0)
    } else {
        (0, 1)
    };
    let middle_offset =
        first_offset - Vector::new(middle_x as f32, middle_y as f32) + Vector::splat(UNSKEW);
    let last_offset = first_offset - Vector::splat(1.0 - 2.0 * UNSKEW);

    let mut value = 0.0;
    let mut gradient = Vector::ZERO;
    for (lattice_x, lattice_y, offset) in [
        (cell_x, cell_y, first_offset),
        (cell_x + middle_x, cell_y + middle_y, middle_offset),
        (cell_x + 1, cell_y + 1, last_offset),
    ] {
        let attenuation = 0.5 - offset.dot(offset);
        if attenuation <= 0.0 {
            continue;
        }
        let lattice_gradient = terrain_simplex_lattice_gradient(field_seed, lattice_x, lattice_y);
        let ramp = lattice_gradient.dot(offset);
        let attenuation_squared = attenuation * attenuation;
        let attenuation_cubed = attenuation_squared * attenuation;
        let attenuation_fourth = attenuation_squared * attenuation_squared;
        value += attenuation_fourth * ramp;
        gradient += attenuation_fourth * lattice_gradient - 8.0 * attenuation_cubed * ramp * offset;
    }

    (
        NORMALIZATION * value,
        gradient * (NORMALIZATION / wavelength_m),
    )
}

fn terrain_simplex_lattice_gradient(field_seed: u64, x: i32, y: i32) -> Vector {
    let key = field_seed
        ^ (x as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (y as i64 as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
    let index = (wind_hash(key) as u32 % SIMPLEX_GRADIENT_DIRECTIONS.len() as u32) as usize;
    let (gradient_x, gradient_y) = SIMPLEX_GRADIENT_DIRECTIONS[index];
    Vector::new(gradient_x, gradient_y)
}

fn terrain_fractal_noise_at(
    field_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    mut wavelength_m: f64,
    octave_count: usize,
) -> f64 {
    let mut noise = 0.0;
    let mut amplitude = 1.0;
    let mut amplitude_sum = 0.0;
    for octave in 0..octave_count {
        noise += amplitude
            * terrain_value_noise_at(
                field_seed ^ (octave as u64).wrapping_mul(0xd1b5_4a32_d192_ed03),
                position_x_m,
                position_y_m,
                wavelength_m,
            );
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    noise / amplitude_sum
}

fn terrain_fractal_noise_sample(
    field_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    mut wavelength_m: f64,
    octave_count: usize,
) -> ScalarNoiseSample {
    let mut value = 0.0;
    let mut gradient_x_per_m = 0.0;
    let mut gradient_y_per_m = 0.0;
    let mut amplitude = 1.0;
    let mut amplitude_sum = 0.0;
    for octave in 0..octave_count {
        let sample = terrain_value_noise_sample(
            field_seed ^ (octave as u64).wrapping_mul(0xd1b5_4a32_d192_ed03),
            position_x_m,
            position_y_m,
            wavelength_m,
        );
        value += amplitude * sample.value;
        gradient_x_per_m += amplitude * sample.gradient_x_per_m;
        gradient_y_per_m += amplitude * sample.gradient_y_per_m;
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    ScalarNoiseSample {
        value: value / amplitude_sum,
        gradient_x_per_m: gradient_x_per_m / amplitude_sum,
        gradient_y_per_m: gradient_y_per_m / amplitude_sum,
    }
}

fn terrain_value_noise_at(
    match_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    wavelength_m: f64,
) -> f64 {
    let x = position_x_m / wavelength_m;
    let y = position_y_m / wavelength_m;
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

fn terrain_value_noise_sample(
    match_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    wavelength_m: f64,
) -> ScalarNoiseSample {
    let x = position_x_m / wavelength_m;
    let y = position_y_m / wavelength_m;
    let x0 = libm::floor(x) as i64;
    let y0 = libm::floor(y) as i64;
    let fraction_x = x - x0 as f64;
    let fraction_y = y - y0 as f64;
    let blend_x = smoothstep(fraction_x);
    let blend_y = smoothstep(fraction_y);
    let bottom_left = terrain_lattice_value(match_seed, x0, y0);
    let bottom_right = terrain_lattice_value(match_seed, x0 + 1, y0);
    let top_left = terrain_lattice_value(match_seed, x0, y0 + 1);
    let top_right = terrain_lattice_value(match_seed, x0 + 1, y0 + 1);
    let bottom = lerp(bottom_left, bottom_right, blend_x);
    let top = lerp(top_left, top_right, blend_x);
    let blend_derivative_x_per_m = 6.0 * fraction_x * (1.0 - fraction_x) / wavelength_m;
    let blend_derivative_y_per_m = 6.0 * fraction_y * (1.0 - fraction_y) / wavelength_m;

    ScalarNoiseSample {
        value: lerp(bottom, top, blend_y),
        gradient_x_per_m: lerp(bottom_right - bottom_left, top_right - top_left, blend_y)
            * blend_derivative_x_per_m,
        gradient_y_per_m: (top - bottom) * blend_derivative_y_per_m,
    }
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
    let mut potential_m2_s = 0.0;
    let mut gradient_x_m_s = 0.0;
    let mut gradient_y_m_s = 0.0;
    let mut synoptic_gradient_x_m_s = 0.0;
    let mut synoptic_gradient_y_m_s = 0.0;
    for (band_index, band) in WIND_BANDS.iter().enumerate() {
        let (transported_x_m, transported_y_m) = transported_weather_position_m(
            match_seed,
            tick,
            position_m,
            band.transport_salt,
            band.transport_speed_scale,
        );
        let band_key = match_seed ^ (band_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let rotation = hash_unit(band_key ^ 0xa076_1d64_78bd_642f);
        let amplitude_m_s = WIND_WEIBULL_SCALE_M_S
            * libm::sqrt(band.variance_fraction / WIND_MODES_PER_BAND as f64);
        let wavelength_log_range = libm::log(band.maximum_wavelength_m / band.minimum_wavelength_m);
        for mode in 0..WIND_MODES_PER_BAND {
            let mode_key = band_key ^ (mode as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
            let direction_rad =
                std::f64::consts::TAU * (mode as f64 / WIND_MODES_PER_BAND as f64 + rotation);
            let (direction_y, direction_x) = libm::sincos(direction_rad);
            let wavelength_fraction = hash_unit(mode_key ^ 0x8ebc_6af0_9c88_c6e3);
            let wavelength_m =
                band.minimum_wavelength_m * libm::exp(wavelength_log_range * wavelength_fraction);
            let phase_rad = std::f64::consts::TAU
                * ((transported_x_m * direction_x + transported_y_m * direction_y) / wavelength_m);
            let (cosine_coefficient, sine_coefficient) = normal_pair(mode_key);
            let (phase_sine, phase_cosine) = libm::sincos(phase_rad);
            let wave = cosine_coefficient * phase_cosine + sine_coefficient * phase_sine;
            let gradient_x = amplitude_m_s * direction_x * wave;
            let gradient_y = amplitude_m_s * direction_y * wave;
            potential_m2_s += amplitude_m_s * wavelength_m / std::f64::consts::TAU
                * (cosine_coefficient * phase_sine - sine_coefficient * phase_cosine);
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

fn transported_weather_position_m(
    match_seed: u64,
    tick: u64,
    position_m: Vector,
    transport_salt: u64,
    speed_scale: f64,
) -> (f64, f64) {
    // Keep accumulated transport in f64 through lattice sampling so long-running
    // matches retain local spatial variation instead of stepping between f32 bins.
    let time_seconds = tick as f64 * f64::from(DT_SECONDS);
    let (transport_velocity_x_m_s, transport_velocity_y_m_s) =
        weather_transport_velocity_m_s(match_seed, transport_salt, speed_scale);
    (
        f64::from(position_m.x) - transport_velocity_x_m_s * time_seconds,
        f64::from(position_m.y) - transport_velocity_y_m_s * time_seconds,
    )
}

fn weather_transport_velocity_m_s(
    match_seed: u64,
    transport_salt: u64,
    speed_scale: f64,
) -> (f64, f64) {
    let base_direction_rad = std::f64::consts::TAU * hash_unit(match_seed ^ 0x243f_6a88_85a3_08d3);
    let direction_offset_rad = WEATHER_TRANSPORT_DIRECTION_SPREAD_RAD
        * (2.0 * hash_unit(match_seed ^ transport_salt ^ 0x1319_8a2e_0370_7344) - 1.0);
    let speed_variation =
        0.9975 + 0.005 * hash_unit(match_seed ^ transport_salt ^ 0xa409_3822_299f_31d0);
    let speed_m_s = BASE_WEATHER_TRANSPORT_SPEED_M_S * speed_scale * speed_variation;
    let (direction_y, direction_x) = libm::sincos(base_direction_rad + direction_offset_rad);
    (speed_m_s * direction_x, speed_m_s * direction_y)
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
    let (sine, cosine) = libm::sincos(angle);
    (radius * cosine, radius * sine)
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
    fn stateless_weather_layers_use_correlated_distinct_transport() {
        let match_seed = 0x5eed;
        let surface = weather_transport_velocity_m_s(
            match_seed,
            SURFACE_AIR_TEMPERATURE_NOISE_SALT,
            SURFACE_AIR_MASS_TRANSPORT_SPEED_SCALE,
        );
        let upper = weather_transport_velocity_m_s(
            match_seed,
            AIR_TEMPERATURE_500_HPA_NOISE_SALT,
            AIR_TEMPERATURE_500_HPA_TRANSPORT_SPEED_SCALE,
        );
        let moisture = weather_transport_velocity_m_s(
            match_seed,
            WATER_VAPOR_NOISE_SALT,
            WATER_VAPOR_TRANSPORT_SPEED_SCALE,
        );
        let speed = |velocity: (f64, f64)| libm::hypot(velocity.0, velocity.1);
        let alignment = |first: (f64, f64), second: (f64, f64)| {
            (first.0 * second.0 + first.1 * second.1) / (speed(first) * speed(second))
        };

        assert!(alignment(surface, upper) > 0.9);
        assert!(alignment(surface, moisture) > 0.9);
        assert!(libm::hypot(surface.0 - upper.0, surface.1 - upper.1) > 0.1);
        assert!(libm::hypot(surface.0 - moisture.0, surface.1 - moisture.1) > 0.1);
        assert!(speed(upper) > speed(surface));

        let band_speeds = WIND_BANDS.map(|band| {
            speed(weather_transport_velocity_m_s(
                match_seed,
                band.transport_salt,
                band.transport_speed_scale,
            ))
        });
        assert!(band_speeds[0] < band_speeds[1]);
        assert!(band_speeds[1] < band_speeds[2]);
    }

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
