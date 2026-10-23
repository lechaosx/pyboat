// Cloud shadows, broad landform and its horizons evaluated once per frame on
// the lattices in `scene.occluder_field_grid_m`: per layer, the first texel
// centre relative to the camera, then the texel spacing.
// `scene.occluder_field_texels` counts the texels written this frame; lookups
// clamp to them. Every value fits half-float precision, so hardware filtering
// reconstructs it in one sample.
@group(0) @binding(2)
var occluder_fields: texture_2d_array<f32>;

@group(0) @binding(3)
var occluder_field_sampler: sampler;

// Keep the layer order synchronized with OCCLUDER_FIELD_LAYERS in field_grids.rs.
// Cloud shadow texels on their plane along the Sun, then the Moon.
const CLOUD_SHADOW_FIELD: u32 = 0u;
const LANDFORM_FIELD: u32 = 2u;
// Sun, then Moon.
const LANDFORM_HORIZON_FIELD: u32 = 3u;

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

fn sampled_broad_landform_elevation_m(position_m: vec2<f32>) -> f32 {
    let field = occluder_field(position_m, LANDFORM_FIELD);
    return field.x + field.y;
}

fn sampled_landform_horizon(
    position_m: vec2<f32>,
    light_direction: vec3<f32>,
    angular_radius_rad: f32,
    source_index: u32,
) -> vec2<f32> {
    let field = occluder_field(position_m, LANDFORM_HORIZON_FIELD + source_index);
    note_occluder_field_lookup(field.w > 0.0);
    return vec2<f32>(field.x + field.y, field.z);
}

fn sampled_ground_bounce_conditions(position_m: vec2<f32>) -> GroundBounceConditions {
    let lookup = cloud_weather_lookup(position_m);
    return ground_bounce_conditions_from_field(
        environment_field_layer(lookup.texel, lookup.weight, GROUND_BOUNCE_FIELD),
    );
}

fn sampled_cloud_transmittance_at(plane_position_m: vec2<f32>, source_index: u32) -> f32 {
    return cloud_shadow_field_transmittance(
        occluder_field(plane_position_m, CLOUD_SHADOW_FIELD + source_index),
    );
}
