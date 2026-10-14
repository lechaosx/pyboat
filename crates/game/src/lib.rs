//! Procedural transcendental functions use `libm` so native and WebAssembly
//! builds do not silently diverge through platform math implementations.

use rapier2d::prelude::Vector;
use simulation::{
    ActuatorTorques, BoatMotion, BoatParameters, Environment, HullParameters, MotionIntent,
    TICKS_PER_SECOND, WorldGeometry, computed_maximum_yaw_rate_rad_s, surface_control_input,
    surface_target_control_torques, yaw_rate_move_control,
};

const TICK_SECONDS: f64 = 1.0 / TICKS_PER_SECOND as f64;
const WIND_WEIBULL_SCALE_M_S: f64 = 9.0;
// These gains fit the accepted k=2, lambda=9 m/s wind distribution; they are
// gameplay calibration, not pressure amplitudes transferred from Earth data.
const PRESSURE_SIMPLEX_SCALE: f32 = 0.495;
const PRESSURE_EVOLUTION_SPEED_M_S: f32 = 6.0;
// 7.71 * sqrt(0.0012) approximates ECMWF's friction-velocity gust term over
// open water when only sustained 10 m wind is available. The instability
// multiplier and procedural pulse are gameplay proxies for missing boundary-
// layer stability and the three-second peak within an observation interval.
const SURFACE_GUST_DRAG_INCREMENT: f32 = 0.27;
// ECMWF adds 0.6 of positive low-level speed shear during deep convection.
const CONVECTIVE_GUST_SHEAR_FRACTION: f32 = 0.60;
const CLIMATE_PRESSURE_ZONAL_WAVELENGTH_M: f32 = 19_200.0;
const CLIMATE_PRESSURE_MERIDIONAL_WORLD_FRACTION: f32 = 0.30;
const PLANETARY_ZONAL_WIND_PEAK_M_S: f32 = 3.1;
const PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M: f32 = 6000.0;
const SURFACE_CROSS_ISOBAR_FRACTION: f32 = 0.12;
const UPPER_WIND_SCALE: f32 = 1.15;
const THERMAL_WIND_SCALE_M2_S_C: f32 = 1_600.0;
const AIR_TEMPERATURE_500_HPA_OFFSET_C: f32 = 35.5;
const AIR_TEMPERATURE_500_HPA_SEASONAL_RANGE_FRACTION: f32 = 0.25;
const SURFACE_AIR_MASS_COUPLING_500_HPA: f32 = 0.45;
const INDEPENDENT_AIR_TEMPERATURE_500_HPA_RANGE_C: f32 = 19.0;
const SURFACE_AIR_TEMPERATURE_NOISE_SALT: u64 = 0x4528_21e6_38d0_1377;
const AIR_TEMPERATURE_500_HPA_NOISE_SALT: u64 = 0xbe54_66cf_34e9_0c6c;
const WATER_VAPOR_NOISE_SALT: u64 = 0x082e_fa98_ec4e_6c89;
// Four-dimensional slices use field-specific gains to preserve the independently
// calibrated temperature tails and clear/cloudy/storm occupancy.
const SURFACE_AIR_TEMPERATURE_NOISE_SCALE: f64 = 0.85;
const AIR_TEMPERATURE_500_HPA_NOISE_SCALE: f64 = 0.75;
const WATER_VAPOR_NOISE_SCALE: f64 = 0.75;
// Calibrated from 1991–2020 NASA POWER/MERRA-2 daily T2M, T2M_MIN, and T2M_MAX:
// eight longitude-balanced, low-elevation land sites per 10° absolute-latitude
// band, with Southern Hemisphere seasons shifted and grid elevation removed by
// the standard lapse rate. The climate knots summarize band means and medians;
// the anomaly tables match pooled daily-maximum p99 tails through the seeded
// noise distribution. Keep these tables synchronized with
// `crates/browser/src/shader/components/environment.wgsl`.
const CLIMATE_ANNUAL_MEAN_C: [f32; 10] =
    [27.4, 27.9, 25.0, 21.9, 16.9, 8.0, -0.2, -7.9, -11.7, -15.0];
const CLIMATE_SEASONAL_RANGE_C: [f32; 10] =
    [2.0, 3.1, 5.8, 15.3, 18.5, 21.3, 31.0, 31.6, 25.9, 20.0];
const CLIMATE_DIURNAL_RANGE_C: [f32; 10] = [4.4, 5.6, 5.2, 9.5, 9.6, 6.1, 6.3, 4.5, 3.2, 2.5];
const TEMPERATURE_ANOMALY_P99_C: [f32; 10] =
    [9.5, 5.4, 5.5, 13.7, 14.0, 16.2, 12.4, 13.3, 6.6, 6.0];
const TEMPERATURE_ANOMALY_SCALE_C: [f32; 10] =
    [19.5, 11.1, 11.3, 28.1, 28.7, 33.2, 25.4, 27.3, 13.5, 12.3];
// These speeds set noise-axis crossing times; they are gameplay evolution rates,
// not air-parcel advection velocities or values scaled from Earth.
const SURFACE_AIR_TEMPERATURE_EVOLUTION_SPEED_M_S: f32 = 5.4;
const AIR_TEMPERATURE_500_HPA_EVOLUTION_SPEED_M_S: f32 = 6.9;
const WATER_VAPOR_EVOLUTION_SPEED_M_S: f32 = 6.0;
const DECORATIVE_WATER_EVOLUTION_SPEED_M_S: f32 = 0.65;
const DECORATIVE_WATER_WAVELENGTH_M: f32 = 18.0;
const LOW_CLOUD_VISUAL_WAVELENGTH_M: f32 = 640.0;
const WEATHER_CANOPY_VISUAL_WAVELENGTH_M: f32 = 4_000.0;
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
const OCEAN_CURRENT_NOISE_SALT: u64 = 0xcbbb_9d5d_c105_9ed8;
// A quarter-circumference wavelength represents an ocean-basin gyre. The
// circumference scaling is Earth-like; the single-band spectrum and strength
// are gameplay tuning rather than a scaled basin circulation model.
const OCEAN_CURRENT_WORLD_FRACTION: f64 = 0.25;
const OCEAN_CURRENT_OCTAVE_COUNT: usize = 3;
const OCEAN_CURRENT_STREAMFUNCTION_SCALE_M2_S: f64 = 5_500.0;
const OCEAN_CURRENT_PRESSURE_COUPLING: f64 = 0.006;
// Current strength, pressure coupling, and evolution rate are provisional
// gameplay values without hydrodynamic calibration; only the gyre wavelength
// fraction carries the Earth-scale analogy.
const OCEAN_CURRENT_EVOLUTION_SPEED_M_S: f32 = 0.012;
const OCEAN_TIDAL_POTENTIAL_SCALE_M2_S: f64 = 1_200.0;
const MAX_SIGNIFICANT_WAVE_HEIGHT_M: f32 = 14.0;
pub const OCEAN_WAVE_MODE_COUNT: usize = 16;
const OCEAN_WIND_SEA_MODE_COUNT: usize = 12;
const GRAVITY_M_S2: f64 = 9.81;
// OpenSimplex2's CC0 24-direction 2D gradient layout reduces repeated contour
// features when a field is thresholded. Keep values and order synchronized with
// `crates/browser/src/shader/components/noise.wgsl` so its hash selects the same gradient.
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
const WORLD_LUNAR_MONTH_DAYS: f64 = 28.0;
const WORLD_YEAR_DAYS: f64 = 365.0;
const WORLD_ORBIT_EPOCH_PHASE: f64 = 0.0021;
const WORLD_LUNAR_PHASE_CYCLE_DAYS: f64 = WORLD_LUNAR_MONTH_DAYS;
const DIURNAL_THERMAL_TIME_CONSTANT_DAYS: f64 = 4.0 / 24.0;
const DIURNAL_DAYLIGHT_HARMONIC_SATURATION: f64 = 0.05;
// The Moon must orbit faster than its 28-day Sun-relative phase cycle because
// the Sun advances through the sky during the same interval.
const WORLD_LUNAR_ORBIT_DAYS: f64 =
    1.0 / (1.0 / WORLD_LUNAR_PHASE_CYCLE_DAYS + 1.0 / WORLD_YEAR_DAYS);
