//! Procedural transcendental functions use `libm` so native and WebAssembly
//! builds do not silently diverge through platform math implementations.

use rapier2d::prelude::Vector;
use simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, DT_SECONDS, Environment, HullParameters,
    MotionIntent, WorldGeometry, computed_maximum_yaw_rate_rad_s, surface_control_input,
    surface_target_control_torques, yaw_rate_move_control,
};

const WIND_WEIBULL_SCALE_M_S: f64 = 9.0;
const WIND_MODES_PER_BAND: usize = 16;
const PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M: f32 = 6000.0;
const SURFACE_CROSS_ISOBAR_FRACTION: f32 = 0.12;
const UPPER_WIND_SCALE: f32 = 1.12;
const THERMAL_WIND_SCALE_M2_S_C: f32 = 2_000.0;
const AIR_TEMPERATURE_500_HPA_OFFSET_C: f32 = 35.5;
const AIR_TEMPERATURE_500_HPA_SEASONAL_RANGE_FRACTION: f32 = 0.25;
const SURFACE_AIR_MASS_COUPLING_500_HPA: f32 = 0.45;
const INDEPENDENT_AIR_TEMPERATURE_500_HPA_RANGE_C: f32 = 19.0;
const BASE_WEATHER_TRANSPORT_SPEED_M_S: f64 = 6.0;
const WEATHER_TRANSPORT_DIRECTION_SPREAD_RAD: f64 = std::f64::consts::PI / 720.0;
const SURFACE_AIR_TEMPERATURE_NOISE_SALT: u64 = 0x4528_21e6_38d0_1377;
const AIR_TEMPERATURE_500_HPA_NOISE_SALT: u64 = 0xbe54_66cf_34e9_0c6c;
const WATER_VAPOR_NOISE_SALT: u64 = 0x082e_fa98_ec4e_6c89;
// Four-dimensional slices need a lower field gain to preserve the calibrated
// clear/cloudy/storm occupancy of the derived weather transfer functions.
const WEATHER_NOISE_SCALE: f64 = 0.75;
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
const TERRAIN_LAND_RELIEF_OCTAVE_COUNT: usize = 6;
const TERRAIN_LAND_MICRO_RELIEF_OCTAVE_COUNT: usize = 3;
const TERRAIN_REEF_OCTAVE_COUNT: usize = 6;
const TERRAIN_OUTER_REEF_OCTAVE_COUNT: usize = 8;
const TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT: usize = 3;
const TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT: usize = 3;
const TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT: usize = 3;
const TIDE_OCTAVE_COUNT: usize = 3;
const TIDE_CIRCLE_OCTAVE_COUNT: usize = 3;
const FINE_TERRAIN_CIRCLE_OCTAVE_COUNT: usize = 0;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeriodicNoiseBasis {
    PeriodicTriangularSimplex2d,
    CircleEmbeddedSimplex4d,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PeriodicNoiseOctaveDiagnostic {
    pub basis: PeriodicNoiseBasis,
    pub requested_wavelength_m: f32,
    pub effective_wavelength_x_m: f32,
    pub effective_wavelength_y_m: f32,
    pub relative_error_x: f32,
    pub relative_error_y: f32,
}

pub fn periodic_noise_octave_diagnostic(
    world_geometry: WorldGeometry,
    basis: PeriodicNoiseBasis,
    requested_wavelength_m: f32,
) -> PeriodicNoiseOctaveDiagnostic {
    let (effective_wavelength_x_m, effective_wavelength_y_m) = match basis {
        PeriodicNoiseBasis::PeriodicTriangularSimplex2d => {
            periodic_simplex_effective_wavelengths(world_geometry, requested_wavelength_m)
        }
        PeriodicNoiseBasis::CircleEmbeddedSimplex4d => {
            (requested_wavelength_m, requested_wavelength_m)
        }
    };
    PeriodicNoiseOctaveDiagnostic {
        basis,
        requested_wavelength_m,
        effective_wavelength_x_m,
        effective_wavelength_y_m,
        relative_error_x: (effective_wavelength_x_m / requested_wavelength_m - 1.0).abs(),
        relative_error_y: (effective_wavelength_y_m / requested_wavelength_m - 1.0).abs(),
    }
}

fn periodic_simplex_effective_wavelengths(
    world_geometry: WorldGeometry,
    requested_wavelength_m: f32,
) -> (f32, f32) {
    let horizontal_modes = (world_geometry.width_m()
        / (std::f32::consts::SQRT_2 * requested_wavelength_m))
        .round()
        .max(1.0);
    let vertical_modes = (world_geometry.height_m() * libm::sqrtf(1.5) / requested_wavelength_m)
        .round()
        .max(1.0);
    (
        world_geometry.width_m() / (std::f32::consts::SQRT_2 * horizontal_modes),
        world_geometry.height_m() * libm::sqrtf(1.5) / vertical_modes,
    )
}

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

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldEnvironment {
    pub celestial: CelestialConditions,
    pub climate: ClimateConditions,
    pub weather: ProceduralWeather,
    pub tide: TidalConditions,
    pub surface_climate: SurfaceClimate,
    pub water_height_m: f32,
    pub environment: Environment,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceWeatherTransport {
    // Bounded offsets preserve late-match motion when the shader cannot convert
    // an absolute `u64` tick to floating point without losing adjacent ticks.
    pub air_temperature_offset_m: Vector,
    pub water_vapor_offset_m: Vector,
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

#[derive(Clone, Copy)]
struct WeatherNoiseParameters {
    salt: u64,
    transport_speed_scale: f64,
    wavelength_m: f64,
    octave_count: usize,
    circle_octave_count: usize,
}

const SURFACE_AIR_TEMPERATURE_NOISE: WeatherNoiseParameters = WeatherNoiseParameters {
    salt: SURFACE_AIR_TEMPERATURE_NOISE_SALT,
    transport_speed_scale: SURFACE_AIR_MASS_TRANSPORT_SPEED_SCALE,
    wavelength_m: 65_536.0,
    octave_count: 4,
    circle_octave_count: 4,
};
const AIR_TEMPERATURE_500_HPA_NOISE: WeatherNoiseParameters = WeatherNoiseParameters {
    salt: AIR_TEMPERATURE_500_HPA_NOISE_SALT,
    transport_speed_scale: AIR_TEMPERATURE_500_HPA_TRANSPORT_SPEED_SCALE,
    wavelength_m: 65_536.0,
    octave_count: 4,
    circle_octave_count: 4,
};
const WATER_VAPOR_NOISE: WeatherNoiseParameters = WeatherNoiseParameters {
    salt: WATER_VAPOR_NOISE_SALT,
    transport_speed_scale: WATER_VAPOR_TRANSPORT_SPEED_SCALE,
    wavelength_m: 3584.0,
    octave_count: 5,
    circle_octave_count: 4,
};

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
    let moon_elevation = libm::sqrt(f64::from(moon_direction[2].max(0.0))) as f32;
    let moon_intensity = 0.12
        * illuminated_fraction as f32
        * smoothstep_range(0.0, 0.08, f64::from(moon_direction[2])) as f32
        * (0.35 + 0.65 * moon_elevation);
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
    let magnitude =
        smoothstep_range(0.0, 45.0, f64::from(geographic_latitude_degrees.abs())) as f32;
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

pub fn world_celestial_conditions(
    world_geometry: WorldGeometry,
    world_time_seconds: f64,
    position_m: Vector,
) -> CelestialConditions {
    let global_day_phase = (0.5833 + world_time_seconds / WORLD_DAY_SECONDS).rem_euclid(1.0) as f32;
    CelestialConditions {
        geographic_latitude_degrees: world_geometry.latitude_degrees(position_m.y),
        season_phase: (0.2521 + world_time_seconds / (WORLD_DAY_SECONDS * WORLD_YEAR_DAYS))
            .rem_euclid(1.0) as f32,
        day_phase: world_geometry.local_solar_phase(global_day_phase, position_m.x),
        moon_phase: (0.25 + world_time_seconds / (WORLD_DAY_SECONDS * WORLD_LUNAR_CYCLE_DAYS))
            .rem_euclid(1.0) as f32,
    }
}

pub fn world_climate_conditions(geographic_latitude_degrees: f32) -> ClimateConditions {
    let polar_fraction = libm::powf(
        (geographic_latitude_degrees.abs() / 90.0).clamp(0.0, 1.0),
        1.5,
    );
    ClimateConditions {
        annual_mean_air_temperature_c: 27.0 - 45.0 * polar_fraction,
        seasonal_temperature_range_c: 3.0 + 41.0 * libm::powf(polar_fraction, 0.9),
        diurnal_temperature_range_c: 6.0 - 3.0 * polar_fraction,
    }
}

pub fn procedural_tidal_conditions(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
) -> TidalConditions {
    // Tide response and lag occupy independent seed domains; sharing terrain or
    // hash-mixer constants would couple unrelated fields at aligned coordinates.
    let response = terrain_fractal_noise(
        world_geometry,
        match_seed ^ 0x6a09_e667_f3bc_c909,
        position_m,
        131_072.0,
        TIDE_OCTAVE_COUNT,
        TIDE_CIRCLE_OCTAVE_COUNT,
    ) as f32;
    let lag = terrain_fractal_noise(
        world_geometry,
        match_seed ^ 0xbb67_ae85_84ca_a73b,
        position_m,
        131_072.0,
        TIDE_OCTAVE_COUNT,
        TIDE_CIRCLE_OCTAVE_COUNT,
    ) as f32;
    TidalConditions {
        reference_range_m: 1.25 + 0.75 * response,
        phase_offset: 0.08 * lag,
    }
}

pub fn procedural_water_height_m(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_time_seconds: f64,
    position_m: Vector,
) -> f32 {
    tide_height_m(
        procedural_tidal_conditions(world_geometry, match_seed, position_m),
        world_celestial_conditions(world_geometry, world_time_seconds, position_m),
    )
}

pub fn procedural_water_depth_m(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_time_seconds: f64,
    position_m: Vector,
) -> f32 {
    let geographic_latitude_degrees = world_geometry.latitude_degrees(position_m.y);
    let climate = world_climate_conditions(geographic_latitude_degrees);
    (procedural_water_height_m(world_geometry, match_seed, world_time_seconds, position_m)
        - procedural_solid_surface_elevation_m(
            world_geometry,
            match_seed,
            position_m,
            climate.annual_mean_air_temperature_c,
            geographic_latitude_degrees,
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
    let polar_latitude = smoothstep_range(
        55.0,
        75.0,
        f64::from(conditions.geographic_latitude_degrees.abs()),
    ) as f32;
    let extreme_cold = 1.0
        - smoothstep_range(
            -30.0,
            -20.0,
            f64::from(climate.annual_mean_air_temperature_c),
        ) as f32;
    let annual_temperature_at_elevation_c =
        climate.annual_mean_air_temperature_c - 0.0065 * elevation_m.max(0.0);
    let solid_ice_cover =
        1.0 - smoothstep_range(-5.0, 1.0, f64::from(annual_temperature_at_elevation_c)) as f32;
    let normal_pole_closure = normal_polar_ice_closure(
        elevation_m,
        climate.annual_mean_air_temperature_c,
        conditions.geographic_latitude_degrees,
        -0.75,
    );
    let extreme_closure = polar_latitude * extreme_cold;
    let polar_ice_closure = normal_pole_closure + extreme_closure * (1.0 - normal_pole_closure);

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

pub fn procedural_match_environment(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> Environment {
    procedural_world_environment(world_geometry, match_seed, tick, position_m).environment
}

pub fn procedural_world_environment(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> WorldEnvironment {
    let world_time_seconds = tick as f64 * f64::from(DT_SECONDS);
    let celestial = world_celestial_conditions(world_geometry, world_time_seconds, position_m);
    let climate = world_climate_conditions(celestial.geographic_latitude_degrees);
    let primary = procedural_atmosphere_primary_with_conditions(
        world_geometry,
        match_seed,
        tick,
        position_m,
        climate,
        celestial,
    );
    let weather = derive_weather(primary, climate, celestial);
    let tide = procedural_tidal_conditions(world_geometry, match_seed, position_m);
    let water_height_m = tide_height_m(tide, celestial);
    let (terrain_elevation_m, polar_landform) =
        procedural_terrain_elevation_and_island_noise(world_geometry, match_seed, position_m);
    let solid_surface_elevation_m = solid_surface_elevation_from_bedrock_m(
        terrain_elevation_m,
        climate.annual_mean_air_temperature_c,
        celestial.geographic_latitude_degrees,
        polar_landform,
    );
    let mut surface_climate = climate_surface(
        climate,
        celestial,
        weather.primary.water_vapor_g_kg,
        terrain_elevation_m,
    );
    surface_climate.polar_ice_closure = normal_polar_ice_closure(
        terrain_elevation_m,
        climate.annual_mean_air_temperature_c,
        celestial.geographic_latitude_degrees,
        polar_landform,
    );
    let environment = Environment {
        true_wind_velocity_m_s: land_sheltered_wind(
            world_geometry,
            match_seed,
            position_m,
            weather.wind_velocity_m_s,
        ),
        true_water_velocity_m_s: Vector::ZERO,
        water_depth_m: (water_height_m - solid_surface_elevation_m).max(0.0),
    };

    WorldEnvironment {
        celestial,
        climate,
        weather,
        tide,
        surface_climate,
        water_height_m,
        environment,
    }
}

pub fn procedural_weather(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> ProceduralWeather {
    let celestial = world_celestial_conditions(
        world_geometry,
        tick as f64 * f64::from(DT_SECONDS),
        position_m,
    );
    let climate = world_climate_conditions(celestial.geographic_latitude_degrees);
    derive_weather(
        procedural_atmosphere_primary_with_conditions(
            world_geometry,
            match_seed,
            tick,
            position_m,
            climate,
            celestial,
        ),
        climate,
        celestial,
    )
}

pub fn procedural_atmosphere_primary(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> AtmospherePrimary {
    let celestial = world_celestial_conditions(
        world_geometry,
        tick as f64 * f64::from(DT_SECONDS),
        position_m,
    );
    procedural_atmosphere_primary_with_conditions(
        world_geometry,
        match_seed,
        tick,
        position_m,
        world_climate_conditions(celestial.geographic_latitude_degrees),
        celestial,
    )
}

fn procedural_atmosphere_primary_with_conditions(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
    climate: ClimateConditions,
    celestial: CelestialConditions,
) -> AtmospherePrimary {
    let pressure = procedural_pressure_sample(world_geometry, match_seed, tick, position_m);
    let surface_temperature =
        sample_surface_air_temperature_anomaly_c(world_geometry, match_seed, tick, position_m);
    let surface_air_temperature_anomaly_c = surface_temperature.value;
    let air_temperature_500_hpa_c = current_500_hpa_air_temperature_c(climate, celestial)
        + SURFACE_AIR_MASS_COUPLING_500_HPA * surface_air_temperature_anomaly_c
        + INDEPENDENT_AIR_TEMPERATURE_500_HPA_RANGE_C
            * procedural_weather_noise(
                world_geometry,
                match_seed,
                tick,
                position_m,
                AIR_TEMPERATURE_500_HPA_NOISE,
            ) as f32;
    let climate_air_temperature_c = current_air_temperature_c(climate, celestial);
    let climate_saturation_capacity_g_kg =
        3.5 * libm::exp(0.055 * f64::from(climate_air_temperature_c + 5.0)) as f32;
    let water_vapor_fraction = 0.67
        + 0.4
            * procedural_weather_noise(
                world_geometry,
                match_seed,
                tick,
                position_m,
                WATER_VAPOR_NOISE,
            ) as f32;
    let water_vapor_g_kg =
        (climate_saturation_capacity_g_kg * water_vapor_fraction).clamp(0.5, 25.0);

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
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> TemperatureSample {
    let noise = procedural_weather_noise_sample(
        world_geometry,
        match_seed,
        tick,
        position_m,
        SURFACE_AIR_TEMPERATURE_NOISE,
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

pub fn procedural_terrain_elevation_m(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
) -> f32 {
    procedural_terrain_elevation_and_island_noise(world_geometry, match_seed, position_m).0
}

fn procedural_terrain_elevation_and_island_noise(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
) -> (f32, f32) {
    // Keep this composition synchronized with `procedural_terrain_sample` in
    // `crates/browser/src/scene.wgsl`: Rust supplies physical depth while WGSL
    // renders the same shoreline.
    let cluster = terrain_simplex_fractal_noise(
        world_geometry,
        match_seed ^ 0xa076_1d64_78bd_642f,
        position_m,
        8192.0,
        TERRAIN_CLUSTER_OCTAVE_COUNT,
        0.5,
        TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    ) as f64;
    let island = terrain_simplex_fractal_noise(
        world_geometry,
        match_seed ^ 0xe703_7ed1_a0b4_28db,
        position_m,
        1536.0,
        TERRAIN_ISLAND_OCTAVE_COUNT,
        0.35,
        TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
    ) as f64;
    let local_density = 0.78 * cluster + 0.22 * island;
    let unscaled_elevation_m = 18.0 * (local_density - 0.30);
    let island_ruggedness = smoothstep_range(0.32, 0.58, cluster);
    let trench_depth = smoothstep_range(0.50, 0.74, -cluster);
    let elevation_scale = 3.0 * (1.0 + 19.0 * island_ruggedness + 29.0 * trench_depth);
    let base_elevation_m = unscaled_elevation_m * elevation_scale;
    let (reef_ruggedness, coastal_rock_amount) =
        if base_elevation_m > -32.0 && base_elevation_m < 4.0 {
            (
                smoothstep_range(
                    -0.25,
                    0.25,
                    terrain_simplex_fractal_noise(
                        world_geometry,
                        match_seed ^ 0xbf58_476d_1ce4_e5b9,
                        position_m,
                        2048.0,
                        3,
                        0.5,
                        TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
                    ) as f64,
                ),
                smoothstep_range(
                    -0.25,
                    0.35,
                    terrain_simplex_fractal_noise(
                        world_geometry,
                        match_seed ^ 0x9e37_79b9_7f4a_7c15,
                        position_m,
                        1536.0,
                        3,
                        0.5,
                        TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
                    ) as f64,
                ),
            )
        } else {
            (0.0, 0.0)
        };
    let base_depth_m = -base_elevation_m;
    let ocean_floor_depth_m = 3_000.0 + 3_000.0 * trench_depth;
    let positive_base_depth_m = base_depth_m.max(0.0);
    let shaped_depth_m = base_depth_m + 0.015 * positive_base_depth_m.powi(4);
    let mut terrain_elevation_m = -shaped_depth_m.min(ocean_floor_depth_m);
    let land_detail_mask = smoothstep_range(4.0, 12.0, base_elevation_m);
    if land_detail_mask > 0.0 {
        let land_ruggedness = smoothstep_range(
            -0.25,
            0.25,
            terrain_simplex_fractal_noise(
                world_geometry,
                match_seed ^ 0xbf58_476d_1ce4_e5b9,
                position_m,
                2048.0,
                3,
                0.5,
                TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
            ) as f64,
        );
        let land_detail = terrain_simplex_fractal_noise(
            world_geometry,
            match_seed ^ 0x2545_f491_4f6c_dd1d,
            position_m,
            256.0,
            TERRAIN_LAND_RELIEF_OCTAVE_COUNT,
            0.65,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        ) as f64;
        let ridged_detail = 1.0 - 2.0 * land_detail.abs();
        let ridge_blend = smoothstep_range(0.35, 0.75, land_ruggedness);
        let shaped_land_detail = lerp(land_detail, ridged_detail, ridge_blend);
        terrain_elevation_m +=
            5.0 * land_detail_mask * (0.08 + 0.92 * land_ruggedness) * shaped_land_detail;
        let land_micro_relief_amount = smoothstep_range(0.40, 0.70, land_ruggedness);
        if land_micro_relief_amount > 0.0 {
            let land_micro_relief = terrain_simplex_fractal_noise(
                world_geometry,
                match_seed ^ 0x3c6e_f372_fe94_f82b,
                position_m,
                4.0,
                TERRAIN_LAND_MICRO_RELIEF_OCTAVE_COUNT,
                0.6,
                FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
            ) as f64;
            let ridged_micro_relief = 1.0 - 2.0 * land_micro_relief.abs();
            let shaped_micro_relief = lerp(land_micro_relief, ridged_micro_relief, ridge_blend);
            terrain_elevation_m +=
                0.35 * land_detail_mask * land_micro_relief_amount * shaped_micro_relief;
        }
    }
    // Reef gates follow the shaped shelf so detail occupies genuinely shallow
    // water rather than a narrow band in the pre-slope density field.
    let seabed_mask = smoothstep_range(-4.0, -2.5, terrain_elevation_m)
        * (1.0 - smoothstep_range(-0.5, -0.1, terrain_elevation_m));
    let rock_mask = smoothstep_range(-2.0, -0.8, terrain_elevation_m)
        * (1.0 - smoothstep_range(1.2, 2.0, terrain_elevation_m));
    let outer_reef_mask = smoothstep_range(-22.0, -18.0, terrain_elevation_m)
        * (1.0 - smoothstep_range(-5.0, -4.0, terrain_elevation_m));
    if seabed_mask == 0.0 && rock_mask == 0.0 && outer_reef_mask == 0.0 {
        return (terrain_elevation_m as f32, island as f32);
    }
    let shoal = if seabed_mask > 0.0 {
        terrain_simplex_fractal_noise(
            world_geometry,
            match_seed ^ 0xd1b5_4a32_d192_ed03,
            position_m,
            256.0,
            TERRAIN_SHOAL_OCTAVE_COUNT,
            0.5,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        ) as f64
    } else {
        0.0
    };
    let coastal_rock_relief_amount = smoothstep_range(0.42, 0.60, coastal_rock_amount);
    let coastal_detail_mask = seabed_mask.max(rock_mask);
    let reef = if coastal_detail_mask > 0.0 && coastal_rock_relief_amount > 0.0 {
        terrain_simplex_fractal_noise(
            world_geometry,
            match_seed ^ 0x5899_65cc_7537_4cc3,
            position_m,
            32.0,
            TERRAIN_REEF_OCTAVE_COUNT,
            0.55,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        ) as f64
    } else {
        0.0
    };
    let reef_peak = smoothstep_range(0.72, 0.90, reef);
    let outer_reef = if outer_reef_mask > 0.0 && coastal_rock_relief_amount > 0.0 {
        terrain_simplex_fractal_noise(
            world_geometry,
            match_seed ^ 0x8ebc_6af0_9c88_c6e3,
            position_m,
            128.0,
            TERRAIN_OUTER_REEF_OCTAVE_COUNT,
            0.6,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        ) as f64
    } else {
        0.0
    };
    let ridged_reef = 1.0 - 2.0 * outer_reef.abs();
    let reef_ridge_blend = smoothstep_range(0.25, 0.80, reef_ruggedness);
    let shaped_outer_reef =
        lerp(outer_reef, ridged_reef, reef_ridge_blend) - 0.3 * reef_ridge_blend;
    (
        (terrain_elevation_m
            + 0.25 * seabed_mask * shoal
            + 1.6
                * coastal_detail_mask
                * coastal_rock_relief_amount
                * (0.05 + 1.55 * reef_ruggedness)
                * reef
            + 2.0 * rock_mask * coastal_rock_relief_amount * reef_peak
            + 1.8
                * outer_reef_mask
                * coastal_rock_relief_amount
                * (0.12 + 1.68 * reef_ruggedness)
                * shaped_outer_reef) as f32,
        island as f32,
    )
}

fn broad_landform_elevation_m(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
) -> f32 {
    // Keep this synchronized with `broad_landform_elevation_m` in `scene.wgsl`.
    // Wind shelter and long shadows need island-scale blockers, not the complete
    // metre-scale terrain spectrum used for depth and grounding.
    let cluster = terrain_simplex_fractal_noise(
        world_geometry,
        match_seed ^ 0xa076_1d64_78bd_642f,
        position_m,
        8192.0,
        TERRAIN_CLUSTER_OCTAVE_COUNT,
        0.5,
        TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    ) as f64;
    let island = terrain_simplex_fractal_noise(
        world_geometry,
        match_seed ^ 0xe703_7ed1_a0b4_28db,
        position_m,
        1536.0,
        TERRAIN_ISLAND_OCTAVE_COUNT,
        0.35,
        TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
    ) as f64;
    let unscaled_elevation_m = 18.0 * (0.78 * cluster + 0.22 * island - 0.30);
    let island_ruggedness = smoothstep_range(0.32, 0.58, cluster);
    let trench_depth = smoothstep_range(0.50, 0.74, -cluster);
    (unscaled_elevation_m * 3.0 * (1.0 + 19.0 * island_ruggedness + 29.0 * trench_depth)) as f32
}

pub fn procedural_solid_surface_elevation_m(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
    annual_mean_air_temperature_c: f32,
    geographic_latitude_degrees: f32,
) -> f32 {
    let (bedrock_elevation_m, polar_landform) =
        procedural_terrain_elevation_and_island_noise(world_geometry, match_seed, position_m);
    solid_surface_elevation_from_bedrock_m(
        bedrock_elevation_m,
        annual_mean_air_temperature_c,
        geographic_latitude_degrees,
        polar_landform,
    )
}

fn normal_polar_ice_closure(
    bedrock_elevation_m: f32,
    annual_mean_air_temperature_c: f32,
    geographic_latitude_degrees: f32,
    polar_landform: f32,
) -> f32 {
    let cold = 1.0
        - smoothstep_range(
            -5.0,
            1.0,
            f64::from(annual_mean_air_temperature_c - 0.0065 * bedrock_elevation_m.max(0.0)),
        ) as f32;
    let depth_support = smoothstep_range(-6_000.0, 4.0, f64::from(bedrock_elevation_m)) as f32;
    let front_latitude = 85.0 - 3.0 * polar_landform - 2.0 * depth_support;
    smoothstep_range(
        f64::from(front_latitude),
        f64::from(front_latitude + 2.0),
        f64::from(geographic_latitude_degrees.abs()),
    ) as f32
        * cold
}

fn solid_surface_elevation_from_bedrock_m(
    bedrock_elevation_m: f32,
    annual_mean_air_temperature_c: f32,
    geographic_latitude_degrees: f32,
    polar_landform: f32,
) -> f32 {
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
    let normal_pole_closure = normal_polar_ice_closure(
        bedrock_elevation_m,
        annual_mean_air_temperature_c,
        geographic_latitude_degrees,
        polar_landform,
    );
    let extreme_closure = polar_latitude
        * (1.0 - smoothstep_range(-30.0, -20.0, f64::from(annual_temperature_c)) as f32);
    let polar_closure = normal_pole_closure + extreme_closure * (1.0 - normal_pole_closure);
    let ice_amount = expansion_amount + polar_closure * (1.0 - expansion_amount);
    let ice_surface_m =
        (20.0 * cold + 0.25 * bedrock_elevation_m.max(0.0)).max(bedrock_elevation_m + 4.0 * cold);
    bedrock_elevation_m
        .max(bedrock_elevation_m + ice_amount * (ice_surface_m - bedrock_elevation_m))
}

pub fn procedural_cloud_cover(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> f32 {
    procedural_weather(world_geometry, match_seed, tick, position_m).cloudiness
}

pub fn surface_weather_transport(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
) -> SurfaceWeatherTransport {
    SurfaceWeatherTransport {
        air_temperature_offset_m: weather_transport_offset_m(
            world_geometry,
            match_seed,
            tick,
            SURFACE_AIR_TEMPERATURE_NOISE.salt,
            SURFACE_AIR_TEMPERATURE_NOISE.transport_speed_scale,
        ),
        water_vapor_offset_m: weather_transport_offset_m(
            world_geometry,
            match_seed,
            tick,
            WATER_VAPOR_NOISE.salt,
            WATER_VAPOR_NOISE.transport_speed_scale,
        ),
    }
}

fn procedural_weather_noise(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
    parameters: WeatherNoiseParameters,
) -> f64 {
    let (transported_x_m, transported_y_m) = transported_weather_position_m(
        world_geometry,
        match_seed,
        tick,
        position_m,
        parameters.salt,
        parameters.transport_speed_scale,
    );
    WEATHER_NOISE_SCALE
        * terrain_fractal_noise_at(
            world_geometry,
            match_seed ^ parameters.salt,
            transported_x_m,
            transported_y_m,
            parameters.wavelength_m,
            parameters.octave_count,
            parameters.circle_octave_count,
        )
}

fn procedural_weather_noise_sample(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
    parameters: WeatherNoiseParameters,
) -> ScalarNoiseSample {
    let (transported_x_m, transported_y_m) = transported_weather_position_m(
        world_geometry,
        match_seed,
        tick,
        position_m,
        parameters.salt,
        parameters.transport_speed_scale,
    );
    let mut sample = terrain_fractal_noise_sample(
        world_geometry,
        match_seed ^ parameters.salt,
        transported_x_m,
        transported_y_m,
        parameters.wavelength_m,
        parameters.octave_count,
        parameters.circle_octave_count,
    );
    sample.value *= WEATHER_NOISE_SCALE;
    sample.gradient_x_per_m *= WEATHER_NOISE_SCALE;
    sample.gradient_y_per_m *= WEATHER_NOISE_SCALE;
    sample
}

fn terrain_fractal_noise(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_m: Vector,
    wavelength_m: f64,
    octave_count: usize,
    circle_octave_count: usize,
) -> f64 {
    terrain_fractal_noise_at(
        world_geometry,
        field_seed,
        f64::from(position_m.x),
        f64::from(position_m.y),
        wavelength_m,
        octave_count,
        circle_octave_count,
    )
}

fn terrain_simplex_fractal_noise(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_m: Vector,
    mut wavelength_m: f32,
    octave_count: usize,
    persistence: f32,
    circle_octave_count: usize,
) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut amplitude_sum = 0.0;
    for octave in 0..octave_count {
        let octave_seed = field_seed ^ (octave as u64).wrapping_mul(0xd1b5_4a32_d192_ed03);
        let basis = if octave < circle_octave_count {
            PeriodicNoiseBasis::CircleEmbeddedSimplex4d
        } else {
            PeriodicNoiseBasis::PeriodicTriangularSimplex2d
        };
        value += amplitude
            * terrain_simplex_noise(world_geometry, octave_seed, position_m, wavelength_m, basis);
        amplitude_sum += amplitude;
        amplitude *= persistence;
        wavelength_m *= 0.5;
    }
    value / amplitude_sum
}

fn terrain_simplex_noise(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_m: Vector,
    wavelength_m: f32,
    basis: PeriodicNoiseBasis,
) -> f32 {
    match basis {
        PeriodicNoiseBasis::PeriodicTriangularSimplex2d => {
            terrain_simplex_noise_2d(world_geometry, field_seed, position_m, wavelength_m)
        }
        PeriodicNoiseBasis::CircleEmbeddedSimplex4d => {
            terrain_simplex_noise_4d(world_geometry, field_seed, position_m, wavelength_m).0
        }
    }
}

fn terrain_simplex_noise_2d(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_m: Vector,
    wavelength_m: f32,
) -> f32 {
    const SKEW: f32 = 0.366_025_42;
    const UNSKEW: f32 = 0.211_324_87;
    const NORMALIZATION: f32 = 70.0;

    let wrapped_m = world_geometry.wrap_position_m([position_m.x, position_m.y]);
    let horizontal_modes = ((f64::from(world_geometry.width_m())
        / (f64::from(wavelength_m) * std::f64::consts::SQRT_2))
        .round()
        .clamp(1.0, f64::from(i32::MAX / 4))) as i32;
    let vertical_modes = ((f64::from(world_geometry.height_m()) * libm::sqrt(1.5)
        / f64::from(wavelength_m))
    .round()
    .clamp(1.0, f64::from(i32::MAX / 4))) as i32;
    let horizontal_phase = wrapped_m[0].rem_euclid(world_geometry.width_m())
        * horizontal_modes as f32
        / world_geometry.width_m();
    let vertical_phase = wrapped_m[1].rem_euclid(world_geometry.height_m()) * vertical_modes as f32
        / (world_geometry.height_m() * 1.732_050_8);
    let scaled = Vector::new(
        horizontal_phase + vertical_phase,
        -horizontal_phase + vertical_phase,
    );
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
    for (lattice_x, lattice_y, offset) in [
        (cell_x, cell_y, first_offset),
        (cell_x + middle_x, cell_y + middle_y, middle_offset),
        (cell_x + 1, cell_y + 1, last_offset),
    ] {
        let attenuation = 0.5 - offset.dot(offset);
        if attenuation <= 0.0 {
            continue;
        }
        let lattice_gradient = terrain_simplex_lattice_gradient(
            field_seed,
            lattice_x,
            lattice_y,
            horizontal_modes,
            vertical_modes,
        );
        let ramp = lattice_gradient.dot(offset);
        let attenuation_squared = attenuation * attenuation;
        let attenuation_fourth = attenuation_squared * attenuation_squared;
        value += attenuation_fourth * ramp;
    }

    NORMALIZATION * value
}

fn terrain_simplex_noise_4d(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_m: Vector,
    wavelength_m: f32,
) -> (f32, Vector) {
    const SKEW: f32 = 0.309_017;
    const UNSKEW: f32 = 0.138_196_6;
    // Matches the portable 2D kernel's RMS value over representative seeds.
    const NORMALIZATION: f32 = 33.4;

    let wrapped_m = world_geometry.wrap_position_m([position_m.x, position_m.y]);
    let angle_x = std::f32::consts::TAU * wrapped_m[0] / world_geometry.width_m();
    let angle_y = std::f32::consts::TAU * wrapped_m[1] / world_geometry.height_m();
    let (sin_x, cos_x) = (libm::sinf(angle_x), libm::cosf(angle_x));
    let (sin_y, cos_y) = (libm::sinf(angle_y), libm::cosf(angle_y));
    let circle_x = world_geometry.width_m() / (std::f32::consts::TAU * wavelength_m);
    let circle_y = world_geometry.height_m() / (std::f32::consts::TAU * wavelength_m);
    let point = [
        circle_x * cos_x,
        circle_x * sin_x,
        circle_y * cos_y,
        circle_y * sin_y,
    ];
    let skew = point.iter().sum::<f32>() * SKEW;
    let lattice = point.map(|coordinate| libm::floorf(coordinate + skew) as i32);
    let unskew = lattice.iter().sum::<i32>() as f32 * UNSKEW;
    let first =
        std::array::from_fn::<_, 4, _>(|axis| point[axis] - (lattice[axis] as f32 - unskew));
    let mut rank = [0_u8; 4];
    for first_axis in 0..4 {
        for second_axis in first_axis + 1..4 {
            if first[first_axis] > first[second_axis] {
                rank[first_axis] += 1;
            } else {
                rank[second_axis] += 1;
            }
        }
    }

    let mut value = 0.0;
    let mut input_gradient = [0.0; 4];
    for vertex in 0..5 {
        let threshold = 4_u8.saturating_sub(vertex as u8);
        let offset_lattice = rank.map(|axis_rank| i32::from(vertex > 0 && axis_rank >= threshold));
        let offset = std::array::from_fn::<_, 4, _>(|axis| {
            first[axis] - offset_lattice[axis] as f32 + vertex as f32 * UNSKEW
        });
        let attenuation = 0.6
            - offset
                .iter()
                .map(|component| component * component)
                .sum::<f32>();
        if attenuation <= 0.0 {
            continue;
        }
        let gradient = terrain_simplex_lattice_gradient_4d(
            field_seed,
            std::array::from_fn(|axis| lattice[axis] + offset_lattice[axis]),
        );
        let ramp = (0..4)
            .map(|axis| gradient[axis] * offset[axis])
            .sum::<f32>();
        let attenuation_squared = attenuation * attenuation;
        let attenuation_cubed = attenuation_squared * attenuation;
        let attenuation_fourth = attenuation_squared * attenuation_squared;
        value += attenuation_fourth * ramp;
        for axis in 0..4 {
            input_gradient[axis] +=
                attenuation_fourth * gradient[axis] - 8.0 * attenuation_cubed * ramp * offset[axis];
        }
    }

    let gradient_per_m = Vector::new(
        NORMALIZATION * (-sin_x * input_gradient[0] + cos_x * input_gradient[1]) / wavelength_m,
        NORMALIZATION * (-sin_y * input_gradient[2] + cos_y * input_gradient[3]) / wavelength_m,
    );
    (NORMALIZATION * value, gradient_per_m)
}

fn terrain_simplex_lattice_gradient_4d(field_seed: u64, lattice: [i32; 4]) -> [f32; 4] {
    let key = field_seed
        ^ (lattice[0] as i64 as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (lattice[1] as i64 as u64).wrapping_mul(0xe703_7ed1_a0b4_28db)
        ^ (lattice[2] as i64 as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (lattice[3] as i64 as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    let hash = wind_hash(key) as u32;
    let zero_axis = (hash & 3) as usize;
    let mut gradient = [0.0; 4];
    let mut sign_bit = 2;
    for (axis, component) in gradient.iter_mut().enumerate() {
        if axis != zero_axis {
            *component = if hash & (1 << sign_bit) == 0 {
                1.0
            } else {
                -1.0
            };
            sign_bit += 1;
        }
    }
    gradient
}

fn terrain_simplex_lattice_gradient(
    field_seed: u64,
    x: i32,
    y: i32,
    horizontal_modes: i32,
    vertical_modes: i32,
) -> Vector {
    // x-y and x+y are the rectangular axes of the triangular lattice. Integer
    // mode counts therefore turn each world period into an exact lattice shift.
    let horizontal_lattice =
        (i64::from(x) - i64::from(y)).rem_euclid(2 * i64::from(horizontal_modes));
    let vertical_lattice = (i64::from(x) + i64::from(y)).rem_euclid(2 * i64::from(vertical_modes));
    let key = field_seed
        ^ (horizontal_lattice as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (vertical_lattice as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
    let index = (wind_hash(key) as u32 % SIMPLEX_GRADIENT_DIRECTIONS.len() as u32) as usize;
    let (gradient_x, gradient_y) = SIMPLEX_GRADIENT_DIRECTIONS[index];
    Vector::new(gradient_x, gradient_y)
}

fn terrain_fractal_noise_at(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    mut wavelength_m: f64,
    octave_count: usize,
    circle_octave_count: usize,
) -> f64 {
    let mut noise = 0.0;
    let mut amplitude = 1.0;
    let mut amplitude_sum = 0.0;
    for octave in 0..octave_count {
        noise += amplitude
            * hybrid_simplex_noise_sample_f64(
                world_geometry,
                field_seed ^ (octave as u64).wrapping_mul(0xd1b5_4a32_d192_ed03),
                position_x_m,
                position_y_m,
                wavelength_m,
                if octave < circle_octave_count {
                    PeriodicNoiseBasis::CircleEmbeddedSimplex4d
                } else {
                    PeriodicNoiseBasis::PeriodicTriangularSimplex2d
                },
            )
            .value;
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    noise / amplitude_sum
}

fn terrain_fractal_noise_sample(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    mut wavelength_m: f64,
    octave_count: usize,
    circle_octave_count: usize,
) -> ScalarNoiseSample {
    let mut value = 0.0;
    let mut gradient_x_per_m = 0.0;
    let mut gradient_y_per_m = 0.0;
    let mut amplitude = 1.0;
    let mut amplitude_sum = 0.0;
    for octave in 0..octave_count {
        let sample = hybrid_simplex_noise_sample_f64(
            world_geometry,
            field_seed ^ (octave as u64).wrapping_mul(0xd1b5_4a32_d192_ed03),
            position_x_m,
            position_y_m,
            wavelength_m,
            if octave < circle_octave_count {
                PeriodicNoiseBasis::CircleEmbeddedSimplex4d
            } else {
                PeriodicNoiseBasis::PeriodicTriangularSimplex2d
            },
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

fn hybrid_simplex_noise_sample_f64(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    wavelength_m: f64,
    basis: PeriodicNoiseBasis,
) -> ScalarNoiseSample {
    match basis {
        PeriodicNoiseBasis::PeriodicTriangularSimplex2d => periodic_simplex_noise_sample_f64(
            world_geometry,
            field_seed,
            position_x_m,
            position_y_m,
            wavelength_m,
        ),
        PeriodicNoiseBasis::CircleEmbeddedSimplex4d => circle_embedded_simplex_noise_sample_f64(
            world_geometry,
            field_seed,
            position_x_m,
            position_y_m,
            wavelength_m,
        ),
    }
}

fn circle_embedded_simplex_noise_sample_f64(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    wavelength_m: f64,
) -> ScalarNoiseSample {
    const SKEW: f64 = 0.309_016_994_374_947_45;
    const UNSKEW: f64 = 0.138_196_601_125_010_5;
    const NORMALIZATION: f64 = 33.4;

    let width_m = f64::from(world_geometry.width_m());
    let height_m = f64::from(world_geometry.height_m());
    let angle_x = std::f64::consts::TAU * position_x_m.rem_euclid(width_m) / width_m;
    let angle_y = std::f64::consts::TAU * position_y_m.rem_euclid(height_m) / height_m;
    let (sin_x, cos_x) = (libm::sin(angle_x), libm::cos(angle_x));
    let (sin_y, cos_y) = (libm::sin(angle_y), libm::cos(angle_y));
    let circle_x = width_m / (std::f64::consts::TAU * wavelength_m);
    let circle_y = height_m / (std::f64::consts::TAU * wavelength_m);
    let point = [
        circle_x * cos_x,
        circle_x * sin_x,
        circle_y * cos_y,
        circle_y * sin_y,
    ];
    let skew = point.iter().sum::<f64>() * SKEW;
    let lattice = point.map(|coordinate| libm::floor(coordinate + skew) as i64);
    let unskew = lattice.iter().sum::<i64>() as f64 * UNSKEW;
    let first =
        std::array::from_fn::<_, 4, _>(|axis| point[axis] - (lattice[axis] as f64 - unskew));
    let mut rank = [0_u8; 4];
    for first_axis in 0..4 {
        for second_axis in first_axis + 1..4 {
            if first[first_axis] > first[second_axis] {
                rank[first_axis] += 1;
            } else {
                rank[second_axis] += 1;
            }
        }
    }

    let mut value = 0.0;
    let mut input_gradient = [0.0; 4];
    for vertex in 0..5 {
        let threshold = 4_u8.saturating_sub(vertex as u8);
        let offset_lattice = rank.map(|axis_rank| i64::from(vertex > 0 && axis_rank >= threshold));
        let offset = std::array::from_fn::<_, 4, _>(|axis| {
            first[axis] - offset_lattice[axis] as f64 + vertex as f64 * UNSKEW
        });
        let attenuation = 0.6
            - offset
                .iter()
                .map(|component| component * component)
                .sum::<f64>();
        if attenuation <= 0.0 {
            continue;
        }
        let gradient = terrain_simplex_lattice_gradient_4d_f64(
            field_seed,
            std::array::from_fn(|axis| lattice[axis] + offset_lattice[axis]),
        );
        let ramp = (0..4)
            .map(|axis| gradient[axis] * offset[axis])
            .sum::<f64>();
        let attenuation_squared = attenuation * attenuation;
        let attenuation_cubed = attenuation_squared * attenuation;
        let attenuation_fourth = attenuation_squared * attenuation_squared;
        value += attenuation_fourth * ramp;
        for axis in 0..4 {
            input_gradient[axis] +=
                attenuation_fourth * gradient[axis] - 8.0 * attenuation_cubed * ramp * offset[axis];
        }
    }

    ScalarNoiseSample {
        value: NORMALIZATION * value,
        gradient_x_per_m: NORMALIZATION * (-sin_x * input_gradient[0] + cos_x * input_gradient[1])
            / wavelength_m,
        gradient_y_per_m: NORMALIZATION * (-sin_y * input_gradient[2] + cos_y * input_gradient[3])
            / wavelength_m,
    }
}

fn terrain_simplex_lattice_gradient_4d_f64(field_seed: u64, lattice: [i64; 4]) -> [f64; 4] {
    let key = field_seed
        ^ (lattice[0] as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (lattice[1] as u64).wrapping_mul(0xe703_7ed1_a0b4_28db)
        ^ (lattice[2] as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9)
        ^ (lattice[3] as u64).wrapping_mul(0x94d0_49bb_1331_11eb);
    let hash = wind_hash(key) as u32;
    let zero_axis = (hash & 3) as usize;
    let mut gradient = [0.0; 4];
    let mut sign_bit = 2;
    for (axis, component) in gradient.iter_mut().enumerate() {
        if axis != zero_axis {
            *component = if hash & (1 << sign_bit) == 0 {
                1.0
            } else {
                -1.0
            };
            sign_bit += 1;
        }
    }
    gradient
}

fn periodic_simplex_noise_sample_f64(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    wavelength_m: f64,
) -> ScalarNoiseSample {
    const SKEW: f64 = 0.366_025_403_784_438_65;
    const UNSKEW: f64 = 0.211_324_865_405_187_13;
    const SQRT_3: f64 = 1.732_050_807_568_877_2;
    const NORMALIZATION: f64 = 70.0;

    let width_m = f64::from(world_geometry.width_m());
    let height_m = f64::from(world_geometry.height_m());
    let horizontal_modes = (width_m / (std::f64::consts::SQRT_2 * wavelength_m))
        .round()
        .clamp(1.0, i64::MAX as f64 / 4.0) as i64;
    let vertical_modes = (height_m * libm::sqrt(1.5) / wavelength_m)
        .round()
        .clamp(1.0, i64::MAX as f64 / 4.0) as i64;
    let horizontal_phase = position_x_m.rem_euclid(width_m) * horizontal_modes as f64 / width_m;
    let vertical_phase =
        position_y_m.rem_euclid(height_m) * vertical_modes as f64 / (SQRT_3 * height_m);
    let scaled_x = horizontal_phase + vertical_phase;
    let scaled_y = -horizontal_phase + vertical_phase;
    let skew = (scaled_x + scaled_y) * SKEW;
    let cell_x = libm::floor(scaled_x + skew) as i64;
    let cell_y = libm::floor(scaled_y + skew) as i64;
    let unskew = (cell_x + cell_y) as f64 * UNSKEW;
    let first_offset_x = scaled_x - (cell_x as f64 - unskew);
    let first_offset_y = scaled_y - (cell_y as f64 - unskew);
    let (middle_x, middle_y) = if first_offset_x > first_offset_y {
        (1_i64, 0_i64)
    } else {
        (0_i64, 1_i64)
    };
    let middle_offset_x = first_offset_x - middle_x as f64 + UNSKEW;
    let middle_offset_y = first_offset_y - middle_y as f64 + UNSKEW;
    let last_offset_x = first_offset_x - (1.0 - 2.0 * UNSKEW);
    let last_offset_y = first_offset_y - (1.0 - 2.0 * UNSKEW);

    let mut value = 0.0;
    let mut gradient_scaled_x = 0.0;
    let mut gradient_scaled_y = 0.0;
    for (lattice_x, lattice_y, offset_x, offset_y) in [
        (cell_x, cell_y, first_offset_x, first_offset_y),
        (
            cell_x + middle_x,
            cell_y + middle_y,
            middle_offset_x,
            middle_offset_y,
        ),
        (cell_x + 1, cell_y + 1, last_offset_x, last_offset_y),
    ] {
        let attenuation = 0.5 - offset_x * offset_x - offset_y * offset_y;
        if attenuation <= 0.0 {
            continue;
        }
        let gradient = periodic_simplex_lattice_gradient_f64(
            field_seed,
            lattice_x,
            lattice_y,
            horizontal_modes,
            vertical_modes,
        );
        let ramp = gradient.0 * offset_x + gradient.1 * offset_y;
        let attenuation_squared = attenuation * attenuation;
        let attenuation_cubed = attenuation_squared * attenuation;
        let attenuation_fourth = attenuation_squared * attenuation_squared;
        value += attenuation_fourth * ramp;
        gradient_scaled_x +=
            attenuation_fourth * gradient.0 - 8.0 * attenuation_cubed * ramp * offset_x;
        gradient_scaled_y +=
            attenuation_fourth * gradient.1 - 8.0 * attenuation_cubed * ramp * offset_y;
    }

    let horizontal_scale_per_m = horizontal_modes as f64 / width_m;
    let vertical_scale_per_m = vertical_modes as f64 / (SQRT_3 * height_m);
    ScalarNoiseSample {
        value: NORMALIZATION * value,
        gradient_x_per_m: NORMALIZATION
            * horizontal_scale_per_m
            * (gradient_scaled_x - gradient_scaled_y),
        gradient_y_per_m: NORMALIZATION
            * vertical_scale_per_m
            * (gradient_scaled_x + gradient_scaled_y),
    }
}

fn periodic_simplex_lattice_gradient_f64(
    field_seed: u64,
    x: i64,
    y: i64,
    horizontal_modes: i64,
    vertical_modes: i64,
) -> (f64, f64) {
    let horizontal_lattice = (x - y).rem_euclid(2 * horizontal_modes);
    let vertical_lattice = (x + y).rem_euclid(2 * vertical_modes);
    let key = field_seed
        ^ (horizontal_lattice as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (vertical_lattice as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
    let index = (wind_hash(key) as u32 % SIMPLEX_GRADIENT_DIRECTIONS.len() as u32) as usize;
    let (gradient_x, gradient_y) = SIMPLEX_GRADIENT_DIRECTIONS[index];
    (f64::from(gradient_x), f64::from(gradient_y))
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
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> Vector {
    let pressure = procedural_pressure_sample(world_geometry, match_seed, tick, position_m);
    wind_from_pressure_gradient(pressure.gradient_m_s / PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M)
}

fn procedural_pressure_sample(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> PressureSample {
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
            world_geometry,
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
        let width_m = f64::from(world_geometry.width_m());
        let height_m = f64::from(world_geometry.height_m());
        let wrapped_x_m = transported_x_m.rem_euclid(width_m);
        let wrapped_y_m = transported_y_m.rem_euclid(height_m);
        for mode in 0..WIND_MODES_PER_BAND {
            let mode_key = band_key ^ (mode as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
            let direction_rad =
                std::f64::consts::TAU * (mode as f64 / WIND_MODES_PER_BAND as f64 + rotation);
            let (direction_y, direction_x) = libm::sincos(direction_rad);
            let wavelength_fraction = hash_unit(mode_key ^ 0x8ebc_6af0_9c88_c6e3);
            let wavelength_m =
                band.minimum_wavelength_m * libm::exp(wavelength_log_range * wavelength_fraction);
            let mut mode_x = libm::round(direction_x * width_m / wavelength_m) as i64;
            let mut mode_y = libm::round(direction_y * height_m / wavelength_m) as i64;
            if mode_x == 0 && mode_y == 0 {
                if direction_x.abs() >= direction_y.abs() {
                    mode_x = if direction_x < 0.0 { -1 } else { 1 };
                } else {
                    mode_y = if direction_y < 0.0 { -1 } else { 1 };
                }
            }
            let frequency_x_per_m = mode_x as f64 / width_m;
            let frequency_y_per_m = mode_y as f64 / height_m;
            let spatial_frequency_per_m = libm::hypot(frequency_x_per_m, frequency_y_per_m);
            let direction_x = frequency_x_per_m / spatial_frequency_per_m;
            let direction_y = frequency_y_per_m / spatial_frequency_per_m;
            let phase_rad = std::f64::consts::TAU
                * (wrapped_x_m * frequency_x_per_m + wrapped_y_m * frequency_y_per_m);
            let (cosine_coefficient, sine_coefficient) = normal_pair(mode_key);
            let (phase_sine, phase_cosine) = libm::sincos(phase_rad);
            let wave = cosine_coefficient * phase_cosine + sine_coefficient * phase_sine;
            let gradient_x = amplitude_m_s * direction_x * wave;
            let gradient_y = amplitude_m_s * direction_y * wave;
            potential_m2_s += amplitude_m_s / (std::f64::consts::TAU * spatial_frequency_per_m)
                * (cosine_coefficient * phase_sine - sine_coefficient * phase_cosine);
            gradient_x_m_s += gradient_x;
            gradient_y_m_s += gradient_y;
            let upper_band_coupling = if band_index == 0 { 1.0 } else { 0.20 };
            synoptic_gradient_x_m_s += upper_band_coupling * gradient_x;
            synoptic_gradient_y_m_s += upper_band_coupling * gradient_y;
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
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
    transport_salt: u64,
    speed_scale: f64,
) -> (f64, f64) {
    let (transport_velocity_x_m_s, transport_velocity_y_m_s) =
        weather_transport_velocity_m_s(match_seed, transport_salt, speed_scale);
    let width_m = f64::from(world_geometry.width_m());
    let height_m = f64::from(world_geometry.height_m());
    (
        (f64::from(position_m.x).rem_euclid(width_m)
            - periodic_transport_displacement_m(tick, transport_velocity_x_m_s, width_m))
        .rem_euclid(width_m),
        (f64::from(position_m.y).rem_euclid(height_m)
            - periodic_transport_displacement_m(tick, transport_velocity_y_m_s, height_m))
        .rem_euclid(height_m),
    )
}

fn weather_transport_offset_m(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    transport_salt: u64,
    speed_scale: f64,
) -> Vector {
    let (velocity_x_m_s, velocity_y_m_s) =
        weather_transport_velocity_m_s(match_seed, transport_salt, speed_scale);
    Vector::new(
        periodic_transport_displacement_m(tick, velocity_x_m_s, f64::from(world_geometry.width_m()))
            as f32,
        periodic_transport_displacement_m(
            tick,
            velocity_y_m_s,
            f64::from(world_geometry.height_m()),
        ) as f32,
    )
}

fn periodic_transport_displacement_m(tick: u64, velocity_m_s: f64, period_m: f64) -> f64 {
    let displacement_per_tick_m = velocity_m_s * f64::from(DT_SECONDS);
    let mut displacement_m = 0.0;
    // Base-2^16 Horner reduction retains every tick bit without ever converting
    // the full u64 to f64, so adjacent late-match ticks keep distinct phases.
    for shift in [48, 32, 16, 0] {
        let chunk = ((tick >> shift) & 0xffff) as f64;
        displacement_m =
            (displacement_m * 65_536.0 + chunk * displacement_per_tick_m).rem_euclid(period_m);
    }
    displacement_m
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

fn land_sheltered_wind(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
    open_wind: Vector,
) -> Vector {
    let speed_m_s = open_wind.length();
    if speed_m_s == 0.0 {
        return open_wind;
    }
    let upwind = -open_wind / speed_m_s;
    let mut shelter: f32 = 0.0;
    for distance_m in LAND_WIND_SAMPLE_DISTANCES_M {
        let elevation_m = broad_landform_elevation_m(
            world_geometry,
            match_seed,
            position_m + distance_m * upwind,
        );
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

    fn procedural_match_environment(match_seed: u64, tick: u64, position_m: Vector) -> Environment {
        super::procedural_match_environment(WorldGeometry::default(), match_seed, tick, position_m)
    }

    fn procedural_terrain_elevation_m(match_seed: u64, position_m: Vector) -> f32 {
        super::procedural_terrain_elevation_m(WorldGeometry::default(), match_seed, position_m)
    }

    fn procedural_unsheltered_wind_velocity_m_s(
        match_seed: u64,
        tick: u64,
        position_m: Vector,
    ) -> Vector {
        super::procedural_unsheltered_wind_velocity_m_s(
            WorldGeometry::default(),
            match_seed,
            tick,
            position_m,
        )
    }

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
    fn semantic_noise_partition_is_fixed() {
        assert_eq!(
            TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
            TERRAIN_CLUSTER_OCTAVE_COUNT
        );
        assert_eq!(TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT, 3);
        assert_eq!(TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT, 3);
        assert_eq!(FINE_TERRAIN_CIRCLE_OCTAVE_COUNT, 0);
        assert_eq!(TIDE_CIRCLE_OCTAVE_COUNT, 3);
        assert_eq!(TIDE_CIRCLE_OCTAVE_COUNT, TIDE_OCTAVE_COUNT);
        assert_eq!(
            SURFACE_AIR_TEMPERATURE_NOISE.circle_octave_count,
            SURFACE_AIR_TEMPERATURE_NOISE.octave_count,
        );
        assert_eq!(
            AIR_TEMPERATURE_500_HPA_NOISE.circle_octave_count,
            AIR_TEMPERATURE_500_HPA_NOISE.octave_count,
        );
        assert_eq!(WATER_VAPOR_NOISE.circle_octave_count, 4);
    }

    #[test]
    fn circle_embedded_noise_gradient_matches_finite_difference_at_rectangular_seams() {
        let epsilon_m = 0.01;
        for world in [
            WorldGeometry::new(8_000.0, 256_000.0).unwrap(),
            WorldGeometry::new(80_000.0, 40_000.0).unwrap(),
            WorldGeometry::new(256_000.0, 8_000.0).unwrap(),
        ] {
            let half_width = f64::from(world.width_m()) / 2.0;
            let half_height = f64::from(world.height_m()) / 2.0;
            for (x, y) in [
                (-half_width, 0.173 * half_height),
                (0.291 * half_width, -half_height),
                (-half_width, -half_height),
            ] {
                let sample =
                    circle_embedded_simplex_noise_sample_f64(world, 0x5eed_cafe, x, y, 8192.0);
                let x_low = circle_embedded_simplex_noise_sample_f64(
                    world,
                    0x5eed_cafe,
                    x - epsilon_m,
                    y,
                    8192.0,
                )
                .value;
                let x_high = circle_embedded_simplex_noise_sample_f64(
                    world,
                    0x5eed_cafe,
                    x + epsilon_m,
                    y,
                    8192.0,
                )
                .value;
                let y_low = circle_embedded_simplex_noise_sample_f64(
                    world,
                    0x5eed_cafe,
                    x,
                    y - epsilon_m,
                    8192.0,
                )
                .value;
                let y_high = circle_embedded_simplex_noise_sample_f64(
                    world,
                    0x5eed_cafe,
                    x,
                    y + epsilon_m,
                    8192.0,
                )
                .value;
                assert!(
                    ((x_high - x_low) / (2.0 * epsilon_m) - sample.gradient_x_per_m).abs() < 1.0e-8
                );
                assert!(
                    ((y_high - y_low) / (2.0 * epsilon_m) - sample.gradient_y_per_m).abs() < 1.0e-8
                );
            }
        }
    }

    #[test]
    fn circle_embedded_noise_keeps_local_gradient_scale_across_world_sizes() {
        let wavelength_m = 8192.0;
        let mut scaled_rms = Vec::new();
        for world in [
            WorldGeometry::new(8_000.0, 256_000.0).unwrap(),
            WorldGeometry::new(80_000.0, 40_000.0).unwrap(),
            WorldGeometry::new(256_000.0, 8_000.0).unwrap(),
        ] {
            let mut square_sum = 0.0;
            for x_index in 0..16 {
                for y_index in 0..16 {
                    let x = f64::from(world.width_m()) * (x_index as f64 + 0.37) / 16.0;
                    let y = f64::from(world.height_m()) * (y_index as f64 + 0.61) / 16.0;
                    let sample = circle_embedded_simplex_noise_sample_f64(
                        world,
                        0x5eed_cafe ^ (x_index * 16 + y_index) as u64,
                        x,
                        y,
                        wavelength_m,
                    );
                    square_sum += sample.gradient_x_per_m * sample.gradient_x_per_m
                        + sample.gradient_y_per_m * sample.gradient_y_per_m;
                }
            }
            scaled_rms.push(wavelength_m * libm::sqrt(square_sum / 256.0));
        }
        let minimum = scaled_rms.iter().copied().fold(f64::INFINITY, f64::min);
        let maximum = scaled_rms.iter().copied().fold(0.0, f64::max);
        assert!(maximum / minimum < 1.35, "{scaled_rms:?}");
    }

    #[test]
    fn portable_circle_noise_tracks_double_precision_reference() {
        for world in [
            WorldGeometry::new(8_000.0, 256_000.0).unwrap(),
            WorldGeometry::new(80_000.0, 40_000.0).unwrap(),
            WorldGeometry::new(256_000.0, 8_000.0).unwrap(),
        ] {
            for position_m in [
                Vector::new(13.25, -7.5),
                Vector::new(0.173 * world.width_m(), -0.291 * world.height_m()),
            ] {
                let portable = terrain_simplex_noise_4d(world, 0x5eed_cafe, position_m, 8192.0);
                let reference = circle_embedded_simplex_noise_sample_f64(
                    world,
                    0x5eed_cafe,
                    f64::from(position_m.x),
                    f64::from(position_m.y),
                    8192.0,
                );
                assert!((f64::from(portable.0) - reference.value).abs() < 2.0e-4);
                assert!((f64::from(portable.1.x) - reference.gradient_x_per_m).abs() < 2.0e-7);
                assert!((f64::from(portable.1.y) - reference.gradient_y_per_m).abs() < 2.0e-7);
            }
        }
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
