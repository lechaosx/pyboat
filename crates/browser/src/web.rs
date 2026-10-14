use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::VecDeque,
    mem,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use bytemuck::{Pod, Zeroable};
use bytes::Bytes;
use futures_util::{
    future::{Either, select},
    stream::{FuturesUnordered, StreamExt},
};
use game::{
    OCEAN_WAVE_MODE_COUNT, celestial_lighting, celestial_orbit, moon_phase,
    procedural_world_environment_for_render_clocks, scalar_field_time_coordinates_interpolated,
    vector_field_time_coordinates_interpolated, world_celestial_conditions,
};
use protocol::{ServerMessage, decode_server_message, encode_client_message};
use rapier2d::prelude::Vector;
use simulation::{TICKS_PER_SECOND, WorldGeometry};
use url::Url;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::HtmlCanvasElement;
use web_transport_wasm::{ClientBuilder, RecvStream, Session};
use wgpu::CurrentSurfaceTexture;

use crate::frame_queue::FrameQueue;
use crate::{
    BoatScene, DEFAULT_VIEW_HEIGHT_M, ENVIRONMENT_FIELD_LAYERS, ENVIRONMENT_FIELD_SHADER,
    ENVIRONMENT_FIELD_TEXELS, FieldGridTuning, FieldGrids, MAX_SERVER_MESSAGE_BYTES,
    MultiplayerClient, OCCLUDER_FIELD_LAYERS, OCCLUDER_FIELD_SHADER, OCCLUDER_FIELD_TEXELS,
    SCENE_SHADER, ViewCloudMotion, ViewExposure, Windsock, append_reliable_server_message_chunk,
    bounded_canvas_dimensions, disconnect_and_clear_pending_snapshot, field_grids,
    ocean_wave_mode_uniforms, periodic_effect_time_interpolated, queue_latest_snapshot,
    requeue_failed_viewport, sample_view_cloud_state_for_render_clocks, sample_world_geometry,
    scale_bar, seasonal_phase_factors, view_width_m, viewport_interest_radius_m,
    visual_pixel_footprint_m, webtransport_server_url, windsock_extension, zoom_view_height_m,
};

#[wasm_bindgen(
    inline_js = "export function canvasHasHdrOutput(canvas) { return canvas.getContext('webgpu').getConfiguration?.()?.toneMapping?.mode === 'extended' && matchMedia('(dynamic-range: high)').matches; }"
)]
extern "C" {
    #[wasm_bindgen(js_name = canvasHasHdrOutput)]
    fn canvas_has_hdr_output(canvas: &HtmlCanvasElement) -> bool;
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub struct PlaygroundExposure {
    exposure: ViewExposure,
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
impl PlaygroundExposure {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            exposure: ViewExposure::default(),
        }
    }

    pub fn advance(
        &mut self,
        width_m: f32,
        height_m: f32,
        world_time_seconds: f64,
        camera_x_m: f32,
        camera_y_m: f32,
        elapsed_seconds: f64,
    ) -> Result<f32, JsValue> {
        let geometry = WorldGeometry::new(width_m, height_m)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let conditions = world_celestial_conditions(
            geometry,
            world_time_seconds,
            Vector::new(camera_x_m, camera_y_m),
        );
        let target = crate::exposure_target(celestial_lighting(conditions).ambient_intensity);
        Ok(self.exposure.advance(target, elapsed_seconds))
    }
}

#[cfg(feature = "playground")]
impl Default for PlaygroundExposure {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "playground")]
use crate::PLAYGROUND_STARTUP_SHADER;

