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
    // Seconds the adapting field adapts over this frame, the output peak, 1
    // for HDR output, then the vision noise phase.
    exposure_and_output: vec4<f32>,
    environment_field_grid_m: vec4<f32>,
    environment_field_texels: vec4<u32>,
    cloud_weather_grid_m: vec4<f32>,
    cloud_weather_texels: vec4<u32>,
    // The shortest wavelength the Sun's and the Moon's cloud shadow grids
    // resolve, then 1 while the Moon's grid is evaluated, else 0.
    cloud_shadow_grids: vec4<f32>,
    occluder_field_grid_m: array<vec4<f32>, OCCLUDER_FIELD_LAYERS>,
    occluder_field_texels: array<vec4<u32>, OCCLUDER_FIELD_LAYERS>,
    terrain_cache_grid_m: vec4<f32>,
    terrain_cache_texels: vec4<u32>,
    terrain_cache_level: vec4<u32>,
    baked_surface_grid_m: vec4<f32>,
    baked_surface_texels: vec4<u32>,
    baked_surface_level: vec4<u32>,
    ocean_wave_modes: array<vec4<f32>, 16>,
};

// Keep synchronized with OCCLUDER_FIELD_LAYERS in field_grids.rs.
const OCCLUDER_FIELD_LAYERS: u32 = 5u;

struct SceneTuning {
    // Attenuation length, then padding.
    water_tuning: vec4<f32>,
    // Relief and shadow visibility, high-light reach, then padding.
    landform_lighting: vec4<f32>,
    // Visibility, then padding.
    cloud_shadows: vec4<f32>,
};


struct RegionalCloudDrivers {
    relative_humidity: f32,
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
    // Afternoon heating of land and warming of the air by warmer water, each
    // from 0 to 1, and the land share beneath.
    land_heating: f32,
    sea_warming: f32,
    land_fraction: f32,
    regional: RegionalCloudDrivers,
};

@group(0) @binding(0)
var<uniform> scene: SceneUniforms;


// Keep synchronized with `TERRAIN_ELEVATION_SCALE` in crates/game/src/lib.rs.
const TERRAIN_ELEVATION_SCALE: f32 = 3.0;
const TERRAIN_CLIFF_SLOPE: f32 = 0.08;
// Compressed from clear seawater's 10-50 m so steep shelves still read.
override WATER_ATTENUATION_LENGTH_M: f32 = 0.5;
const WATER_BOTTOM_REFLECTANCE: f32 = 3.0;
const WATER_BOTTOM_VEIL: f32 = 0.35;
override LANDFORM_SHADOW_REACH_M: f32 = 320.0;
// Lab-only worst-case timing: evaluate work that conditions normally skip, then
// select the same result. False folds away when pipelines are compiled.
override EVALUATE_ALL_BRANCHES: bool = false;
// Lab-only comparison: true thins ground cloud shadows between light heights
// 0.22 and 0.04 and holds landform shadows to 70% of their strength over the
// high-light reach at every light height.
override LEGACY_SHADOW_LOOK: bool = false;
// Graphics tiers set these through `GraphicsQuality::scene_constants`; the
// defaults are the high tier.
override DECORATIVE_WATER_OCTAVES: u32 = 4u;
override MATERIAL_MICRO_OCTAVES: u32 = 6u;

// Entry points initialize private state so shared rendering functions work with
// either production overrides or the playground tuning uniform.
var<private> tuning: SceneTuning;
var<private> landscape_visibility: vec4<f32>;
var<private> sun_light_direction: vec3<f32>;
var<private> geocentric_moon_light_direction: vec3<f32>;
var<private> lunar_penumbra_fraction: f32;
var<private> lunar_umbra_fraction: f32;
var<private> local_aerosol_optical_depth: f32;
var<private> regional_aerosol_optical_depth: f32;
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
// Above every shadow receiver; keep synchronized with field_grids.rs.
const CLOUD_SHADOW_PLANE_HEIGHT_M: f32 = 900.0;
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
const CUMULUS_NOISE_FIELD: u32 = 32u;
const DECK_NOISE_FIELD: u32 = 33u;
const CIRRUS_NOISE_FIELD: u32 = 34u;
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