const LUNAR_ORBIT_INCLINATION_RAD: f64 = 5.145_f64.to_radians();
// This node retains paired eclipse seasons while placing one full Moon close
// enough to the orbital plane for totality.
const LUNAR_ASCENDING_NODE_PHASE: f64 = 0.5104333333333333;
const LUNAR_DISTANCE_PLANET_RADII: f64 = 60.3;
const SUN_ANGULAR_RADIUS_RAD: f64 = 0.00465;
const MOON_ANGULAR_RADIUS_RAD: f64 = 0.00452;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeriodicNoiseBasis {
    LongitudePeriodicTriangularSimplex2d,
    LongitudeCircleSimplex4d,
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
        PeriodicNoiseBasis::LongitudePeriodicTriangularSimplex2d => {
            periodic_simplex_effective_wavelengths(world_geometry, requested_wavelength_m)
        }
        PeriodicNoiseBasis::LongitudeCircleSimplex4d => {
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
    (
        world_geometry.width_m() / (std::f32::consts::SQRT_2 * horizontal_modes),
        requested_wavelength_m,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmospherePrimary {
    pub pressure_hpa: f32,
    pub pressure_gradient_hpa_per_m: Vector,
    pub synoptic_pressure_gradient_hpa_per_m: Vector,
    pub surface_gust_signal: f32,
    pub surface_air_temperature_anomaly_c: f32,
    pub air_temperature_500_hpa_c: f32,
    pub local_temperature_gradient_c_per_m: Vector,
    pub water_vapor_g_kg: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialConditions {
    pub geographic_latitude_degrees: f32,
    pub celestial_latitude_degrees: f32,
    pub season_phase: f32,
    pub day_phase: f32,
    pub lunar_longitude_phase: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialOrbit {
    pub solar_declination_rad: f64,
    pub solar_right_ascension_rad: f64,
    pub lunar_declination_rad: f64,
    pub lunar_right_ascension_rad: f64,
    pub illuminated_fraction: f64,
    pub lunar_penumbra_fraction: f32,
    pub lunar_umbra_fraction: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CelestialLighting {
    pub sun_direction: [f32; 3],
    pub sun_intensity: f32,
    pub moon_direction: [f32; 3],
    pub moon_intensity: f32,
    pub moon_ambient_intensity: f32,
    pub moon_color: [f32; 3],
    pub solar_eclipse_fraction: f32,
    pub lunar_penumbra_fraction: f32,
    pub lunar_umbra_fraction: f32,
    pub ambient_intensity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimateConditions {
    pub annual_mean_air_temperature_c: f32,
    pub seasonal_temperature_range_c: f32,
    pub diurnal_temperature_range_c: f32,
    pub annual_mean_water_temperature_c: f32,
    pub seasonal_water_temperature_range_c: f32,
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
    pub surface_gust_velocity_m_s: Vector,
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
pub struct OceanState {
    pub current_velocity_m_s: Vector,
    pub wave_orbital_velocity_m_s: Vector,
    pub significant_wave_height_m: f32,
    pub surface_displacement_m: f32,
    pub surface_slope: Vector,
    pub dominant_wave_direction: Vector,
    pub rogue_crest_excess_m: f32,
}

/// A wave mode's spatial phase is `wave_number_x_per_m * x + wave_number_y_per_m * y`
/// with `x` in `[0, width)`; `phase_rad` adds the mode's phase at the queried clock.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OceanWaveMode {
    pub wave_number_x_per_m: f64,
    pub wave_number_y_per_m: f64,
    pub wave_number_per_m: f64,
    pub angular_frequency_rad_s: f64,
    pub phase_rad: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldEnvironment {
    pub celestial: CelestialConditions,
    pub climate: ClimateConditions,
    pub weather: ProceduralWeather,
    pub tide: TidalConditions,
    pub surface_climate: SurfaceClimate,
    pub water_height_m: f32,
    pub ocean: OceanState,
    pub environment: Environment,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalarFieldTimeCoordinates {
    pub air_temperature: f32,
    pub upper_air_temperature: f32,
    pub water_vapor: f32,
    pub decorative_water: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CloudFieldTimeCoordinates {
    pub low: f32,
    pub upper: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VectorFieldTimeCoordinates {
    pub pressure: [f32; 4],
    pub ocean_current: f32,
}

struct ScalarNoiseSample {
    value: f64,
    gradient_x_per_m: f64,
    gradient_y_per_m: f64,
}

#[derive(Clone, Copy)]
struct TidalSample {
    conditions: TidalConditions,
    range_gradient_per_m: Vector,
    phase_gradient_per_m: Vector,
}

#[derive(Clone, Copy)]
struct OceanQuery {
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    current_time_coordinate: f32,
    position_m: Vector,
    celestial: CelestialConditions,
    weather: ProceduralWeather,
    tidal_sample: TidalSample,
    water_height_m: f32,
    water_depth_m: f32,
    wave_forcing_wind_velocity_m_s: Vector,
}

struct TemperatureSample {
    value: f32,
    gradient_per_m: Vector,
}

#[derive(Clone, Copy)]
struct WeatherNoiseParameters {
    salt: u64,
    scale: f64,
    evolution_speed_m_s: f32,
    wavelength_m: f32,
    octave_count: usize,
}

const SURFACE_AIR_TEMPERATURE_NOISE: WeatherNoiseParameters = WeatherNoiseParameters {
    salt: SURFACE_AIR_TEMPERATURE_NOISE_SALT,
    scale: SURFACE_AIR_TEMPERATURE_NOISE_SCALE,
    evolution_speed_m_s: SURFACE_AIR_TEMPERATURE_EVOLUTION_SPEED_M_S,
    wavelength_m: 65_536.0,
    octave_count: 4,
};
const AIR_TEMPERATURE_500_HPA_NOISE: WeatherNoiseParameters = WeatherNoiseParameters {
    salt: AIR_TEMPERATURE_500_HPA_NOISE_SALT,
    scale: AIR_TEMPERATURE_500_HPA_NOISE_SCALE,
    evolution_speed_m_s: AIR_TEMPERATURE_500_HPA_EVOLUTION_SPEED_M_S,
    wavelength_m: 65_536.0,
    octave_count: 4,
};
const WATER_VAPOR_NOISE: WeatherNoiseParameters = WeatherNoiseParameters {
    salt: WATER_VAPOR_NOISE_SALT,
    scale: WATER_VAPOR_NOISE_SCALE,
    evolution_speed_m_s: WATER_VAPOR_EVOLUTION_SPEED_M_S,
    // Humidity organizes weather systems; the visual fields supply smaller cloud
    // bodies. Keep this spectrum synchronized with
    // `crates/browser/src/shader/components/environment.wgsl`.
    wavelength_m: 8192.0,
    octave_count: 5,
};

pub fn celestial_orbit(conditions: CelestialConditions) -> CelestialOrbit {
    let solar_longitude = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.25);
    let obliquity = 23.44_f64.to_radians();
    let sun_equatorial = ecliptic_direction(solar_longitude, 0.0, obliquity);
    let solar_declination = libm::asin(sun_equatorial[2]);
    let solar_right_ascension = libm::atan2(sun_equatorial[1], sun_equatorial[0]);
    let lunar_longitude =
        std::f64::consts::TAU * f64::from(conditions.lunar_longitude_phase.rem_euclid(1.0));
    let lunar_latitude = libm::asin(
        libm::sin(LUNAR_ORBIT_INCLINATION_RAD)
            * libm::sin(lunar_longitude - std::f64::consts::TAU * LUNAR_ASCENDING_NODE_PHASE),
    );
    let moon_equatorial = ecliptic_direction(lunar_longitude, lunar_latitude, obliquity);
    let lunar_declination = libm::asin(moon_equatorial[2]);
    let lunar_right_ascension = libm::atan2(moon_equatorial[1], moon_equatorial[0]);
    let illuminated_fraction = 0.5 - 0.5 * dot3(sun_equatorial, moon_equatorial);
    let anti_solar = [-sun_equatorial[0], -sun_equatorial[1], -sun_equatorial[2]];
    let lunar_shadow_separation = angular_separation(moon_equatorial, anti_solar);
    let planet_angular_radius = libm::asin(1.0 / LUNAR_DISTANCE_PLANET_RADII);
    let umbra_radius = planet_angular_radius - SUN_ANGULAR_RADIUS_RAD;
    let penumbra_radius = planet_angular_radius + SUN_ANGULAR_RADIUS_RAD;
    let lunar_umbra_fraction = circle_overlap_fraction(
        lunar_shadow_separation,
        MOON_ANGULAR_RADIUS_RAD,
        umbra_radius,
    ) as f32;
    let lunar_penumbra_fraction = circle_overlap_fraction(
        lunar_shadow_separation,
        MOON_ANGULAR_RADIUS_RAD,
        penumbra_radius,
    ) as f32;

    CelestialOrbit {
        solar_declination_rad: solar_declination,
        solar_right_ascension_rad: solar_right_ascension,
        lunar_declination_rad: lunar_declination,
        lunar_right_ascension_rad: lunar_right_ascension,
        illuminated_fraction,
        lunar_penumbra_fraction,
        lunar_umbra_fraction,
    }
}

pub fn celestial_lighting(conditions: CelestialConditions) -> CelestialLighting {
    celestial_lighting_at_height(WorldGeometry::default(), conditions, 0.0)
}

pub fn celestial_lighting_at_height(
    world_geometry: WorldGeometry,
    conditions: CelestialConditions,
    receiver_height_m: f32,
) -> CelestialLighting {
    let orbit = celestial_orbit(conditions);
    let latitude = f64::from(conditions.celestial_latitude_degrees).to_radians();
    let solar_hour_angle = std::f64::consts::TAU * (f64::from(conditions.day_phase) - 0.5);
    let sun_direction =
        celestial_direction(latitude, orbit.solar_declination_rad, solar_hour_angle);
    let horizon_z = celestial_horizon_z_at_height(world_geometry, receiver_height_m);
    let sun_elevation = ((sun_direction[2] - horizon_z) / (1.0 - horizon_z)).clamp(0.0, 1.0);
    let sun_elevation = libm::sqrt(f64::from(sun_elevation)) as f32;
    let lunar_hour_angle =
        solar_hour_angle + orbit.solar_right_ascension_rad - orbit.lunar_right_ascension_rad;
    let geocentric_moon_direction =
        celestial_direction(latitude, orbit.lunar_declination_rad, lunar_hour_angle);
    let world_radius_m = world_geometry.width_m() / std::f32::consts::TAU;
    let observer_radius_planet_radii = (world_radius_m + receiver_height_m) / world_radius_m;
    let moon_direction =
        topocentric_moon_direction(geocentric_moon_direction, observer_radius_planet_radii);
    let solar_separation =
        angular_separation(array3_f64(sun_direction), array3_f64(moon_direction));
    let solar_eclipse_fraction = circle_overlap_fraction(
        solar_separation,
        SUN_ANGULAR_RADIUS_RAD,
        MOON_ANGULAR_RADIUS_RAD,
    ) as f32;

    let sun_intensity = (1.0 - solar_eclipse_fraction)
        * celestial_disc_visibility_at_height(
            world_geometry,
            receiver_height_m,
            sun_direction[2],
            SUN_ANGULAR_RADIUS_RAD as f32,
        )
        * (0.35 + 0.65 * sun_elevation);
    let moon_elevation = ((moon_direction[2] - horizon_z) / (1.0 - horizon_z)).clamp(0.0, 1.0);
    let moon_elevation = libm::sqrt(f64::from(moon_elevation)) as f32;
    let lunar_shadow_transmittance =
        (1.0 - 0.18 * orbit.lunar_penumbra_fraction - 0.78 * orbit.lunar_umbra_fraction).max(0.04);
    let moon_intensity = lunar_shadow_transmittance
        * 0.12
        * orbit.illuminated_fraction as f32
        * celestial_disc_visibility_at_height(
            world_geometry,
            receiver_height_m,
            moon_direction[2],
            MOON_ANGULAR_RADIUS_RAD as f32,
        )
        * (0.35 + 0.65 * moon_elevation);
    // These atmospheric horizon thresholds must stay synchronized with lighting.wgsl.
    let lunar_daylight =
        smoothstep_range(-0.034_899_5, 0.139_173_1, f64::from(moon_direction[2])) as f32;
    let lunar_lower_twilight =
        smoothstep_range(-0.156_434_5, 0.017_452_4, f64::from(moon_direction[2])) as f32;
    let lunar_upper_afterglow =
        smoothstep_range(-0.309_017, -0.139_173_1, f64::from(moon_direction[2])) as f32
            * (1.0
                - smoothstep_range(-0.139_173_1, -0.034_899_5, f64::from(moon_direction[2]))
                    as f32);
    let lunar_ambient = lunar_daylight * (0.45 + 0.515 * moon_elevation)
        + 0.30 * (1.0 - lunar_daylight) * lunar_lower_twilight
        + 0.065 * lunar_upper_afterglow;
    let moon_ambient_intensity =
        lunar_shadow_transmittance * 0.12 * orbit.illuminated_fraction as f32 * lunar_ambient;
    let eclipse_warmth = smoothstep_range(0.05, 0.90, f64::from(orbit.lunar_umbra_fraction)) as f32;
    let moon_color = [
        0.48 + eclipse_warmth * (1.20 - 0.48),
        0.62 + eclipse_warmth * (0.28 - 0.62),
        1.00 + eclipse_warmth * (0.06 - 1.00),
    ];
    // These sine-of-elevation thresholds must stay synchronized with lighting.wgsl.
    let daylight = smoothstep_range(-0.034_899_5, 0.139_173_1, f64::from(sun_direction[2])) as f32;
    let lower_twilight =
        smoothstep_range(-0.156_434_5, 0.017_452_4, f64::from(sun_direction[2])) as f32;
    let upper_afterglow = smoothstep_range(-0.309_017, -0.139_173_1, f64::from(sun_direction[2]))
        as f32
        * (1.0 - smoothstep_range(-0.139_173_1, -0.034_899_5, f64::from(sun_direction[2])) as f32);
    let solar_ambient = daylight * (0.45 + 0.515 * sun_elevation)
        + 0.30 * (1.0 - daylight) * lower_twilight
        + 0.065 * upper_afterglow;
    let solar_ambient_transmittance = 1.0 - 0.85 * solar_eclipse_fraction;
    let ambient_intensity =
        (0.035 + solar_ambient_transmittance * solar_ambient + 0.85 * moon_ambient_intensity)
            .min(1.0);

    CelestialLighting {
        sun_direction,
        sun_intensity,
        moon_direction,
        moon_intensity,
        moon_ambient_intensity,
        moon_color,
        solar_eclipse_fraction,
        lunar_penumbra_fraction: orbit.lunar_penumbra_fraction,
        lunar_umbra_fraction: orbit.lunar_umbra_fraction,
        ambient_intensity,
    }
}

pub fn celestial_disc_visibility_at_height(
    world_geometry: WorldGeometry,
    height_m: f32,
    direction_z: f32,
    angular_radius_rad: f32,
) -> f32 {
    let horizon_z = celestial_horizon_z_at_height(world_geometry, height_m);
    let horizon_cosine = (1.0 - horizon_z * horizon_z).max(0.0).sqrt();
    let disc_sine = angular_radius_rad.sin();
    let disc_cosine = angular_radius_rad.cos();
    smoothstep_range(
        f64::from(horizon_z * disc_cosine - horizon_cosine * disc_sine),
        f64::from(horizon_z * disc_cosine + horizon_cosine * disc_sine),
        f64::from(direction_z),
    ) as f32
}

fn celestial_horizon_z_at_height(world_geometry: WorldGeometry, height_m: f32) -> f32 {
    let world_radius_m = world_geometry.width_m() / std::f32::consts::TAU;
    let observer_radius_m = world_radius_m + height_m.max(0.0);
    let radius_ratio = world_radius_m / observer_radius_m;
    -(1.0 - radius_ratio * radius_ratio).max(0.0).sqrt()
}

pub fn celestial_visible_disc_sample_direction_z_at_height(
    world_geometry: WorldGeometry,
    height_m: f32,
    direction_z: f32,
    angular_radius_rad: f32,
) -> f32 {
    let world_radius_m = world_geometry.width_m() / std::f32::consts::TAU;
    let observer_radius_m = world_radius_m + height_m.max(0.0);
    let radius_ratio = world_radius_m / observer_radius_m;
    let horizon_z = -(1.0 - radius_ratio * radius_ratio).max(0.0).sqrt();
    let direction_z = direction_z.clamp(-1.0, 1.0);
    let direction_horizontal = (1.0 - direction_z * direction_z).max(0.0).sqrt();
    let disc_sine = angular_radius_rad.sin();
    let disc_cosine = angular_radius_rad.cos();
    let lower_limb_z = direction_z * disc_cosine - direction_horizontal * disc_sine;
    let upper_limb_z = direction_z * disc_cosine + direction_horizontal * disc_sine;
    if lower_limb_z >= horizon_z || upper_limb_z <= horizon_z {
        return direction_z;
    }
    0.5 * (horizon_z + upper_limb_z)
}

pub fn moon_phase(conditions: CelestialConditions) -> f32 {
    let solar_longitude_phase = (conditions.season_phase - 0.25).rem_euclid(1.0);
    (conditions.lunar_longitude_phase - solar_longitude_phase).rem_euclid(1.0)
}

pub fn current_air_temperature_c(
    climate: ClimateConditions,
    conditions: CelestialConditions,
) -> f32 {
    let seasonal_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.34);
    let latitude_factor = seasonal_latitude_factor(conditions.geographic_latitude_degrees);
    climate.annual_mean_air_temperature_c
        + 0.5
            * climate.seasonal_temperature_range_c
            * latitude_factor
            * libm::sin(seasonal_phase) as f32
        + 0.5 * climate.diurnal_temperature_range_c * diurnal_temperature_factor(conditions)
}

fn diurnal_temperature_factor(conditions: CelestialConditions) -> f32 {
    let latitude = f64::from(conditions.celestial_latitude_degrees).to_radians();
    let solar_longitude = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.25);
    let declination = libm::asin(libm::sin(23.44_f64.to_radians()) * libm::sin(solar_longitude));
    let mean_solar_elevation = libm::sin(latitude) * libm::sin(declination);
    let varying_solar_elevation = libm::cos(latitude) * libm::cos(declination);
    let varying_magnitude = varying_solar_elevation.abs();
    // Closed-form first cosine coefficient of max(0, sun_direction.z).
    let daylight_harmonic = if varying_magnitude < 1.0e-9 {
        0.0
    } else if mean_solar_elevation >= varying_magnitude {
        varying_solar_elevation
    } else if mean_solar_elevation <= -varying_magnitude {
        0.0
    } else {
        let sunset_hour_angle =
            libm::acos((-mean_solar_elevation / varying_magnitude).clamp(-1.0, 1.0));
        let sunset_sine = libm::sin(sunset_hour_angle);
        varying_solar_elevation.signum()
            * (2.0 * mean_solar_elevation * sunset_sine
                + varying_magnitude
                    * (sunset_hour_angle + sunset_sine * libm::cos(sunset_hour_angle)))
            / std::f64::consts::PI
    };
    let hour_angle = std::f64::consts::TAU * (f64::from(conditions.day_phase) - 0.5);
    let thermal_frequency_time = std::f64::consts::TAU * DIURNAL_THERMAL_TIME_CONSTANT_DAYS;
    // Normalized steady response of a first-order thermal reservoir.
    let thermal_response = (libm::cos(hour_angle) + thermal_frequency_time * libm::sin(hour_angle))
        / libm::sqrt(1.0 + thermal_frequency_time * thermal_frequency_time);
    let daylight_response = smoothstep_range(
        0.0,
        DIURNAL_DAYLIGHT_HARMONIC_SATURATION,
        daylight_harmonic.abs(),
    );
    (daylight_harmonic.signum() * daylight_response * thermal_response) as f32
}

pub fn current_500_hpa_air_temperature_c(
    climate: ClimateConditions,
    conditions: CelestialConditions,
) -> f32 {
    let seasonal_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.34);
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
        solar_hour_angle + std::f64::consts::TAU * f64::from(moon_phase(conditions));
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
        celestial_latitude_degrees: world_geometry.latitude_phase_degrees(position_m.y),
        season_phase: (WORLD_ORBIT_EPOCH_PHASE
            + world_time_seconds / (WORLD_DAY_SECONDS * WORLD_YEAR_DAYS))
            .rem_euclid(1.0) as f32,
        day_phase: world_geometry.local_solar_phase(global_day_phase, position_m.x),
        lunar_longitude_phase: (WORLD_ORBIT_EPOCH_PHASE
            + world_time_seconds / (WORLD_DAY_SECONDS * WORLD_LUNAR_ORBIT_DAYS))
            .rem_euclid(1.0) as f32,
    }
}

pub fn world_climate_conditions(geographic_latitude_degrees: f32) -> ClimateConditions {
    let normalized_latitude = (geographic_latitude_degrees.abs() / 90.0).clamp(0.0, 1.0);
    let polar_fraction = libm::powf(normalized_latitude, 1.5);
    ClimateConditions {
        annual_mean_air_temperature_c: latitude_curve_value(
            CLIMATE_ANNUAL_MEAN_C,
            geographic_latitude_degrees,
        ),
        seasonal_temperature_range_c: latitude_curve_value(
            CLIMATE_SEASONAL_RANGE_C,
            geographic_latitude_degrees,
        ),
        diurnal_temperature_range_c: latitude_curve_value(
            CLIMATE_DIURNAL_RANGE_C,
            geographic_latitude_degrees,
        ),
        annual_mean_water_temperature_c: 27.0 - 45.0 * polar_fraction,
        seasonal_water_temperature_range_c: 3.0 + 41.0 * libm::powf(polar_fraction, 0.9),
    }
}

fn latitude_curve_value(knots: [f32; 10], geographic_latitude_degrees: f32) -> f32 {
    let knot_position = (geographic_latitude_degrees.abs() / 10.0).clamp(0.0, 9.0);
    let lower = libm::floorf(knot_position) as usize;
    let upper = (lower + 1).min(knots.len() - 1);
    let fraction = knot_position - lower as f32;
    let amount = fraction * fraction * (3.0 - 2.0 * fraction);
    knots[lower] + amount * (knots[upper] - knots[lower])
}

pub fn procedural_tidal_conditions(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
) -> TidalConditions {
    procedural_tidal_sample(world_geometry, match_seed, position_m).conditions
}

fn procedural_tidal_sample(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
) -> TidalSample {
    // Tide response and lag occupy independent seed domains; sharing terrain or
    // hash-mixer constants would couple unrelated fields at aligned coordinates.
    let response = terrain_fractal_noise_sample(
        world_geometry,
        match_seed ^ 0x6a09_e667_f3bc_c909,
        f64::from(position_m.x),
        f64::from(position_m.y),
        131_072.0,
        TIDE_OCTAVE_COUNT,
        TIDE_CIRCLE_OCTAVE_COUNT,
    );
    let lag = terrain_fractal_noise_sample(
        world_geometry,
        match_seed ^ 0xbb67_ae85_84ca_a73b,
        f64::from(position_m.x),
        f64::from(position_m.y),
        131_072.0,
        TIDE_OCTAVE_COUNT,
        TIDE_CIRCLE_OCTAVE_COUNT,
    );
    TidalSample {
        conditions: TidalConditions {
            reference_range_m: 1.25 + 0.75 * response.value as f32,
            phase_offset: 0.08 * lag.value as f32,
        },
        range_gradient_per_m: Vector::new(
            (0.75 * response.gradient_x_per_m) as f32,
            (0.75 * response.gradient_y_per_m) as f32,
        ),
        phase_gradient_per_m: Vector::new(
            (0.08 * lag.gradient_x_per_m) as f32,
            (0.08 * lag.gradient_y_per_m) as f32,
        ),
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
    let celestial = world_celestial_conditions(world_geometry, world_time_seconds, position_m);
    let climate = world_climate_conditions(celestial.geographic_latitude_degrees);
    (procedural_water_height_m(world_geometry, match_seed, world_time_seconds, position_m)
        - procedural_solid_surface_elevation_m(
            world_geometry,
            match_seed,
            position_m,
            climate.annual_mean_air_temperature_c,
            seasonal_water_temperature_c(climate, celestial),
        ))
    .max(0.0)
}

pub fn climate_surface(
    climate: ClimateConditions,
    conditions: CelestialConditions,
    water_vapor_g_kg: f32,
    surface_air_temperature_anomaly_c: f32,
    precipitation_intensity: f32,
    elevation_m: f32,
) -> SurfaceClimate {
    let air_temperature_c =
        current_air_temperature_c(climate, conditions) + surface_air_temperature_anomaly_c;
    let latitude_factor = seasonal_latitude_factor(conditions.geographic_latitude_degrees);
    let seasonal_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.38);
    let ground_temperature_c = climate.annual_mean_air_temperature_c
        + 0.42
            * climate.seasonal_temperature_range_c
            * latitude_factor
            * libm::sin(seasonal_phase) as f32
        + 0.15 * climate.diurnal_temperature_range_c * diurnal_temperature_factor(conditions)
        + 0.25 * surface_air_temperature_anomaly_c;
    let water_temperature_c = seasonal_water_temperature_c(climate, conditions);
    let ground_temperature_at_elevation_c = ground_temperature_c - 0.0065 * elevation_m.max(0.0);
    let air_temperature_at_elevation_c = air_temperature_c - 0.0065 * elevation_m.max(0.0);
    let relative_humidity = (water_vapor_g_kg
        / water_vapor_saturation_capacity_g_kg(air_temperature_at_elevation_c))
    .clamp(0.0, 1.5);
    let moisture_support = smoothstep_range(0.45, 0.95, f64::from(relative_humidity)) as f32;
    let weather_standard_deviation_c = latitude_curve_value(
        TEMPERATURE_ANOMALY_P99_C,
        conditions.geographic_latitude_degrees,
    ) / 2.326;
    // The 1.702 logistic approximates a standard normal CDF without truncating
    // the observed temperature tails at one standard deviation.
    let freezing_probability = 1.0
        / (1.0
            + libm::expf(1.702 * ground_temperature_at_elevation_c / weather_standard_deviation_c));
    let snow_retention = 1.0 - (1.0 - freezing_probability).powi(3);
    let persistent_snow = snow_retention * (0.35 + 0.65 * moisture_support);
    let snowfall_cold =
        1.0 - smoothstep_range(-1.0, 2.0, f64::from(air_temperature_at_elevation_c)) as f32;
    let fresh_snow = precipitation_intensity.clamp(0.0, 1.0) * snowfall_cold;
    let snow_before_melt = persistent_snow + fresh_snow * (1.0 - persistent_snow);
    let melt = smoothstep_range(0.0, 6.0, f64::from(air_temperature_at_elevation_c)) as f32;
    let seasonal_snow_cover = snow_before_melt * (1.0 - 0.75 * melt);
    let annual_temperature_at_elevation_c =
        climate.annual_mean_air_temperature_c - 0.0065 * elevation_m.max(0.0);
    let solid_ice_cover =
        1.0 - smoothstep_range(-12.0, -8.0, f64::from(annual_temperature_at_elevation_c)) as f32;
    let polar_ice_closure = water_ice_cover(elevation_m, water_temperature_c);

    SurfaceClimate {
        air_temperature_c,
        ground_temperature_c,
        water_temperature_c,
        seasonal_snow_cover,
        polar_ice_closure,
        solid_ice_cover,
    }
}

fn seasonal_water_temperature_c(
    climate: ClimateConditions,
    conditions: CelestialConditions,
) -> f32 {
    let latitude_factor = seasonal_latitude_factor(conditions.geographic_latitude_degrees);
    let normalized_latitude = (conditions.geographic_latitude_degrees.abs() / 90.0).clamp(0.0, 1.0);
    let maritime_moderation_c =
        65.0 * (libm::powf(normalized_latitude, 1.5) - libm::powf(normalized_latitude, 2.6));
    let water_phase = std::f64::consts::TAU * (f64::from(conditions.season_phase) - 0.425);
    climate.annual_mean_water_temperature_c
        + maritime_moderation_c
        + 0.28
            * climate.seasonal_water_temperature_range_c
            * latitude_factor
            * libm::sin(water_phase) as f32
}

fn water_ice_cover(bedrock_elevation_m: f32, effective_water_temperature_c: f32) -> f32 {
    if bedrock_elevation_m >= 0.0 {
        0.0
    } else {
        water_freezing_potential(effective_water_temperature_c)
    }
}

fn water_freezing_potential(effective_water_temperature_c: f32) -> f32 {
    1.0 - smoothstep_range(-2.0, 0.0, f64::from(effective_water_temperature_c)) as f32
}

fn water_vapor_saturation_capacity_g_kg(air_temperature_c: f32) -> f32 {
    3.5 * libm::exp(0.055 * f64::from(air_temperature_c + 5.0)) as f32
}

fn ecliptic_direction(longitude: f64, latitude: f64, obliquity: f64) -> [f64; 3] {
    let ecliptic_x = libm::cos(latitude) * libm::cos(longitude);
    let ecliptic_y = libm::cos(latitude) * libm::sin(longitude);
    let ecliptic_z = libm::sin(latitude);
    [
        ecliptic_x,
        ecliptic_y * libm::cos(obliquity) - ecliptic_z * libm::sin(obliquity),
        ecliptic_y * libm::sin(obliquity) + ecliptic_z * libm::cos(obliquity),
    ]
}

fn topocentric_moon_direction(
    geocentric_direction: [f32; 3],
    observer_radius_planet_radii: f32,
) -> [f32; 3] {
    let x = LUNAR_DISTANCE_PLANET_RADII * f64::from(geocentric_direction[0]);
    let y = LUNAR_DISTANCE_PLANET_RADII * f64::from(geocentric_direction[1]);
    let z = LUNAR_DISTANCE_PLANET_RADII * f64::from(geocentric_direction[2])
        - f64::from(observer_radius_planet_radii);
    let length = libm::sqrt(x * x + y * y + z * z);
    [
        (x / length) as f32,
        (y / length) as f32,
        (z / length) as f32,
    ]
}

fn array3_f64(value: [f32; 3]) -> [f64; 3] {
    [
        f64::from(value[0]),
        f64::from(value[1]),
        f64::from(value[2]),
    ]
}

fn dot3(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn angular_separation(left: [f64; 3], right: [f64; 3]) -> f64 {
    let cross = [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ];
    libm::atan2(libm::sqrt(dot3(cross, cross)), dot3(left, right))
}

fn circle_overlap_fraction(separation: f64, target_radius: f64, occluder_radius: f64) -> f64 {
    if separation >= target_radius + occluder_radius {
        return 0.0;
    }
    if separation <= (target_radius - occluder_radius).abs() {
        return if occluder_radius >= target_radius {
            1.0
        } else {
            occluder_radius * occluder_radius / (target_radius * target_radius)
        };
    }
    let target_angle = libm::acos(
        ((separation * separation + target_radius * target_radius
            - occluder_radius * occluder_radius)
            / (2.0 * separation * target_radius))
            .clamp(-1.0, 1.0),
    );
    let occluder_angle = libm::acos(
        ((separation * separation + occluder_radius * occluder_radius
            - target_radius * target_radius)
            / (2.0 * separation * occluder_radius))
            .clamp(-1.0, 1.0),
    );
    let lens = 0.5
        * libm::sqrt(
            (-separation + target_radius + occluder_radius)
                * (separation + target_radius - occluder_radius)
                * (separation - target_radius + occluder_radius)
                * (separation + target_radius + occluder_radius),
        );
    (target_radius * target_radius * target_angle
        + occluder_radius * occluder_radius * occluder_angle
        - lens)
        / (std::f64::consts::PI * target_radius * target_radius)
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
    surface_gust_signal: f32,
}

#[derive(Clone, Copy)]
struct WindBand {
    wavelength_m: f32,
    variance_fraction: f32,
    midlatitude_variance_transfer: f32,
    midlatitude_zonal_aspect: f32,
    noise_salt: u64,
    synoptic_coupling: f32,
}

// The climate band retains a 19.2 km zonal wavelength while its meridional
// wavelength spans 30% of world height, so resizing latitude bands does not
// resize their east-west structure. The remaining wavelengths retain the
// default world's 1:625 reduction of roughly 2000, 600, and 160 km Earth
// structures; the 2000 km reference lies within the NWS 1000–2500 km synoptic
// range (forecast.weather.gov/glossary.php?word=synoptic+scale).
// Variance, its latitude transfer, anisotropy, coupling, and evolution rates are
// gameplay tuning rather than a scaled atmospheric energy spectrum. Transfers
// sum to zero, and anisotropy is gradient-normalized, preserving calibrated
// total wind energy. The 256 m band supplies gust timing and does not contribute
// to resolved pressure. Keep the mirrored WGSL values synchronized.
const WIND_BANDS: [WindBand; 4] = [
    WindBand {
        wavelength_m: 0.0,
        variance_fraction: 0.30,
        midlatitude_variance_transfer: -0.15,
        midlatitude_zonal_aspect: 1.25,
        noise_salt: 0x510e_527f_ade6_82d1,
        synoptic_coupling: 1.0,
    },
    WindBand {
        wavelength_m: 3_200.0,
        variance_fraction: 0.54,
        midlatitude_variance_transfer: 0.15,
        midlatitude_zonal_aspect: 1.4,
        noise_salt: 0x9b05_688c_2b3e_6c1f,
        synoptic_coupling: 1.0,
    },
    WindBand {
        wavelength_m: 960.0,
        variance_fraction: 0.08,
        midlatitude_variance_transfer: 0.0,
        midlatitude_zonal_aspect: 1.0,
        noise_salt: 0x1f83_d9ab_fb41_bd6b,
        synoptic_coupling: 0.20,
    },
    WindBand {
        wavelength_m: 256.0,
        variance_fraction: 0.02,
        midlatitude_variance_transfer: 0.0,
        midlatitude_zonal_aspect: 1.0,
        noise_salt: 0x5be0_cd19_137e_2179,
        synoptic_coupling: 0.0,
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
    procedural_world_environment_for_render_clocks(
        world_geometry,
        match_seed,
        tick,
        0.0,
        tick,
        0.0,
        tick,
        0.0,
        tick,
        0.0,
        position_m,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn procedural_world_environment_for_render_clocks(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    scalar_tick: u64,
    scalar_tick_fraction: f32,
    pressure_tick: u64,
    pressure_tick_fraction: f32,
    current_tick: u64,
    current_tick_fraction: f32,
    position_m: Vector,
) -> WorldEnvironment {
    let world_time_seconds = (world_tick as f64 + f64::from(world_tick_fraction)) * TICK_SECONDS;
    let celestial = world_celestial_conditions(world_geometry, world_time_seconds, position_m);
    let climate = world_climate_conditions(celestial.geographic_latitude_degrees);
    let scalar_time = scalar_field_time_coordinates_interpolated(scalar_tick, scalar_tick_fraction);
    let pressure_time = vector_field_time_coordinates_interpolated(
        world_geometry,
        pressure_tick,
        pressure_tick_fraction,
    );
    let current_time = vector_field_time_coordinates_interpolated(
        world_geometry,
        current_tick,
        current_tick_fraction,
    );
    let primary = procedural_atmosphere_primary_with_conditions(
        world_geometry,
        match_seed,
        position_m,
        climate,
        celestial,
        scalar_time,
        pressure_time,
    );
    let weather = derive_weather(primary, climate, celestial);
    let tidal_sample = procedural_tidal_sample(world_geometry, match_seed, position_m);
    let tide = tidal_sample.conditions;
    let water_height_m = tide_height_m(tide, celestial);
    let terrain = procedural_terrain_surface_causes(world_geometry, match_seed, position_m);
    let terrain_elevation_m = terrain.elevation_m;
    let mut surface_climate = climate_surface(
        climate,
        celestial,
        weather.primary.water_vapor_g_kg,
        weather.primary.surface_air_temperature_anomaly_c,
        weather.rain_intensity,
        terrain_elevation_m,
    );
    let solid_surface_elevation_m = solid_surface_elevation_from_bedrock_m(
        terrain_elevation_m,
        climate.annual_mean_air_temperature_c,
        surface_climate.water_temperature_c + terrain.maritime_temperature_anomaly_c,
    );
    surface_climate.polar_ice_closure = water_ice_cover(
        terrain_elevation_m,
        surface_climate.water_temperature_c + terrain.maritime_temperature_anomaly_c,
    );
    let true_wind_velocity_m_s = land_sheltered_wind(
        world_geometry,
        match_seed,
        position_m,
        weather.wind_velocity_m_s,
    );
    // A 256 m gust changes boat forcing immediately but does not build a fully
    // developed wind sea. Resolved pressure flow is the stateless sustained-wind
    // proxy used by the wave spectrum.
    let wave_forcing_wind_velocity_m_s = land_sheltered_wind(
        world_geometry,
        match_seed,
        position_m,
        wind_from_pressure_gradient(
            weather.primary.pressure_gradient_hpa_per_m,
            celestial.geographic_latitude_degrees,
        ),
    );
    let water_depth_m = (water_height_m - solid_surface_elevation_m).max(0.0);
    let ocean = procedural_ocean_state(OceanQuery {
        world_geometry,
        match_seed,
        world_tick,
        world_tick_fraction,
        current_time_coordinate: current_time.ocean_current,
        position_m,
        celestial,
        weather,
        tidal_sample,
        water_height_m,
        water_depth_m,
        wave_forcing_wind_velocity_m_s,
    });
    let environment = Environment {
        true_wind_velocity_m_s,
        true_water_velocity_m_s: ocean.current_velocity_m_s + ocean.wave_orbital_velocity_m_s,
        water_depth_m,
    };

    WorldEnvironment {
        celestial,
        climate,
        weather,
        tide,
        surface_climate,
        water_height_m,
        ocean,
        environment,
    }
}

fn procedural_ocean_state(query: OceanQuery) -> OceanState {
    if query.water_depth_m == 0.0 {
        return OceanState {
            current_velocity_m_s: Vector::ZERO,
            wave_orbital_velocity_m_s: Vector::ZERO,
            significant_wave_height_m: 0.0,
            surface_displacement_m: 0.0,
            surface_slope: Vector::ZERO,
            dominant_wave_direction: Vector::ZERO,
            rogue_crest_excess_m: 0.0,
        };
    }

    let current_velocity_m_s = procedural_current_velocity_m_s(query);
    let mut waves = procedural_wave_state(query);
    waves.current_velocity_m_s = current_velocity_m_s;
    waves
}

fn procedural_current_velocity_m_s(query: OceanQuery) -> Vector {
    let basin = ocean_basin_capacity_sample(
        query.world_geometry,
        query.match_seed,
        query.position_m,
        query.celestial,
    );
    let gyre = spatiotemporal_fractal_noise_sample(
        query.world_geometry,
        query.match_seed ^ OCEAN_CURRENT_NOISE_SALT,
        query.position_m,
        query.world_geometry.width_m() * OCEAN_CURRENT_WORLD_FRACTION as f32,
        query.current_time_coordinate,
        OCEAN_CURRENT_OCTAVE_COUNT,
        0.5,
    );
    let basin_squared = (basin.value * basin.value) as f32;
    let basin_squared_gradient = Vector::new(
        (2.0 * basin.value * basin.gradient_x_per_m) as f32,
        (2.0 * basin.value * basin.gradient_y_per_m) as f32,
    );
    let gyre_gradient = Vector::new(gyre.gradient_x_per_m as f32, gyre.gradient_y_per_m as f32);
    let latitude_rad = f64::from(query.celestial.geographic_latitude_degrees).to_radians();
    let hemisphere = libm::sin(latitude_rad) as f32;
    let hemisphere_gradient_y_per_m = (libm::cos(latitude_rad)
        * f64::from(
            query
                .world_geometry
                .latitude_gradient_degrees_per_m(query.position_m.y),
        )
        .to_radians()) as f32;
    let base_gradient = OCEAN_CURRENT_STREAMFUNCTION_SCALE_M2_S as f32
        * (hemisphere
            * (basin_squared * gyre_gradient + gyre.value as f32 * basin_squared_gradient)
            + Vector::new(
                0.0,
                basin_squared * gyre.value as f32 * hemisphere_gradient_y_per_m,
            ));
    let pressure_potential_m2_s = f64::from(query.weather.primary.pressure_hpa - 1013.25)
        * f64::from(PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M);
    let pressure_gradient_m_s =
        query.weather.primary.pressure_gradient_hpa_per_m * PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M;
    let pressure_envelope = basin_squared * hemisphere;
    let pressure_envelope_gradient = hemisphere * basin_squared_gradient
        + Vector::new(0.0, basin_squared * hemisphere_gradient_y_per_m);
    let weather_gradient = OCEAN_CURRENT_PRESSURE_COUPLING as f32
        * (pressure_envelope * pressure_gradient_m_s
            + pressure_potential_m2_s as f32 * pressure_envelope_gradient);

    let tide_gradient =
        tide_height_gradient_per_m(query.world_geometry, query.celestial, query.tidal_sample);
    let tidal_potential_gradient = OCEAN_TIDAL_POTENTIAL_SCALE_M2_S as f32
        * (basin.value as f32 * tide_gradient
            + query.water_height_m
                * Vector::new(basin.gradient_x_per_m as f32, basin.gradient_y_per_m as f32));
    let streamfunction_gradient = base_gradient + weather_gradient;
    Vector::new(-streamfunction_gradient.y, streamfunction_gradient.x) - tidal_potential_gradient
}

fn spatiotemporal_fractal_noise_sample(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_m: Vector,
    mut wavelength_m: f32,
    mut time_coordinate: f32,
    octave_count: usize,
    persistence: f32,
) -> ScalarNoiseSample {
    let mut value = 0.0;
    let mut gradient = Vector::ZERO;
    let mut amplitude = 1.0;
    let mut amplitude_sum = 0.0;
    for octave in 0..octave_count {
        let octave_seed = field_seed ^ (octave as u64).wrapping_mul(0xd1b5_4a32_d192_ed03);
        let octave_sample = spatiotemporal_simplex_noise_4d(
            world_geometry,
            octave_seed,
            position_m,
            wavelength_m,
            time_coordinate,
        );
        value += amplitude * octave_sample.0;
        gradient += amplitude * octave_sample.1;
        amplitude_sum += amplitude;
        amplitude *= persistence;
        wavelength_m *= 0.5;
        time_coordinate *= 2.0;
    }
    ScalarNoiseSample {
        value: f64::from(value / amplitude_sum),
        gradient_x_per_m: f64::from(gradient.x / amplitude_sum),
        gradient_y_per_m: f64::from(gradient.y / amplitude_sum),
    }
}

fn ocean_basin_capacity_sample(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
    celestial: CelestialConditions,
) -> ScalarNoiseSample {
    let cluster = terrain_fractal_noise_sample(
        world_geometry,
        match_seed ^ 0xa076_1d64_78bd_642f,
        f64::from(position_m.x),
        f64::from(position_m.y),
        8192.0,
        TERRAIN_CLUSTER_OCTAVE_COUNT,
        TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    );
    let threshold_low = 0.02;
    let threshold_high = 0.48;
    let transition =
        ((cluster.value - threshold_low) / (threshold_high - threshold_low)).clamp(0.0, 1.0);
    let open_water = 1.0 - smoothstep(transition);
    let open_water_derivative = if transition == 0.0 || transition == 1.0 {
        0.0
    } else {
        -6.0 * transition * (1.0 - transition) / (threshold_high - threshold_low)
    };
    let latitude_rad = f64::from(celestial.geographic_latitude_degrees).to_radians();
    let polar_capacity = libm::cos(latitude_rad).powi(2);
    let polar_gradient_y_per_m = -libm::sin(2.0 * latitude_rad)
        * f64::from(world_geometry.latitude_gradient_degrees_per_m(position_m.y)).to_radians();
    ScalarNoiseSample {
        value: open_water * polar_capacity,
        gradient_x_per_m: open_water_derivative * cluster.gradient_x_per_m * polar_capacity,
        gradient_y_per_m: open_water_derivative * cluster.gradient_y_per_m * polar_capacity
            + open_water * polar_gradient_y_per_m,
    }
}

fn tide_height_gradient_per_m(
    world_geometry: WorldGeometry,
    celestial: CelestialConditions,
    tidal_sample: TidalSample,
) -> Vector {
    let solar_hour_angle = std::f64::consts::TAU * (f64::from(celestial.day_phase) - 0.5);
    let lunar_hour_angle =
        solar_hour_angle + std::f64::consts::TAU * f64::from(moon_phase(celestial));
    let phase_offset = std::f64::consts::TAU * f64::from(tidal_sample.conditions.phase_offset);
    let lunar_angle = 2.0 * (lunar_hour_angle + phase_offset);
    let solar_angle = 2.0 * (solar_hour_angle + phase_offset);
    let tide_sum = libm::cos(lunar_angle) + 0.46 * libm::cos(solar_angle);
    let local_hour_gradient = Vector::new(
        (std::f64::consts::TAU / f64::from(world_geometry.width_m())) as f32,
        0.0,
    );
    let angle_gradient =
        local_hour_gradient + std::f32::consts::TAU * tidal_sample.phase_gradient_per_m;
    let tide_sum_gradient =
        (-2.0 * libm::sin(lunar_angle) - 0.92 * libm::sin(solar_angle)) as f32 * angle_gradient;
    (0.5 / 1.46)
        * (tide_sum as f32 * tidal_sample.range_gradient_per_m
            + tidal_sample.conditions.reference_range_m * tide_sum_gradient)
}

fn procedural_wave_state(query: OceanQuery) -> OceanState {
    let wind_speed_m_s = query.wave_forcing_wind_velocity_m_s.length();
    let upper_wind_speed_m_s = query.weather.upper_wind_velocity_m_s.length();
    let wind_direction = if wind_speed_m_s > 0.0 {
        query.wave_forcing_wind_velocity_m_s / wind_speed_m_s
    } else {
        Vector::ZERO
    };
    let upper_wind_direction = if upper_wind_speed_m_s > 0.0 {
        query.weather.upper_wind_velocity_m_s / upper_wind_speed_m_s
    } else {
        Vector::ZERO
    };
    let wind_sea_height_m = 0.05 + 0.016 * wind_speed_m_s.powi(2);
    let swell_height_m = 0.006 * upper_wind_speed_m_s.powi(2);
    let unrestricted_height_m = libm::hypotf(wind_sea_height_m, swell_height_m);
    let significant_wave_height_m = unrestricted_height_m
        .min(0.78 * query.water_depth_m)
        .min(MAX_SIGNIFICANT_WAVE_HEIGHT_M);
    let depth_scale = if unrestricted_height_m > 0.0 {
        significant_wave_height_m / unrestricted_height_m
    } else {
        0.0
    };

    let wave_modes = ocean_wave_modes(
        query.world_geometry,
        query.match_seed,
        query.world_tick,
        query.world_tick_fraction,
    );
    let mut modes = [(0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64); OCEAN_WAVE_MODE_COUNT];
    let mut wind_weight_square_sum = 0.0;
    let mut swell_weight_square_sum = 0.0;
    for (mode_index, (mode, wave_mode)) in modes.iter_mut().zip(wave_modes).enumerate() {
        let is_wind_sea = mode_index < OCEAN_WIND_SEA_MODE_COUNT;
        let actual_direction_x = wave_mode.wave_number_x_per_m / wave_mode.wave_number_per_m;
        let actual_direction_y = wave_mode.wave_number_y_per_m / wave_mode.wave_number_per_m;
        let source_direction = if is_wind_sea {
            wind_direction
        } else {
            upper_wind_direction
        };
        let alignment = (actual_direction_x as f32 * source_direction.x
            + actual_direction_y as f32 * source_direction.y)
            .max(0.0);
        let weight = 0.20 + f64::from(alignment.powi(4));
        if is_wind_sea {
            wind_weight_square_sum += weight * weight;
        } else {
            swell_weight_square_sum += weight * weight;
        }
        let width_m = f64::from(query.world_geometry.width_m());
        let phase = wave_mode.wave_number_x_per_m
            * f64::from(query.position_m.x).rem_euclid(width_m)
            + wave_mode.wave_number_y_per_m * f64::from(query.position_m.y)
            + wave_mode.phase_rad;
        *mode = (
            actual_direction_x,
            actual_direction_y,
            wave_mode.wave_number_per_m,
            wave_mode.angular_frequency_rad_s,
            phase,
            weight,
        );
    }

    let wind_amplitude_scale = f64::from(depth_scale * wind_sea_height_m)
        / (4.0 * libm::sqrt(0.5 * wind_weight_square_sum));
    let swell_amplitude_scale =
        f64::from(depth_scale * swell_height_m) / (4.0 * libm::sqrt(0.5 * swell_weight_square_sum));
    let mut displacement_m = 0.0;
    let mut slope = Vector::ZERO;
    let mut orbital_velocity_m_s = Vector::ZERO;
    let mut energy_direction = Vector::ZERO;
    for (mode_index, mode) in modes.into_iter().enumerate() {
        let (direction_x, direction_y, wave_number, angular_frequency, phase, weight) = mode;
        let amplitude_m = weight
            * if mode_index < OCEAN_WIND_SEA_MODE_COUNT {
                wind_amplitude_scale
            } else {
                swell_amplitude_scale
            };
        let (phase_sine, phase_cosine) = libm::sincos(phase);
        displacement_m += amplitude_m * phase_cosine;
        slope += Vector::new(
            (-amplitude_m * phase_sine * wave_number * direction_x) as f32,
            (-amplitude_m * phase_sine * wave_number * direction_y) as f32,
        );
        orbital_velocity_m_s += Vector::new(
            (amplitude_m * angular_frequency * phase_cosine * direction_x) as f32,
            (amplitude_m * angular_frequency * phase_cosine * direction_y) as f32,
        );
        energy_direction += Vector::new(
            (amplitude_m * amplitude_m * direction_x) as f32,
            (amplitude_m * amplitude_m * direction_y) as f32,
        );
    }
    let nonlinear_strength = 0.5 * smoothstep_range(1.5, 3.0, f64::from(significant_wave_height_m));
    let significant_height_m = f64::from(significant_wave_height_m);
    let bound_displacement_m = if significant_height_m > 0.0 {
        nonlinear_strength
            * (displacement_m * displacement_m - significant_height_m * significant_height_m / 16.0)
            / significant_height_m
    } else {
        0.0
    };
    let bound_response = if significant_height_m > 0.0 {
        (1.0 + 2.0 * nonlinear_strength * displacement_m / significant_height_m) as f32
    } else {
        1.0
    };
    displacement_m += bound_displacement_m;
    slope *= bound_response;
    let dominant_wave_direction = if energy_direction == Vector::ZERO {
        Vector::ZERO
    } else {
        energy_direction.normalize()
    };
    let surface_displacement_m = displacement_m as f32;
    OceanState {
        current_velocity_m_s: Vector::ZERO,
        wave_orbital_velocity_m_s: orbital_velocity_m_s,
        significant_wave_height_m,
        surface_displacement_m,
        surface_slope: slope,
        dominant_wave_direction,
        rogue_crest_excess_m: (surface_displacement_m - significant_wave_height_m).max(0.0),
    }
}

pub fn ocean_wave_modes(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
) -> [OceanWaveMode; OCEAN_WAVE_MODE_COUNT] {
    let rotation = std::f64::consts::TAU * hash_unit(match_seed ^ 0x3c6e_f372_fe94_f82b);
    let width_m = f64::from(world_geometry.width_m());
    std::array::from_fn(|mode_index| {
        let mode_key = match_seed
            ^ 0xa409_3822_299f_31d0
            ^ (mode_index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        let direction_angle =
            rotation + std::f64::consts::TAU * mode_index as f64 / OCEAN_WAVE_MODE_COUNT as f64;
        let (direction_y, direction_x) = libm::sincos(direction_angle);
        let wavelength_fraction = hash_unit(mode_key ^ 0xe703_7ed1_a0b4_28db);
        let (minimum_wavelength_m, maximum_wavelength_m) = if mode_index < OCEAN_WIND_SEA_MODE_COUNT
        {
            (8.0_f64, 180.0_f64)
        } else {
            (120.0_f64, 600.0_f64)
        };
        let wavelength_m = minimum_wavelength_m
            * libm::pow(
                maximum_wavelength_m / minimum_wavelength_m,
                wavelength_fraction,
            );
        let mut mode_x = libm::round(direction_x * width_m / wavelength_m) as i64;
        if mode_x == 0 && direction_y.abs() < 1.0e-6 {
            mode_x = if direction_x < 0.0 { -1 } else { 1 };
        }
        let wave_number_x_per_m = std::f64::consts::TAU * mode_x as f64 / width_m;
        let wave_number_y_per_m = std::f64::consts::TAU * direction_y / wavelength_m;
        let wave_number_per_m = libm::hypot(wave_number_x_per_m, wave_number_y_per_m);
        let angular_frequency_rad_s = libm::sqrt(GRAVITY_M_S2 * wave_number_per_m);
        OceanWaveMode {
            wave_number_x_per_m,
            wave_number_y_per_m,
            wave_number_per_m,
            angular_frequency_rad_s,
            phase_rad: std::f64::consts::TAU * hash_unit(mode_key ^ 0xbf58_476d_1ce4_e5b9)
                - tick_phase_radians(world_tick, angular_frequency_rad_s * TICK_SECONDS)
                - f64::from(world_tick_fraction) * angular_frequency_rad_s * TICK_SECONDS,
        }
    })
}

fn tick_phase_radians(tick: u64, phase_per_tick_radians: f64) -> f64 {
    let mut phase = 0.0;
    for shift in [48, 32, 16, 0] {
        let chunk = ((tick >> shift) & 0xffff) as f64;
        phase =
            (phase * 65_536.0 + chunk * phase_per_tick_radians).rem_euclid(std::f64::consts::TAU);
    }
    phase
}

pub fn procedural_weather(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> ProceduralWeather {
    let celestial =
        world_celestial_conditions(world_geometry, tick as f64 * TICK_SECONDS, position_m);
    let climate = world_climate_conditions(celestial.geographic_latitude_degrees);
    let scalar_time = scalar_field_time_coordinates(tick);
    let vector_time = vector_field_time_coordinates(world_geometry, tick);
    derive_weather(
        procedural_atmosphere_primary_with_conditions(
            world_geometry,
            match_seed,
            position_m,
            climate,
            celestial,
            scalar_time,
            vector_time,
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
    let celestial =
        world_celestial_conditions(world_geometry, tick as f64 * TICK_SECONDS, position_m);
    let scalar_time = scalar_field_time_coordinates(tick);
    let vector_time = vector_field_time_coordinates(world_geometry, tick);
    procedural_atmosphere_primary_with_conditions(
        world_geometry,
        match_seed,
        position_m,
        world_climate_conditions(celestial.geographic_latitude_degrees),
        celestial,
        scalar_time,
        vector_time,
    )
}

fn procedural_atmosphere_primary_with_conditions(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
    climate: ClimateConditions,
    celestial: CelestialConditions,
    scalar_time: ScalarFieldTimeCoordinates,
    vector_time: VectorFieldTimeCoordinates,
) -> AtmospherePrimary {
    let pressure =
        procedural_pressure_sample_at_time(world_geometry, match_seed, vector_time, position_m);
    let surface_temperature = sample_surface_air_temperature_anomaly_c(
        world_geometry,
        match_seed,
        scalar_time.air_temperature,
        position_m,
    );
    let surface_air_temperature_anomaly_c = surface_temperature.value;
    let air_temperature_500_hpa_c = current_500_hpa_air_temperature_c(climate, celestial)
        + SURFACE_AIR_MASS_COUPLING_500_HPA * surface_air_temperature_anomaly_c
        + INDEPENDENT_AIR_TEMPERATURE_500_HPA_RANGE_C
            * procedural_weather_noise(
                world_geometry,
                match_seed,
                position_m,
                AIR_TEMPERATURE_500_HPA_NOISE,
                scalar_time.upper_air_temperature,
            ) as f32;
    let climate_air_temperature_c = current_air_temperature_c(
        ClimateConditions {
            diurnal_temperature_range_c: 0.0,
            ..climate
        },
        celestial,
    );
    let climate_saturation_capacity_g_kg =
        water_vapor_saturation_capacity_g_kg(climate_air_temperature_c);
    let water_vapor_fraction = 0.72
        + 0.65
            * procedural_weather_noise(
                world_geometry,
                match_seed,
                position_m,
                WATER_VAPOR_NOISE,
                scalar_time.water_vapor,
            ) as f32;
    let water_vapor_g_kg =
        (climate_saturation_capacity_g_kg * water_vapor_fraction).clamp(0.5, 25.0);

    AtmospherePrimary {
        pressure_hpa: 1013.25
            + pressure.potential_m2_s as f32 / PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M,
        pressure_gradient_hpa_per_m: pressure.gradient_m_s / PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M,
        synoptic_pressure_gradient_hpa_per_m: pressure.synoptic_gradient_m_s
            / PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M,
        surface_gust_signal: pressure.surface_gust_signal,
        surface_air_temperature_anomaly_c,
        air_temperature_500_hpa_c,
        local_temperature_gradient_c_per_m: surface_temperature.gradient_per_m,
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
    let resolved_surface_wind_m_s = wind_from_pressure_gradient(
        primary.pressure_gradient_hpa_per_m,
        conditions.geographic_latitude_degrees,
    );
    let upper_pressure_wind_m_s = UPPER_WIND_SCALE
        * wind_from_pressure_gradient(
            primary.synoptic_pressure_gradient_hpa_per_m,
            conditions.geographic_latitude_degrees,
        );
    let thermal_wind_m_s = THERMAL_WIND_SCALE_M2_S_C
        * Vector::new(
            -primary.local_temperature_gradient_c_per_m.y,
            primary.local_temperature_gradient_c_per_m.x,
        );
    let upper_wind_velocity_m_s = upper_pressure_wind_m_s + thermal_wind_m_s;
    let saturation_capacity_g_kg = water_vapor_saturation_capacity_g_kg(surface_air_temperature_c);
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
        f64::from(primary.local_temperature_gradient_c_per_m.length()),
    ) as f32;
    let lift = (0.25 * pressure_lift + 0.55 * gradient_lift + 0.20 * frontal_lift).clamp(0.0, 1.0);
    let condensation = smoothstep_range(0.45, 0.95, f64::from(relative_humidity)) as f32;
    // ECMWF diagnoses peak 10 m gust speed from a surface-stress/stability term
    // plus 0.6 of positive low-level speed shear when deep convection is active.
    // This model lacks friction velocity, boundary-layer stability, 850/950 hPa
    // winds, and a convection scheme, so sustained wind, column instability,
    // condensation, and lift are their deterministic local proxies.
    let mechanical_stability_factor = 1.0 + 0.35 * instability;
    let mechanical_gust_speed_m_s = SURFACE_GUST_DRAG_INCREMENT
        * mechanical_stability_factor
        * resolved_surface_wind_m_s.length();
    let convective_activity = instability
        * smoothstep_range(0.65, 0.95, f64::from(relative_humidity)) as f32
        * smoothstep_range(0.35, 0.80, f64::from(lift)) as f32;
    let convective_gust_speed_m_s = CONVECTIVE_GUST_SHEAR_FRACTION
        * convective_activity
        * (upper_wind_velocity_m_s.length() - resolved_surface_wind_m_s.length()).max(0.0);
    let mechanical_gust_velocity_m_s = if resolved_surface_wind_m_s == Vector::ZERO {
        Vector::ZERO
    } else {
        mechanical_gust_speed_m_s * resolved_surface_wind_m_s.normalize()
    };
    let convective_gust_velocity_m_s = if upper_wind_velocity_m_s == Vector::ZERO {
        Vector::ZERO
    } else {
        convective_gust_speed_m_s * upper_wind_velocity_m_s.normalize()
    };
    let surface_gust_velocity_m_s =
        primary.surface_gust_signal * (mechanical_gust_velocity_m_s + convective_gust_velocity_m_s);
    let wind_velocity_m_s = resolved_surface_wind_m_s + surface_gust_velocity_m_s;
    let cloud_signal = 0.68 * condensation + 0.22 * lift + 0.10 * instability * condensation;
    let cloudiness = smoothstep_range(0.50, 1.01, f64::from(cloud_signal)) as f32;
    let upper_cloud_signal = 0.55 * condensation + 0.30 * lift + 0.15 * frontal_lift;
    let upper_cloudiness = smoothstep_range(0.68, 0.90, f64::from(upper_cloud_signal)) as f32;
    let rain_signal = smoothstep_range(0.65, 0.75, f64::from(cloudiness)) as f32
        * cloudiness
        * (0.45 + 0.20 * instability + 0.45 * lift);

    ProceduralWeather {
        primary,
        surface_air_temperature_c,
        wind_velocity_m_s,
        surface_gust_velocity_m_s,
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
    time_coordinate: f32,
    position_m: Vector,
) -> TemperatureSample {
    let noise = procedural_weather_noise_sample(
        world_geometry,
        match_seed,
        position_m,
        SURFACE_AIR_TEMPERATURE_NOISE,
        time_coordinate,
    );
    let latitude = world_geometry.latitude_degrees(position_m.y);
    let scale_c = latitude_curve_value(TEMPERATURE_ANOMALY_SCALE_C, latitude);
    // The latitude amplitude envelope is statistical calibration, not a physical
    // front; only the local anomaly gradient drives thermal wind and frontal lift.
    TemperatureSample {
        value: scale_c * noise.value as f32,
        gradient_per_m: Vector::new(
            (f64::from(scale_c) * noise.gradient_x_per_m) as f32,
            (f64::from(scale_c) * noise.gradient_y_per_m) as f32,
        ),
    }
}

fn wind_from_pressure_gradient(
    pressure_gradient_hpa_per_m: Vector,
    latitude_degrees: f32,
) -> Vector {
    let gradient_length = pressure_gradient_hpa_per_m.length();
    if gradient_length == 0.0 {
        return Vector::ZERO;
    }
    let latitude_sine = latitude_degrees.to_radians().sin();
    // This bounded fade avoids the equatorial singularity of a geostrophic
    // solve. Its latitude width is gameplay tuning, not a physical Coriolis rate.
    let normalized_coriolis = ((latitude_sine.abs() - 0.02) / 0.23).clamp(0.0, 1.0);
    let geostrophic_amount = normalized_coriolis.powi(2) * (3.0 - 2.0 * normalized_coriolis);
    let hemisphere = if latitude_sine < 0.0 { -1.0 } else { 1.0 };
    let along_isobar = hemisphere
        * Vector::new(
            -pressure_gradient_hpa_per_m.y,
            pressure_gradient_hpa_per_m.x,
        );
    let toward_low = -pressure_gradient_hpa_per_m;
    let flow = geostrophic_amount * along_isobar
        + (1.0 - geostrophic_amount * (1.0 - SURFACE_CROSS_ISOBAR_FRACTION)) * toward_low;
    PRESSURE_WIND_SCALE_M_S_PER_HPA_PER_M * gradient_length * flow.normalize()
}

pub fn procedural_terrain_elevation_m(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
) -> f32 {
    procedural_terrain_surface_causes(world_geometry, match_seed, position_m).elevation_m
}

#[derive(Clone, Copy)]
struct TerrainSurfaceCauses {
    elevation_m: f32,
    maritime_temperature_anomaly_c: f32,
}

fn procedural_terrain_surface_causes(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
) -> TerrainSurfaceCauses {
    // Keep this composition synchronized with `procedural_terrain_sample` in
    // `crates/browser/src/shader/components/terrain.wgsl`: Rust supplies physical
    // depth while WGSL renders the same shoreline.
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
    let polar_taper = 1.0
        - smoothstep_range(
            75.0,
            90.0,
            f64::from(world_geometry.latitude_degrees(position_m.y).abs()),
        );
    let maritime_temperature_anomaly_c = (16.0 * polar_taper * cluster) as f32;
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
        return TerrainSurfaceCauses {
            elevation_m: terrain_elevation_m as f32,
            maritime_temperature_anomaly_c,
        };
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
    TerrainSurfaceCauses {
        elevation_m: (terrain_elevation_m
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
        maritime_temperature_anomaly_c,
    }
}

fn broad_landform_elevation_m(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
) -> f32 {
    // Keep this synchronized with `broad_landform_elevation_m` in
    // `crates/browser/src/shader/components/terrain.wgsl`.
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
    water_temperature_c: f32,
) -> f32 {
    let terrain = procedural_terrain_surface_causes(world_geometry, match_seed, position_m);
    solid_surface_elevation_from_bedrock_m(
        terrain.elevation_m,
        annual_mean_air_temperature_c,
        water_temperature_c + terrain.maritime_temperature_anomaly_c,
    )
}

fn solid_surface_elevation_from_bedrock_m(
    bedrock_elevation_m: f32,
    annual_mean_air_temperature_c: f32,
    effective_water_temperature_c: f32,
) -> f32 {
    let annual_temperature_c =
        annual_mean_air_temperature_c - 0.0065 * bedrock_elevation_m.max(0.0);
    let cold = 1.0 - smoothstep_range(-12.0, -8.0, f64::from(annual_temperature_c)) as f32;
    if bedrock_elevation_m < 0.0 {
        let cover = water_ice_cover(bedrock_elevation_m, effective_water_temperature_c);
        return if cover > 0.5 {
            0.25 + 1.75 * cover
        } else {
            bedrock_elevation_m
        };
    }
    let ice_surface_m =
        (20.0 * cold + 0.25 * bedrock_elevation_m.max(0.0)).max(bedrock_elevation_m + 4.0 * cold);
    bedrock_elevation_m.max(bedrock_elevation_m + cold * (ice_surface_m - bedrock_elevation_m))
}

pub fn procedural_cloud_cover(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> f32 {
    procedural_weather(world_geometry, match_seed, tick, position_m).cloudiness
}

pub fn scalar_field_time_coordinates(tick: u64) -> ScalarFieldTimeCoordinates {
    scalar_field_time_coordinates_interpolated(tick, 0.0)
}

pub fn scalar_field_time_coordinates_interpolated(
    tick: u64,
    tick_fraction: f32,
) -> ScalarFieldTimeCoordinates {
    ScalarFieldTimeCoordinates {
        air_temperature: noise_time_coordinate_interpolated(
            tick,
            tick_fraction,
            SURFACE_AIR_TEMPERATURE_NOISE.evolution_speed_m_s,
            SURFACE_AIR_TEMPERATURE_NOISE.wavelength_m,
        ),
        upper_air_temperature: noise_time_coordinate_interpolated(
            tick,
            tick_fraction,
            AIR_TEMPERATURE_500_HPA_NOISE.evolution_speed_m_s,
            AIR_TEMPERATURE_500_HPA_NOISE.wavelength_m,
        ),
        water_vapor: noise_time_coordinate_interpolated(
            tick,
            tick_fraction,
            WATER_VAPOR_NOISE.evolution_speed_m_s,
            WATER_VAPOR_NOISE.wavelength_m,
        ),
        decorative_water: noise_time_coordinate_interpolated(
            tick,
            tick_fraction,
            DECORATIVE_WATER_EVOLUTION_SPEED_M_S,
            DECORATIVE_WATER_WAVELENGTH_M,
        ),
    }
}

pub fn cloud_field_time_coordinates(tick: u64) -> CloudFieldTimeCoordinates {
    cloud_field_time_coordinates_interpolated(tick, 0.0)
}

pub fn cloud_field_time_coordinates_interpolated(
    tick: u64,
    tick_fraction: f32,
) -> CloudFieldTimeCoordinates {
    CloudFieldTimeCoordinates {
        low: noise_time_coordinate_interpolated(
            tick,
            tick_fraction,
            1.0,
            LOW_CLOUD_VISUAL_WAVELENGTH_M,
        ),
        upper: noise_time_coordinate_interpolated(
            tick,
            tick_fraction,
            1.0,
            WEATHER_CANOPY_VISUAL_WAVELENGTH_M,
        ),
    }
}

pub fn vector_field_time_coordinates(
    world_geometry: WorldGeometry,
    tick: u64,
) -> VectorFieldTimeCoordinates {
    vector_field_time_coordinates_interpolated(world_geometry, tick, 0.0)
}

pub fn vector_field_time_coordinates_interpolated(
    world_geometry: WorldGeometry,
    tick: u64,
    tick_fraction: f32,
) -> VectorFieldTimeCoordinates {
    VectorFieldTimeCoordinates {
        pressure: WIND_BANDS.map(|band| {
            let wavelength_m = if band.wavelength_m == 0.0 {
                libm::sqrtf(
                    CLIMATE_PRESSURE_ZONAL_WAVELENGTH_M
                        * CLIMATE_PRESSURE_MERIDIONAL_WORLD_FRACTION
                        * world_geometry.height_m(),
                )
            } else {
                band.wavelength_m
            };
            noise_time_coordinate_interpolated(
                tick,
                tick_fraction,
                PRESSURE_EVOLUTION_SPEED_M_S,
                wavelength_m,
            )
        }),
        ocean_current: noise_time_coordinate_interpolated(
            tick,
            tick_fraction,
            OCEAN_CURRENT_EVOLUTION_SPEED_M_S,
            world_geometry.width_m() * OCEAN_CURRENT_WORLD_FRACTION as f32,
        ),
    }
}

fn procedural_weather_noise(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
    parameters: WeatherNoiseParameters,
    time: f32,
) -> f64 {
    parameters.scale
        * spatiotemporal_fractal_noise_sample(
            world_geometry,
            match_seed ^ parameters.salt,
            position_m,
            parameters.wavelength_m,
            time,
            parameters.octave_count,
            0.5,
        )
        .value
}

fn procedural_weather_noise_sample(
    world_geometry: WorldGeometry,
    match_seed: u64,
    position_m: Vector,
    parameters: WeatherNoiseParameters,
    time: f32,
) -> ScalarNoiseSample {
    let mut sample = spatiotemporal_fractal_noise_sample(
        world_geometry,
        match_seed ^ parameters.salt,
        position_m,
        parameters.wavelength_m,
        time,
        parameters.octave_count,
        0.5,
    );
    sample.value *= parameters.scale;
    sample.gradient_x_per_m *= parameters.scale;
    sample.gradient_y_per_m *= parameters.scale;
    sample
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
            PeriodicNoiseBasis::LongitudeCircleSimplex4d
        } else {
            PeriodicNoiseBasis::LongitudePeriodicTriangularSimplex2d
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
        PeriodicNoiseBasis::LongitudePeriodicTriangularSimplex2d => {
            terrain_simplex_noise_2d(world_geometry, field_seed, position_m, wavelength_m)
        }
        PeriodicNoiseBasis::LongitudeCircleSimplex4d => {
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

    let wrapped_x_m = world_geometry.wrap_position_m([position_m.x, position_m.y])[0];
    let horizontal_modes = ((f64::from(world_geometry.width_m())
        / (f64::from(wavelength_m) * std::f64::consts::SQRT_2))
        .round()
        .clamp(1.0, f64::from(i32::MAX / 4))) as i32;
    let horizontal_phase = wrapped_x_m.rem_euclid(world_geometry.width_m())
        * horizontal_modes as f32
        / world_geometry.width_m();
    let vertical_phase = position_m.y / (std::f32::consts::SQRT_2 * wavelength_m);
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
        let lattice_gradient =
            terrain_simplex_lattice_gradient(field_seed, lattice_x, lattice_y, horizontal_modes);
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
    let wrapped_x_m = world_geometry.wrap_position_m([position_m.x, position_m.y])[0];
    let angle_x = std::f32::consts::TAU * wrapped_x_m / world_geometry.width_m();
    let (sin_x, cos_x) = (libm::sinf(angle_x), libm::cosf(angle_x));
    let circle_x = world_geometry.width_m() / (std::f32::consts::TAU * wavelength_m);
    let linear_y = position_m.y / (std::f32::consts::SQRT_2 * wavelength_m);
    let point = [circle_x * cos_x, circle_x * sin_x, linear_y, linear_y + 0.5];
    let (value, input_gradient) = simplex_noise_sample_4d(field_seed, point);
    // The linear north-south slice needs this scale to retain the calibrated
    // broad-terrain occupancy and coastal-slope distribution.
    let cylindrical_scale = 35.7 / 33.4;
    let gradient_per_m = cylindrical_scale
        * Vector::new(
            (-sin_x * input_gradient[0] + cos_x * input_gradient[1]) / wavelength_m,
            (input_gradient[2] + input_gradient[3]) / (std::f32::consts::SQRT_2 * wavelength_m),
        );
    (cylindrical_scale * value, gradient_per_m)
}

fn spatiotemporal_simplex_noise_4d(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_m: Vector,
    wavelength_m: f32,
    time_coordinate: f32,
) -> (f32, Vector) {
    let wrapped_x_m = world_geometry.wrap_position_m([position_m.x, position_m.y])[0];
    let orientation = wind_hash(field_seed ^ 0x6a09_e667_f3bc_c909) as u32;
    let longitude_offset = std::f32::consts::TAU * (orientation & 0xff) as f32 / 256.0;
    let latitude_sign = if orientation & 0x100 == 0 { 1.0 } else { -1.0 };
    let latitude_offset = ((orientation >> 9) & 0xff) as f32 / 16.0;
    // Independent phase keeps the ensemble stationary as the shared clock advances.
    let time_offset = ((orientation >> 17) & 0x7fff) as f32 / 64.0;
    let longitude =
        std::f32::consts::TAU * wrapped_x_m / world_geometry.width_m() + longitude_offset;
    let (sin_longitude, cos_longitude) = (libm::sinf(longitude), libm::cosf(longitude));
    let circle_radius = world_geometry.width_m() / (std::f32::consts::TAU * wavelength_m);
    let point = [
        circle_radius * cos_longitude,
        circle_radius * sin_longitude,
        latitude_sign * position_m.y / wavelength_m + latitude_offset,
        time_coordinate + time_offset,
    ];
    let (value, input_gradient) = simplex_noise_sample_4d(field_seed, point);
    (
        value,
        Vector::new(
            (-sin_longitude * input_gradient[0] + cos_longitude * input_gradient[1]) / wavelength_m,
            latitude_sign * input_gradient[2] / wavelength_m,
        ),
    )
}

#[allow(clippy::too_many_arguments)]
fn spatiotemporal_pressure_noise_4d(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_m: Vector,
    zonal_wavelength_m: f32,
    meridional_wavelength_m: f32,
    time_coordinate: f32,
    zonal_aspect: f32,
    zonal_aspect_gradient_per_m: f32,
) -> (f32, Vector) {
    let wrapped_x_m = world_geometry.wrap_position_m([position_m.x, position_m.y])[0];
    let orientation = wind_hash(field_seed ^ 0x6a09_e667_f3bc_c909) as u32;
    let longitude_offset = std::f32::consts::TAU * (orientation & 0xff) as f32 / 256.0;
    let latitude_sign = if orientation & 0x100 == 0 { 1.0 } else { -1.0 };
    let latitude_offset = ((orientation >> 9) & 0xff) as f32 / 16.0;
    let time_offset = ((orientation >> 17) & 0x7fff) as f32 / 64.0;
    let longitude =
        std::f32::consts::TAU * wrapped_x_m / world_geometry.width_m() + longitude_offset;
    let (sin_longitude, cos_longitude) = (libm::sinf(longitude), libm::cosf(longitude));
    let circle_radius =
        world_geometry.width_m() / (std::f32::consts::TAU * zonal_wavelength_m * zonal_aspect);
    let circle_radius_gradient_per_m = -circle_radius * zonal_aspect_gradient_per_m / zonal_aspect;
    let point = [
        circle_radius * cos_longitude,
        circle_radius * sin_longitude,
        latitude_sign * position_m.y / meridional_wavelength_m + latitude_offset,
        time_coordinate + time_offset,
    ];
    let (value, input_gradient) = simplex_noise_sample_4d(field_seed, point);
    (
        value,
        Vector::new(
            (-sin_longitude * input_gradient[0] + cos_longitude * input_gradient[1])
                / (zonal_wavelength_m * zonal_aspect),
            circle_radius_gradient_per_m
                * (cos_longitude * input_gradient[0] + sin_longitude * input_gradient[1])
                + latitude_sign * input_gradient[2] / meridional_wavelength_m,
        ),
    )
}

fn simplex_noise_sample_4d(field_seed: u64, point: [f32; 4]) -> (f32, [f32; 4]) {
    const SKEW: f32 = 0.309_017;
    const UNSKEW: f32 = 0.138_196_6;
    // Matches the portable 2D kernel's RMS value over representative seeds.
    const NORMALIZATION: f32 = 33.4;

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

    (
        NORMALIZATION * value,
        input_gradient.map(|component| NORMALIZATION * component),
    )
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
) -> Vector {
    // x-y is the longitude axis of the triangular lattice. Wrapping only this
    // residue preserves the cylindrical seam without repeating latitude.
    let horizontal_lattice =
        (i64::from(x) - i64::from(y)).rem_euclid(2 * i64::from(horizontal_modes));
    let vertical_lattice = i64::from(x) + i64::from(y);
    let key = field_seed
        ^ (horizontal_lattice as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)
        ^ (vertical_lattice as u64).wrapping_mul(0xe703_7ed1_a0b4_28db);
    let index = (wind_hash(key) as u32 % SIMPLEX_GRADIENT_DIRECTIONS.len() as u32) as usize;
    let (gradient_x, gradient_y) = SIMPLEX_GRADIENT_DIRECTIONS[index];
    Vector::new(gradient_x, gradient_y)
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
                PeriodicNoiseBasis::LongitudeCircleSimplex4d
            } else {
                PeriodicNoiseBasis::LongitudePeriodicTriangularSimplex2d
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
        PeriodicNoiseBasis::LongitudePeriodicTriangularSimplex2d => {
            periodic_simplex_noise_sample_f64(
                world_geometry,
                field_seed,
                position_x_m,
                position_y_m,
                wavelength_m,
            )
        }
        PeriodicNoiseBasis::LongitudeCircleSimplex4d => cylindrical_simplex_noise_sample_f64(
            world_geometry,
            field_seed,
            position_x_m,
            position_y_m,
            wavelength_m,
        ),
    }
}

fn cylindrical_simplex_noise_sample_f64(
    world_geometry: WorldGeometry,
    field_seed: u64,
    position_x_m: f64,
    position_y_m: f64,
    wavelength_m: f64,
) -> ScalarNoiseSample {
    const SKEW: f64 = 0.309_016_994_374_947_45;
    const UNSKEW: f64 = 0.138_196_601_125_010_5;
    const NORMALIZATION: f64 = 35.7;

    let width_m = f64::from(world_geometry.width_m());
    let angle_x = std::f64::consts::TAU * position_x_m.rem_euclid(width_m) / width_m;
    let (sin_x, cos_x) = (libm::sin(angle_x), libm::cos(angle_x));
    let circle_x = width_m / (std::f64::consts::TAU * wavelength_m);
    let linear_y = position_y_m / (std::f64::consts::SQRT_2 * wavelength_m);
    let point = [circle_x * cos_x, circle_x * sin_x, linear_y, linear_y + 0.5];
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
        gradient_y_per_m: NORMALIZATION * (input_gradient[2] + input_gradient[3])
            / (std::f64::consts::SQRT_2 * wavelength_m),
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
    const NORMALIZATION: f64 = 70.0;

    let width_m = f64::from(world_geometry.width_m());
    let horizontal_modes = (width_m / (std::f64::consts::SQRT_2 * wavelength_m))
        .round()
        .clamp(1.0, i64::MAX as f64 / 4.0) as i64;
    let horizontal_phase = position_x_m.rem_euclid(width_m) * horizontal_modes as f64 / width_m;
    let vertical_phase = position_y_m / (std::f64::consts::SQRT_2 * wavelength_m);
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
    let vertical_scale_per_m = 1.0 / (std::f64::consts::SQRT_2 * wavelength_m);
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
) -> (f64, f64) {
    let horizontal_lattice = (x - y).rem_euclid(2 * horizontal_modes);
    let vertical_lattice = x + y;
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
    procedural_weather(world_geometry, match_seed, tick, position_m).wind_velocity_m_s
}

#[cfg(test)]
fn procedural_pressure_sample(
    world_geometry: WorldGeometry,
    match_seed: u64,
    tick: u64,
    position_m: Vector,
) -> PressureSample {
    let time = vector_field_time_coordinates(world_geometry, tick);
    procedural_pressure_sample_at_time(world_geometry, match_seed, time, position_m)
}

fn procedural_pressure_sample_at_time(
    world_geometry: WorldGeometry,
    match_seed: u64,
    time: VectorFieldTimeCoordinates,
    position_m: Vector,
) -> PressureSample {
    let latitude_rad = world_geometry.latitude_degrees(position_m.y).to_radians();
    let midlatitude_sine = libm::sinf(2.0 * latitude_rad);
    let midlatitude = midlatitude_sine * midlatitude_sine;
    let latitude_gradient_rad_per_m = world_geometry
        .latitude_gradient_degrees_per_m(position_m.y)
        .to_radians();
    let midlatitude_gradient_per_m =
        2.0 * libm::sinf(4.0 * latitude_rad) * latitude_gradient_rad_per_m;
    let mut potential_m2_s = 0.0_f32;
    let mut gradient_m_s = Vector::ZERO;
    let mut synoptic_gradient_m_s = Vector::ZERO;
    let mut surface_gust_signal = 0.0;
    for (band_index, band) in WIND_BANDS.into_iter().enumerate() {
        let (zonal_wavelength_m, meridional_wavelength_m) = if band.wavelength_m == 0.0 {
            (
                CLIMATE_PRESSURE_ZONAL_WAVELENGTH_M,
                CLIMATE_PRESSURE_MERIDIONAL_WORLD_FRACTION * world_geometry.height_m(),
            )
        } else {
            (band.wavelength_m, band.wavelength_m)
        };
        let variance_fraction =
            band.variance_fraction + band.midlatitude_variance_transfer * midlatitude;
        let variance_scale = libm::sqrtf(variance_fraction);
        let variance_scale_gradient_per_m =
            0.5 * band.midlatitude_variance_transfer * midlatitude_gradient_per_m / variance_scale;
        let zonal_aspect = 1.0 + (band.midlatitude_zonal_aspect - 1.0) * midlatitude;
        let zonal_aspect_gradient_per_m =
            (band.midlatitude_zonal_aspect - 1.0) * midlatitude_gradient_per_m;
        let noise = spatiotemporal_pressure_noise_4d(
            world_geometry,
            match_seed ^ band.noise_salt,
            position_m,
            zonal_wavelength_m,
            meridional_wavelength_m,
            time.pressure[band_index],
            zonal_aspect,
            zonal_aspect_gradient_per_m,
        );
        if band_index == WIND_BANDS.len() - 1 {
            surface_gust_signal = smoothstep_range(0.05, 0.65, f64::from(noise.0)) as f32;
            continue;
        }
        let total_zonal_aspect = zonal_wavelength_m * zonal_aspect / meridional_wavelength_m;
        let total_zonal_aspect_gradient_per_m =
            zonal_wavelength_m * zonal_aspect_gradient_per_m / meridional_wavelength_m;
        let zonal_aspect_squared = total_zonal_aspect * total_zonal_aspect;
        let anisotropy_normalization = libm::sqrtf(2.0 / (1.0 + 1.0 / zonal_aspect_squared));
        let anisotropy_normalization_gradient_per_m = anisotropy_normalization
            * total_zonal_aspect_gradient_per_m
            / (total_zonal_aspect * (zonal_aspect_squared + 1.0));
        let base_potential_scale_m2_s =
            PRESSURE_SIMPLEX_SCALE * WIND_WEIBULL_SCALE_M_S as f32 * meridional_wavelength_m;
        let potential_scale_m2_s =
            base_potential_scale_m2_s * variance_scale * anisotropy_normalization;
        let potential_scale_gradient_m_s = base_potential_scale_m2_s
            * (variance_scale_gradient_per_m * anisotropy_normalization
                + variance_scale * anisotropy_normalization_gradient_per_m);
        let band_gradient_m_s = potential_scale_m2_s * noise.1
            + Vector::new(0.0, potential_scale_gradient_m_s * noise.0);
        potential_m2_s += potential_scale_m2_s * noise.0;
        gradient_m_s += band_gradient_m_s;
        synoptic_gradient_m_s += band.synoptic_coupling * band_gradient_m_s;
    }
    // NOAA's idealized global circulation has an equatorial low, subtropical
    // highs near 30°, subpolar lows near 60°, and polar highs. Scaling potential
    // with world height keeps the climatological wind gradient fixed as geometry
    // changes; evolving seeded noise breaks up the belts.
    let zonal_phase = 6.0 * latitude_rad;
    let zonal_potential_scale_m2_s =
        PLANETARY_ZONAL_WIND_PEAK_M_S * world_geometry.height_m() / (6.0 * std::f32::consts::PI);
    let zonal_potential_m2_s = -zonal_potential_scale_m2_s * libm::cosf(zonal_phase);
    let zonal_gradient_m_s =
        Vector::new(0.0, PLANETARY_ZONAL_WIND_PEAK_M_S * libm::sinf(zonal_phase));
    potential_m2_s += zonal_potential_m2_s;
    gradient_m_s += zonal_gradient_m_s;
    synoptic_gradient_m_s += zonal_gradient_m_s;
    PressureSample {
        potential_m2_s: f64::from(potential_m2_s),
        gradient_m_s,
        synoptic_gradient_m_s,
        surface_gust_signal,
    }
}

fn noise_time_coordinate_interpolated(
    tick: u64,
    tick_fraction: f32,
    evolution_speed_m_s: f32,
    wavelength_m: f32,
) -> f32 {
    const TIME_COORDINATE_PERIOD: f64 = 8192.0;
    let coordinate_per_tick =
        TICK_SECONDS * f64::from(evolution_speed_m_s) / f64::from(wavelength_m);
    let mut coordinate = 0.0;
    // Horner reduction incorporates the full tick without first rounding the
    // absolute clock to floating point.
    for shift in [48, 32, 16, 0] {
        let chunk = ((tick >> shift) & 0xffff) as f64;
        coordinate = (coordinate * 65_536.0 + chunk * coordinate_per_tick) % TIME_COORDINATE_PERIOD;
    }
    coordinate =
        (coordinate + f64::from(tick_fraction) * coordinate_per_tick) % TIME_COORDINATE_PERIOD;
    coordinate as f32
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
    fn elevated_cloud_layers_see_the_sun_below_the_surface_horizon() {
        let world = WorldGeometry::default();

        assert_eq!(
            celestial_disc_visibility_at_height(world, 0.0, -0.20, SUN_ANGULAR_RADIUS_RAD as f32,),
            0.0,
        );
        assert_eq!(
            celestial_disc_visibility_at_height(world, 900.0, -0.20, SUN_ANGULAR_RADIUS_RAD as f32,),
            1.0,
        );
        assert_eq!(
            celestial_disc_visibility_at_height(world, 900.0, -0.50, SUN_ANGULAR_RADIUS_RAD as f32,),
            0.0,
        );
        assert_eq!(
            celestial_disc_visibility_at_height(
                world,
                3_600.0,
                -0.50,
                SUN_ANGULAR_RADIUS_RAD as f32,
            ),
            1.0,
        );
    }

    #[test]
    fn partial_solar_disc_uses_a_ray_that_clears_the_planet() {
        let world = WorldGeometry::default();
        let height_m = 900.0;
        let world_radius_m = world.width_m() / std::f32::consts::TAU;
        let observer_radius_m = world_radius_m + height_m;
        let horizon_z = -(1.0 - (world_radius_m / observer_radius_m).powi(2)).sqrt();
        let center_z = horizon_z - 0.5 * SUN_ANGULAR_RADIUS_RAD as f32;

        let sampled_z = celestial_visible_disc_sample_direction_z_at_height(
            world,
            height_m,
            center_z,
            SUN_ANGULAR_RADIUS_RAD as f32,
        );
        let closest_ray_radius_m = observer_radius_m * (1.0 - sampled_z * sampled_z).sqrt();

        assert!(sampled_z > horizon_z, "{sampled_z} <= {horizon_z}");
        assert!(closest_ray_radius_m >= world_radius_m);
    }

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
    fn scalar_field_time_coordinates_preserve_late_tick_evolution() {
        let world = WorldGeometry::default();
        let before = scalar_field_time_coordinates(u64::MAX - 216_000);
        let after = scalar_field_time_coordinates(u64::MAX);
        assert_ne!(before.air_temperature, after.air_temperature);
        assert_ne!(before.upper_air_temperature, after.upper_air_temperature);
        assert_ne!(before.water_vapor, after.water_vapor);
        assert_ne!(before.decorative_water, after.decorative_water);

        let before = vector_field_time_coordinates(world, u64::MAX - 216_000);
        let after = vector_field_time_coordinates(world, u64::MAX);
        assert_ne!(before.pressure, after.pressure);
        assert_ne!(before.ocean_current, after.ocean_current);
    }

    #[test]
    fn render_clock_environment_matches_the_integer_simulation_clock() {
        let world = WorldGeometry::default();
        let position = Vector::new(123.0, -456.0);
        let tick = 12_345;

        assert_eq!(
            procedural_world_environment(world, 42, tick, position),
            procedural_world_environment_for_render_clocks(
                world, 42, tick, 0.0, tick, 0.0, tick, 0.0, tick, 0.0, position,
            ),
        );
    }

    #[test]
    fn render_clock_uses_exact_sixtieths_after_one_year() {
        let world = WorldGeometry::default();
        let position = Vector::new(123.0, -456.0);
        let seconds = 365_u64 * 86_400;
        let tick = seconds * 60;
        let rendered = procedural_world_environment_for_render_clocks(
            world, 42, tick, 0.0, tick, 0.0, tick, 0.0, tick, 0.0, position,
        );

        assert_eq!(
            rendered.celestial,
            world_celestial_conditions(world, seconds as f64, position),
        );
    }

    #[test]
    fn only_climate_pressure_evolution_scales_with_world_height() {
        let tick = 12_345_678;
        let reference = WorldGeometry::new(32_000.0, 24_000.0).unwrap();
        let wider = WorldGeometry::new(256_000.0, 24_000.0).unwrap();
        let taller = WorldGeometry::new(32_000.0, 192_000.0).unwrap();

        let reference = vector_field_time_coordinates(reference, tick).pressure;
        let wider = vector_field_time_coordinates(wider, tick).pressure;
        let taller = vector_field_time_coordinates(taller, tick).pressure;

        assert_eq!(reference, wider);
        assert_ne!(reference[0], taller[0]);
        assert_eq!(reference[1..], taller[1..]);
    }

    #[test]
    fn midlatitude_pressure_systems_are_zonally_elongated() {
        let world = WorldGeometry::default();
        let center = Vector::new(1_234.0, 0.25 * world.height_m());
        let separation_m = 2_400.0;
        let offsets = [
            Vector::ZERO,
            Vector::new(separation_m, 0.0),
            Vector::new(0.0, separation_m),
        ];
        let mut sums = [0.0; 3];
        let mut square_sums = [0.0; 3];
        let mut product_sums = [0.0; 2];
        let sample_count = 4_096;

        for seed in 0..sample_count {
            let pressures = offsets.map(|offset| {
                procedural_pressure_sample(world, seed, 1_234_567, center + offset).potential_m2_s
            });
            for index in 0..pressures.len() {
                sums[index] += pressures[index];
                square_sums[index] += pressures[index] * pressures[index];
            }
            product_sums[0] += pressures[0] * pressures[1];
            product_sums[1] += pressures[0] * pressures[2];
        }

        let count = sample_count as f64;
        let variances = [0, 1, 2].map(|index| {
            square_sums[index] / count - (sums[index] / count) * (sums[index] / count)
        });
        let correlations = [1, 2].map(|index| {
            let covariance =
                product_sums[index - 1] / count - sums[0] * sums[index] / (count * count);
            covariance / libm::sqrt(variances[0] * variances[index])
        });

        assert!(
            correlations[0] > correlations[1],
            "east correlation {}, north correlation {}",
            correlations[0],
            correlations[1],
        );
    }

    #[test]
    fn midlatitude_pressure_shape_preserves_transient_gradient_energy() {
        let world = WorldGeometry::default();
        let position_m = Vector::new(1_234.0, 0.25 * world.height_m());
        let sample_count = 32_768;
        let mut gradient_sum = Vector::ZERO;
        let mut gradient_square_sum = 0.0;

        for seed in 0..sample_count {
            let gradient =
                procedural_pressure_sample(world, seed, 1_234_567, position_m).gradient_m_s;
            gradient_sum += gradient;
            gradient_square_sum += gradient.dot(gradient);
        }

        let count = sample_count as f32;
        let mean_gradient = gradient_sum / count;
        let transient_gradient_energy =
            gradient_square_sum / count - mean_gradient.dot(mean_gradient);
        assert!(
            (70.0..=90.0).contains(&transient_gradient_energy),
            "transient gradient energy {transient_gradient_energy}",
        );
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
    }

    #[test]
    fn cylindrical_noise_gradient_matches_finite_difference() {
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
                let sample = cylindrical_simplex_noise_sample_f64(world, 0x5eed_cafe, x, y, 8192.0);
                let x_low = cylindrical_simplex_noise_sample_f64(
                    world,
                    0x5eed_cafe,
                    x - epsilon_m,
                    y,
                    8192.0,
                )
                .value;
                let x_high = cylindrical_simplex_noise_sample_f64(
                    world,
                    0x5eed_cafe,
                    x + epsilon_m,
                    y,
                    8192.0,
                )
                .value;
                let y_low = cylindrical_simplex_noise_sample_f64(
                    world,
                    0x5eed_cafe,
                    x,
                    y - epsilon_m,
                    8192.0,
                )
                .value;
                let y_high = cylindrical_simplex_noise_sample_f64(
                    world,
                    0x5eed_cafe,
                    x,
                    y + epsilon_m,
                    8192.0,
                )
                .value;
                assert!(
                    ((x_high - x_low) / (2.0 * epsilon_m) - sample.gradient_x_per_m).abs() < 1.0e-8,
                    "x={x} y={y}: finite difference {}, analytic {}",
                    (x_high - x_low) / (2.0 * epsilon_m),
                    sample.gradient_x_per_m,
                );
                assert!(
                    ((y_high - y_low) / (2.0 * epsilon_m) - sample.gradient_y_per_m).abs() < 1.0e-8,
                    "x={x} y={y}: finite difference {}, analytic {}",
                    (y_high - y_low) / (2.0 * epsilon_m),
                    sample.gradient_y_per_m,
                );
            }
        }
    }

    #[test]
    fn cylindrical_noise_keeps_local_gradient_scale_across_world_sizes() {
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
                    let sample = cylindrical_simplex_noise_sample_f64(
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
    fn portable_cylindrical_noise_tracks_double_precision_reference() {
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
                let reference = cylindrical_simplex_noise_sample_f64(
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
        const SAMPLE_COUNT: usize = 65_536;
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
        assert!((7.6..=8.4).contains(&mean_speed), "mean speed {mean_speed}");
        assert!(
            (38.0..=43.0).contains(&(x_square_sum / SAMPLE_COUNT as f32)),
            "x variance {}",
            x_square_sum / SAMPLE_COUNT as f32,
        );
        assert!(
            (38.0..=43.0).contains(&(y_square_sum / SAMPLE_COUNT as f32)),
            "y variance {}",
            y_square_sum / SAMPLE_COUNT as f32,
        );
        assert!(
            (cross_sum / SAMPLE_COUNT as f32).abs() < 1.0,
            "cross covariance {}",
            cross_sum / SAMPLE_COUNT as f32,
        );
        let mode_bin = speed_bins
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| *count)
            .unwrap()
            .0;
        assert!((5..=7).contains(&mode_bin), "mode bin {mode_bin}");
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