const INITIAL_VERTEX_CAPACITY: usize = 32768;
const MAXIMUM_PENDING_DATAGRAMS: usize = 15;
const MAXIMUM_CONCURRENT_RELIABLE_RECEIVES: usize = 8;
const PRECIPITATION_CONTACT_PERIOD_TICKS: u64 = 108_000;

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub fn production_scene_shader_source() -> String {
    SCENE_SHADER.to_owned()
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub fn occluder_field_shader_source() -> String {
    OCCLUDER_FIELD_SHADER.to_owned()
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub fn playground_scene_shader_source() -> String {
    crate::playground_scene_shader_source().to_owned()
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub fn environment_field_shader_source() -> String {
    ENVIRONMENT_FIELD_SHADER.to_owned()
}

#[cfg(feature = "playground")]
/// Width, height, and layer count of the environment field texture.
#[wasm_bindgen]
pub fn environment_field_texture_size() -> Vec<u32> {
    vec![
        ENVIRONMENT_FIELD_TEXELS,
        ENVIRONMENT_FIELD_TEXELS,
        ENVIRONMENT_FIELD_LAYERS,
    ]
}

#[cfg(feature = "playground")]
/// Width, height, and layer count of the occluder field texture.
#[wasm_bindgen]
pub fn occluder_field_texture_size() -> Vec<u32> {
    vec![
        OCCLUDER_FIELD_TEXELS,
        OCCLUDER_FIELD_TEXELS,
        OCCLUDER_FIELD_LAYERS as u32,
    ]
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub fn playground_occluder_field_shader_source() -> String {
    crate::playground_occluder_field_shader_source().to_owned()
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub struct PlaygroundSceneFrame {
    uniforms: Vec<u8>,
    field_grids: Vec<f32>,
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
impl PlaygroundSceneFrame {
    #[wasm_bindgen(getter)]
    pub fn uniforms(&self) -> Vec<u8> {
        self.uniforms.clone()
    }

    /// Environment and occluder workgroup counts (2 and 3 values), then column
    /// spacing, row spacing, columns and rows for each occluder layer.
    #[wasm_bindgen(getter)]
    pub fn field_grids(&self) -> Vec<f32> {
        self.field_grids.clone()
    }
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub fn playground_direct_scene_shader_source() -> String {
    crate::playground_direct_scene_shader_source().to_owned()
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub fn playground_startup_shader_source() -> String {
    PLAYGROUND_STARTUP_SHADER.to_owned()
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub fn playground_diagnostic_shader_source(name: &str) -> Result<String, JsValue> {
    crate::playground_diagnostic_shader_source(name)
        .map(str::to_owned)
        .ok_or_else(|| JsValue::from_str("unknown playground diagnostic shader"))
}

#[wasm_bindgen]
pub struct PlaygroundWorldGeometrySample {
    raw_x_m: f32,
    raw_y_m: f32,
    canonical_x_m: f32,
    canonical_y_m: f32,
    normalized_x: f32,
    normalized_y: f32,
}

#[wasm_bindgen]
pub struct PlaygroundEnvironmentSample {
    geographic_latitude_degrees: f32,
    season_phase: f32,
    day_phase: f32,
    moon_phase: f32,
    annual_mean_air_temperature_c: f32,
    seasonal_temperature_range_c: f32,
    diurnal_temperature_range_c: f32,
    pressure_hpa: f32,
    pressure_gradient_x_hpa_per_m: f32,
    pressure_gradient_y_hpa_per_m: f32,
    synoptic_pressure_gradient_x_hpa_per_m: f32,
    synoptic_pressure_gradient_y_hpa_per_m: f32,
    surface_air_temperature_anomaly_c: f32,
    air_temperature_500_hpa_c: f32,
    temperature_gradient_x_c_per_m: f32,
    temperature_gradient_y_c_per_m: f32,
    water_vapor_g_kg: f32,
    reference_tidal_range_m: f32,
    tidal_phase_offset: f32,
    surface_air_temperature_c: f32,
    wind_x_m_s: f32,
    wind_y_m_s: f32,
    upper_wind_x_m_s: f32,
    upper_wind_y_m_s: f32,
    relative_humidity: f32,
    lift: f32,
    cloudiness: f32,
    instability: f32,
    upper_cloudiness: f32,
    rain_intensity: f32,
    sun_x: f32,
    sun_y: f32,
    sun_z: f32,
    sun_intensity: f32,
    moon_x: f32,
    moon_y: f32,
    moon_z: f32,
    moon_intensity: f32,
    solar_eclipse_fraction: f32,
    lunar_penumbra_fraction: f32,
    lunar_umbra_fraction: f32,
    ambient_intensity: f32,
    ground_temperature_c: f32,
    water_temperature_c: f32,
    seasonal_snow_cover: f32,
    polar_ice_closure: f32,
    solid_ice_cover: f32,
    water_height_m: f32,
    water_depth_m: f32,
    current_x_m_s: f32,
    current_y_m_s: f32,
    wave_orbital_x_m_s: f32,
    wave_orbital_y_m_s: f32,
    significant_wave_height_m: f32,
    wave_surface_displacement_m: f32,
    rogue_crest_excess_m: f32,
}

macro_rules! environment_sample_getters {
    ($($name:ident),+ $(,)?) => {
        $(
            #[wasm_bindgen]
            impl PlaygroundEnvironmentSample {
                #[wasm_bindgen(getter)]
                pub fn $name(&self) -> f32 {
                    self.$name
                }
            }
        )+
    };
}

environment_sample_getters!(
    geographic_latitude_degrees,
    season_phase,
    day_phase,
    moon_phase,
    annual_mean_air_temperature_c,
    seasonal_temperature_range_c,
    diurnal_temperature_range_c,
    pressure_hpa,
    pressure_gradient_x_hpa_per_m,
    pressure_gradient_y_hpa_per_m,
    synoptic_pressure_gradient_x_hpa_per_m,
    synoptic_pressure_gradient_y_hpa_per_m,
    surface_air_temperature_anomaly_c,
    air_temperature_500_hpa_c,
    temperature_gradient_x_c_per_m,
    temperature_gradient_y_c_per_m,
    water_vapor_g_kg,
    reference_tidal_range_m,
    tidal_phase_offset,
    surface_air_temperature_c,
    wind_x_m_s,
    wind_y_m_s,
    upper_wind_x_m_s,
    upper_wind_y_m_s,
    relative_humidity,
    lift,
    cloudiness,
    instability,
    upper_cloudiness,
    rain_intensity,
    sun_x,
    sun_y,
    sun_z,
    sun_intensity,
    moon_x,
    moon_y,
    moon_z,
    moon_intensity,
    solar_eclipse_fraction,
    lunar_penumbra_fraction,
    lunar_umbra_fraction,
    ambient_intensity,
    ground_temperature_c,
    water_temperature_c,
    seasonal_snow_cover,
    polar_ice_closure,
    solid_ice_cover,
    water_height_m,
    water_depth_m,
    current_x_m_s,
    current_y_m_s,
    wave_orbital_x_m_s,
    wave_orbital_y_m_s,
    significant_wave_height_m,
    wave_surface_displacement_m,
    rogue_crest_excess_m,
);

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn sample_playground_environment(
    width_m: f32,
    height_m: f32,
    match_seed: u64,
    scalar_tick: u64,
    scalar_tick_fraction: f32,
    world_tick: u64,
    world_tick_fraction: f32,
    pressure_tick: u64,
    pressure_tick_fraction: f32,
    current_tick: u64,
    current_tick_fraction: f32,
    center_x_m: f32,
    center_y_m: f32,
) -> Result<PlaygroundEnvironmentSample, JsValue> {
    let geometry = WorldGeometry::new(width_m, height_m).map_err(|error| error.to_string())?;
    let sample = procedural_world_environment_for_render_clocks(
        geometry,
        match_seed,
        world_tick,
        world_tick_fraction,
        scalar_tick,
        scalar_tick_fraction,
        pressure_tick,
        pressure_tick_fraction,
        current_tick,
        current_tick_fraction,
        Vector::new(center_x_m, center_y_m),
    );
    let primary = sample.weather.primary;
    let lighting = celestial_lighting(sample.celestial);
    Ok(PlaygroundEnvironmentSample {
        geographic_latitude_degrees: sample.celestial.geographic_latitude_degrees,
        season_phase: sample.celestial.season_phase,
        day_phase: sample.celestial.day_phase,
        moon_phase: moon_phase(sample.celestial),
        annual_mean_air_temperature_c: sample.climate.annual_mean_air_temperature_c,
        seasonal_temperature_range_c: sample.climate.seasonal_temperature_range_c,
        diurnal_temperature_range_c: sample.climate.diurnal_temperature_range_c,
        pressure_hpa: primary.pressure_hpa,
        pressure_gradient_x_hpa_per_m: primary.pressure_gradient_hpa_per_m.x,
        pressure_gradient_y_hpa_per_m: primary.pressure_gradient_hpa_per_m.y,
        synoptic_pressure_gradient_x_hpa_per_m: primary.synoptic_pressure_gradient_hpa_per_m.x,
        synoptic_pressure_gradient_y_hpa_per_m: primary.synoptic_pressure_gradient_hpa_per_m.y,
        surface_air_temperature_anomaly_c: primary.surface_air_temperature_anomaly_c,
        air_temperature_500_hpa_c: primary.air_temperature_500_hpa_c,
        temperature_gradient_x_c_per_m: primary.local_temperature_gradient_c_per_m.x,
        temperature_gradient_y_c_per_m: primary.local_temperature_gradient_c_per_m.y,
        water_vapor_g_kg: primary.water_vapor_g_kg,
        reference_tidal_range_m: sample.tide.reference_range_m,
        tidal_phase_offset: sample.tide.phase_offset,
        surface_air_temperature_c: sample.weather.surface_air_temperature_c,
        wind_x_m_s: sample.weather.wind_velocity_m_s.x,
        wind_y_m_s: sample.weather.wind_velocity_m_s.y,
        upper_wind_x_m_s: sample.weather.upper_wind_velocity_m_s.x,
        upper_wind_y_m_s: sample.weather.upper_wind_velocity_m_s.y,
        relative_humidity: sample.weather.relative_humidity,
        lift: sample.weather.lift,
        cloudiness: sample.weather.cloudiness,
        instability: sample.weather.instability,
        upper_cloudiness: sample.weather.upper_cloudiness,
        rain_intensity: sample.weather.rain_intensity,
        sun_x: lighting.sun_direction[0],
        sun_y: lighting.sun_direction[1],
        sun_z: lighting.sun_direction[2],
        sun_intensity: lighting.sun_intensity,
        moon_x: lighting.moon_direction[0],
        moon_y: lighting.moon_direction[1],
        moon_z: lighting.moon_direction[2],
        moon_intensity: lighting.moon_intensity,
        solar_eclipse_fraction: lighting.solar_eclipse_fraction,
        lunar_penumbra_fraction: lighting.lunar_penumbra_fraction,
        lunar_umbra_fraction: lighting.lunar_umbra_fraction,
        ambient_intensity: lighting.ambient_intensity,
        ground_temperature_c: sample.surface_climate.ground_temperature_c,
        water_temperature_c: sample.surface_climate.water_temperature_c,
        seasonal_snow_cover: sample.surface_climate.seasonal_snow_cover,
        polar_ice_closure: sample.surface_climate.polar_ice_closure,
        solid_ice_cover: sample.surface_climate.solid_ice_cover,
        water_height_m: sample.water_height_m,
        water_depth_m: sample.environment.water_depth_m,
        current_x_m_s: sample.ocean.current_velocity_m_s.x,
        current_y_m_s: sample.ocean.current_velocity_m_s.y,
        wave_orbital_x_m_s: sample.ocean.wave_orbital_velocity_m_s.x,
        wave_orbital_y_m_s: sample.ocean.wave_orbital_velocity_m_s.y,
        significant_wave_height_m: sample.ocean.significant_wave_height_m,
        wave_surface_displacement_m: sample.ocean.surface_displacement_m,
        rogue_crest_excess_m: sample.ocean.rogue_crest_excess_m,
    })
}

#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn sample_playground_view_cloud_state(
    world_width_m: f32,
    world_height_m: f32,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    cloud_tick: u64,
    cloud_tick_fraction: f32,
    pressure_tick: u64,
    pressure_tick_fraction: f32,
    current_tick: u64,
    current_tick_fraction: f32,
    camera_x_m: f32,
    camera_y_m: f32,
) -> Result<Vec<f32>, JsValue> {
    let geometry = WorldGeometry::new(world_width_m, world_height_m)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let state = sample_view_cloud_state_for_render_clocks(
        geometry,
        match_seed,
        world_tick,
        world_tick_fraction,
        cloud_tick,
        cloud_tick_fraction,
        pressure_tick,
        pressure_tick_fraction,
        current_tick,
        current_tick_fraction,
        Vector::new(camera_x_m, camera_y_m),
    );
    Ok(vec![
        state.low_wind_velocity_m_s.x,
        state.low_wind_velocity_m_s.y,
        state.upper_wind_velocity_m_s.x,
        state.upper_wind_velocity_m_s.y,
        state.cloud_time_coordinates.low,
        state.cloud_time_coordinates.upper,
        0.0,
        0.0,
    ])
}

#[wasm_bindgen]
pub fn sample_playground_world_phases(world_time_seconds: f64) -> Vec<f32> {
    let conditions =
        world_celestial_conditions(WorldGeometry::default(), world_time_seconds, Vector::ZERO);
    vec![
        conditions.day_phase,
        conditions.season_phase,
        conditions.lunar_longitude_phase,
        moon_phase(conditions),
    ]
}

#[cfg(feature = "playground")]
#[wasm_bindgen]
pub fn playground_scene_uniform_bytes() -> usize {
    mem::size_of::<SceneUniforms>()
}

#[cfg(feature = "playground")]
/// Scene uniforms for the lab's independent clocks, with the field grids they
/// describe. The view-local cloud motion, cloud time, and exposure/output rows
/// come from lab state.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn playground_scene_uniforms(
    world_width_m: f32,
    world_height_m: f32,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    cloud_tick: u64,
    cloud_tick_fraction: f32,
    pressure_tick: u64,
    pressure_tick_fraction: f32,
    current_tick: u64,
    current_tick_fraction: f32,
    camera_x_m: f32,
    camera_y_m: f32,
    view_height_m: f32,
    render_width_px: u32,
    render_height_px: u32,
    view_cloud_motion_m: &[f32],
    cloud_time_coordinates: &[f32],
    exposure_and_output: &[f32],
    low_cloud_body_size: f32,
    landform_shadow_reach_m: f32,
) -> Result<PlaygroundSceneFrame, JsValue> {
    let geometry = WorldGeometry::new(world_width_m, world_height_m)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let row = |values: &[f32]| -> Result<[f32; 4], JsValue> {
        values
            .try_into()
            .map_err(|_| JsValue::from_str("scene uniform rows need four values"))
    };
    let view_cloud_motion_m = row(view_cloud_motion_m)?;
    let precipitation_time = periodic_effect_time_interpolated(
        cloud_tick,
        cloud_tick_fraction,
        PRECIPITATION_CONTACT_PERIOD_TICKS,
    );
    let scalar_time = scalar_field_time_coordinates_interpolated(cloud_tick, cloud_tick_fraction);
    let pressure_time =
        vector_field_time_coordinates_interpolated(geometry, pressure_tick, pressure_tick_fraction);
    let current_time =
        vector_field_time_coordinates_interpolated(geometry, current_tick, current_tick_fraction);
    let celestial = world_celestial_conditions(
        geometry,
        (world_tick as f64 + f64::from(world_tick_fraction)) / TICKS_PER_SECOND as f64,
        Vector::ZERO,
    );
    let orbit = celestial_orbit(celestial);
    let view_size_m = [
        view_width_m(
            render_width_px as f32 / render_height_px as f32,
            view_height_m,
        ),
        view_height_m,
    ];
    let grids = field_grids(
        geometry,
        (world_tick as f64 + f64::from(world_tick_fraction)) / TICKS_PER_SECOND as f64,
        [camera_x_m, camera_y_m],
        view_size_m,
        view_cloud_motion_m,
        FieldGridTuning {
            low_cloud_body_size,
            landform_shadow_reach_m,
        },
    );
    let (occluder_field_grid_m, occluder_field_texels) = grids.occluder_uniforms();
    let uniforms = SceneUniforms {
        camera_and_view_m: [camera_x_m, camera_y_m, view_size_m[0], view_size_m[1]],
        scalar_field_time_coordinates: [
            scalar_time.air_temperature,
            scalar_time.upper_air_temperature,
            scalar_time.water_vapor,
            scalar_time.decorative_water,
        ],
        match_seed: [match_seed as u32, (match_seed >> 32) as u32],
        precipitation_epoch: [
            precipitation_time.epoch as u32,
            (precipitation_time.epoch >> 32) as u32,
        ],
        world_size_m: [world_width_m, world_height_m],
        inverse_world_size_m: [world_width_m.recip(), world_height_m.recip()],
        precipitation_phase_and_padding: [precipitation_time.phase, 0.0, 0.0, 0.0],
        world_phases_and_pixel_footprint_m: [
            celestial.day_phase,
            celestial.season_phase,
            celestial.lunar_longitude_phase,
            visual_pixel_footprint_m(view_height_m, render_height_px),
        ],
        pressure_time_coordinate: pressure_time.pressure,
        ocean_current_time_coordinate_and_padding: [current_time.ocean_current, 0.0, 0.0, 0.0],
        seasonal_phase_factors: seasonal_phase_factors(celestial.season_phase),
        celestial_orbit_angles: [
            orbit.solar_declination_rad as f32,
            orbit.solar_right_ascension_rad as f32,
            orbit.lunar_declination_rad as f32,
            orbit.lunar_right_ascension_rad as f32,
        ],
        celestial_orbit_light: [
            orbit.illuminated_fraction as f32,
            orbit.lunar_penumbra_fraction,
            orbit.lunar_umbra_fraction,
            0.0,
        ],
        view_cloud_motion_m,
        cloud_time_coordinates: row(cloud_time_coordinates)?,
        exposure_and_output: row(exposure_and_output)?,
        environment_field_grid_m: grids.environment.uniforms,
        environment_field_texels: grids.environment.texel_uniforms(),
        occluder_field_grid_m,
        occluder_field_texels,
        ocean_wave_modes: ocean_wave_mode_uniforms(
            geometry,
            match_seed,
            world_tick,
            world_tick_fraction,
        ),
    };
    let mut field_grid_summary: Vec<f32> = grids
        .environment
        .workgroups()
        .into_iter()
        .chain(grids.occluder_workgroups())
        .map(|count| count as f32)
        .collect();
    for grid in &grids.occluders {
        field_grid_summary.extend([
            grid.uniforms[2],
            grid.uniforms[3],
            grid.texels[0] as f32,
            grid.texels[1] as f32,
        ]);
    }
    Ok(PlaygroundSceneFrame {
        uniforms: bytemuck::bytes_of(&uniforms).to_vec(),
        field_grids: field_grid_summary,
    })
}

#[wasm_bindgen]
impl PlaygroundWorldGeometrySample {
    #[wasm_bindgen(getter)]
    pub fn raw_x_m(&self) -> f32 {
        self.raw_x_m
    }

    #[wasm_bindgen(getter)]
    pub fn raw_y_m(&self) -> f32 {
        self.raw_y_m
    }

    #[wasm_bindgen(getter)]
    pub fn canonical_x_m(&self) -> f32 {
        self.canonical_x_m
    }

    #[wasm_bindgen(getter)]
    pub fn canonical_y_m(&self) -> f32 {
        self.canonical_y_m
    }

    #[wasm_bindgen(getter)]
    pub fn normalized_x(&self) -> f32 {
        self.normalized_x
    }

    #[wasm_bindgen(getter)]
    pub fn normalized_y(&self) -> f32 {
        self.normalized_y
    }
}

#[wasm_bindgen]
pub fn sample_playground_world_geometry(
    width_m: f32,
    height_m: f32,
    raw_x_m: f32,
    raw_y_m: f32,
) -> Result<PlaygroundWorldGeometrySample, JsValue> {
    let geometry = WorldGeometry::new(width_m, height_m).map_err(|error| error.to_string())?;
    let sample = sample_world_geometry(geometry, [raw_x_m, raw_y_m], 0.0);
    Ok(PlaygroundWorldGeometrySample {
        raw_x_m: sample.raw_center_m[0],
        raw_y_m: sample.raw_center_m[1],
        canonical_x_m: sample.canonical_center_m[0],
        canonical_y_m: sample.canonical_center_m[1],
        normalized_x: sample.normalized_world[0],
        normalized_y: sample.normalized_world[1],
    })
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 2],
    color: [f32; 3],
    is_terrain: f32,
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3, 2 => Float32];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SceneUniforms {
    camera_and_view_m: [f32; 4],
    scalar_field_time_coordinates: [f32; 4],
    match_seed: [u32; 2],
    precipitation_epoch: [u32; 2],
    world_size_m: [f32; 2],
    inverse_world_size_m: [f32; 2],
    precipitation_phase_and_padding: [f32; 4],
    world_phases_and_pixel_footprint_m: [f32; 4],
    pressure_time_coordinate: [f32; 4],
    ocean_current_time_coordinate_and_padding: [f32; 4],
    seasonal_phase_factors: [f32; 4],
    celestial_orbit_angles: [f32; 4],
    celestial_orbit_light: [f32; 4],
    view_cloud_motion_m: [f32; 4],
    cloud_time_coordinates: [f32; 4],
    exposure_and_output: [f32; 4],
    environment_field_grid_m: [f32; 4],
    environment_field_texels: [u32; 4],
    occluder_field_grid_m: [[f32; 4]; OCCLUDER_FIELD_LAYERS],
    occluder_field_texels: [[u32; 4]; OCCLUDER_FIELD_LAYERS],
    ocean_wave_modes: [[f32; 4]; OCEAN_WAVE_MODE_COUNT],
}

struct EnvironmentFields {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    pipeline: wgpu::ComputePipeline,
    output_bind_group: wgpu::BindGroup,
}

impl EnvironmentFields {
    fn new(device: &wgpu::Device, scene_uniform_layout: &wgpu::BindGroupLayout) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Environment fields"),
            size: wgpu::Extent3d {
                width: ENVIRONMENT_FIELD_TEXELS,
                height: ENVIRONMENT_FIELD_TEXELS,
                depth_or_array_layers: ENVIRONMENT_FIELD_LAYERS,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Environment field output layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba32Float,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Environment field shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(ENVIRONMENT_FIELD_SHADER)),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Environment field pipeline layout"),
            bind_group_layouts: &[Some(scene_uniform_layout), Some(&output_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Environment field pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("generate_environment_fields"),
            compilation_options: Default::default(),
            cache: None,
        });
        let output_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Environment field output bind group"),
            layout: &output_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        Self {
            _texture: texture,
            view,
            pipeline,
            output_bind_group,
        }
    }

    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene_uniform_bind_group: &wgpu::BindGroup,
        grids: &FieldGrids,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Environment field pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, scene_uniform_bind_group, &[]);
        pass.set_bind_group(1, &self.output_bind_group, &[]);
        let [columns, rows] = grids.environment.workgroups();
        pass.dispatch_workgroups(columns, rows, 1);
    }
}

struct OccluderFields {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    pipeline: wgpu::ComputePipeline,
    input_bind_group: wgpu::BindGroup,
    output_bind_group: wgpu::BindGroup,
}

impl OccluderFields {
    fn new(
        device: &wgpu::Device,
        uniform_buffer: &wgpu::Buffer,
        environment_fields: &EnvironmentFields,
        empty_layout: &wgpu::BindGroupLayout,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Occluder fields"),
            size: wgpu::Extent3d {
                width: OCCLUDER_FIELD_TEXELS,
                height: OCCLUDER_FIELD_TEXELS,
                depth_or_array_layers: OCCLUDER_FIELD_LAYERS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let input_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Occluder field input layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let output_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Occluder field output layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::StorageTexture {
                    access: wgpu::StorageTextureAccess::WriteOnly,
                    format: wgpu::TextureFormat::Rgba16Float,
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Occluder field shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(OCCLUDER_FIELD_SHADER)),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Occluder field pipeline layout"),
            bind_group_layouts: &[
                Some(&input_layout),
                Some(empty_layout),
                Some(&output_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Occluder field pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("generate_occluder_fields"),
            compilation_options: Default::default(),
            cache: None,
        });
        let input_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Occluder field input bind group"),
            layout: &input_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&environment_fields.view),
                },
            ],
        });
        let output_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Occluder field output bind group"),
            layout: &output_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        Self {
            _texture: texture,
            view,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Occluder field sampler"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            pipeline,
            input_bind_group,
            output_bind_group,
        }
    }

    fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        empty_bind_group: &wgpu::BindGroup,
        grids: &FieldGrids,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Occluder field pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.input_bind_group, &[]);
        pass.set_bind_group(1, empty_bind_group, &[]);
        pass.set_bind_group(2, &self.output_bind_group, &[]);
        let [columns, rows, layers] = grids.occluder_workgroups();
        pass.dispatch_workgroups(columns, rows, layers);
    }
}

