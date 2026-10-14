// Low clouds, canopy and broad landform evaluated once per frame on the
// lattices in `scene.occluder_field_grid_m`: per layer, the first texel centre
// relative to the camera, then the texel spacing. `scene.occluder_field_texels`
// counts the texels written this frame; lookups clamp to them. Every value fits
// half-float precision, so hardware filtering reconstructs it in one sample.
@group(0) @binding(2)
var occluder_fields: texture_2d_array<f32>;

@group(0) @binding(3)
var occluder_field_sampler: sampler;

// Keep the layer order synchronized with OCCLUDER_FIELD_LAYERS in field_grids.rs.
const LOW_CLOUD_BODY_FIELD: u32 = 0u;
// Sun, then Moon.
const LOW_CLOUD_SHADOW_FIELD: u32 = 1u;
const CANOPY_FIELD: u32 = 3u;
const LANDFORM_FIELD: u32 = 4u;

fn occluder_field(position_m: vec2<f32>, layer: u32) -> vec4<f32> {
    let grid = scene.occluder_field_grid_m[layer];
    let texel_position = (camera_relative_position_m(position_m) - grid.xy) / grid.zw;
    let last_texel = vec2<f32>(scene.occluder_field_texels[layer].xy - vec2<u32>(1u));
    note_occluder_field_lookup(
        any(texel_position < vec2<f32>(0.0)) || any(texel_position > last_texel),
    );
    let texel = clamp(texel_position, vec2<f32>(0.0), last_texel);
    return textureSampleLevel(
        occluder_fields,
        occluder_field_sampler,
        (texel + vec2<f32>(0.5)) / vec2<f32>(textureDimensions(occluder_fields)),
        layer,
        0.0,
    );
}

fn sampled_low_cloud_sample(position_m: vec2<f32>) -> LowCloudSample {
    let field = occluder_field(position_m, LOW_CLOUD_BODY_FIELD);
    return LowCloudSample(field.x, field.y, field.zw);
}

fn low_cloud_weather_at(position_m: vec2<f32>) -> CloudWeatherSample {
    return cloud_weather_from_environment_fields(position_m);
}

fn sampled_upper_cloud_optical_depth_at(position_m: vec2<f32>) -> f32 {
    return occluder_field(position_m, CANOPY_FIELD).x;
}

fn sampled_broad_landform_elevation_m(position_m: vec2<f32>) -> f32 {
    let field = occluder_field(position_m, LANDFORM_FIELD);
    return field.x + field.y;
}

fn sampled_cloud_shadow_depths_at(
    projection: CloudShadowProjection,
    source_index: u32,
) -> CloudShadowDepths {
    var depths = CloudShadowDepths(0.0, 0.0);
    if layer_visibility.y > 0.0 {
        depths.low = layer_visibility.y * tuning.cloud_shadows.x * occluder_field(
            projection.low_position_m,
            LOW_CLOUD_SHADOW_FIELD + source_index,
        ).x;
    }
    if layer_visibility.z > 0.0 {
        depths.upper = layer_visibility.z * tuning.cloud_shadows.y
            * sampled_upper_cloud_optical_depth_at(projection.upper_position_m);
    }
    return depths;
}
