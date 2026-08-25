struct SceneUniforms {
    camera_and_view_m: vec4<f32>,
    pressure: vec4<f32>,
    synoptic_pressure_and_temperature: vec4<f32>,
    moisture_and_temperature_gradient: vec4<f32>,
    celestial_conditions: vec4<f32>,
    world_surface_conditions: vec4<f32>,
    match_seed: vec2<u32>,
    low_cloud_epoch: vec2<u32>,
    aerosol_and_padding: vec2<f32>,
    world_size_m: vec2<f32>,
    upper_cloud_epoch: vec2<u32>,
    cloud_anchor_phase: vec2<f32>,
    surface_weather_transport_m: vec4<f32>,
    world_phases: vec4<f32>,
};

struct SceneTuning {
    cloud_detail: vec4<f32>,
    cloud_wisps: vec4<f32>,
    cloud_rendering: vec4<f32>,
    terrain_tuning: vec4<f32>,
    water_tuning: vec4<f32>,
    landform_lighting: vec4<f32>,
};

struct PlaygroundUniforms {
    cloud_detail: vec4<f32>,
    cloud_wisps: vec4<f32>,
    cloud_rendering: vec4<f32>,
    terrain_tuning: vec4<f32>,
    water_tuning: vec4<f32>,
    landform_lighting: vec4<f32>,
    layer_visibility: vec4<f32>,
    landscape_visibility: vec4<f32>,
    wind_model: vec4<f32>,
    climate_visibility: vec4<f32>,
    diagnostics: vec4<u32>,
};

struct CloudWeatherSample {
    relative_humidity: f32,
    instability: f32,
    low_coverage: f32,
    upper_coverage: f32,
    rain: f32,
    stratiform: f32,
    convective: f32,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;

@group(1) @binding(0)
var<uniform> playground: PlaygroundUniforms;

override CLOUD_DETAIL_CHARACTER: f32 = 0.35;
override CLOUD_DETAIL_BLOBINESS: f32 = 0.25;
override CLOUD_DETAIL_CONTRAST: f32 = 1.15;
override CLOUD_DETAIL_EDGE_SOFTNESS: f32 = 0.06;
override CLOUD_MAX_WISP_COVERAGE: f32 = 0.25;
override CLOUD_OPTICAL_DEPTH: f32 = 1.0;
override CLOUD_OPACITY: f32 = 1.0;
override CLOUD_DARKNESS: f32 = 1.0;
override CLOUD_RELIEF: f32 = 1.0;
override TERRAIN_HORIZONTAL_SCALE: f32 = 1.0;
override TERRAIN_ELEVATION_SCALE: f32 = 3.0;
override TERRAIN_CLIFF_SLOPE: f32 = 0.08;
override WATER_ATTENUATION_LENGTH_M: f32 = 0.5;
override WATER_BOTTOM_VISIBILITY: f32 = 1.0;
override LANDFORM_RELIEF_STRENGTH: f32 = 1.0;
override LANDFORM_SHADOW_STRENGTH: f32 = 0.7;
override LANDFORM_SHADOW_REACH_M: f32 = 320.0;

// Entry points initialize private state so shared rendering functions work with
// either production overrides or the playground tuning uniform.
var<private> tuning: SceneTuning;
var<private> layer_visibility: vec4<f32>;
var<private> landscape_visibility: vec4<f32>;
var<private> atmosphere_model: vec4<f32>;
var<private> climate_visibility: vec4<f32>;
var<private> celestial_visibility: f32;
var<private> sun_light_direction: vec3<f32>;
var<private> moon_light_direction: vec3<f32>;
var<private> sun_light_amount: f32;
var<private> moon_light_amount: f32;
var<private> ambient_light_color: vec3<f32>;
var<private> sunset_scattering: f32;
var<private> visual_pixel_footprint_m: f32;
var<private> surface_position_m: vec2<f32>;
var<private> surface_tidal_range_m: f32;
var<private> surface_tidal_phase_offset: f32;
var<private> surface_cloud_weather: CloudWeatherSample;

const PI: f32 = 3.141592653589793;
const TAU: f32 = 6.283185307179586;
const TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT: u32 = 3u;
const TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT: u32 = 3u;
const TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT: u32 = 3u;
const FINE_TERRAIN_CIRCLE_OCTAVE_COUNT: u32 = 0u;
// Keep this CC0 OpenSimplex2 gradient order synchronized with `game` so CPU and
// WGSL hashes select the same direction for authoritative terrain.
const SIMPLEX_GRADIENT_DIRECTIONS = array<vec2<f32>, 24>(
    vec2<f32>(0.9914449, 0.1305262),
    vec2<f32>(0.9238795, 0.3826834),
    vec2<f32>(0.7933533, 0.6087614),
    vec2<f32>(0.6087614, 0.7933533),
    vec2<f32>(0.3826834, 0.9238795),
    vec2<f32>(0.1305262, 0.9914449),
    vec2<f32>(-0.1305262, 0.9914449),
    vec2<f32>(-0.3826834, 0.9238795),
    vec2<f32>(-0.6087614, 0.7933533),
    vec2<f32>(-0.7933533, 0.6087614),
    vec2<f32>(-0.9238795, 0.3826834),
    vec2<f32>(-0.9914449, 0.1305262),
    vec2<f32>(-0.9914449, -0.1305262),
    vec2<f32>(-0.9238795, -0.3826834),
    vec2<f32>(-0.7933533, -0.6087614),
    vec2<f32>(-0.6087614, -0.7933533),
    vec2<f32>(-0.3826834, -0.9238795),
    vec2<f32>(-0.1305262, -0.9914449),
    vec2<f32>(0.1305262, -0.9914449),
    vec2<f32>(0.3826834, -0.9238795),
    vec2<f32>(0.6087614, -0.7933533),
    vec2<f32>(0.7933533, -0.6087614),
    vec2<f32>(0.9238795, -0.3826834),
    vec2<f32>(0.9914449, -0.1305262),
);
fn rotate_left(vector: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(-vector.y, vector.x);
}

fn surface_wind_m_s() -> vec2<f32> {
    let cross_isobar_fraction = 0.12 * atmosphere_model.y;
    let gradient = scene.pressure.yz;
    return 6000.0 * atmosphere_model.x
        * (rotate_left(gradient) - cross_isobar_fraction * gradient)
        / sqrt(1.0 + cross_isobar_fraction * cross_isobar_fraction);
}

fn upper_wind_m_s() -> vec2<f32> {
    let cross_isobar_fraction = 0.12 * atmosphere_model.y;
    let gradient = scene.synoptic_pressure_and_temperature.xy;
    let pressure_wind = 6000.0 * atmosphere_model.x
        * (rotate_left(gradient) - cross_isobar_fraction * gradient)
        / sqrt(1.0 + cross_isobar_fraction * cross_isobar_fraction);
    return 1.12 * pressure_wind
        + 2000.0 * atmosphere_model.z
            * rotate_left(scene.moisture_and_temperature_gradient.yz);
}

fn surface_latitude_degrees_at(position_m: vec2<f32>) -> f32 {
    return 180.0 * position_m.y / scene.world_size_m.y;
}

fn surface_latitude_degrees() -> f32 {
    return surface_latitude_degrees_at(surface_position_m);
}

fn surface_local_solar_phase_at(position_m: vec2<f32>) -> f32 {
    return fract(scene.world_phases.x + position_m.x / scene.world_size_m.x);
}

fn surface_local_solar_phase() -> f32 {
    return surface_local_solar_phase_at(surface_position_m);
}

fn surface_seasonal_latitude_factor_at(position_m: vec2<f32>) -> f32 {
    let latitude = surface_latitude_degrees_at(position_m);
    let magnitude = smoothstep(0.0, 45.0, abs(latitude));
    return select(magnitude, -magnitude, latitude < 0.0);
}

fn surface_seasonal_latitude_factor() -> f32 {
    return surface_seasonal_latitude_factor_at(surface_position_m);
}

fn surface_polar_fraction_at(position_m: vec2<f32>) -> f32 {
    return pow(clamp(abs(surface_latitude_degrees_at(position_m)) / 90.0, 0.0, 1.0), 1.5);
}

fn surface_annual_mean_air_temperature_c_at(position_m: vec2<f32>) -> f32 {
    return 27.0 - 45.0 * surface_polar_fraction_at(position_m);
}

fn surface_annual_mean_air_temperature_c() -> f32 {
    return surface_annual_mean_air_temperature_c_at(surface_position_m);
}

fn surface_seasonal_temperature_range_c_at(position_m: vec2<f32>) -> f32 {
    return 3.0 + 41.0 * pow(surface_polar_fraction_at(position_m), 0.9);
}

fn surface_seasonal_temperature_range_c() -> f32 {
    return surface_seasonal_temperature_range_c_at(surface_position_m);
}

fn surface_diurnal_temperature_range_c_at(position_m: vec2<f32>) -> f32 {
    return 6.0 - 3.0 * surface_polar_fraction_at(position_m);
}

fn surface_diurnal_temperature_range_c() -> f32 {
    return surface_diurnal_temperature_range_c_at(surface_position_m);
}

fn surface_air_temperature_anomaly_c_at(position_m: vec2<f32>) -> f32 {
    return 13.0 * 0.75 * spatial_weather_fractal(
        vec2<u32>(0x38d01377u, 0x452821e6u),
        position_m - scene.surface_weather_transport_m.xy,
        65536.0,
        4u,
        4u,
    );
}

fn surface_water_vapor_g_kg_at(position_m: vec2<f32>) -> f32 {
    let seasonal_phase = TAU * (scene.world_phases.y - 0.09);
    let climate_temperature_c = surface_annual_mean_air_temperature_c_at(position_m)
        + 0.5 * surface_seasonal_temperature_range_c_at(position_m)
            * surface_seasonal_latitude_factor_at(position_m) * sin(seasonal_phase);
    let capacity_g_kg = 3.5 * exp(0.055 * (climate_temperature_c + 5.0));
    let noise = 0.75 * spatial_weather_fractal(
        vec2<u32>(0xec4e6c89u, 0x082efa98u),
        position_m - scene.surface_weather_transport_m.zw,
        3584.0,
        5u,
        4u,
    );
    return clamp(capacity_g_kg * (0.67 + 0.65 * noise), 0.5, 25.0);
}

fn surface_water_vapor_g_kg() -> f32 {
    return surface_water_vapor_g_kg_at(surface_position_m);
}

fn surface_current_air_temperature_c_at(position_m: vec2<f32>) -> f32 {
    let seasonal_phase = TAU * (scene.world_phases.y - 0.09);
    let diurnal_phase = TAU * (surface_local_solar_phase_at(position_m) - 0.625);
    return surface_annual_mean_air_temperature_c_at(position_m)
        + 0.5 * surface_seasonal_temperature_range_c_at(position_m)
            * surface_seasonal_latitude_factor_at(position_m) * sin(seasonal_phase)
        + 0.5 * surface_diurnal_temperature_range_c_at(position_m) * cos(diurnal_phase)
        + surface_air_temperature_anomaly_c_at(position_m);
}

fn surface_current_air_temperature_c() -> f32 {
    return surface_current_air_temperature_c_at(surface_position_m);
}

fn ground_temperature_c(elevation_m: f32) -> f32 {
    let seasonal_phase = TAU * (scene.world_phases.y - 0.13);
    let diurnal_phase = TAU * (surface_local_solar_phase() - 0.625);
    let sea_level_temperature = surface_annual_mean_air_temperature_c()
        + 0.42 * surface_seasonal_temperature_range_c() * surface_seasonal_latitude_factor()
            * sin(seasonal_phase)
        + 0.15 * surface_diurnal_temperature_range_c() * cos(diurnal_phase);
    return sea_level_temperature
        + 0.25 * surface_air_temperature_anomaly_c_at(surface_position_m)
        - 0.0065 * max(elevation_m, 0.0);
}

fn water_temperature_c() -> f32 {
    let seasonal_phase = TAU * (scene.world_phases.y - 0.175);
    return surface_annual_mean_air_temperature_c()
        + 0.28 * surface_seasonal_temperature_range_c() * surface_seasonal_latitude_factor()
            * sin(seasonal_phase);
}

fn climate_moisture_support() -> f32 {
    return smoothstep(2.0, 10.0, surface_water_vapor_g_kg());
}

fn seasonal_snow_cover(elevation_m: f32) -> f32 {
    let ground_temperature = ground_temperature_c(elevation_m);
    let air_temperature = surface_current_air_temperature_c()
        - 0.0065 * max(elevation_m, 0.0);
    let persistent_cold = 1.0 - smoothstep(-4.0, 4.0, ground_temperature);
    let persistent_snow = persistent_cold * (0.35 + 0.65 * climate_moisture_support());
    let snowfall_cold = 1.0 - smoothstep(-1.0, 2.0, air_temperature);
    let fresh_snow = surface_cloud_weather.rain * snowfall_cold;
    let snow_before_melt = persistent_snow + fresh_snow * (1.0 - persistent_snow);
    let melt = smoothstep(0.0, 6.0, air_temperature);
    return snow_before_melt * (1.0 - 0.7 * melt);
}

fn autumn_vegetation_amount(elevation_m: f32) -> f32 {
    let annual_temperature = surface_annual_mean_air_temperature_c()
        - 0.0065 * max(elevation_m, 0.0);
    let temperate = smoothstep(3.0, 10.0, annual_temperature)
        * (1.0 - smoothstep(18.0, 25.0, annual_temperature));
    let cooling = smoothstep(
        0.10,
        0.80,
        -surface_seasonal_latitude_factor()
            * cos(TAU * (scene.world_phases.y - 0.13)),
    );
    let temperature = ground_temperature_c(elevation_m);
    let mild = smoothstep(-2.0, 8.0, temperature)
        * (1.0 - smoothstep(14.0, 20.0, temperature));
    return temperate * cooling * mild;
}

fn vegetation_dormancy(elevation_m: f32) -> f32 {
    return 1.0 - smoothstep(-2.0, 8.0, ground_temperature_c(elevation_m));
}

fn tide_height_m() -> f32 {
    let solar_hour_angle = TAU * (surface_local_solar_phase() - 0.5);
    let lunar_hour_angle = solar_hour_angle + TAU * scene.world_phases.z;
    let phase_offset = TAU * surface_tidal_phase_offset;
    let lunar_tide = cos(2.0 * (lunar_hour_angle + phase_offset));
    let solar_tide = 0.46 * cos(2.0 * (solar_hour_angle + phase_offset));
    return 0.5 * surface_tidal_range_m * (lunar_tide + solar_tide) / 1.46;
}

fn tide_reference_range_m() -> f32 {
    return surface_tidal_range_m;
}

fn initialize_surface_tide() {
    let live_range_m = 1.25 + 0.75 * spatial_weather_fractal(
        vec2<u32>(0xf3bcc909u, 0x6a09e667u),
        surface_position_m,
        131072.0,
        3u,
        3u,
    );
    surface_tidal_range_m = scene.world_surface_conditions.x * live_range_m / 1.25;
    surface_tidal_phase_offset = scene.world_surface_conditions.y
        + 0.08 * spatial_weather_fractal(
            vec2<u32>(0x84caa73bu, 0xbb67ae85u),
            surface_position_m,
            131072.0,
            3u,
            3u,
        );
}

fn cloud_weather_at(position_m: vec2<f32>) -> CloudWeatherSample {
    let air_temperature_c = surface_current_air_temperature_c_at(position_m);
    let saturation_capacity_g_kg = 3.5 * exp(0.055 * (air_temperature_c + 5.0));
    let relative_humidity = clamp(
        surface_water_vapor_g_kg_at(position_m) / saturation_capacity_g_kg,
        0.0,
        1.5,
    );
    let instability = smoothstep(
        28.0,
        48.0,
        air_temperature_c - scene.synoptic_pressure_and_temperature.w,
    );
    let pressure_lift = 1.0 - smoothstep(990.0, 1012.0, scene.pressure.x);
    let gradient_lift = smoothstep(0.0002, 0.0015, length(scene.pressure.yz));
    let frontal_lift = smoothstep(
        0.00005,
        0.0005,
        length(scene.moisture_and_temperature_gradient.yz),
    );
    let lift = clamp(
        0.45 * pressure_lift + 0.35 * gradient_lift + 0.20 * frontal_lift,
        0.0,
        1.0,
    );
    let condensation = smoothstep(0.45, 0.95, relative_humidity);
    let signal = 0.68 * condensation
        + 0.22 * lift
        + 0.10 * instability * condensation;
    let low_coverage = smoothstep(0.50, 0.98, signal);
    let upper_coverage = smoothstep(
        0.68,
        0.90,
        0.55 * condensation + 0.30 * lift + 0.15 * frontal_lift,
    );
    let rain_signal = smoothstep(0.65, 0.75, low_coverage)
        * low_coverage
        * (0.45 + 0.20 * instability + 0.45 * lift);
    let rain = smoothstep(0.55, 0.90, rain_signal);

    var weather: CloudWeatherSample;
    weather.relative_humidity = relative_humidity;
    weather.instability = instability;
    weather.low_coverage = low_coverage;
    weather.upper_coverage = upper_coverage;
    weather.rain = rain;
    weather.stratiform = clamp(
        smoothstep(0.78, 1.05, relative_humidity)
            * (1.0 - instability)
            * smoothstep(0.18, 0.75, lift),
        0.0,
        1.0,
    );
    weather.convective = clamp(
        instability
            * smoothstep(0.35, 0.85, lift)
            * mix(0.45, 1.0, rain),
        0.0,
        1.0,
    );
    return weather;
}

fn relative_humidity() -> f32 {
    return surface_cloud_weather.relative_humidity;
}

fn low_cloud_coverage() -> f32 {
    return surface_cloud_weather.low_coverage;
}

fn upper_cloud_coverage() -> f32 {
    return surface_cloud_weather.upper_coverage;
}

fn rain_intensity() -> f32 {
    return surface_cloud_weather.rain;
}

fn wind_shear_amount() -> f32 {
    return smoothstep(4.0, 15.0, length(upper_wind_m_s() - surface_wind_m_s()));
}

fn production_tuning() -> SceneTuning {
    return SceneTuning(
        vec4<f32>(
            CLOUD_DETAIL_CHARACTER,
            CLOUD_DETAIL_BLOBINESS,
            CLOUD_DETAIL_CONTRAST,
            CLOUD_DETAIL_EDGE_SOFTNESS,
        ),
        vec4<f32>(
            CLOUD_MAX_WISP_COVERAGE,
            0.0,
            0.0,
            0.0,
        ),
        vec4<f32>(
            CLOUD_OPTICAL_DEPTH,
            CLOUD_OPACITY,
            CLOUD_DARKNESS,
            CLOUD_RELIEF,
        ),
        vec4<f32>(
            TERRAIN_HORIZONTAL_SCALE,
            TERRAIN_ELEVATION_SCALE,
            TERRAIN_CLIFF_SLOPE,
            0.0,
        ),
        vec4<f32>(
            WATER_ATTENUATION_LENGTH_M,
            WATER_BOTTOM_VISIBILITY,
            0.0,
            0.0,
        ),
        vec4<f32>(
            LANDFORM_RELIEF_STRENGTH,
            LANDFORM_SHADOW_STRENGTH,
            LANDFORM_SHADOW_REACH_M,
            0.0,
        ),
    );
}

fn playground_scene_tuning() -> SceneTuning {
    return SceneTuning(
        playground.cloud_detail,
        playground.cloud_wisps,
        playground.cloud_rendering,
        playground.terrain_tuning,
        playground.water_tuning,
        playground.landform_lighting,
    );
}

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) is_terrain: f32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) world_position_m: vec2<f32>,
    @location(2) @interpolate(flat) is_terrain: f32,
};

