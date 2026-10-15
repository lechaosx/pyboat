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
// Each seeded noise field owns sixteen octave streams; distinct identifiers give
// every field and octave an independent lattice. Keep the fields `game` also
// samples synchronized with crates/game/src/lib.rs.
const TERRAIN_CLUSTER_NOISE_FIELD: u32 = 1u;
const TERRAIN_ISLAND_NOISE_FIELD: u32 = 2u;
const TERRAIN_RUGGEDNESS_NOISE_FIELD: u32 = 3u;
const TERRAIN_COASTAL_SUBSTRATE_NOISE_FIELD: u32 = 4u;
const TERRAIN_LAND_RELIEF_NOISE_FIELD: u32 = 5u;
const TERRAIN_LAND_MICRO_RELIEF_NOISE_FIELD: u32 = 6u;
const TERRAIN_SHOAL_NOISE_FIELD: u32 = 7u;
const TERRAIN_REEF_NOISE_FIELD: u32 = 8u;
const TERRAIN_OUTER_REEF_NOISE_FIELD: u32 = 9u;
const TIDE_RESPONSE_NOISE_FIELD: u32 = 10u;
const TIDE_LAG_NOISE_FIELD: u32 = 11u;
const SURFACE_AIR_TEMPERATURE_NOISE_FIELD: u32 = 12u;
const AIR_TEMPERATURE_500_HPA_NOISE_FIELD: u32 = 13u;
const WATER_VAPOR_NOISE_FIELD: u32 = 14u;
const OCEAN_CURRENT_NOISE_FIELD: u32 = 15u;
const PRESSURE_BAND_NOISE_FIELDS = array<u32, 4>(16u, 17u, 18u, 19u);
const ROCK_GEOLOGY_NOISE_FIELD: u32 = 20u;
const ROCK_SURFACE_HEIGHT_NOISE_FIELD: u32 = 21u;
const MATERIAL_MICRO_RELIEF_NOISE_FIELD: u32 = 22u;
const ICE_FINE_NOISE_FIELD: u32 = 23u;
const SEA_ICE_BROAD_NOISE_FIELD: u32 = 24u;
const SEA_ICE_FINE_NOISE_FIELD: u32 = 25u;
const SEA_ICE_SLAB_NOISE_FIELD: u32 = 26u;
const VEGETATION_NOISE_FIELD: u32 = 27u;
const CANOPY_DETAIL_NOISE_FIELD: u32 = 28u;
const LAND_SURFACE_NOISE_FIELD: u32 = 29u;
const PRECIPITATION_CONTACT_NOISE_FIELD: u32 = 30u;
const DECORATIVE_WATER_NOISE_FIELD: u32 = 31u;
const LOW_CLOUD_NOISE_FIELD: u32 = 32u;
const UPPER_CLOUD_NOISE_FIELD: u32 = 33u;
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
