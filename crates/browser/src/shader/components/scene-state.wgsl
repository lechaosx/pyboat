struct SceneUniforms {
    camera_and_view_m: vec4<f32>,
    scalar_field_time_coordinates: vec4<f32>,
    match_seed: vec2<u32>,
    precipitation_epoch: vec2<u32>,
    world_size_m: vec2<f32>,
    inverse_world_size_m: vec2<f32>,
    precipitation_phase_and_css_view_height_px: vec4<f32>,
    world_phases_and_pixel_footprint_m: vec4<f32>,
    pressure_time_coordinate: vec4<f32>,
    ocean_current_time_coordinate_and_padding: vec4<f32>,
    seasonal_phase_factors: vec4<f32>,
    celestial_orbit_angles: vec4<f32>,
    celestial_orbit_light: vec4<f32>,
    view_cloud_motion_m: vec4<f32>,
    cloud_time_coordinates: vec4<f32>,
    exposure_and_output: vec4<f32>,
    environment_field_grid_m: vec4<f32>,
    environment_field_texels: vec4<u32>,
    occluder_field_grid_m: array<vec4<f32>, OCCLUDER_FIELD_LAYERS>,
    occluder_field_texels: array<vec4<u32>, OCCLUDER_FIELD_LAYERS>,
    terrain_cache_grid_m: vec4<f32>,
    terrain_cache_texels: vec4<u32>,
    terrain_cache_level: vec4<u32>,
    ocean_wave_modes: array<vec4<f32>, 16>,
};

// Keep synchronized with OCCLUDER_FIELD_LAYERS in field_grids.rs.
const OCCLUDER_FIELD_LAYERS: u32 = 5u;

struct SceneTuning {
    cloud_detail: vec4<f32>,
    cloud_wisps: vec4<f32>,
    cloud_rendering: vec4<f32>,
    terrain_tuning: vec4<f32>,
    water_tuning: vec4<f32>,
    landform_lighting: vec4<f32>,
    cloud_lighting: vec4<f32>,
    cloud_shadows: vec4<f32>,
    water_surface: vec4<f32>,
    ice_surface: vec4<f32>,
    ice_structure: vec4<f32>,
    land_material_normals: vec4<f32>,
    material_coverage: vec4<f32>,
    light_balance: vec4<f32>,
};


struct RegionalCloudDrivers {
    condensation: f32,
    pressure_lift: f32,
    instability: f32,
};