struct Renderer {
    canvas: HtmlCanvasElement,
    device: wgpu::Device,
    queue: wgpu::Queue,
    frame_queue: FrameQueue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    scene_bind_group: wgpu::BindGroup,
    empty_bind_group: wgpu::BindGroup,
    environment_fields: EnvironmentFields,
    occluder_fields: OccluderFields,
    terminal_error: Arc<AtomicBool>,
    cloud_motion: ViewCloudMotion,
    exposure: ViewExposure,
    vision_noise_phase: f64,
}

impl Renderer {
    async fn new(canvas: HtmlCanvasElement) -> Result<Self, JsValue> {
        let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU;
        let instance = wgpu::Instance::new(instance_descriptor);
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
            .map_err(js_error)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(js_error)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("WebGPU device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(js_error)?;
        let terminal_error = Arc::new(AtomicBool::new(false));
        let device_lost_callback = Arc::clone(&terminal_error);
        device.set_device_lost_callback(move |_, _| {
            device_lost_callback.store(true, Ordering::Release);
        });
        let uncaptured_error_callback = Arc::clone(&terminal_error);
        device.on_uncaptured_error(Arc::new(move |_| {
            uncaptured_error_callback.store(true, Ordering::Release);
        }));
        let (width, height) = canvas_dimensions(&canvas, device.limits().max_texture_dimension_2d);
        canvas.set_width(width);
        canvas.set_height(height);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| JsValue::from_str("WebGPU surface has no supported configuration"))?;
        let capabilities = surface.get_capabilities(&adapter);
        let hdr_display = surface
            .display_hdr_info(&adapter)
            .coarse
            .is_some_and(|info| info.high_dynamic_range == Some(true));
        if hdr_display
            && capabilities
                .color_spaces(wgpu::TextureFormat::Rgba16Float)
                .contains(wgpu::SurfaceColorSpaces::EXTENDED_SRGB)
        {
            let sdr_config = config.clone();
            config.format = wgpu::TextureFormat::Rgba16Float;
            config.color_space = wgpu::SurfaceColorSpace::ExtendedSrgb;
            surface.configure(&device, &config);
            if !canvas_has_hdr_output(&canvas) {
                config = sdr_config;
            }
        }
        let output_format = config.format.add_srgb_suffix();
        if output_format != config.format {
            config.view_formats.push(output_format);
        }
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Scene shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SCENE_SHADER)),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene uniforms layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let empty_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Empty bind group layout"),
                entries: &[],
            });
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Scene layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Scene pipeline layout"),
            bind_group_layouts: &[Some(&scene_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Scene pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[Some(Vertex::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[Some(output_format.into())],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Dynamic vertices"),
            size: (INITIAL_VERTEX_CAPACITY * mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Scene uniforms"),
            size: mem::size_of::<SceneUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scene uniforms bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let environment_fields = EnvironmentFields::new(&device, &bind_group_layout);
        let occluder_fields = OccluderFields::new(
            &device,
            &uniform_buffer,
            &environment_fields,
            &empty_bind_group_layout,
        );
        let scene_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Scene bind group"),
            layout: &scene_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&environment_fields.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&occluder_fields.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&occluder_fields.sampler),
                },
            ],
        });
        let empty_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Empty bind group"),
            layout: &empty_bind_group_layout,
            entries: &[],
        });

        Ok(Self {
            canvas,
            device,
            queue,
            frame_queue: FrameQueue::default(),
            surface,
            config,
            pipeline,
            vertex_buffer,
            vertex_capacity: INITIAL_VERTEX_CAPACITY,
            uniform_buffer,
            bind_group,
            scene_bind_group,
            empty_bind_group,
            environment_fields,
            occluder_fields,
            terminal_error,
            cloud_motion: ViewCloudMotion::default(),
            exposure: ViewExposure::default(),
            vision_noise_phase: 0.0,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn render(
        &mut self,
        scenes: &[BoatScene],
        match_seed: Option<u64>,
        world_geometry: WorldGeometry,
        render_clock: Option<(u64, f32)>,
        elapsed_seconds: f64,
        camera_m: Vector,
        view_height_m: f32,
    ) -> Result<(), JsValue> {
        if self.terminal_error.load(Ordering::Acquire) {
            return Err(JsValue::from_str("WebGPU device was lost"));
        }
        self.resize();
        let view_width_m = view_width_m(
            self.config.width as f32 / self.config.height as f32,
            view_height_m,
        );
        let canonical_camera_m = world_geometry.wrap_position_m(camera_m.into());
        let (tick, tick_fraction) = render_clock.unwrap_or_default();
        let celestial = world_celestial_conditions(
            world_geometry,
            (tick as f64 + f64::from(tick_fraction)) / TICKS_PER_SECOND as f64,
            Vector::ZERO,
        );
        let orbit = celestial_orbit(celestial);
        let has_match = match_seed.is_some();
        let match_seed = match_seed.unwrap_or(0);
        let view_cloud_state = sample_view_cloud_state_for_render_clocks(
            world_geometry,
            match_seed,
            tick,
            tick_fraction,
            tick,
            tick_fraction,
            tick,
            tick_fraction,
            tick,
            tick_fraction,
            canonical_camera_m.into(),
        );
        if render_clock.is_some() {
            self.cloud_motion.advance(
                elapsed_seconds,
                view_cloud_state.low_wind_velocity_m_s,
                view_cloud_state.upper_wind_velocity_m_s,
            );
        }
        let precipitation_time = periodic_effect_time_interpolated(
            tick,
            tick_fraction,
            PRECIPITATION_CONTACT_PERIOD_TICKS,
        );
        let scalar_time = scalar_field_time_coordinates_interpolated(tick, tick_fraction);
        let vector_time =
            vector_field_time_coordinates_interpolated(world_geometry, tick, tick_fraction);
        let cloud_time = view_cloud_state.cloud_time_coordinates;
        let view_cloud_motion_m = [
            self.cloud_motion.low_offset_m[0] as f32,
            self.cloud_motion.low_offset_m[1] as f32,
            self.cloud_motion.upper_offset_m[0] as f32,
            self.cloud_motion.upper_offset_m[1] as f32,
        ];
        let field_grids = field_grids(
            world_geometry,
            (tick as f64 + f64::from(tick_fraction)) / TICKS_PER_SECOND as f64,
            canonical_camera_m,
            [view_width_m, view_height_m],
            view_cloud_motion_m,
            FieldGridTuning::PRODUCTION,
        );
        let (occluder_field_grid_m, occluder_field_texels) = field_grids.occluder_uniforms();
        let local_celestial = world_celestial_conditions(
            world_geometry,
            (tick as f64 + f64::from(tick_fraction)) / TICKS_PER_SECOND as f64,
            canonical_camera_m.into(),
        );
        let target = crate::exposure_target(celestial_lighting(local_celestial).ambient_intensity);
        let exposure = if render_clock.is_some() {
            self.exposure.advance(target, elapsed_seconds)
        } else {
            1.0
        };
        self.vision_noise_phase =
            (self.vision_noise_phase + elapsed_seconds.max(0.0) * 8.0) % 4096.0;
        let hdr = self.config.format == wgpu::TextureFormat::Rgba16Float;
        let output_peak = if hdr && canvas_has_hdr_output(&self.canvas) {
            4.0
        } else {
            1.0
        };
        let Some(frame_permit) = self.frame_queue.try_acquire() else {
            return Ok(());
        };
        let frame = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(frame) => frame,
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return Ok(()),
            CurrentSurfaceTexture::Suboptimal(frame) => {
                drop(frame);
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            CurrentSurfaceTexture::Validation => {
                self.terminal_error.store(true, Ordering::Release);
                return Err(JsValue::from_str(
                    "WebGPU rejected the current surface texture",
                ));
            }
        };
        let vertices = scene_vertices(
            scenes,
            has_match,
            camera_m,
            self.config.width,
            self.config.height,
            view_height_m,
        );
        if vertices.len() > self.vertex_capacity {
            self.vertex_capacity = vertices.len().next_power_of_two();
            self.vertex_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Dynamic vertices"),
                size: (self.vertex_capacity * mem::size_of::<Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        self.queue
            .write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&SceneUniforms {
                camera_and_view_m: [
                    canonical_camera_m[0],
                    canonical_camera_m[1],
                    view_width_m,
                    view_height_m,
                ],
                scalar_field_time_coordinates: [
                    scalar_time.air_temperature,
                    scalar_time.upper_air_temperature,
                    scalar_time.water_vapor,
                    scalar_time.decorative_water,
                ],
                match_seed: [match_seed as u32, (match_seed >> 32) as u32],
                precipitation_epoch: [
                    precipitation_time.epoch as u32,
                    (precipitation_time.epoch >> 32) as u32,
                ],
                world_size_m: [world_geometry.width_m(), world_geometry.height_m()],
                inverse_world_size_m: [
                    world_geometry.width_m().recip(),
                    world_geometry.height_m().recip(),
                ],
                precipitation_phase_and_padding: [precipitation_time.phase, 0.0, 0.0, 0.0],
                world_phases_and_pixel_footprint_m: [
                    celestial.day_phase,
                    celestial.season_phase,
                    celestial.lunar_longitude_phase,
                    visual_pixel_footprint_m(view_height_m, self.config.height),
                ],
                pressure_time_coordinate: vector_time.pressure,
                ocean_current_time_coordinate_and_padding: [
                    vector_time.ocean_current,
                    0.0,
                    0.0,
                    0.0,
                ],
                seasonal_phase_factors: seasonal_phase_factors(celestial.season_phase),
                celestial_orbit_angles: [
                    orbit.solar_declination_rad as f32,
                    orbit.solar_right_ascension_rad as f32,
                    orbit.lunar_declination_rad as f32,
                    orbit.lunar_right_ascension_rad as f32,
                ],
                celestial_orbit_light: [
                    orbit.illuminated_fraction as f32,
                    orbit.lunar_penumbra_fraction,
                    orbit.lunar_umbra_fraction,
                    0.0,
                ],
                view_cloud_motion_m,
                cloud_time_coordinates: [cloud_time.low, cloud_time.upper, 0.0, 0.0],
                // WebGPU exposes HDR capability but no numeric display headroom.
                exposure_and_output: [
                    exposure,
                    output_peak,
                    f32::from(hdr),
                    self.vision_noise_phase as f32,
                ],
                environment_field_grid_m: field_grids.environment.uniforms,
                environment_field_texels: field_grids.environment.texel_uniforms(),
                occluder_field_grid_m,
                occluder_field_texels,
                ocean_wave_modes: ocean_wave_mode_uniforms(
                    world_geometry,
                    match_seed,
                    tick,
                    tick_fraction,
                ),
            }),
        );

        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.config.format.add_srgb_suffix()),
            ..Default::default()
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame encoder"),
            });
        self.environment_fields
            .encode(&mut encoder, &self.bind_group, &field_grids);
        self.occluder_fields
            .encode(&mut encoder, &self.empty_bind_group, &field_grids);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scene pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.11,
                            b: 0.15,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.scene_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue
            .on_submitted_work_done(move || drop(frame_permit));
        self.queue.present(frame);
        Ok(())
    }

    fn resize(&mut self) {
        let (width, height) =
            canvas_dimensions(&self.canvas, self.device.limits().max_texture_dimension_2d);
        if self.config.width == width && self.config.height == height {
            return;
        }
        self.canvas.set_width(width);
        self.canvas.set_height(height);
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    fn aspect_ratio(&self) -> f32 {
        let (width, height) =
            canvas_dimensions(&self.canvas, self.device.limits().max_texture_dimension_2d);
        width as f32 / height as f32
    }

    fn is_available(&self) -> bool {
        !self.terminal_error.load(Ordering::Acquire)
    }
}

