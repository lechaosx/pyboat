@group(2) @binding(0)
var landform_horizon_output: texture_storage_2d_array<rgba16float, write>;

// As in the landform layer, a coarse multiple plus a small residual keeps
// kilometre-scale heights precise after f16 conversion.
const LANDFORM_HORIZON_COARSE_STEP_M: f32 = 16.0;

// Reads the landform layer, so it runs after that layer is written. One z
// slice per light, in the scene's horizon-layer order.
@compute @workgroup_size(8, 8)
fn generate_landform_horizons(
    @builtin(global_invocation_id) invocation_id: vec3<u32>,
) {
    let source_index = invocation_id.z;
    let layer = LANDFORM_HORIZON_FIELD + source_index;
    if any(invocation_id.xy >= scene.occluder_field_texels[layer].xy) {
        return;
    }
    let grid = scene.occluder_field_grid_m[layer];
    let position_m = canonical_world_position(
        scene.camera_and_view_m.xy + grid.xy + vec2<f32>(invocation_id.xy) * grid.zw,
    );
    initialize_occluder_field_generation(position_m);
    if source_index >= 2u {
        let light = scene.lightning_sources[source_index - 2u];
        var horizon = LANDFORM_UNSHADOWED_HEIGHT_M;
        if light.w > 0.0 {
            horizon = finite_lightning_horizon(position_m, light);
        }
        let coarse = floor(horizon / LANDFORM_HORIZON_COARSE_STEP_M) * LANDFORM_HORIZON_COARSE_STEP_M;
        textureStore(landform_horizon_output, invocation_id.xy, source_index,
            vec4<f32>(horizon - coarse, coarse, 0.0, 0.0));
        return;
    }
    let receiver = celestial_receiver_light_at_position_and_height(
        position_m,
        max(sampled_broad_landform_elevation_m(position_m), 0.0),
    );
    let source = celestial_source_light(receiver, source_index);
    // Receivers up to the highest one may still see part of the disc.
    let lit = landform_shadow_strength() > 0.0
        && celestial_disc_visibility_at_height(
            highest_shadow_receiver_m(),
            source.direction,
            source.angular_radius_rad,
        ) > 0.0;
    var horizon = vec2<f32>(LANDFORM_UNSHADOWED_HEIGHT_M, 1.0);
    if lit || EVALUATE_ALL_BRANCHES {
        horizon = select(
            horizon,
            landform_horizon(
                position_m,
                landform_shadow_sample_direction(receiver, source),
                source.angular_radius_rad,
            ),
            lit,
        );
    }
    let coarse_height_m = floor(horizon.x / LANDFORM_HORIZON_COARSE_STEP_M)
        * LANDFORM_HORIZON_COARSE_STEP_M;
    // The scene's grid inspection reports lookups the march clamped.
    textureStore(
        landform_horizon_output,
        invocation_id.xy,
        source_index,
        vec4<f32>(
            horizon.x - coarse_height_m,
            coarse_height_m,
            horizon.y,
            select(0.0, 1.0, occluder_field_lookups_outside),
        ),
    );
}
