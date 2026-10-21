@group(2) @binding(0)
var occluder_field_output: texture_storage_2d_array<rgba16float, write>;

// A coarse multiple plus a small residual keeps kilometre-scale elevations
// precise below the shortest land-shadow transition after f16 conversion.
const LANDFORM_FIELD_COARSE_STEP_M: f32 = 16.0;

// Cloud shadows along one light from a point on the plane, with each cloud
// type's drivers from the cloud weather grid.
fn cloud_shadow_field(plane_position_m: vec2<f32>, source_index: u32) -> vec4<f32> {
    let rays = cloud_shadow_rays(plane_position_m, source_index);
    if rays.amount <= 0.0 && !EVALUATE_ALL_BRANCHES {
        return cloud_shadow_texel_field(CloudShadowTexel(-CLOUD_SHADOW_RIM_LIMIT, 1.0, 1.0));
    }
    let guess = cumulus_ray(
        plane_position_m,
        rays,
        cumulus_drivers_from_cloud_weather_fields(rays.deck.position_m).drivers.z,
    );
    let cumulus = cumulus_ray(
        plane_position_m,
        rays,
        cumulus_drivers_from_cloud_weather_fields(guess.position_m).drivers.z,
    );
    return cloud_shadow_texel(
        plane_position_m,
        rays,
        cumulus,
        cumulus_drivers_from_cloud_weather_fields(cumulus.position_m),
        deck_drivers_from_cloud_weather_fields(rays.deck.position_m),
        scene.cloud_shadow_grids[source_index],
    );
}

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
    if layer < 2u {
        field = cloud_shadow_field(position_m, layer);
    } else {
        let elevation_m = broad_landform_elevation_m(position_m);
        let coarse_elevation_m = floor(elevation_m / LANDFORM_FIELD_COARSE_STEP_M)
            * LANDFORM_FIELD_COARSE_STEP_M;
        field = vec4<f32>(elevation_m - coarse_elevation_m, coarse_elevation_m, 0.0, 0.0);
    }
    textureStore(occluder_field_output, invocation_id.xy, layer, field);
}