@vertex
fn vertex(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(input.position, 0.0, 1.0);
    output.color = input.color;
    output.world_position_m = scene.camera_and_view_m.xy + vec2<f32>(
        input.position.x * scene.camera_and_view_m.z * 0.5,
        -input.position.y * scene.camera_and_view_m.w * 0.5,
    );
    output.is_terrain = input.is_terrain;
    return output;
}

fn canonical_world_component(value_m: f32, period_m: f32) -> f32 {
    let wrapped_m = value_m - floor(value_m / period_m) * period_m;
    return select(wrapped_m, wrapped_m - period_m, wrapped_m >= 0.5 * period_m);
}

fn canonical_world_position(position_m: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(
        canonical_world_component(position_m.x, scene.world_size_m.x),
        canonical_world_component(position_m.y, scene.world_size_m.y),
    );
}

fn multiply_high_u32(a: u32, b: u32) -> u32 {
    let a_low = a & 0xffffu;
    let a_high = a >> 16u;
    let b_low = b & 0xffffu;
    let b_high = b >> 16u;
    let low_product = a_low * b_low;
    let middle = a_high * b_low + (low_product >> 16u);
    let middle_low = middle & 0xffffu;
    let middle_high = middle >> 16u;
    let upper_middle = a_low * b_high + middle_low;
    return a_high * b_high + middle_high + (upper_middle >> 16u);
}

fn multiply_u64(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    return vec2<u32>(
        a.x * b.x,
        multiply_high_u32(a.x, b.x) + a.x * b.y + a.y * b.x,
    );
}

fn shift_right_u64(value: vec2<u32>, amount: u32) -> vec2<u32> {
    return vec2<u32>(
        (value.x >> amount) | (value.y << (32u - amount)),
        value.y >> amount,
    );
}

fn signed_u64(value: i32) -> vec2<u32> {
    return vec2<u32>(
        bitcast<u32>(value),
        select(0u, 0xffffffffu, value < 0),
    );
}

fn terrain_hash(initial_value: vec2<u32>) -> vec2<u32> {
    var value = initial_value;
    value = multiply_u64(
        value ^ shift_right_u64(value, 30u),
        vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
    );
    value = multiply_u64(
        value ^ shift_right_u64(value, 27u),
        vec2<u32>(0x133111ebu, 0x94d049bbu),
    );
    return value ^ shift_right_u64(value, 31u);
}

struct TerrainNoiseSample {
    value: f32,
    gradient: vec2<f32>,
};

struct TerrainSample {
    elevation_m: f32,
    gradient: vec2<f32>,
    material_elevation_m: f32,
    material_gradient: vec2<f32>,
    ice_amount: f32,
    water_ice_amount: f32,
    bedrock_elevation_m: f32,
    coastal_rock_amount: f32,
    coastal_outcrop_amount: f32,
    coastal_sediment_amount: f32,
};

struct TerrainSurface {
    color: vec3<f32>,
    detail_gradient: vec2<f32>,
    roughness: f32,
};

fn positive_mod(value: i32, period: i32) -> i32 {
    return ((value % period) + period) % period;
}

fn terrain_simplex_lattice_gradient(
    seed: vec2<u32>,
    x: i32,
    y: i32,
    x_cycles: i32,
    y_cycles: i32,
) -> vec2<f32> {
    // These residues are the integer axes of the triangular lattice. Integer
    // cycle counts make each rectangular world period an exact lattice shift.
    let periodic_x = positive_mod(x - y, 2 * x_cycles);
    let periodic_y = positive_mod(x + y, 2 * y_cycles);
    let key = seed
        ^ multiply_u64(
            signed_u64(periodic_x),
            vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
        )
        ^ multiply_u64(
            signed_u64(periodic_y),
            vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        );
    return SIMPLEX_GRADIENT_DIRECTIONS[terrain_hash(key).x % 24u];
}