#[wasm_bindgen]
pub struct BrowserGame {
    client: Rc<RefCell<MultiplayerClient>>,
    pending_snapshot: Rc<RefCell<Option<ServerMessage>>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
    close_notice: Rc<RefCell<Option<String>>>,
    transport: Rc<RefCell<Option<Session>>>,
    pending_datagrams: Rc<RefCell<VecDeque<Vec<u8>>>>,
    pending_viewport: Rc<RefCell<Option<Vec<u8>>>>,
    sending_datagrams: Rc<Cell<bool>>,
    sending_reliable: Rc<Cell<bool>>,
    renderer: Renderer,
    view_height_m: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum ConnectionStatus {
    Connecting,
    Open,
    Error,
    Closed,
}

#[wasm_bindgen]
impl BrowserGame {
    pub async fn create(canvas: HtmlCanvasElement) -> Result<BrowserGame, JsValue> {
        console_error_panic_hook::set_once();
        let renderer = Renderer::new(canvas).await?;
        let client = Rc::new(RefCell::new(MultiplayerClient::new()));
        let pending_snapshot = Rc::new(RefCell::new(None));
        let connection_status = Rc::new(Cell::new(ConnectionStatus::Connecting));
        let close_notice = Rc::new(RefCell::new(None));
        let transport = Rc::new(RefCell::new(None));
        let pending_datagrams = Rc::new(RefCell::new(VecDeque::new()));
        let pending_viewport = Rc::new(RefCell::new(None));
        let sending_datagrams = Rc::new(Cell::new(false));
        let sending_reliable = Rc::new(Cell::new(false));

        connect_webtransport(
            Rc::clone(&client),
            Rc::clone(&pending_snapshot),
            Rc::clone(&connection_status),
            Rc::clone(&close_notice),
            Rc::clone(&transport),
            Rc::clone(&pending_datagrams),
            Rc::clone(&pending_viewport),
            Rc::clone(&sending_datagrams),
            Rc::clone(&sending_reliable),
        )?;

        Ok(Self {
            client,
            pending_snapshot,
            connection_status,
            close_notice,
            transport,
            pending_datagrams,
            pending_viewport,
            sending_datagrams,
            sending_reliable,
            renderer,
            view_height_m: DEFAULT_VIEW_HEIGHT_M,
        })
    }

