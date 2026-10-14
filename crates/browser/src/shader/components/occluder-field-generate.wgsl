@group(2) @binding(0)
var occluder_field_output: texture_storage_2d_array<rgba16float, write>;

// A coarse multiple plus a small residual keeps kilometre-scale elevations
// precise below the shortest land-shadow transition after f16 conversion.
const LANDFORM_FIELD_COARSE_STEP_M: f32 = 16.0;

// One z slice per layer; slices smaller than the dispatch stop early.
@compute @workgroup_size(8, 8)
fn generate_occluder_fields(
    @builtin(global_invocation_id) invocation_id: vec3<u32>,
) {
    let layer = invocation_id.z;
    if any(invocation_id.xy >= scene.occluder_field_texels[layer].xy) {
        return;
    }
    let grid = scene.occluder_field_grid_m[layer];
    let position_m = canonical_world_position(
        scene.camera_and_view_m.xy + grid.xy + vec2<f32>(invocation_id.xy) * grid.zw,
    );
    initialize_occluder_field_generation(position_m);
    var field = vec4<f32>(0.0);
    if layer == 0u {
        let low_cloud = procedural_low_cloud_sample(
            position_m,
            cloud_weather_from_environment_fields(position_m),
        );
        field = vec4<f32>(low_cloud.body_amount, low_cloud.optical_depth, low_cloud.gradient);
    } else if layer < 3u {
        field.x = procedural_low_cloud_optical_depth(
            position_m,
            cloud_weather_from_environment_fields(position_m),
        );
    } else if layer == 3u {
        field.x = procedural_upper_cloud_optical_depth(
            position_m,
            regional_canopy_conditions_at(position_m),
        );
    } else {
        let elevation_m = broad_landform_elevation_m(position_m);
        let coarse_elevation_m = floor(elevation_m / LANDFORM_FIELD_COARSE_STEP_M)
            * LANDFORM_FIELD_COARSE_STEP_M;
        field = vec4<f32>(elevation_m - coarse_elevation_m, coarse_elevation_m, 0.0, 0.0);
    }
    textureStore(occluder_field_output, invocation_id.xy, layer, field);
}