fn terrain_simplex_noise(
    seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> TerrainNoiseSample {
    let skew_factor = 0.36602542;
    let unskew_factor = 0.21132487;
    let x_cycles = max(1, i32(round(scene.world_size_m.x / (sqrt(2.0) * wavelength_m))));
    let y_cycles = max(1, i32(round(
        scene.world_size_m.y * sqrt(1.5) / wavelength_m,
    )));
    let canonical_position = canonical_world_position(position_m);
    let positive_position = select(
        canonical_position,
        canonical_position + scene.world_size_m,
        canonical_position < vec2<f32>(0.0),
    );
    let normalized_position = positive_position / scene.world_size_m;
    let scaled = vec2<f32>(
        f32(x_cycles) * normalized_position.x
            + f32(y_cycles) * normalized_position.y / sqrt(3.0),
        -f32(x_cycles) * normalized_position.x
            + f32(y_cycles) * normalized_position.y / sqrt(3.0),
    );
    let skew = (scaled.x + scaled.y) * skew_factor;
    let cell = vec2<i32>(floor(scaled + vec2<f32>(skew)));
    let unskew = f32(cell.x + cell.y) * unskew_factor;
    let first_offset = scaled - (vec2<f32>(cell) - vec2<f32>(unskew));
    let middle_lattice_offset = select(
        vec2<i32>(0, 1),
        vec2<i32>(1, 0),
        first_offset.x > first_offset.y,
    );
    let middle_offset = first_offset
        - vec2<f32>(middle_lattice_offset)
        + vec2<f32>(unskew_factor);
    let last_offset = first_offset - vec2<f32>(1.0 - 2.0 * unskew_factor);

    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    let lattice_offsets = array<vec2<i32>, 3>(
        vec2<i32>(0),
        middle_lattice_offset,
        vec2<i32>(1),
    );
    let offsets = array<vec2<f32>, 3>(first_offset, middle_offset, last_offset);
    for (var vertex = 0u; vertex < 3u; vertex++) {
        let offset = offsets[vertex];
        let attenuation = 0.5 - dot(offset, offset);
        if attenuation <= 0.0 {
            continue;
        }
        let lattice = cell + lattice_offsets[vertex];
        let gradient = terrain_simplex_lattice_gradient(
            seed,
            lattice.x,
            lattice.y,
            x_cycles,
            y_cycles,
        );
        let ramp = dot(gradient, offset);
        let attenuation_squared = attenuation * attenuation;
        let attenuation_cubed = attenuation_squared * attenuation;
        let attenuation_fourth = attenuation_squared * attenuation_squared;
        sample.value += attenuation_fourth * ramp;
        sample.gradient += attenuation_fourth * gradient
            - 8.0 * attenuation_cubed * ramp * offset;
    }
    sample.value *= 70.0;
    let scaled_gradient = 70.0 * sample.gradient;
    sample.gradient = vec2<f32>(
        f32(x_cycles) / scene.world_size_m.x * (scaled_gradient.x - scaled_gradient.y),
        f32(y_cycles) / (sqrt(3.0) * scene.world_size_m.y)
            * (scaled_gradient.x + scaled_gradient.y),
    );
    return sample;
}

fn terrain_simplex_lattice_gradient_4d(
    seed: vec2<u32>,
    lattice: vec4<i32>,
) -> vec4<f32> {
    let key = seed
        ^ multiply_u64(signed_u64(lattice.x), vec2<u32>(0x7f4a7c15u, 0x9e3779b9u))
        ^ multiply_u64(signed_u64(lattice.y), vec2<u32>(0xa0b428dbu, 0xe7037ed1u))
        ^ multiply_u64(signed_u64(lattice.z), vec2<u32>(0x1ce4e5b9u, 0xbf58476du))
        ^ multiply_u64(signed_u64(lattice.w), vec2<u32>(0x133111ebu, 0x94d049bbu));
    let hash = terrain_hash(key).x;
    let zero_axis = hash & 3u;
    var sign_bit = 2u;
    var gradient = vec4<f32>(0.0);
    for (var axis = 0u; axis < 4u; axis++) {
        if axis != zero_axis {
            gradient[axis] = select(1.0, -1.0, (hash & (1u << sign_bit)) != 0u);
            sign_bit += 1u;
        }
    }
    return gradient;
}

fn terrain_simplex_noise_4d(
    seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
) -> TerrainNoiseSample {
    let canonical_position = canonical_world_position(position_m);
    let angle = TAU * canonical_position / scene.world_size_m;
    let circle_radius = scene.world_size_m / (TAU * wavelength_m);
    let point = vec4<f32>(
        circle_radius.x * cos(angle.x),
        circle_radius.x * sin(angle.x),
        circle_radius.y * cos(angle.y),
        circle_radius.y * sin(angle.y),
    );
    let skew_factor = 0.309017;
    let unskew_factor = 0.1381966;
    let skew = dot(point, vec4<f32>(skew_factor));
    let lattice = vec4<i32>(floor(point + vec4<f32>(skew)));
    let unskew = f32(lattice.x + lattice.y + lattice.z + lattice.w) * unskew_factor;
    let first = point - (vec4<f32>(lattice) - vec4<f32>(unskew));
    var rank = vec4<i32>(0);
    if first.x > first.y { rank.x += 1; } else { rank.y += 1; }
    if first.x > first.z { rank.x += 1; } else { rank.z += 1; }
    if first.x > first.w { rank.x += 1; } else { rank.w += 1; }
    if first.y > first.z { rank.y += 1; } else { rank.z += 1; }
    if first.y > first.w { rank.y += 1; } else { rank.w += 1; }
    if first.z > first.w { rank.z += 1; } else { rank.w += 1; }

    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    var input_gradient = vec4<f32>(0.0);
    for (var vertex = 0i; vertex < 5i; vertex++) {
        let threshold = 4i - vertex;
        var lattice_offset = vec4<i32>(0);
        if vertex > 0i {
            lattice_offset = select(
                vec4<i32>(0),
                vec4<i32>(1),
                rank >= vec4<i32>(threshold),
            );
        }
        let offset = first - vec4<f32>(lattice_offset)
            + vec4<f32>(f32(vertex) * unskew_factor);
        let attenuation = 0.6 - dot(offset, offset);
        if attenuation > 0.0 {
            let gradient = terrain_simplex_lattice_gradient_4d(seed, lattice + lattice_offset);
            let ramp = dot(gradient, offset);
            let attenuation_squared = attenuation * attenuation;
            let attenuation_cubed = attenuation_squared * attenuation;
            let attenuation_fourth = attenuation_squared * attenuation_squared;
            sample.value += attenuation_fourth * ramp;
            input_gradient += attenuation_fourth * gradient
                - 8.0 * attenuation_cubed * ramp * offset;
        }
    }
    // Keep this RMS calibration synchronized with the portable Rust kernel.
    sample.value *= 33.4;
    sample.gradient = 33.4 * vec2<f32>(
        -sin(angle.x) * input_gradient.x + cos(angle.x) * input_gradient.y,
        -sin(angle.y) * input_gradient.z + cos(angle.y) * input_gradient.w,
    ) / wavelength_m;
    return sample;
}

fn terrain_simplex_fractal_sample(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    persistence: f32,
    circle_octave_count: u32,
) -> TerrainNoiseSample {
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m * tuning.terrain_tuning.x;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ multiply_u64(
                vec2<u32>(octave, 0u),
                vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            );
        var octave_sample: TerrainNoiseSample;
        if octave < circle_octave_count {
            octave_sample = terrain_simplex_noise_4d(octave_seed, position_m, wavelength_m);
        } else {
            octave_sample = terrain_simplex_noise(octave_seed, position_m, wavelength_m);
        }
        sample.value += amplitude * octave_sample.value;
        sample.gradient += amplitude * octave_sample.gradient;
        amplitude_sum += amplitude;
        amplitude *= persistence;
        wavelength_m *= 0.5;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    return sample;
}

fn spatial_weather_fractal(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    circle_octave_count: u32,
) -> f32 {
    var value = 0.0;
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_seed = scene.match_seed
            ^ field_salt
            ^ multiply_u64(
                vec2<u32>(octave, 0u),
                vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            );
        var octave_value = 0.0;
        if octave < circle_octave_count {
            octave_value = terrain_simplex_noise_4d(
                octave_seed,
                position_m,
                wavelength_m,
            ).value;
        } else {
            octave_value = terrain_simplex_noise(
                octave_seed,
                position_m,
                wavelength_m,
            ).value;
        }
        value += amplitude * octave_value;
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.5;
    }
    return value / amplitude_sum;
}

fn material_fractal_sample(
    field_salt: vec2<u32>,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    octave_count: u32,
) -> TerrainNoiseSample {
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_visibility = smoothstep(
            visual_pixel_footprint_m,
            2.0 * visual_pixel_footprint_m,
            wavelength_m,
        );
        if octave_visibility > 0.0 {
            let octave_seed = scene.match_seed
                ^ field_salt
                ^ multiply_u64(
                    vec2<u32>(octave, 0u),
                    vec2<u32>(0xd192ed03u, 0xd1b54a32u),
                );
            let octave_sample = terrain_simplex_noise(
                octave_seed,
                position_m,
                wavelength_m,
            );
            sample.value += amplitude * octave_visibility * octave_sample.value;
            sample.gradient += amplitude * octave_visibility * octave_sample.gradient;
        }
        amplitude_sum += amplitude;
        amplitude *= 0.5;
        wavelength_m *= 0.53;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    return sample;
}

fn visual_cloud_noise_sample(
    position_m: vec2<f32>,
    velocity_m_s: vec2<f32>,
    field_salt: vec2<u32>,
    initial_wavelength_m: f32,
    octave_count: u32,
    anchor_epoch: vec2<u32>,
    anchor_phase: f32,
    refresh_period_seconds: f32,
    pixel_footprint_m: f32,
) -> TerrainNoiseSample {
    var sample: TerrainNoiseSample;
    sample.value = 0.0;
    sample.gradient = vec2<f32>(0.0);
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    let next_epoch = anchor_epoch + vec2<u32>(1u, select(0u, 1u, anchor_epoch.x == 0xffffffffu));
    let phase = clamp(anchor_phase, 0.0, 1.0);
    let smoother_phase = phase * phase * phase
        * (phase * (phase * 6.0 - 15.0) + 10.0);
    let angle = 0.5 * PI * smoother_phase;
    let current_position_m = position_m
        - velocity_m_s * (phase * refresh_period_seconds);
    let next_position_m = position_m
        + velocity_m_s * ((1.0 - phase) * refresh_period_seconds);
    for (var octave = 0u; octave < octave_count; octave++) {
        let octave_visibility = smoothstep(
            pixel_footprint_m,
            2.0 * pixel_footprint_m,
            wavelength_m,
        );
        if octave_visibility > 0.0 {
            let octave_seed = scene.match_seed
                ^ field_salt
                ^ multiply_u64(
                    vec2<u32>(octave, 0u),
                    vec2<u32>(0xd192ed03u, 0xd1b54a32u),
                );
            let current_sample = terrain_simplex_noise_4d(
                octave_seed ^ multiply_u64(
                    anchor_epoch,
                    vec2<u32>(0x94d049bbu, 0x133111ebu),
                ),
                current_position_m,
                wavelength_m,
            );
            let next_sample = terrain_simplex_noise_4d(
                octave_seed ^ multiply_u64(
                    next_epoch,
                    vec2<u32>(0x94d049bbu, 0x133111ebu),
                ),
                next_position_m,
                wavelength_m,
            );
            var octave_sample: TerrainNoiseSample;
            octave_sample.value = cos(angle) * current_sample.value
                + sin(angle) * next_sample.value;
            octave_sample.gradient = cos(angle) * current_sample.gradient
                + sin(angle) * next_sample.gradient;
            sample.value += amplitude * octave_visibility * octave_sample.value;
            sample.gradient += amplitude * octave_visibility * octave_sample.gradient;
        }
        amplitude_sum += amplitude;
        amplitude *= 0.58;
        wavelength_m *= 0.5;
    }
    sample.value /= amplitude_sum;
    sample.gradient /= amplitude_sum;
    return sample;
}

struct LowCloudSample {
    thickness: f32,
    density: f32,
    optical_depth: f32,
    height_m: f32,
    darkness: f32,
    relief: f32,
    gradient: vec2<f32>,
    detail_amount: f32,
    detail_gate: f32,
};

struct UpperCloudSample {
    thickness: f32,
    optical_depth: f32,
};

fn procedural_low_cloud_sample_with_weather(
    position_m: vec2<f32>,
    weather: CloudWeatherSample,
    pixel_footprint_m: f32,
) -> LowCloudSample {
    let cover = weather.low_coverage;
    let rain = weather.rain;
    if cover <= 0.0 {
        var empty: LowCloudSample;
        empty.thickness = 0.0;
        empty.density = 0.0;
        empty.optical_depth = 0.0;
        empty.height_m = 0.0;
        empty.darkness = 0.0;
        empty.relief = 0.0;
        empty.gradient = vec2<f32>(0.0);
        empty.detail_amount = 0.0;
        empty.detail_gate = 0.0;
        return empty;
    }
    let stratiform = weather.stratiform;
    let convective = weather.convective;
    let raw_detail = visual_cloud_noise_sample(
        position_m,
        surface_wind_m_s(),
        vec2<u32>(0x38d01377u, 0x452821e6u),
        640.0,
        14u,
        scene.low_cloud_epoch,
        scene.cloud_anchor_phase.x,
        1800.0,
        pixel_footprint_m,
    );
    let detail_character = mix(
        mix(clamp(tuning.cloud_detail.x, 0.0, 1.0), 0.05, 0.85 * stratiform),
        1.0,
        0.75 * convective,
    );
    let billow_rounding = 0.05;
    let rounded_magnitude = (
        sqrt(raw_detail.value * raw_detail.value + billow_rounding * billow_rounding)
            - billow_rounding
    ) / (sqrt(1.0 + billow_rounding * billow_rounding) - billow_rounding);
    let rounded_detail_value = 2.0 * rounded_magnitude - 1.0;
    let rounded_detail_gradient = 2.0 * raw_detail.value * raw_detail.gradient
        / (
            sqrt(raw_detail.value * raw_detail.value + billow_rounding * billow_rounding)
                * (sqrt(1.0 + billow_rounding * billow_rounding) - billow_rounding)
        );
    var detail: TerrainNoiseSample;
    detail.value = mix(
        raw_detail.value,
        rounded_detail_value + 0.10,
        detail_character,
    );
    detail.gradient = mix(
        raw_detail.gradient,
        rounded_detail_gradient,
        detail_character,
    );
    let detail_contrast = max(tuning.cloud_detail.z, 0.01);
    let unclamped_detail_amount = 0.5 + 0.5 * detail_contrast * detail.value;
    let detail_amount = clamp(unclamped_detail_amount, 0.0, 1.0);
    let detail_gradient = select(
        vec2<f32>(0.0),
        0.5 * detail_contrast * detail.gradient,
        unclamped_detail_amount > 0.0 && unclamped_detail_amount < 1.0,
    );

    let target_coverage = clamp(
        tuning.cloud_wisps.x * sqrt(cover),
        0.0,
        0.95,
    );
    let body_threshold = mix(0.78, 0.32, sqrt(target_coverage));
    let edge_softness = max(tuning.cloud_detail.w, 0.005);
    let detail_blobiness = mix(
        mix(clamp(tuning.cloud_detail.y, 0.0, 1.0), 0.0, stratiform),
        1.0,
        0.80 * convective,
    );
    let weather_presence = smoothstep(0.0, 0.08, cover);
    let thickness_span = max(1.0 - body_threshold, edge_softness);
    let raw_thickness = clamp(
        (detail_amount - body_threshold) / thickness_span,
        0.0,
        1.0,
    );
    let rounded_thickness = raw_thickness * raw_thickness
        * (3.0 - 2.0 * raw_thickness);
    let thickness = weather_presence * mix(
        raw_thickness,
        rounded_thickness,
        0.45 * detail_blobiness,
    );
    let thickness_inside = select(
        0.0,
        1.0,
        detail_amount > body_threshold && detail_amount < 1.0,
    );
    let raw_thickness_gradient = thickness_inside * detail_gradient / thickness_span;
    let rounded_thickness_gradient = 6.0
        * raw_thickness * (1.0 - raw_thickness) * raw_thickness_gradient;
    let thickness_gradient = weather_presence * mix(
        raw_thickness_gradient,
        rounded_thickness_gradient,
        0.45 * detail_blobiness,
    );
    let density = thickness;
    let height_detail = 0.35 + 0.65 * detail_amount;
    let height_scale_m = mix(
        mix(320.0, 120.0, stratiform),
        560.0,
        convective,
    );
    let height_m = height_scale_m * thickness * height_detail;
    let height_gradient = height_scale_m * (
        thickness_gradient * height_detail
        + 0.65 * thickness * detail_gradient
    );

    var sample: LowCloudSample;
    sample.thickness = thickness;
    sample.density = density;
    sample.optical_depth = thickness
        * (0.30 + 1.20 * detail_amount)
        * mix(1.0, 1.35, stratiform)
        * mix(1.0, 1.65, convective)
        * (1.0 + 0.80 * rain);
    sample.height_m = height_m;
    sample.darkness = thickness
        * (0.05 + 0.18 * stratiform + 0.18 * rain);
    sample.relief = height_m / height_scale_m;
    sample.gradient = height_gradient;
    sample.detail_amount = detail_amount;
    sample.detail_gate = smoothstep(0.0, edge_softness, thickness);
    return sample;
}