    pub fn frame(&mut self, elapsed_seconds: f64) -> Result<(), JsValue> {
        if !self.renderer.is_available() {
            return Err(JsValue::from_str("WebGPU renderer is unavailable"));
        }
        let viewport_radius_m =
            viewport_interest_radius_m(self.renderer.aspect_ratio(), self.view_height_m);
        let mut client = self.client.borrow_mut();
        let viewport = client.update_viewport(viewport_radius_m);
        let inputs = client.take_inputs(elapsed_seconds);
        drop(client);
        if let Some(viewport) = viewport {
            self.pending_viewport
                .replace(Some(encode_client_message(&viewport).map_err(js_error)?));
        }
        for input in inputs {
            let mut pending = self.pending_datagrams.borrow_mut();
            if pending.len() == MAXIMUM_PENDING_DATAGRAMS {
                pending.pop_front();
            }
            pending.push_back(encode_client_message(&input).map_err(js_error)?);
        }
        pump_datagrams(
            &self.transport,
            &self.pending_datagrams,
            &self.sending_datagrams,
        );
        pump_reliable(
            &self.transport,
            &self.pending_viewport,
            &self.sending_reliable,
        );
        {
            let mut client = self.client.borrow_mut();
            if client.is_ready()
                && let Some(snapshot) = self.pending_snapshot.borrow_mut().take()
            {
                client.accept(snapshot);
            }
        }
        let client = self.client.borrow();
        self.renderer.render(
            &client.scenes(),
            client.match_seed(),
            client.world_geometry().unwrap_or_default(),
            client.render_clock(),
            elapsed_seconds,
            client.camera_m().unwrap_or(Vector::ZERO),
            self.view_height_m,
        )
    }