struct CloudWeatherSample {
    relative_humidity: f32,
    instability: f32,
    lift: f32,
    low_coverage: f32,
    rain: f32,
    stratiform: f32,
    regional: RegionalCloudDrivers,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;


override CLOUD_DETAIL_CHARACTER: f32 = 0.35;
override CLOUD_DETAIL_BLOBINESS: f32 = 0.25;
override CLOUD_DETAIL_CONTRAST: f32 = 1.15;
override CLOUD_DETAIL_EDGE_SOFTNESS: f32 = 0.06;
override CLOUD_MAX_WISP_COVERAGE: f32 = 0.25;
override CLOUD_OPTICAL_DEPTH: f32 = 1.0;
override CLOUD_OPACITY: f32 = 1.0;
override CLOUD_DARKNESS: f32 = 1.0;
override LOW_CLOUD_PARALLAX: f32 = 1.0;
override TERRAIN_HORIZONTAL_SCALE: f32 = 1.0;
override TERRAIN_ELEVATION_SCALE: f32 = 3.0;
override TERRAIN_CLIFF_SLOPE: f32 = 0.08;
override WATER_ATTENUATION_LENGTH_M: f32 = 0.5;
override WATER_BOTTOM_VISIBILITY: f32 = 1.0;
override LANDFORM_RELIEF_STRENGTH: f32 = 1.0;
override LANDFORM_SHADOW_STRENGTH: f32 = 0.7;
override LANDFORM_SHADOW_REACH_M: f32 = 320.0;
// Lab-only worst-case timing: evaluate work that conditions normally skip, then
// select the same result. False folds away when pipelines are compiled.
override EVALUATE_ALL_BRANCHES: bool = false;
// Lab-only comparison: 1 takes each cloud-shadow lookup from one direction at
// the centre of the visible disc instead of averaging three across it.
override CLOUD_SHADOW_TAPS: u32 = 3u;
// Graphics tiers set these through `GraphicsQuality::scene_constants`; the
// defaults are the high tier.
override DECORATIVE_WATER_OCTAVES: u32 = 4u;
override MATERIAL_MICRO_OCTAVES: u32 = 6u;

// Entry points initialize private state so shared rendering functions work with
// either production overrides or the playground tuning uniform.
var<private> tuning: SceneTuning;
var<private> layer_visibility: vec4<f32>;
var<private> landscape_visibility: vec4<f32>;
var<private> atmosphere_model: vec4<f32>;
var<private> climate_visibility: vec4<f32>;
var<private> sun_light_direction: vec3<f32>;
var<private> geocentric_moon_light_direction: vec3<f32>;
var<private> lunar_penumbra_fraction: f32;
var<private> lunar_umbra_fraction: f32;
var<private> local_horizon_scattering: f32;
var<private> regional_horizon_scattering: f32;
var<private> visual_pixel_footprint_m: f32;
var<private> surface_position_m: vec2<f32>;
var<private> surface_tidal_range_m: f32;
var<private> surface_tidal_phase_offset: f32;
var<private> surface_cloud_weather: CloudWeatherSample;

const PI: f32 = 3.141592653589793;
const TAU: f32 = 6.283185307179586;
const CELESTIAL_DISC_ANGULAR_RADIUS_RAD: f32 = 0.00465;
const MOON_ANGULAR_RADIUS_RAD: f32 = 0.00452;
const LUNAR_DISTANCE_PLANET_RADII: f32 = 60.3;
const LOW_CLOUD_BASE_HEIGHT_M: f32 = 900.0;
const LOW_CLOUD_PROJECTION_EYE_HEIGHT_M: f32 = 12000.0;
const WEATHER_CANOPY_HEIGHT_M: f32 = 3600.0;
const TERRAIN_CLUSTER_CIRCLE_OCTAVE_COUNT: u32 = 3u;
const TERRAIN_ISLAND_CIRCLE_OCTAVE_COUNT: u32 = 3u;
const TERRAIN_REGIONAL_CIRCLE_OCTAVE_COUNT: u32 = 3u;
const FINE_TERRAIN_CIRCLE_OCTAVE_COUNT: u32 = 0u;
const OCTAVE_SALTS: array<vec2<u32>, 16> = array<vec2<u32>, 16>(
    vec2<u32>(0x00000000u, 0x00000000u),
    vec2<u32>(0xd192ed03u, 0xd1b54a32u),
    vec2<u32>(0xa325da06u, 0xa36a9465u),
    vec2<u32>(0x74b8c709u, 0x751fde98u),
    vec2<u32>(0x464bb40cu, 0x46d528cbu),
    vec2<u32>(0x17dea10fu, 0x188a72feu),
    vec2<u32>(0xe9718e12u, 0xea3fbd30u),
    vec2<u32>(0xbb047b15u, 0xbbf50763u),
    vec2<u32>(0x8c976818u, 0x8daa5196u),
    vec2<u32>(0x5e2a551bu, 0x5f5f9bc9u),
    vec2<u32>(0x2fbd421eu, 0x3114e5fcu),
    vec2<u32>(0x01502f21u, 0x02ca302fu),
    vec2<u32>(0xd2e31c24u, 0xd47f7a61u),
    vec2<u32>(0xa4760927u, 0xa634c494u),
    vec2<u32>(0x7608f62au, 0x77ea0ec7u),
    vec2<u32>(0x479be32du, 0x499f58fau),
);
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