fn procedural_low_cloud_sample(position_m: vec2<f32>) -> LowCloudSample {
    return procedural_low_cloud_sample_with_weather(
        position_m,
        cloud_weather_at(position_m),
        visual_pixel_footprint_m,
    );
}

fn procedural_upper_cloud_sample_filtered(
    position_m: vec2<f32>,
    pixel_footprint_m: f32,
) -> UpperCloudSample {
    let weather = cloud_weather_at(position_m);
    let upper_coverage = clamp(
        max(
            weather.upper_coverage,
            0.30 * weather.low_coverage + 0.70 * weather.rain,
        ),
        0.0,
        1.0,
    );
    if upper_coverage <= 0.0 {
        var empty: UpperCloudSample;
        empty.thickness = 0.0;
        empty.optical_depth = 0.0;
        return empty;
    }
    let upper_noise = visual_cloud_noise_sample(
        position_m,
        upper_wind_m_s(),
        vec2<u32>(0x75374cc3u, 0x589965ccu),
        512.0,
        6u,
        scene.upper_cloud_epoch,
        scene.cloud_anchor_phase.y,
        3600.0,
        pixel_footprint_m,
    );
    let rain = weather.rain;
    let upper_threshold = mix(0.62, -0.18, upper_coverage) - 0.18 * rain;
    let thickness = smoothstep(0.0, 0.08, upper_coverage) * clamp(
        (upper_noise.value - upper_threshold) / max(1.0 - upper_threshold, 0.10),
        0.0,
        1.0,
    );
    var sample: UpperCloudSample;
    sample.thickness = thickness;
    sample.optical_depth = thickness * (0.45 + 1.15 * rain);
    return sample;
}

fn procedural_upper_cloud_sample(position_m: vec2<f32>) -> UpperCloudSample {
    return procedural_upper_cloud_sample_filtered(
        position_m,
        visual_pixel_footprint_m,
    );
}

fn low_cloud_base_height_m(weather: CloudWeatherSample) -> f32 {
    return 650.0 + 650.0 * weather.instability + 550.0 * weather.rain;
}

fn celestial_direction(declination: f32, hour_angle: f32) -> vec3<f32> {
    let latitude = surface_latitude_degrees() * PI / 180.0;
    return vec3<f32>(
        -cos(declination) * sin(hour_angle),
        sin(declination) * cos(latitude)
            - cos(declination) * sin(latitude) * cos(hour_angle),
        sin(latitude) * sin(declination)
            + cos(latitude) * cos(declination) * cos(hour_angle),
    );
}

fn initialize_celestial_lighting() {
    let obliquity = 23.44 * PI / 180.0;
    let solar_longitude = TAU * scene.world_phases.y;
    let declination = asin(sin(obliquity) * sin(solar_longitude));
    let hour_angle = TAU * (surface_local_solar_phase() - 0.5);
    sun_light_direction = celestial_direction(declination, hour_angle);
    let sun_horizon = smoothstep(0.0, 0.08, sun_light_direction.z);
    sun_light_amount = celestial_visibility * sun_horizon
        * (0.35 + 0.65 * sqrt(max(sun_light_direction.z, 0.0)));
    let phase_angle = TAU * fract(scene.world_phases.z);
    let lunar_declination = asin(sin(obliquity) * sin(solar_longitude + phase_angle));
    moon_light_direction = celestial_direction(lunar_declination, hour_angle + phase_angle);
    let illuminated_fraction = 0.5 - 0.5 * cos(phase_angle);
    let moon_elevation = sqrt(max(moon_light_direction.z, 0.0));
    moon_light_amount = celestial_visibility * 0.12 * illuminated_fraction
        * smoothstep(0.0, 0.08, moon_light_direction.z)
        * (0.35 + 0.65 * moon_elevation);
}

fn sun_direction() -> vec3<f32> {
    return sun_light_direction;
}

fn moon_direction() -> vec3<f32> {
    return moon_light_direction;
}

fn direct_sun_amount() -> f32 {
    return sun_light_amount;
}

fn direct_moon_amount() -> f32 {
    return moon_light_amount;
}

fn direct_light_amount() -> f32 {
    return direct_sun_amount() + direct_moon_amount();
}

fn primary_light_direction() -> vec3<f32> {
    return select(moon_direction(), sun_direction(), direct_sun_amount() >= direct_moon_amount());
}

fn calculate_atmospheric_ambient_color() -> vec3<f32> {
    if celestial_visibility <= 0.0 {
        return vec3<f32>(1.0);
    }
    let sun = sun_direction();
    let sun_elevation = sqrt(max(sun.z, 0.0));
    let twilight = smoothstep(-0.18, 0.12, sun.z);
    let clear_intensity = celestial_visibility * min(
        1.0,
        0.035 + twilight * (0.45 + 0.515 * sun_elevation) + 0.85 * direct_moon_amount(),
    );
    let daylight = smoothstep(-0.08, 0.35, sun.z);
    let horizon_glow = smoothstep(-0.12, 0.02, sun.z)
        * (1.0 - smoothstep(0.18, 0.55, sun.z));
    var color = mix(vec3<f32>(0.42, 0.52, 0.75), vec3<f32>(0.84, 0.90, 1.0), twilight);
    color = mix(color, vec3<f32>(1.0), daylight);
    let sky_openness = 1.0 - 0.75 * upper_cloud_coverage();
    color = mix(
        color,
        vec3<f32>(1.30, 0.52, 0.30),
        0.62 * horizon_glow * sunset_scattering * sky_openness,
    );
    let overcast = clamp(
        0.08 * low_cloud_coverage() + 0.82 * upper_cloud_coverage() + 0.30 * rain_intensity(),
        0.0,
        1.0,
    );
    let weather_attenuation = 1.0 - 0.42 * overcast;
    let weather_tint = mix(vec3<f32>(1.0), vec3<f32>(0.74, 0.82, 0.90), 0.55 * overcast);
    return clear_intensity * weather_attenuation * color * weather_tint;
}

fn atmospheric_ambient_color() -> vec3<f32> {
    return ambient_light_color;
}

fn calculate_sunset_scattering() -> f32 {
    let aerosol = smoothstep(0.06, 0.40, clamp(scene.aerosol_and_padding.x, 0.0, 1.0));
    let moist_haze = smoothstep(0.65, 1.10, relative_humidity());
    return clamp(0.82 * aerosol + 0.18 * moist_haze, 0.0, 1.0);
}

fn sun_direct_light_color() -> vec3<f32> {
    let low_sun = 1.0 - smoothstep(0.08, 0.55, sun_direction().z);
    let warmth = low_sun * mix(0.06, 1.0, sunset_scattering);
    return mix(vec3<f32>(1.0), vec3<f32>(1.55, 0.34, 0.06), warmth);
}

fn ambient_contribution_color() -> vec3<f32> {
    let sun_share = 0.38 * sqrt(clamp(direct_sun_amount(), 0.0, 1.0));
    let moon_share = 0.16 * sqrt(clamp(direct_moon_amount(), 0.0, 1.0));
    return atmospheric_ambient_color() * max(1.0 - sun_share - moon_share, 0.0);
}

fn direct_contribution_color() -> vec3<f32> {
    let sun_share = 0.38 * sqrt(clamp(direct_sun_amount(), 0.0, 1.0));
    let moon_share = 0.16 * sqrt(clamp(direct_moon_amount(), 0.0, 1.0));
    return sun_share * sun_direct_light_color()
        + moon_share * vec3<f32>(0.48, 0.62, 1.0);
}

fn projected_cloud_position(
    position_m: vec2<f32>,
    source_height_m: f32,
    cloud_height_m: f32,
) -> vec2<f32> {
    let light = primary_light_direction();
    let height_m = max(cloud_height_m - source_height_m, 0.0);
    let horizontal_light = length(light.xy);
    if height_m <= 0.0 || light.z <= 0.0 || horizontal_light <= 0.001 {
        return position_m;
    }
    let longitude = TAU * position_m.x / scene.world_size_m.x;
    let latitude = PI * position_m.y / scene.world_size_m.y;
    let longitude_sin = sin(longitude);
    let longitude_cos = cos(longitude);
    let latitude_sin = sin(latitude);
    let latitude_cos = cos(latitude);
    let up = vec3<f32>(
        latitude_cos * longitude_cos,
        latitude_cos * longitude_sin,
        latitude_sin,
    );
    let east = vec3<f32>(-longitude_sin, longitude_cos, 0.0);
    let north = vec3<f32>(
        -latitude_sin * longitude_cos,
        -latitude_sin * longitude_sin,
        latitude_cos,
    );
    let world_light = light.x * east + light.y * north + light.z * up;
    let world_radius_m = scene.world_size_m.x / TAU;
    let source_radius_m = world_radius_m + max(source_height_m, 0.0);
    let cloud_radius_m = world_radius_m + cloud_height_m;
    let ray_origin = source_radius_m * up;
    let origin_along_ray_m = dot(ray_origin, world_light);
    let ray_distance_m = -origin_along_ray_m + sqrt(
        origin_along_ray_m * origin_along_ray_m
            + cloud_radius_m * cloud_radius_m
            - source_radius_m * source_radius_m,
    );
    let cloud_position = ray_origin + ray_distance_m * world_light;
    let cloud_direction = normalize(cloud_position);
    return vec2<f32>(
        scene.world_size_m.x * atan2(cloud_direction.y, cloud_direction.x) / TAU,
        scene.world_size_m.y * asin(clamp(cloud_direction.z, -1.0, 1.0)) / PI,
    );
}

fn cloud_shadow_contrast() -> f32 {
    return smoothstep(0.04, 0.22, primary_light_direction().z);
}

fn terrain_smoothstep_sample(
    lower: f32,
    upper: f32,
    input: TerrainNoiseSample,
) -> TerrainNoiseSample {
    let normalized = (input.value - lower) / (upper - lower);
    let amount = clamp(normalized, 0.0, 1.0);
    var sample: TerrainNoiseSample;
    sample.value = amount * amount * (3.0 - 2.0 * amount);
    sample.gradient = vec2<f32>(0.0);
    if normalized > 0.0 && normalized < 1.0 {
        sample.gradient = input.gradient * 6.0 * amount * (1.0 - amount) / (upper - lower);
    }
    return sample;
}

fn solid_ice_sample(
    terrain: TerrainSample,
    maritime_temperature: TerrainNoiseSample,
) -> TerrainSample {
    var bedrock: TerrainNoiseSample;
    bedrock.value = terrain.elevation_m;
    bedrock.gradient = terrain.gradient;
    if bedrock.value < 0.0 {
        var effective_water_temperature: TerrainNoiseSample;
        let maritime_scale = 12.0 * (
            1.0 - smoothstep(75.0, 90.0, abs(surface_latitude_degrees()))
        );
        effective_water_temperature.value = water_temperature_c()
            + maritime_scale * maritime_temperature.value;
        effective_water_temperature.gradient = maritime_scale * maritime_temperature.gradient;
        let open_water = terrain_smoothstep_sample(-2.0, 0.0, effective_water_temperature);
        let cover = 1.0 - open_water.value;
        if cover <= 0.5 {
            return terrain;
        }
        var surface = terrain;
        surface.elevation_m = 0.25 + 1.75 * cover;
        surface.gradient = -1.75 * open_water.gradient;
        surface.ice_amount = smoothstep(0.5, 0.65, cover);
        surface.water_ice_amount = cover;
        surface.bedrock_elevation_m = bedrock.value;
        return surface;
    }
    var positive_bedrock: TerrainNoiseSample;
    positive_bedrock.value = max(bedrock.value, 0.0);
    positive_bedrock.gradient = select(vec2<f32>(0.0), bedrock.gradient, bedrock.value > 0.0);
    var annual_temperature: TerrainNoiseSample;
    annual_temperature.value = surface_annual_mean_air_temperature_c()
        - 0.0065 * positive_bedrock.value;
    let latitude = surface_latitude_degrees();
    let normalized_latitude = clamp(abs(latitude) / 90.0, 0.0, 1.0);
    let latitude_sign = select(-1.0, 1.0, latitude >= 0.0);
    let climate_gradient_y = select(
        0.0,
        -135.0 * sqrt(normalized_latitude) * latitude_sign / scene.world_size_m.y,
        normalized_latitude > 0.0,
    );
    annual_temperature.gradient = vec2<f32>(0.0, climate_gradient_y)
        - 0.0065 * positive_bedrock.gradient;
    if annual_temperature.value >= 1.0 {
        return terrain;
    }
    let warm = terrain_smoothstep_sample(-5.0, 1.0, annual_temperature);
    var cold: TerrainNoiseSample;
    cold.value = 1.0 - warm.value;
    cold.gradient = -warm.gradient;
    let amount = cold.value;
    let amount_gradient = cold.gradient;
    let broad_surface = 20.0 * cold.value + 0.25 * positive_bedrock.value;
    let broad_gradient = 20.0 * cold.gradient + 0.25 * positive_bedrock.gradient;
    let local_surface = bedrock.value + 4.0 * cold.value;
    let local_gradient = bedrock.gradient + 4.0 * cold.gradient;
    let ice_surface = max(broad_surface, local_surface);
    let ice_gradient = select(
        local_gradient,
        broad_gradient,
        broad_surface >= local_surface,
    );
    let candidate_elevation = mix(bedrock.value, ice_surface, amount);
    let candidate_gradient = mix(bedrock.gradient, ice_gradient, amount)
        + amount_gradient * (ice_surface - bedrock.value);
    let thickness = max(candidate_elevation - bedrock.value, 0.0);
    var surface = terrain;
    surface.elevation_m = max(bedrock.value, candidate_elevation);
    surface.gradient = select(
        bedrock.gradient,
        candidate_gradient,
        candidate_elevation > bedrock.value,
    );
    surface.ice_amount = smoothstep(0.1, 2.0, thickness);
    return surface;
}