    pub fn renderer_is_available(&self) -> bool {
        self.renderer.is_available()
    }

    pub fn shutdown(&mut self) {
        self.connection_status.set(ConnectionStatus::Closed);
        self.pending_datagrams.borrow_mut().clear();
        self.pending_viewport.borrow_mut().take();
        disconnect_and_clear_pending_snapshot(
            &mut self.client.borrow_mut(),
            &mut self.pending_snapshot.borrow_mut(),
        );
        if let Some(session) = self.transport.borrow_mut().take() {
            session.close(0, "Client shutdown");
        }
    }

    pub fn zoom(&mut self, wheel_delta_y: f32) {
        self.view_height_m = zoom_view_height_m(self.view_height_m, wheel_delta_y);
    }

    pub fn reset_zoom(&mut self) {
        self.view_height_m = DEFAULT_VIEW_HEIGHT_M;
    }

    pub fn scale_bar_width_px(&self) -> f32 {
        scale_bar(
            self.view_height_m,
            self.renderer.canvas.client_height() as f32,
        )
        .width_px
    }

    pub fn scale_bar_label(&self) -> String {
        scale_bar(
            self.view_height_m,
            self.renderer.canvas.client_height() as f32,
        )
        .label
    }

    pub fn set_player_input(
        &mut self,
        keyboard_forward: f32,
        keyboard_turn: f32,
        world_x: f32,
        world_y: f32,
    ) {
        self.client.borrow_mut().set_player_input(
            keyboard_forward,
            keyboard_turn,
            world_x,
            world_y,
        );
    }

    pub fn speed_m_s(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.speed_m_s)
            .unwrap_or(0.0)
    }

    pub fn apparent_wind_speed_m_s(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.apparent_wind_speed_m_s)
            .unwrap_or(0.0)
    }

    pub fn water_depth_m(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.water_depth_m)
            .unwrap_or(0.0)
    }

    pub fn bottom_clearance_m(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.bottom_clearance_m)
            .unwrap_or(0.0)
    }

    pub fn bottom_contact_fraction(&self) -> f32 {
        self.client
            .borrow()
            .view()
            .map(|view| view.bottom_contact_fraction)
            .unwrap_or(0.0)
    }

    pub fn health(&self) -> f32 {
        self.client.borrow().health().unwrap_or(0.0)
    }

    pub fn point_of_sail(&self) -> String {
        self.client
            .borrow()
            .view()
            .map(|view| view.point_of_sail.label().to_owned())
            .unwrap_or_else(|| "waiting".to_owned())
    }

    pub fn status(&self) -> String {
        match self.connection_status.get() {
            ConnectionStatus::Connecting => "Connecting to authoritative server…".to_owned(),
            ConnectionStatus::Open => self
                .client
                .borrow()
                .player_id()
                .map(|player_id| format!("Authoritative server · {player_id}"))
                .unwrap_or_else(|| "Connected · waiting for identity…".to_owned()),
            ConnectionStatus::Error => "Server connection failed".to_owned(),
            ConnectionStatus::Closed => self
                .close_notice
                .borrow()
                .as_ref()
                .map(|notice| format!("Server connection closed · {notice}"))
                .unwrap_or_else(|| "Server connection closed".to_owned()),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn connect_webtransport(
    protocol_client: Rc<RefCell<MultiplayerClient>>,
    pending_snapshot: Rc<RefCell<Option<ServerMessage>>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
    close_notice: Rc<RefCell<Option<String>>>,
    transport: Rc<RefCell<Option<Session>>>,
    pending_datagrams: Rc<RefCell<VecDeque<Vec<u8>>>>,
    pending_viewport: Rc<RefCell<Option<Vec<u8>>>>,
    sending_datagrams: Rc<Cell<bool>>,
    sending_reliable: Rc<Cell<bool>>,
) -> Result<(), JsValue> {
    let url = Url::parse(&server_url()?).map_err(js_error)?;
    let builder = ClientBuilder::new()
        .with_pooling(false)
        .with_unreliable(true);
    let webtransport_client = match configured_certificate_hash()? {
        Some(hash) => builder.with_server_certificate_hashes(vec![hash]),
        None => builder.with_system_roots(),
    };

    spawn_local(async move {
        let session = match webtransport_client.connect(url).await {
            Ok(session) => session,
            Err(error) => {
                if connection_status.get() == ConnectionStatus::Connecting {
                    protocol_client.borrow_mut().disconnect();
                    close_notice.replace(Some(error.to_string()));
                    connection_status.set(ConnectionStatus::Error);
                }
                return;
            }
        };
        if connection_status.get() != ConnectionStatus::Connecting {
            session.close(0, "Client shutdown");
            return;
        }
        transport.replace(Some(session.clone()));
        connection_status.set(ConnectionStatus::Open);
        receive_datagrams(
            session.clone(),
            Rc::clone(&protocol_client),
            Rc::clone(&pending_snapshot),
            Rc::clone(&connection_status),
            Rc::clone(&transport),
        );
        receive_reliable_messages(
            session.clone(),
            Rc::clone(&protocol_client),
            Rc::clone(&pending_snapshot),
            Rc::clone(&connection_status),
            Rc::clone(&transport),
        );
        watch_connection_close(
            session,
            protocol_client,
            connection_status,
            close_notice,
            Rc::clone(&pending_snapshot),
            Rc::clone(&transport),
        );
        pump_datagrams(&transport, &pending_datagrams, &sending_datagrams);
        pump_reliable(&transport, &pending_viewport, &sending_reliable);
    });
    Ok(())
}

fn receive_datagrams(
    session: Session,
    protocol_client: Rc<RefCell<MultiplayerClient>>,
    pending_snapshot: Rc<RefCell<Option<ServerMessage>>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
    transport: Rc<RefCell<Option<Session>>>,
) {
    spawn_local(async move {
        while let Ok(message) = session.recv_datagram().await {
            if accept_server_message(
                &message,
                &protocol_client,
                &pending_snapshot,
                &connection_status,
            )
            .is_err()
            {
                fail_protocol(
                    &session,
                    &protocol_client,
                    &pending_snapshot,
                    &connection_status,
                    &transport,
                );
                break;
            }
        }
    });
}

fn receive_reliable_messages(
    session: Session,
    protocol_client: Rc<RefCell<MultiplayerClient>>,
    pending_snapshot: Rc<RefCell<Option<ServerMessage>>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
    transport: Rc<RefCell<Option<Session>>>,
) {
    spawn_local(async move {
        let mut reads = FuturesUnordered::new();
        loop {
            if reads.is_empty() {
                match session.accept_uni().await {
                    Ok(stream) => reads.push(read_reliable_server_message(stream)),
                    Err(_) => break,
                }
                continue;
            }
            let message = if reads.len() >= MAXIMUM_CONCURRENT_RELIABLE_RECEIVES {
                reads
                    .next()
                    .await
                    .unwrap_or(ReliableServerMessageRead::Discarded)
            } else {
                match select(Box::pin(session.accept_uni()), Box::pin(reads.next())).await {
                    Either::Left((stream, read)) => {
                        drop(read);
                        match stream {
                            Ok(stream) => reads.push(read_reliable_server_message(stream)),
                            Err(_) => break,
                        }
                        continue;
                    }
                    Either::Right((message, accept)) => {
                        drop(accept);
                        message.unwrap_or(ReliableServerMessageRead::Discarded)
                    }
                }
            };
            let message = match message {
                ReliableServerMessageRead::Complete(message) => message,
                ReliableServerMessageRead::Discarded => continue,
                ReliableServerMessageRead::TooLarge => {
                    fail_protocol(
                        &session,
                        &protocol_client,
                        &pending_snapshot,
                        &connection_status,
                        &transport,
                    );
                    break;
                }
            };
            if accept_server_message(
                &message,
                &protocol_client,
                &pending_snapshot,
                &connection_status,
            )
            .is_err()
            {
                fail_protocol(
                    &session,
                    &protocol_client,
                    &pending_snapshot,
                    &connection_status,
                    &transport,
                );
                break;
            }
        }
    });
}

enum ReliableServerMessageRead {
    Complete(Vec<u8>),
    Discarded,
    TooLarge,
}

async fn read_reliable_server_message(mut stream: RecvStream) -> ReliableServerMessageRead {
    let mut message = Vec::new();
    loop {
        match stream.read(MAX_SERVER_MESSAGE_BYTES + 1).await {
            Ok(Some(chunk)) => {
                if !append_reliable_server_message_chunk(&mut message, &chunk) {
                    return ReliableServerMessageRead::TooLarge;
                }
            }
            Ok(None) => return ReliableServerMessageRead::Complete(message),
            Err(_) => return ReliableServerMessageRead::Discarded,
        }
    }
}

fn accept_server_message(
    bytes: &[u8],
    protocol_client: &RefCell<MultiplayerClient>,
    pending_snapshot: &RefCell<Option<ServerMessage>>,
    connection_status: &Cell<ConnectionStatus>,
) -> Result<(), protocol::ProtocolError> {
    if connection_status.get() != ConnectionStatus::Open {
        return Ok(());
    }
    match decode_server_message(bytes)? {
        message @ ServerMessage::Welcome { .. } => protocol_client.borrow_mut().accept(message),
        message @ ServerMessage::Snapshot { .. } => {
            queue_latest_snapshot(&mut pending_snapshot.borrow_mut(), message);
        }
    }
    Ok(())
}

fn fail_protocol(
    session: &Session,
    protocol_client: &RefCell<MultiplayerClient>,
    pending_snapshot: &RefCell<Option<ServerMessage>>,
    connection_status: &Cell<ConnectionStatus>,
    transport: &RefCell<Option<Session>>,
) {
    if connection_status.get() != ConnectionStatus::Open {
        return;
    }
    disconnect_and_clear_pending_snapshot(
        &mut protocol_client.borrow_mut(),
        &mut pending_snapshot.borrow_mut(),
    );
    connection_status.set(ConnectionStatus::Error);
    transport.borrow_mut().take();
    session.close(1, "Protocol error");
}

fn watch_connection_close(
    session: Session,
    protocol_client: Rc<RefCell<MultiplayerClient>>,
    connection_status: Rc<Cell<ConnectionStatus>>,
    close_notice: Rc<RefCell<Option<String>>>,
    pending_snapshot: Rc<RefCell<Option<ServerMessage>>>,
    transport: Rc<RefCell<Option<Session>>>,
) {
    spawn_local(async move {
        let error = session.closed().await;
        transport.borrow_mut().take();
        disconnect_and_clear_pending_snapshot(
            &mut protocol_client.borrow_mut(),
            &mut pending_snapshot.borrow_mut(),
        );
        close_notice.replace(Some(error.to_string()));
        if connection_status.get() != ConnectionStatus::Error {
            connection_status.set(ConnectionStatus::Closed);
        }
    });
}

fn pump_datagrams(
    transport: &Rc<RefCell<Option<Session>>>,
    pending: &Rc<RefCell<VecDeque<Vec<u8>>>>,
    sending: &Rc<Cell<bool>>,
) {
    if sending.get() {
        return;
    }
    let Some(session) = transport.borrow().clone() else {
        return;
    };
    sending.set(true);
    let pending = Rc::clone(pending);
    let sending = Rc::clone(sending);
    spawn_local(async move {
        loop {
            let Some(message) = pending.borrow_mut().pop_front() else {
                break;
            };
            if session.send_datagram(Bytes::from(message)).await.is_err() {
                pending.borrow_mut().clear();
                break;
            }
        }
        sending.set(false);
    });
}

fn pump_reliable(
    transport: &Rc<RefCell<Option<Session>>>,
    pending: &Rc<RefCell<Option<Vec<u8>>>>,
    sending: &Rc<Cell<bool>>,
) {
    if sending.get() {
        return;
    }
    let Some(session) = transport.borrow().clone() else {
        return;
    };
    sending.set(true);
    let pending = Rc::clone(pending);
    let sending = Rc::clone(sending);
    let transport = Rc::clone(transport);
    spawn_local(async move {
        loop {
            let Some(message) = pending.borrow_mut().take() else {
                break;
            };
            let result = async {
                let mut stream = session.open_uni().await?;
                stream.write(&message).await?;
                stream.finish()?;
                stream.closed().await
            }
            .await;
            if !matches!(result, Ok(None)) {
                if transport.borrow().is_some() {
                    requeue_failed_viewport(&mut pending.borrow_mut(), message);
                }
                break;
            }
        }
        sending.set(false);
    });
}

fn configured_certificate_hash() -> Result<Option<Vec<u8>>, JsValue> {
    let search = web_sys::window()
        .ok_or_else(|| JsValue::from_str("browser window is unavailable"))?
        .location()
        .search()?;
    let Some(value) = search
        .trim_start_matches('?')
        .split('&')
        .filter_map(|parameter| parameter.split_once('='))
        .find_map(|(key, value)| (key == "webtransportCertificateHash").then_some(value))
    else {
        return Ok(None);
    };
    let bytes = value
        .split(':')
        .map(|byte| {
            u8::from_str_radix(byte, 16)
                .map_err(|_| JsValue::from_str("invalid WebTransport certificate hash"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if bytes.len() != 32 {
        return Err(JsValue::from_str(
            "WebTransport certificate hash must contain 32 bytes",
        ));
    }
    Ok(Some(bytes))
}

fn server_url() -> Result<String, JsValue> {
    let location = web_sys::window()
        .ok_or_else(|| JsValue::from_str("browser window is unavailable"))?
        .location();
    webtransport_server_url(&location.hostname()?, &location.search()?)
        .map_err(|error| JsValue::from_str(&error))
}

fn canvas_dimensions(canvas: &HtmlCanvasElement, maximum_dimension: u32) -> (u32, u32) {
    let scale = web_sys::window()
        .map(|window| window.device_pixel_ratio())
        .unwrap_or(1.0);
    bounded_canvas_dimensions(
        canvas.client_width(),
        canvas.client_height(),
        scale,
        maximum_dimension,
    )
}

fn scene_vertices(
    scenes: &[BoatScene],
    render_terrain: bool,
    camera_m: Vector,
    width: u32,
    height: u32,
    view_height_m: f32,
) -> Vec<Vertex> {
    let aspect = width as f32 / height as f32;
    let view_width_m = view_width_m(aspect, view_height_m);
    let mut vertices = Vec::new();

    if render_terrain {
        for position in [
            [-1.0, -1.0],
            [1.0, -1.0],
            [1.0, 1.0],
            [-1.0, -1.0],
            [1.0, 1.0],
            [-1.0, 1.0],
        ] {
            vertices.push(Vertex {
                position,
                color: [0.0; 3],
                is_terrain: 1.0,
            });
        }
    }

    for scene in scenes {
        let hull_color = if scene.wreck {
            [0.43, 0.37, 0.29]
        } else if scene.recently_damaged {
            [1.0, 0.43, 0.35]
        } else if scene.is_local {
            [0.42, 0.82, 0.69]
        } else {
            [0.91, 0.42, 0.32]
        };
        for [first, second, third] in [[0, 1, 2], [0, 2, 3], [0, 3, 4]] {
            for point in [
                scene.hull_m[first],
                scene.hull_m[second],
                scene.hull_m[third],
            ] {
                vertices.push(vertex(
                    point,
                    hull_color,
                    camera_m,
                    view_width_m,
                    view_height_m,
                ));
            }
        }
        push_quad(
            &mut vertices,
            scene.sail_m[0],
            scene.sail_m[1],
            0.07,
            [0.96, 0.76, 0.25],
            camera_m,
            view_width_m,
            view_height_m,
        );
        push_quad(
            &mut vertices,
            scene.rudder_m[0],
            scene.rudder_m[1],
            0.08,
            [0.86, 0.91, 0.91],
            camera_m,
            view_width_m,
            view_height_m,
        );
        if let Some(windsock) = scene.windsock {
            push_windsock(
                &mut vertices,
                windsock,
                camera_m,
                view_width_m,
                view_height_m,
            );
        }
    }
    vertices
}

#[allow(clippy::too_many_arguments)]
fn push_quad(
    vertices: &mut Vec<Vertex>,
    start_m: Vector,
    end_m: Vector,
    half_width_m: f32,
    color: [f32; 3],
    camera_m: Vector,
    view_width_m: f32,
    view_height_m: f32,
) {
    let direction = end_m - start_m;
    let normal = Vector::new(-direction.y, direction.x).normalize() * half_width_m;
    let corners = [
        start_m + normal,
        start_m - normal,
        end_m - normal,
        end_m + normal,
    ];
    for index in [0, 1, 2, 0, 2, 3] {
        vertices.push(vertex(
            corners[index],
            color,
            camera_m,
            view_width_m,
            view_height_m,
        ));
    }
}

fn push_windsock(
    vertices: &mut Vec<Vertex>,
    windsock: Windsock,
    camera_m: Vector,
    view_width_m: f32,
    view_height_m: f32,
) {
    let speed_m_s = windsock.apparent_wind_velocity_m_s.length();
    if speed_m_s <= f32::EPSILON {
        return;
    }
    let direction = windsock.apparent_wind_velocity_m_s / speed_m_s;
    let normal = Vector::new(-direction.y, direction.x);
    let extension = windsock_extension(speed_m_s);
    let length_m = 1.2;
    for segment in 0..5 {
        let start = segment as f32 / 5.0;
        if start >= extension {
            break;
        }
        let end = ((segment + 1) as f32 / 5.0).min(extension);
        let start_m = windsock.anchor_m + direction * length_m * start;
        let end_m = windsock.anchor_m + direction * length_m * end;
        let start_half_width_m = 0.1 - 0.075 * start;
        let end_half_width_m = 0.1 - 0.075 * end;
        let corners = [
            start_m + normal * start_half_width_m,
            start_m - normal * start_half_width_m,
            end_m - normal * end_half_width_m,
            end_m + normal * end_half_width_m,
        ];
        let color = if segment % 2 == 0 {
            [0.94, 0.18, 0.12]
        } else {
            [0.96, 0.94, 0.84]
        };
        for index in [0, 1, 2, 0, 2, 3] {
            vertices.push(vertex(
                corners[index],
                color,
                camera_m,
                view_width_m,
                view_height_m,
            ));
        }
    }
}

fn vertex(
    point_m: Vector,
    color: [f32; 3],
    camera_m: Vector,
    view_width_m: f32,
    view_height_m: f32,
) -> Vertex {
    Vertex {
        position: world_to_clip_position(point_m, camera_m, view_width_m, view_height_m),
        color,
        is_terrain: 0.0,
    }
}

fn world_to_clip_position(
    point_m: Vector,
    camera_m: Vector,
    view_width_m: f32,
    view_height_m: f32,
) -> [f32; 2] {
    let relative_m = point_m - camera_m;
    [relative_m.x / view_width_m, relative_m.y / view_height_m].map(|component| 2.0 * component)
}

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}