fn procedural_terrain_sample(position_m: vec2<f32>) -> TerrainSample {
    // Keep the density composition synchronized with
    // `game::procedural_terrain_elevation_m`; Rust remains authoritative
    // for physical depth.
    let terrain_position_m = position_m;
    let cluster = terrain_simplex_fractal_sample(
        vec2<u32>(0x78bd642fu, 0xa0761d64u),
        terrain_position_m,
        8192.0,
        3u,
        0.5,
        TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    );
    let island = terrain_simplex_fractal_sample(
        vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        terrain_position_m,
        1536.0,
        5u,
        0.35,
        TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
    );
    var unscaled: TerrainNoiseSample;
    unscaled.value = 18.0 * (0.78 * cluster.value + 0.22 * island.value - 0.30);
    unscaled.gradient = 18.0 * (0.78 * cluster.gradient + 0.22 * island.gradient);
    let island_ruggedness = terrain_smoothstep_sample(0.32, 0.58, cluster);
    var negative_cluster: TerrainNoiseSample;
    negative_cluster.value = -cluster.value;
    negative_cluster.gradient = -cluster.gradient;
    let trench_depth = terrain_smoothstep_sample(0.50, 0.74, negative_cluster);
    let elevation_scale = tuning.terrain_tuning.y
        * (1.0 + 19.0 * island_ruggedness.value + 29.0 * trench_depth.value);
    var base: TerrainNoiseSample;
    base.value = unscaled.value * elevation_scale;
    base.gradient = unscaled.gradient * elevation_scale
        + unscaled.value
            * tuning.terrain_tuning.y
            * (19.0 * island_ruggedness.gradient + 29.0 * trench_depth.gradient);

    var base_depth: TerrainNoiseSample;
    base_depth.value = -base.value;
    base_depth.gradient = -base.gradient;
    var reef_ruggedness: TerrainNoiseSample;
    reef_ruggedness.value = 0.0;
    reef_ruggedness.gradient = vec2<f32>(0.0);
    var coastal_rock: TerrainNoiseSample;
    coastal_rock.value = 0.0;
    coastal_rock.gradient = vec2<f32>(0.0);
    if base.value > -32.0 && base.value < 4.0 {
        let reef_ruggedness_noise = terrain_simplex_fractal_sample(
            vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
            terrain_position_m,
            2048.0,
            3u,
            0.5,
            TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
        );
        reef_ruggedness = terrain_smoothstep_sample(
            -0.25,
            0.25,
            reef_ruggedness_noise,
        );
        let coastal_substrate = terrain_simplex_fractal_sample(
            vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
            terrain_position_m,
            1536.0,
            3u,
            0.5,
            TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
        );
        coastal_rock = terrain_smoothstep_sample(-0.25, 0.35, coastal_substrate);
    }
    var coastal_sediment: TerrainNoiseSample;
    coastal_sediment.value = (1.0 - coastal_rock.value)
        * (1.0 - reef_ruggedness.value);
    coastal_sediment.gradient = -coastal_rock.gradient
        * (1.0 - reef_ruggedness.value)
        - reef_ruggedness.gradient * (1.0 - coastal_rock.value);
    var ocean_floor_depth: TerrainNoiseSample;
    ocean_floor_depth.value = 3000.0 + 3000.0 * trench_depth.value;
    ocean_floor_depth.gradient = 3000.0 * trench_depth.gradient;
    let positive_base_depth_m = max(base_depth.value, 0.0);
    var shaped_depth: TerrainNoiseSample;
    shaped_depth.value = base_depth.value
        + 0.015 * positive_base_depth_m * positive_base_depth_m
            * positive_base_depth_m * positive_base_depth_m;
    shaped_depth.gradient = base_depth.gradient;
    if positive_base_depth_m > 0.0 {
        shaped_depth.gradient *= 1.0
            + 0.06 * positive_base_depth_m * positive_base_depth_m
                * positive_base_depth_m;
    }
    let deepened_depth_m = min(
        shaped_depth.value,
        ocean_floor_depth.value,
    );
    let deepened_depth_gradient = select(
        ocean_floor_depth.gradient,
        shaped_depth.gradient,
        shaped_depth.value <= ocean_floor_depth.value,
    );

    var terrain: TerrainSample;
    terrain.elevation_m = -deepened_depth_m;
    terrain.gradient = -deepened_depth_gradient;
    terrain.material_elevation_m = terrain.elevation_m;
    terrain.material_gradient = terrain.gradient;
    terrain.ice_amount = 0.0;
    terrain.water_ice_amount = 0.0;
    terrain.bedrock_elevation_m = terrain.elevation_m;
    terrain.coastal_rock_amount = 0.0;
    terrain.coastal_outcrop_amount = 0.0;
    terrain.coastal_sediment_amount = 0.0;
    let land_detail_mask = terrain_smoothstep_sample(4.0, 12.0, base);
    if land_detail_mask.value > 0.0 {
        let land_ruggedness_noise = terrain_simplex_fractal_sample(
            vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
            terrain_position_m,
            2048.0,
            3u,
            0.5,
            TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
        );
        let land_ruggedness = terrain_smoothstep_sample(
            -0.25,
            0.25,
            land_ruggedness_noise,
        );
        let land_detail = terrain_simplex_fractal_sample(
            vec2<u32>(0x4f6cdd1du, 0x2545f491u),
            terrain_position_m,
            256.0,
            6u,
            0.65,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
        var ridged_detail: TerrainNoiseSample;
        ridged_detail.value = 1.0 - 2.0 * abs(land_detail.value);
        ridged_detail.gradient = -2.0
            * select(-1.0, 1.0, land_detail.value >= 0.0)
            * land_detail.gradient;
        let ridge_blend = terrain_smoothstep_sample(0.35, 0.75, land_ruggedness);
        var shaped_land_detail: TerrainNoiseSample;
        shaped_land_detail.value = mix(
            land_detail.value,
            ridged_detail.value,
            ridge_blend.value,
        );
        shaped_land_detail.gradient = mix(
            land_detail.gradient,
            ridged_detail.gradient,
            ridge_blend.value,
        ) + ridge_blend.gradient * (ridged_detail.value - land_detail.value);
        let land_detail_amount = 0.08 + 0.92 * land_ruggedness.value;
        terrain.elevation_m += 5.0 * land_detail_mask.value
            * land_detail_amount * shaped_land_detail.value * landscape_visibility.x;
        terrain.gradient += 5.0 * landscape_visibility.x * (
            land_detail_mask.gradient * land_detail_amount * shaped_land_detail.value
                + land_ruggedness.gradient * 0.92
                    * land_detail_mask.value * shaped_land_detail.value
                + shaped_land_detail.gradient
                    * land_detail_mask.value * land_detail_amount
        );
        let land_micro_relief_amount = terrain_smoothstep_sample(
            0.40,
            0.70,
            land_ruggedness,
        );
        if land_micro_relief_amount.value > 0.0 {
            let land_micro_relief = terrain_simplex_fractal_sample(
                vec2<u32>(0xfe94f82bu, 0x3c6ef372u),
                terrain_position_m,
                4.0,
                3u,
                0.6,
                FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
            );
            var ridged_micro_relief: TerrainNoiseSample;
            ridged_micro_relief.value = 1.0 - 2.0 * abs(land_micro_relief.value);
            ridged_micro_relief.gradient = -2.0
                * select(-1.0, 1.0, land_micro_relief.value >= 0.0)
                * land_micro_relief.gradient;
            var shaped_micro_relief: TerrainNoiseSample;
            shaped_micro_relief.value = mix(
                land_micro_relief.value,
                ridged_micro_relief.value,
                ridge_blend.value,
            );
            shaped_micro_relief.gradient = mix(
                land_micro_relief.gradient,
                ridged_micro_relief.gradient,
                ridge_blend.value,
            ) + ridge_blend.gradient
                * (ridged_micro_relief.value - land_micro_relief.value);
            terrain.elevation_m += 0.35 * landscape_visibility.x
                * land_detail_mask.value
                * land_micro_relief_amount.value
                * shaped_micro_relief.value;
            terrain.gradient += 0.35 * landscape_visibility.x * (
                land_detail_mask.gradient
                    * land_micro_relief_amount.value * shaped_micro_relief.value
                    + land_micro_relief_amount.gradient
                        * land_detail_mask.value * shaped_micro_relief.value
                    + shaped_micro_relief.gradient
                        * land_detail_mask.value * land_micro_relief_amount.value
            );
        }
    }
    var shaped_bed: TerrainNoiseSample;
    shaped_bed.value = terrain.elevation_m;
    shaped_bed.gradient = terrain.gradient;
    let seabed_deep_gate = terrain_smoothstep_sample(-4.0, -2.5, shaped_bed);
    let seabed_shore_rise = terrain_smoothstep_sample(-0.5, -0.1, shaped_bed);
    let seabed_shore_gate_value = 1.0 - seabed_shore_rise.value;
    let seabed_mask_value = seabed_deep_gate.value * seabed_shore_gate_value;
    let rock_deep_gate = terrain_smoothstep_sample(-2.0, -0.8, shaped_bed);
    let rock_shore_rise = terrain_smoothstep_sample(1.2, 2.0, shaped_bed);
    let rock_shore_gate_value = 1.0 - rock_shore_rise.value;
    let rock_mask_value = rock_deep_gate.value * rock_shore_gate_value;
    let outer_reef_deep_gate = terrain_smoothstep_sample(-22.0, -18.0, shaped_bed);
    let outer_reef_shore_rise = terrain_smoothstep_sample(-5.0, -4.0, shaped_bed);
    let outer_reef_shore_gate_value = 1.0 - outer_reef_shore_rise.value;
    let outer_reef_mask_value = outer_reef_deep_gate.value
        * outer_reef_shore_gate_value;
    if seabed_mask_value == 0.0
        && rock_mask_value == 0.0
        && outer_reef_mask_value == 0.0
    {
        return solid_ice_sample(terrain, cluster);
    }
    let seabed_mask_gradient = seabed_deep_gate.gradient * seabed_shore_gate_value
        - seabed_shore_rise.gradient * seabed_deep_gate.value;
    let rock_mask_gradient = rock_deep_gate.gradient * rock_shore_gate_value
        - rock_shore_rise.gradient * rock_deep_gate.value;
    let outer_reef_mask_gradient = outer_reef_deep_gate.gradient
        * outer_reef_shore_gate_value
        - outer_reef_shore_rise.gradient * outer_reef_deep_gate.value;
    var shoal: TerrainNoiseSample;
    shoal.value = 0.0;
    shoal.gradient = vec2<f32>(0.0);
    if seabed_mask_value > 0.0 {
        shoal = terrain_simplex_fractal_sample(
            vec2<u32>(0xd192ed03u, 0xd1b54a32u),
            terrain_position_m,
            256.0,
            3u,
            0.5,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
    }
    let coastal_rock_amount = coastal_rock.value;
    let coastal_rock_relief = terrain_smoothstep_sample(0.42, 0.60, coastal_rock);
    let reef_detail_amount = coastal_rock_relief.value
        * (0.05 + 1.55 * reef_ruggedness.value);
    var coastal_detail_mask_value = seabed_mask_value;
    var coastal_detail_mask_gradient = seabed_mask_gradient;
    if rock_mask_value > seabed_mask_value {
        coastal_detail_mask_value = rock_mask_value;
        coastal_detail_mask_gradient = rock_mask_gradient;
    }
    var reef: TerrainNoiseSample;
    reef.value = 0.0;
    reef.gradient = vec2<f32>(0.0);
    if coastal_detail_mask_value > 0.0 && coastal_rock_relief.value > 0.0 {
        reef = terrain_simplex_fractal_sample(
            vec2<u32>(0x75374cc3u, 0x589965ccu),
            terrain_position_m,
            32.0,
            6u,
            0.55,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
    }
    let reef_peak = terrain_smoothstep_sample(0.72, 0.90, reef);
    var outer_reef: TerrainNoiseSample;
    outer_reef.value = 0.0;
    outer_reef.gradient = vec2<f32>(0.0);
    if outer_reef_mask_value > 0.0 && coastal_rock_relief.value > 0.0 {
        outer_reef = terrain_simplex_fractal_sample(
            vec2<u32>(0x9c88c6e3u, 0x8ebc6af0u),
            terrain_position_m,
            128.0,
            8u,
            0.6,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        );
    }
    var ridged_outer_reef: TerrainNoiseSample;
    ridged_outer_reef.value = 1.0 - 2.0 * abs(outer_reef.value);
    ridged_outer_reef.gradient = -2.0
        * select(-1.0, 1.0, outer_reef.value >= 0.0)
        * outer_reef.gradient;
    let outer_reef_ridge_blend = terrain_smoothstep_sample(
        0.25,
        0.80,
        reef_ruggedness,
    );
    var shaped_outer_reef: TerrainNoiseSample;
    shaped_outer_reef.value = mix(
        outer_reef.value,
        ridged_outer_reef.value,
        outer_reef_ridge_blend.value,
    ) - 0.3 * outer_reef_ridge_blend.value;
    shaped_outer_reef.gradient = mix(
        outer_reef.gradient,
        ridged_outer_reef.gradient,
        outer_reef_ridge_blend.value,
    ) + outer_reef_ridge_blend.gradient
        * (ridged_outer_reef.value - outer_reef.value - 0.3);
    let outer_reef_detail_amount = coastal_rock_relief.value
        * (0.12 + 1.68 * reef_ruggedness.value);
    terrain.elevation_m += 0.25 * seabed_mask_value * shoal.value
        * landscape_visibility.x
        + 1.6 * coastal_detail_mask_value * reef_detail_amount * reef.value
            * landscape_visibility.x
        + 2.0 * rock_mask_value
            * coastal_rock_relief.value * reef_peak.value * landscape_visibility.x
        + 1.8 * outer_reef_mask_value
            * outer_reef_detail_amount * shaped_outer_reef.value
            * landscape_visibility.x;
    terrain.gradient += landscape_visibility.x * 0.25 * (
        seabed_mask_gradient * shoal.value + shoal.gradient * seabed_mask_value
    ) + 1.6 * landscape_visibility.x * (
        coastal_detail_mask_gradient * reef_detail_amount * reef.value
            + coastal_rock_relief.gradient
                * (0.05 + 1.55 * reef_ruggedness.value)
                * coastal_detail_mask_value * reef.value
            + reef_ruggedness.gradient * 1.55
                * coastal_rock_relief.value * coastal_detail_mask_value * reef.value
            + reef.gradient * coastal_detail_mask_value * reef_detail_amount
    ) + landscape_visibility.x * 2.0 * (
        rock_mask_gradient * coastal_rock_relief.value * reef_peak.value
            + coastal_rock_relief.gradient * rock_mask_value * reef_peak.value
            + reef_peak.gradient * rock_mask_value * coastal_rock_relief.value
    ) + 1.8 * landscape_visibility.x * (
        outer_reef_mask_gradient
            * outer_reef_detail_amount * shaped_outer_reef.value
            + coastal_rock_relief.gradient
                * (0.12 + 1.68 * reef_ruggedness.value)
                * outer_reef_mask_value * shaped_outer_reef.value
            + reef_ruggedness.gradient * 1.68
                * coastal_rock_relief.value
                * outer_reef_mask_value * shaped_outer_reef.value
            + shaped_outer_reef.gradient
                * outer_reef_mask_value * outer_reef_detail_amount
    );
    var visible_coastal_mask_value = coastal_detail_mask_value;
    if outer_reef_mask_value > visible_coastal_mask_value {
        visible_coastal_mask_value = outer_reef_mask_value;
    }
    let submerged_material_mask_value = 1.0
        - smoothstep(-0.1, 0.0, terrain.elevation_m);
    if submerged_material_mask_value > visible_coastal_mask_value {
        visible_coastal_mask_value = submerged_material_mask_value;
    }
    terrain.coastal_rock_amount = landscape_visibility.x
        * visible_coastal_mask_value * coastal_rock_amount;
    terrain.coastal_outcrop_amount = landscape_visibility.x
        * rock_mask_value * coastal_rock_relief.value * reef_peak.value;
    terrain.coastal_sediment_amount = landscape_visibility.x
        * visible_coastal_mask_value
        * (1.0 - coastal_rock_amount)
        * (1.0 - reef_ruggedness.value);
    return solid_ice_sample(terrain, cluster);
}

fn broad_landform_elevation_m(position_m: vec2<f32>) -> f32 {
    // Long terrain shadows follow island-scale relief; coastal detail remains
    // locally lit instead of multiplying the full terrain cost at every sample.
    let terrain_position_m = position_m;
    let cluster = terrain_simplex_fractal_sample(
        vec2<u32>(0x78bd642fu, 0xa0761d64u),
        terrain_position_m,
        8192.0,
        3u,
        0.5,
        TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
    ).value;
    let island = terrain_simplex_fractal_sample(
        vec2<u32>(0xa0b428dbu, 0xe7037ed1u),
        terrain_position_m,
        1536.0,
        5u,
        0.35,
        TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
    ).value;
    let unscaled_elevation_m = 18.0 * (0.78 * cluster + 0.22 * island - 0.30);
    let island_ruggedness = smoothstep(0.32, 0.58, cluster);
    let trench_depth = smoothstep(0.50, 0.74, -cluster);
    let elevation_scale = tuning.terrain_tuning.y
        * (1.0 + 19.0 * island_ruggedness + 29.0 * trench_depth);
    return unscaled_elevation_m * elevation_scale;
}

fn landform_light_grade(
    surface_gradient: vec2<f32>,
    roughness: f32,
) -> vec2<f32> {
    let strength = clamp(
        tuning.landform_lighting.x * landscape_visibility.w,
        0.0,
        2.0,
    );
    let normal = normalize(vec3<f32>(-surface_gradient, 1.0));
    let light_direction = primary_light_direction();
    let sky_direction = normalize(vec3<f32>(0.28 * light_direction.xy, 1.0));
    let sky_facing = clamp(dot(normal, sky_direction), 0.0, 1.0);
    let sky_grade = mix(0.70, 1.08, smoothstep(0.20, 1.0, sky_facing));
    let direct_facing = smoothstep(
        -0.12,
        1.0,
        dot(normal, light_direction),
    );
    let specular_exponent = mix(56.0, 7.0, clamp(roughness, 0.0, 1.0));
    let half_direction_sum = light_direction + vec3<f32>(0.0, 0.0, 1.0);
    var specular = 0.0;
    if length(half_direction_sum) > 0.001 {
        specular = pow(
            clamp(dot(normal, normalize(half_direction_sum)), 0.0, 1.0),
            specular_exponent,
        ) * mix(0.16, 0.025, clamp(roughness, 0.0, 1.0));
    }
    let direct_grade = mix(0.44, 1.30, direct_facing) + specular;
    return clamp(
        vec2<f32>(1.0) + strength * (vec2<f32>(sky_grade, direct_grade) - 1.0),
        vec2<f32>(0.50),
        vec2<f32>(1.40),
    );
}

fn landform_shadow_transmittance(
    position_m: vec2<f32>,
    elevation_m: f32,
) -> f32 {
    let strength = clamp(
        tuning.landform_lighting.y * landscape_visibility.w,
        0.0,
        1.0,
    );
    let light_direction = primary_light_direction();
    let horizontal_light = length(light_direction.xy);
    if strength <= 0.0
        || direct_light_amount() <= 0.0
        || light_direction.z <= 0.0
        || horizontal_light <= 0.001
    {
        return 1.0;
    }
    let toward_light = light_direction.xy / horizontal_light;
    let ray_slope = light_direction.z / horizontal_light;
    let reach_m = max(tuning.landform_lighting.z, 1.0);
    var occlusion = 0.0;
    for (var sample_index = 0u; sample_index < 5u; sample_index++) {
        let distance_m = reach_m * exp2(f32(sample_index) - 4.0);
        let blocker_elevation_m = broad_landform_elevation_m(
            position_m + distance_m * toward_light,
        );
        let ray_elevation_m = elevation_m + distance_m * ray_slope;
        let softness_m = 0.6 + 0.0125 * distance_m;
        occlusion = max(
            occlusion,
            smoothstep(0.0, softness_m, blocker_elevation_m - ray_elevation_m),
        );
    }
    return 1.0 - strength * occlusion;
}

fn terrain_rock_surface(
    position_m: vec2<f32>,
    terrain_gradient: vec2<f32>,
    material_gradient: vec2<f32>,
    elevation_m: f32,
) -> TerrainSurface {
    let rock_geology = material_fractal_sample(
        vec2<u32>(0x1f83d9abu, 0x5be0cd19u),
        position_m,
        384.0,
        3u,
    ).value;
    let surface_height = material_fractal_sample(
        vec2<u32>(0x243f6a88u, 0x85a308d3u),
        position_m,
        28.0,
        4u,
    );
    let micro_height = material_fractal_sample(
        vec2<u32>(0xcbbb9d5du, 0x629a292au),
        position_m,
        5.0,
        6u,
    );
    let physical_ruggedness = smoothstep(
        0.01,
        0.08,
        length(terrain_gradient - material_gradient),
    );
    let rock_character = smoothstep(-0.45, 0.45, rock_geology);
    let fracture_amount = smoothstep(0.18, 0.42, rock_character)
        * (1.0 - smoothstep(0.68, 0.88, rock_character));
    let rock_ruggedness = max(
        physical_ruggedness,
        smoothstep(0.62, 0.92, rock_character),
    );
    let surface_height_m = 0.18
        + 1.15 * fracture_amount
        + 2.20 * rock_ruggedness;
    let micro_height_m = 0.06
        + 0.18 * fracture_amount
        + 0.38 * rock_ruggedness;
    let plate_direction = surface_height.value
        / sqrt(surface_height.value * surface_height.value + 0.0025);
    let geology_grade = clamp(
        0.5 + landscape_visibility.z * (
            0.40 * rock_geology
                + 0.20 * surface_height.value
                + 0.10 * micro_height.value
        ),
        0.0,
        1.0,
    );
    let rock_color = mix(
        vec3<f32>(0.25, 0.23, 0.20),
        vec3<f32>(0.49, 0.46, 0.40),
        0.18 * smoothstep(20.0, 240.0, elevation_m) + 0.82 * geology_grade,
    );
    return TerrainSurface(
        rock_color,
        surface_height_m * mix(
            surface_height.gradient,
            plate_direction * surface_height.gradient,
            fracture_amount,
        )
            + micro_height_m * micro_height.gradient,
        mix(0.92, 0.68, rock_ruggedness),
    );
}

fn ice_surface(position_m: vec2<f32>, surface: f32) -> TerrainSurface {
    let micro_height = material_fractal_sample(
        vec2<u32>(0xcbbb9d5du, 0x629a292au),
        position_m,
        4.0,
        6u,
    );
    let fine = material_fractal_sample(
        vec2<u32>(0x13198a2eu, 0x03707344u),
        position_m,
        72.0,
        3u,
    ).value;
    let surface_grade = 0.5 + 0.5 * landscape_visibility.z * surface;
    return TerrainSurface(
        mix(
            vec3<f32>(0.56, 0.75, 0.80),
            vec3<f32>(0.94, 0.97, 0.96),
            clamp(0.45 + 0.35 * surface_grade + 0.22 * fine, 0.0, 1.0),
        ),
        0.18 * micro_height.gradient,
        0.32,
    );
}

fn sea_ice_surface(
    position_m: vec2<f32>,
    cover: f32,
    bedrock_elevation_m: f32,
) -> TerrainSurface {
    let broad = material_fractal_sample(
        vec2<u32>(0xa4093822u, 0x299f31d0u),
        position_m,
        768.0,
        4u,
    );
    let fine = material_fractal_sample(
        vec2<u32>(0x13198a2eu, 0x03707344u),
        position_m,
        72.0,
        3u,
    );
    let micro = material_fractal_sample(
        vec2<u32>(0xcbbb9d5du, 0x629a292au),
        position_m,
        4.0,
        6u,
    );
    let shallow = smoothstep(-120.0, -3.0, bedrock_elevation_m);
    let compactness = smoothstep(0.50, 0.90, cover);
    let plate_boundary = 1.0 - smoothstep(0.0, 0.16, abs(broad.value));
    let surface_grade = clamp(
        0.55
            + 0.28 * broad.value
            + 0.10 * fine.value
            + 0.10 * compactness
            - 0.12 * plate_boundary,
        0.0,
        1.0,
    );
    let deep_color = mix(
        vec3<f32>(0.48, 0.68, 0.75),
        vec3<f32>(0.94, 0.98, 0.98),
        surface_grade,
    );
    let shallow_color = mix(deep_color, vec3<f32>(0.69, 0.87, 0.88), 0.24);
    return TerrainSurface(
        mix(deep_color, shallow_color, shallow),
        1.6 * broad.gradient + 0.32 * fine.gradient + 0.18 * micro.gradient,
        clamp(0.30 + 0.20 * plate_boundary + 0.10 * shallow, 0.0, 1.0),
    );
}

fn terrain_surface(
    position_m: vec2<f32>,
    elevation_m: f32,
    terrain_gradient: vec2<f32>,
    material_elevation_m: f32,
    material_gradient: vec2<f32>,
    coastal_rock_amount: f32,
    coastal_outcrop_amount: f32,
    coastal_sediment_amount: f32,
    water_height_m: f32,
    surface: f32,
) -> TerrainSurface {
    let slope = length(material_gradient);
    if elevation_m < water_height_m {
        let depth_m = water_height_m - elevation_m;
        let water_color = vec3<f32>(0.025, 0.11, 0.15);
        let bottom_transmittance = exp(
            -2.0 * depth_m / max(tuning.water_tuning.x, 0.01),
        );
        if bottom_transmittance > 0.02 {
            let rock_amount = smoothstep(
                0.25,
                0.75,
                max(coastal_rock_amount, coastal_outcrop_amount),
            );
            let sediment_amount = smoothstep(0.08, 0.60, coastal_sediment_amount)
                * (1.0 - rock_amount);
            let sediment_reflectance = 0.84;
            let rock_reflectance = mix(
                0.56,
                0.72,
                coastal_outcrop_amount,
            );
            var bottom_reflectance = mix(0.32, sediment_reflectance, sediment_amount);
            bottom_reflectance = mix(bottom_reflectance, rock_reflectance, rock_amount);
            let bottom_water_color = water_color * (1.0 + 3.0 * bottom_reflectance);
            let water_veiled_bottom = mix(
                bottom_water_color,
                water_color,
                0.35 * (1.0 - bottom_transmittance),
            );
            let bottom_visibility = clamp(
                tuning.water_tuning.y * bottom_transmittance,
                0.0,
                1.0,
            );
            return TerrainSurface(
                mix(water_color, water_veiled_bottom, bottom_visibility),
                vec2<f32>(0.0),
                1.0,
            );
        }
        return TerrainSurface(water_color, vec2<f32>(0.0), 1.0);
    }

    let micro_height = material_fractal_sample(
        vec2<u32>(0xcbbb9d5du, 0x629a292au),
        position_m,
        2.5,
        6u,
    );
    let surface_grade = clamp(
        0.5 + landscape_visibility.z
            * (0.40 * surface + 0.22 * micro_height.value),
        0.0,
        1.0,
    );
    let vegetation = material_fractal_sample(
        vec2<u32>(0x510e527fu, 0x9b05688cu),
        position_m,
        1536.0,
        4u,
    ).value;
    let altitude_exposure = smoothstep(50.0, 200.0, material_elevation_m);
    let rock_slope_threshold = tuning.terrain_tuning.z
        * mix(3.0, 0.8, altitude_exposure)
        * mix(0.72, 1.28, smoothstep(-0.45, 0.45, surface));
    let snow_amount = smoothstep(
        0.30,
        0.75,
        climate_visibility.x * seasonal_snow_cover(material_elevation_m) + 0.18 * surface,
    );
    if snow_amount > 0.5 {
        return TerrainSurface(
            mix(
                vec3<f32>(0.72, 0.76, 0.77),
                vec3<f32>(0.96, 0.97, 0.96),
                0.55 + 0.20 * surface_grade,
            ),
            0.22 * micro_height.gradient,
            0.70,
        );
    }
    if slope >= rock_slope_threshold {
        return terrain_rock_surface(
            position_m,
            terrain_gradient,
            material_gradient,
            elevation_m,
        );
    }
    if coastal_outcrop_amount > 0.25 {
        return terrain_rock_surface(
            position_m,
            terrain_gradient,
            material_gradient,
            elevation_m,
        );
    }

    let alpine_rock_line_m = 110.0 + 45.0 * vegetation;
    if material_elevation_m >= alpine_rock_line_m {
        return terrain_rock_surface(
            position_m,
            terrain_gradient,
            material_gradient,
            elevation_m,
        );
    }

    if coastal_rock_amount > 0.45 {
        return terrain_rock_surface(
            position_m,
            terrain_gradient,
            material_gradient,
            elevation_m,
        );
    }

    let beach_sediment = smoothstep(0.08, 0.42, coastal_sediment_amount);
    let beach_bottom_m = mix(-0.15, -4.0, beach_sediment);
    let beach_top_m = mix(-0.15, 1.4, beach_sediment);
    let beach_micro_height_m = mix(
        0.42,
        0.10,
        smoothstep(0.25, 0.85, beach_sediment),
    );
    let beach_color = mix(
        vec3<f32>(0.48, 0.43, 0.33),
        vec3<f32>(0.84, 0.76, 0.50),
        clamp(0.18 + 0.62 * beach_sediment + 0.16 * surface_grade, 0.0, 1.0),
    );
    if material_elevation_m > beach_bottom_m && material_elevation_m < beach_top_m {
        if beach_sediment > 0.18 {
            return TerrainSurface(
                beach_color,
                beach_micro_height_m * micro_height.gradient,
                0.82,
            );
        }
    }

    let intertidal_ceiling_m = 0.5 * tide_reference_range_m();
    if material_elevation_m < intertidal_ceiling_m {
        if beach_sediment > 0.18 {
            let exposed_dryness = smoothstep(
                0.05,
                0.8,
                material_elevation_m - water_height_m,
            );
            return TerrainSurface(
                mix(0.72 * beach_color, beach_color, exposed_dryness),
                beach_micro_height_m * micro_height.gradient,
                mix(0.90, 0.82, exposed_dryness),
            );
        }

        let exposed_dryness = smoothstep(
            0.05,
            0.8,
            material_elevation_m - water_height_m,
        );
        let intertidal_grade = clamp(
            0.20 + 0.42 * surface_grade + 0.22 * coastal_rock_amount,
            0.0,
            1.0,
        );
        let intertidal_color = mix(
            vec3<f32>(0.20, 0.22, 0.18),
            vec3<f32>(0.43, 0.40, 0.31),
            intertidal_grade,
        );
        return TerrainSurface(
            mix(0.70 * intertidal_color, intertidal_color, exposed_dryness),
            mix(0.12, 0.28, coastal_rock_amount) * micro_height.gradient,
            mix(0.96, 0.88, exposed_dryness),
        );
    }

    let tree_line = 1.0 - smoothstep(150.0, 250.0, material_elevation_m);
    let forest_amount = smoothstep(-0.18, 0.35, vegetation)
        * tree_line
        * (1.0 - smoothstep(0.04, 0.08, slope));
    let canopy_detail = material_fractal_sample(
        vec2<u32>(0x428a2f98u, 0x71374491u),
        position_m,
        56.0,
        3u,
    );
    let canopy_cluster = smoothstep(-0.22, 0.38, canopy_detail.value + 0.32 * vegetation);
    var grass_color = mix(
        vec3<f32>(0.09, 0.24, 0.10),
        vec3<f32>(0.32, 0.38, 0.17),
        0.20 * surface_grade
            + 0.20 * (0.5 + 0.5 * vegetation)
            + 0.18 * (0.5 + 0.5 * micro_height.value)
            + 0.42 * smoothstep(8.0, 150.0, material_elevation_m),
    );
    var forest_color = mix(
        vec3<f32>(0.03, 0.12, 0.06),
        vec3<f32>(0.09, 0.27, 0.11),
        0.18 + 0.34 * canopy_cluster + 0.18 * (0.5 + 0.5 * micro_height.value),
    );
    let autumn = climate_visibility.w * autumn_vegetation_amount(material_elevation_m);
    let dormancy = climate_visibility.w * vegetation_dormancy(material_elevation_m);
    grass_color = mix(grass_color, vec3<f32>(0.52, 0.28, 0.04), 0.62 * autumn);
    forest_color = mix(
        forest_color,
        vec3<f32>(0.62, 0.18, 0.025),
        0.80 * autumn * (0.45 + 0.55 * canopy_cluster),
    );
    grass_color = mix(grass_color, vec3<f32>(0.23, 0.22, 0.15), 0.42 * dormancy);
    forest_color = mix(forest_color, vec3<f32>(0.12, 0.13, 0.10), 0.32 * dormancy);
    let forest_visibility = forest_amount * canopy_cluster * landscape_visibility.y;
    return TerrainSurface(
        mix(grass_color, forest_color, forest_visibility),
        mix(
            0.45 * micro_height.gradient,
            3.5 * canopy_detail.gradient + 0.24 * micro_height.gradient,
            forest_visibility,
        ),
        mix(0.88, 0.96, forest_visibility),
    );
}

fn cloud_color(
    surface_color: vec3<f32>,
    cloud_sample: LowCloudSample,
    upper_transmittance: f32,
) -> vec3<f32> {
    let density = cloud_sample.density;
    let relief_gradient = tuning.cloud_rendering.w * cloud_sample.gradient;
    let limited_gradient = relief_gradient
        / (1.0 + length(relief_gradient) / 0.25);
    let cloud_normal = normalize(vec3<f32>(
        -limited_gradient,
        1.0,
    ));
    let direct_facing = max(dot(cloud_normal, primary_light_direction()), 0.0);
    let direct_light = direct_facing
        * clamp(direct_light_amount(), 0.0, 1.0);
    let sunlight = direct_light_amount() * upper_transmittance;
    let relief = cloud_sample.relief / max(density, 0.001);
    let edge_light = sunlight
        * (1.0 - smoothstep(0.30, 0.78, density))
        * smoothstep(0.10, 0.55, density)
        * 0.18;
    let depth_shadow = smoothstep(0.45, 1.35, cloud_sample.optical_depth) * 0.14;
    let cloud_material = mix(
        vec3<f32>(0.68, 0.72, 0.74),
        vec3<f32>(0.96, 0.97, 0.96),
        0.30 + 0.45 * relief + 0.25 * direct_light,
    );
    var cloud = max(vec3<f32>(0.16, 0.18, 0.20), mix(
        cloud_material,
        vec3<f32>(0.12, 0.15, 0.17),
        clamp(cloud_sample.darkness * tuning.cloud_rendering.z, 0.0, 1.0),
    ) * (0.70 + sunlight * mix(0.12, 0.36, direct_light))
        + edge_light
        - depth_shadow);
    cloud *= ambient_contribution_color()
        + direct_contribution_color()
            * upper_transmittance
            * mix(0.25, 1.0, direct_facing);
    let body = smoothstep(0.08, 0.18, density);
    let optical_depth = tuning.cloud_rendering.x * cloud_sample.optical_depth;
    let opacity = tuning.cloud_rendering.y * body * (1.0 - exp(-1.35 * optical_depth));
    return mix(surface_color, cloud, opacity);
}

fn upper_cloud_transmittance_at_low_cloud(
    position_m: vec2<f32>,
    low_height_m: f32,
    upper_height_m: f32,
) -> f32 {
    if direct_light_amount() <= 0.0 {
        return 1.0;
    }
    let upper = procedural_upper_cloud_sample(projected_cloud_position(
        position_m,
        low_height_m,
        upper_height_m,
    ));
    return mix(
        1.0,
        exp(
            -0.85
                * tuning.cloud_rendering.x
                * cloud_shadow_contrast()
                * upper.optical_depth,
        ),
        layer_visibility.w,
    );
}

fn cloud_shadow_color(
    surface_color: vec3<f32>,
    position_m: vec2<f32>,
    elevation_m: f32,
    terrain_shadow_transmittance: f32,
    landform_grade: vec2<f32>,
) -> vec3<f32> {
    let low_height_m = low_cloud_base_height_m(surface_cloud_weather);
    let upper_height_m = 6200.0;
    let surface_height_m = max(elevation_m, 0.0);
    let projected_low_position_m = projected_cloud_position(
        position_m,
        surface_height_m,
        low_height_m,
    );
    let projected_upper_position_m = projected_cloud_position(
        position_m,
        surface_height_m,
        upper_height_m,
    );
    let projected_low_footprint_m = max(
        length(dpdx(projected_low_position_m)),
        length(dpdy(projected_low_position_m)),
    );
    let projected_upper_footprint_m = max(
        length(dpdx(projected_upper_position_m)),
        length(dpdy(projected_upper_position_m)),
    );
    if direct_light_amount() <= 0.0 {
        return surface_color * landform_grade.x * ambient_contribution_color();
    }
    var shadow_depth = 0.0;
    if layer_visibility.y > 0.0 {
        let projected_low = procedural_low_cloud_sample_with_weather(
            projected_low_position_m,
            cloud_weather_at(projected_low_position_m),
            projected_low_footprint_m,
        );
        shadow_depth += layer_visibility.y * projected_low.optical_depth;
    }
    if layer_visibility.z > 0.0 {
        let projected_upper = procedural_upper_cloud_sample_filtered(
            projected_upper_position_m,
            projected_upper_footprint_m,
        );
        shadow_depth += layer_visibility.z * projected_upper.optical_depth;
    }
    let shadow_transmittance = exp(
        -0.85
            * tuning.cloud_rendering.x
            * cloud_shadow_contrast()
            * shadow_depth,
    );
    let direct_transmittance = mix(
        1.0,
        shadow_transmittance,
        clamp(tuning.cloud_rendering.y, 0.0, 1.0),
    );
    return surface_color * (
        landform_grade.x * ambient_contribution_color()
            + terrain_shadow_transmittance
                * direct_transmittance
                * landform_grade.y
                * direct_contribution_color()
    );
}

fn final_scene_color(input: VertexOutput) -> vec4<f32> {
    var surface_color = input.color;
    if input.is_terrain > 0.5 {
        let terrain = procedural_terrain_sample(input.world_position_m);
        let water_height_m = tide_height_m();
        let is_land = terrain.elevation_m >= water_height_m;
        var land_surface = 0.0;
        if is_land && terrain.water_ice_amount == 0.0 {
            land_surface = material_fractal_sample(
                vec2<u32>(0x6a09e667u, 0xbb67ae85u),
                input.world_position_m,
                96.0,
                4u,
            ).value;
        }
        var surface: TerrainSurface;
        if terrain.water_ice_amount > 0.0 {
            surface = sea_ice_surface(
                input.world_position_m,
                terrain.water_ice_amount,
                terrain.bedrock_elevation_m,
            );
        } else {
            surface = terrain_surface(
                input.world_position_m,
                terrain.elevation_m,
                terrain.gradient,
                terrain.material_elevation_m,
                terrain.material_gradient,
                terrain.coastal_rock_amount,
                terrain.coastal_outcrop_amount,
                terrain.coastal_sediment_amount,
                water_height_m,
                land_surface,
            );
        }
        var surface_light_grade = vec2<f32>(1.0);
        if is_land {
            if terrain.water_ice_amount == 0.0 && terrain.ice_amount > 0.0 {
                let ice = ice_surface(input.world_position_m, land_surface);
                surface.color = mix(
                    surface.color,
                    ice.color,
                    terrain.ice_amount,
                );
                surface.detail_gradient = mix(
                    surface.detail_gradient,
                    ice.detail_gradient,
                    terrain.ice_amount,
                );
                surface.roughness = mix(
                    surface.roughness,
                    ice.roughness,
                    terrain.ice_amount,
                );
            }
            surface_light_grade = landform_light_grade(
                3.0 * terrain.gradient + surface.detail_gradient,
                surface.roughness,
            );
        }
        surface_color = surface.color;
        var terrain_shadow_transmittance = 1.0;
        if is_land {
            terrain_shadow_transmittance = landform_shadow_transmittance(
                input.world_position_m,
                terrain.elevation_m,
            );
        }
        surface_color = cloud_shadow_color(
            surface_color,
            input.world_position_m,
            max(terrain.elevation_m, water_height_m),
            terrain_shadow_transmittance,
            surface_light_grade,
        );
        if layer_visibility.x > 0.0 {
            let low_height_m = low_cloud_base_height_m(surface_cloud_weather);
            let upper_height_m = 6200.0;
            let low_cloud = procedural_low_cloud_sample_with_weather(
                input.world_position_m,
                surface_cloud_weather,
                visual_pixel_footprint_m,
            );
            var upper_transmittance = 1.0;
            if low_cloud.density > 0.01 && layer_visibility.w > 0.0 {
                upper_transmittance = upper_cloud_transmittance_at_low_cloud(
                    input.world_position_m,
                    low_height_m + tuning.cloud_rendering.w * low_cloud.height_m,
                    upper_height_m,
                );
            }
            surface_color = mix(
                surface_color,
                cloud_color(surface_color, low_cloud, upper_transmittance),
                layer_visibility.x,
            );
        }
    }
    return vec4<f32>(surface_color, 1.0);
}

fn scalar_debug_color(value: f32) -> vec3<f32> {
    let amount = clamp(value, 0.0, 1.0);
    return mix(vec3<f32>(0.015, 0.025, 0.035), vec3<f32>(0.92, 0.96, 0.94), amount);
}

fn signed_debug_color(value: f32) -> vec3<f32> {
    let amount = clamp(value, -1.0, 1.0);
    return select(
        mix(vec3<f32>(0.07, 0.10, 0.14), vec3<f32>(0.15, 0.52, 0.86), -amount),
        mix(vec3<f32>(0.07, 0.10, 0.14), vec3<f32>(0.92, 0.52, 0.16), amount),
        amount >= 0.0,
    );
}

fn approximate_shelter(position_m: vec2<f32>, wind_m_s: vec2<f32>) -> f32 {
    // This diagnostic favors shader cost over parity. Gameplay shelter is sampled
    // by `game` and remains authoritative.
    let speed = length(wind_m_s);
    if speed < 0.01 {
        return 1.0;
    }
    let upwind = -wind_m_s / speed;
    var obstruction = 0.0;
    for (var sample_index = 0u; sample_index < 4u; sample_index++) {
        let distance_m = 32.0 * exp2(f32(sample_index) * 1.3333333);
        let terrain = procedural_terrain_sample(position_m + distance_m * upwind);
        obstruction = max(
            obstruction,
            smoothstep(0.0, 1.5, terrain.elevation_m) / (1.0 + distance_m / 256.0),
        );
    }
    return 1.0 - 0.70 * clamp(playground.wind_model.w, 0.0, 1.0) * obstruction;
}

fn wind_debug_color(wind_m_s: vec2<f32>) -> vec3<f32> {
    let speed = length(wind_m_s);
    if speed < 0.01 {
        return vec3<f32>(0.06, 0.075, 0.085);
    }
    let direction = wind_m_s / speed;
    let direction_color = vec3<f32>(
        0.5 + 0.5 * direction.x,
        0.5 + 0.5 * direction.y,
        0.5 - 0.5 * direction.x,
    );
    return direction_color * mix(0.35, 1.0, smoothstep(0.0, 20.0, speed));
}

fn playground_debug_color(input: VertexOutput, mode: u32) -> vec4<f32> {
    let position_m = input.world_position_m;
    if mode >= 1u && mode <= 6u {
        let low = procedural_low_cloud_sample(position_m);
        if mode == 1u { return vec4<f32>(scalar_debug_color(low.detail_amount), 1.0); }
        if mode == 2u { return vec4<f32>(scalar_debug_color(low.detail_gate), 1.0); }
        if mode == 3u { return vec4<f32>(scalar_debug_color(low.density), 1.0); }
        if mode == 4u { return vec4<f32>(scalar_debug_color(low.optical_depth), 1.0); }
        if mode == 5u {
            let normal = normalize(vec3<f32>(-low.gradient, 1.0));
            return vec4<f32>(0.5 + 0.5 * normal, 1.0);
        }
        let upper = procedural_upper_cloud_sample(position_m);
        return vec4<f32>(scalar_debug_color(upper.optical_depth), 1.0);
    }
    if mode == 12u {
        let low = procedural_low_cloud_sample(position_m);
        return vec4<f32>(scalar_debug_color(low.height_m / 320.0), 1.0);
    }
    if mode >= 9u && mode <= 11u {
        let terrain = procedural_terrain_sample(position_m);
        let surface_height_m = max(terrain.elevation_m, tide_height_m());
        let low_height_m = low_cloud_base_height_m(surface_cloud_weather);
        let projected_low_position_m = projected_cloud_position(
            position_m, surface_height_m, low_height_m,
        );
        let projected_upper_position_m = projected_cloud_position(
            position_m, surface_height_m, 6200.0,
        );
        let projected_low_footprint_m = max(
            length(dpdx(projected_low_position_m)),
            length(dpdy(projected_low_position_m)),
        );
        let projected_upper_footprint_m = max(
            length(dpdx(projected_upper_position_m)),
            length(dpdy(projected_upper_position_m)),
        );
        let low = procedural_low_cloud_sample_with_weather(
            projected_low_position_m,
            cloud_weather_at(projected_low_position_m),
            projected_low_footprint_m,
        );
        let upper = procedural_upper_cloud_sample_filtered(
            projected_upper_position_m,
            projected_upper_footprint_m,
        );
        if mode == 9u {
            return vec4<f32>(scalar_debug_color(low.optical_depth), 1.0);
        }
        if mode == 10u {
            return vec4<f32>(scalar_debug_color(upper.optical_depth), 1.0);
        }
        let transmittance = exp(
            -0.85
                * tuning.cloud_rendering.x
                * cloud_shadow_contrast()
                * (low.optical_depth + upper.optical_depth),
        );
        return vec4<f32>(scalar_debug_color(transmittance), 1.0);
    }
    if mode >= 16u && mode <= 21u {
        let surface = surface_wind_m_s();
        let upper = upper_wind_m_s();
        let shear = upper - surface;
        let pressure_gradient = 6000.0 * scene.pressure.yz;
        let temperature_gradient = 2000.0 * scene.moisture_and_temperature_gradient.yz;
        if mode == 16u { return vec4<f32>(wind_debug_color(surface), 1.0); }
        if mode == 17u { return vec4<f32>(wind_debug_color(upper), 1.0); }
        if mode == 18u { return vec4<f32>(wind_debug_color(shear), 1.0); }
        if mode == 19u { return vec4<f32>(wind_debug_color(pressure_gradient), 1.0); }
        if mode == 20u { return vec4<f32>(wind_debug_color(temperature_gradient), 1.0); }
        let shelter = approximate_shelter(position_m, surface);
        return vec4<f32>(mix(
            vec3<f32>(0.84, 0.17, 0.10),
            vec3<f32>(0.10, 0.72, 0.65),
            shelter,
        ), 1.0);
    }
    let terrain_position_m = position_m;
    let terrain = procedural_terrain_sample(position_m);
    if mode == 32u {
        return vec4<f32>(signed_debug_color(terrain.elevation_m / 12.0), 1.0);
    }
    if mode == 33u {
        return vec4<f32>(scalar_debug_color(length(terrain.gradient) / 0.25), 1.0);
    }
    if mode == 34u {
        let normal = normalize(vec3<f32>(-terrain.gradient, 1.0));
        return vec4<f32>(0.5 + 0.5 * normal, 1.0);
    }
    if mode == 35u {
        let cluster = terrain_simplex_fractal_sample(
            vec2<u32>(0x78bd642fu, 0xa0761d64u), terrain_position_m, 8192.0, 3u, 0.5,
            TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
        ).value;
        return vec4<f32>(signed_debug_color(cluster), 1.0);
    }
    if mode == 36u {
        let island = terrain_simplex_fractal_sample(
            vec2<u32>(0xa0b428dbu, 0xe7037ed1u), terrain_position_m, 1536.0, 5u, 0.35,
            TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT,
        ).value;
        return vec4<f32>(signed_debug_color(island), 1.0);
    }
    if mode == 37u {
        let reef = terrain_simplex_fractal_sample(
            vec2<u32>(0x75374cc3u, 0x589965ccu),
            terrain_position_m,
            32.0,
            6u,
            0.55,
            FINE_TERRAIN_CIRCLE_OCTAVE_COUNT,
        ).value;
        return vec4<f32>(signed_debug_color(reef), 1.0);
    }
    if mode == 38u {
        let vegetation = material_fractal_sample(
            vec2<u32>(0x510e527fu, 0x9b05688cu), position_m, 1536.0, 4u,
        ).value;
        return vec4<f32>(signed_debug_color(vegetation), 1.0);
    }
    if mode == 39u {
        let substrate = terrain_simplex_fractal_sample(
            vec2<u32>(0x7f4a7c15u, 0x9e3779b9u),
            terrain_position_m,
            1536.0,
            3u,
            0.5,
            TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
        ).value;
        let ruggedness_noise = terrain_simplex_fractal_sample(
            vec2<u32>(0x1ce4e5b9u, 0xbf58476du),
            terrain_position_m,
            2048.0,
            3u,
            0.5,
            TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT,
        ).value;
        let rockiness = smoothstep(-0.25, 0.35, substrate);
        let ruggedness = smoothstep(-0.25, 0.25, ruggedness_noise);
        let sediment = (1.0 - rockiness) * (1.0 - ruggedness);
        // Red is rocky substrate, green is depositional sediment, and blue is
        // rugged exposure. Magenta therefore identifies reef-forming rock,
        // while blue without red identifies low-sediment vegetated coast.
        return vec4<f32>(rockiness, sediment, ruggedness, 1.0);
    }
    if mode == 40u {
        return vec4<f32>(scalar_debug_color(seasonal_snow_cover(terrain.elevation_m)), 1.0);
    }
    if mode == 41u {
        return vec4<f32>(scalar_debug_color(terrain.ice_amount), 1.0);
    }
    if mode == 42u {
        return vec4<f32>(scalar_debug_color(autumn_vegetation_amount(terrain.elevation_m)), 1.0);
    }
    if mode == 48u {
        return vec4<f32>(signed_debug_color(tide_height_m() / 2.0), 1.0);
    }
    if mode == 49u {
        return vec4<f32>(signed_debug_color(surface_latitude_degrees() / 90.0), 1.0);
    }
    if mode == 50u {
        return vec4<f32>(scalar_debug_color(surface_local_solar_phase()), 1.0);
    }
    if mode == 51u {
        return vec4<f32>(scalar_debug_color(
            (surface_annual_mean_air_temperature_c() + 18.0) / 45.0,
        ), 1.0);
    }
    if mode == 52u {
        return vec4<f32>(signed_debug_color(surface_current_air_temperature_c() / 40.0), 1.0);
    }
    if mode == 53u {
        return vec4<f32>(scalar_debug_color(seasonal_snow_cover(terrain.elevation_m)), 1.0);
    }
    if mode == 54u {
        let maritime_temperature = 12.0
            * (1.0 - smoothstep(75.0, 90.0, abs(surface_latitude_degrees())))
            * terrain_simplex_fractal_sample(
            vec2<u32>(0x78bd642fu, 0xa0761d64u),
            surface_position_m,
            8192.0,
            3u,
            0.5,
            TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT,
        ).value;
        let water_ice = 1.0 - smoothstep(
            -2.0,
            0.0,
            water_temperature_c() + maritime_temperature,
        );
        return vec4<f32>(scalar_debug_color(water_ice), 1.0);
    }
    if mode == 55u {
        return vec4<f32>(signed_debug_color(tide_height_m() / 2.0), 1.0);
    }
    if mode == 56u {
        return vec4<f32>(scalar_debug_color(0.5 + 0.5 * sun_direction().z), 1.0);
    }
    if mode == 57u {
        return vec4<f32>(
            clamp((surface_annual_mean_air_temperature_c() + 18.0) / 45.0, 0.0, 1.0),
            seasonal_snow_cover(terrain.elevation_m),
            terrain.ice_amount,
            1.0,
        );
    }
    if mode == 58u {
        let formation = surface_cloud_weather.low_coverage;
        return vec4<f32>(vec3<f32>(formation), 1.0);
    }
    if mode == 59u {
        let thickness = procedural_low_cloud_sample_with_weather(
            position_m,
            surface_cloud_weather,
            visual_pixel_footprint_m,
        ).thickness;
        return vec4<f32>(vec3<f32>(thickness), 1.0);
    }
    return final_scene_color(input);
}

@fragment
fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {
    visual_pixel_footprint_m = max(
        0.0001,
        max(
            length(dpdx(input.world_position_m)),
            length(dpdy(input.world_position_m)),
        ),
    );
    tuning = production_tuning();
    layer_visibility = vec4<f32>(1.0);
    landscape_visibility = vec4<f32>(1.0);
    atmosphere_model = vec4<f32>(1.0);
    climate_visibility = vec4<f32>(1.0);
    celestial_visibility = 1.0;
    var periodic_input = input;
    periodic_input.world_position_m = canonical_world_position(input.world_position_m);
    surface_position_m = periodic_input.world_position_m;
    surface_cloud_weather = cloud_weather_at(surface_position_m);
    initialize_surface_tide();
    initialize_celestial_lighting();
    sunset_scattering = calculate_sunset_scattering();
    ambient_light_color = calculate_atmospheric_ambient_color();
    return final_scene_color(periodic_input);
}

@fragment
fn fragment_playground(input: VertexOutput) -> @location(0) vec4<f32> {
    visual_pixel_footprint_m = max(
        0.0001,
        max(
            length(dpdx(input.world_position_m)),
            length(dpdy(input.world_position_m)),
        ),
    );
    tuning = playground_scene_tuning();
    layer_visibility = playground.layer_visibility;
    landscape_visibility = playground.landscape_visibility;
    atmosphere_model = playground.wind_model;
    climate_visibility = playground.climate_visibility;
    celestial_visibility = f32(playground.diagnostics.y);
    var periodic_input = input;
    periodic_input.world_position_m = canonical_world_position(input.world_position_m);
    surface_position_m = periodic_input.world_position_m;
    surface_cloud_weather = cloud_weather_at(surface_position_m);
    initialize_surface_tide();
    initialize_celestial_lighting();
    sunset_scattering = calculate_sunset_scattering();
    ambient_light_color = calculate_atmospheric_ambient_color();
    if playground.diagnostics.x == 0u {
        return final_scene_color(periodic_input);
    }
    return playground_debug_color(periodic_input, playground.diagnostics.x);
}
