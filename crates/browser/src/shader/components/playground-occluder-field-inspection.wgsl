var<private> occluder_field_lookups_outside: bool;

fn note_occluder_field_lookup(outside: bool) {
    occluder_field_lookups_outside = occluder_field_lookups_outside || outside;
}

// 0: final composition; 1: grid windows; 2: composition with clamped lookups in magenta.
// Under EVALUATE_ALL_BRANCHES, taps whose results are discarded also flag.
fn occluder_field_inspection_mode() -> u32 {
    return playground.occluder_field_inspection.x;
}

fn occluder_field_inspection_color(layer: u32, field: vec4<f32>) -> vec3<f32> {
    if layer == LOW_CLOUD_BODY_FIELD {
        return mix(vec3<f32>(0.04, 0.08, 0.16), vec3<f32>(1.0), clamp(field.x, 0.0, 1.0));
    }
    if layer == LOW_CLOUD_SHADOW_FIELD {
        return vec3<f32>(1.0, 0.62, 0.22) * (0.08 + 0.92 * clamp(field.x / 1.25, 0.0, 1.0));
    }
    if layer == LOW_CLOUD_SHADOW_FIELD + 1u {
        return vec3<f32>(0.55, 0.72, 1.0) * (0.08 + 0.92 * clamp(field.x / 1.25, 0.0, 1.0));
    }
    if layer == CANOPY_FIELD {
        return vec3<f32>(0.72, 0.45, 1.0) * (0.08 + 0.92 * clamp(field.x / 5.43, 0.0, 1.0));
    }
    let height_m = field.x + field.y;
    if layer >= LANDFORM_HORIZON_FIELD {
        // Half-cover receiver heights; black where no landform shadow can fall.
        let tint = select(vec3<f32>(0.50, 0.55, 1.0), vec3<f32>(1.0, 0.85, 0.30), layer == LANDFORM_HORIZON_FIELD);
        return select(
            tint * clamp((height_m + 50.0) / 450.0, 0.08, 1.0),
            vec3<f32>(0.0),
            height_m <= LANDFORM_UNSHADOWED_HEIGHT_M,
        );
    }
    let elevation_m = height_m;
    return select(vec3<f32>(0.10, 0.25, 0.60), vec3<f32>(0.20, 0.75, 0.30), elevation_m >= 0.0)
        * clamp(abs(elevation_m) / 400.0, 0.08, 1.0);
}

// Each layer's written texels, unfiltered and aspect-preserved in a 4 × 2
// arrangement, with the view outlined where the layer covers it.
fn occluder_field_inspection_grids(fragment_position_px: vec2<f32>) -> vec3<f32> {
    let background = vec3<f32>(0.02);
    let cell_px = vec2<f32>(playground.occluder_field_inspection.yz) / vec2<f32>(4.0, 2.0);
    let cell = min(vec2<u32>(fragment_position_px / cell_px), vec2<u32>(3u, 1u));
    let layer = cell.y * 4u + cell.x;
    if layer >= OCCLUDER_FIELD_LAYERS {
        return background;
    }
    let texels = vec2<f32>(scene.occluder_field_texels[layer].xy);
    let tile_scale = 0.92 * min(cell_px.x / texels.x, cell_px.y / texels.y);
    let tile_origin_px = (vec2<f32>(cell) + vec2<f32>(0.5)) * cell_px
        - 0.5 * tile_scale * texels;
    let texel_position = (fragment_position_px - tile_origin_px) / tile_scale;
    if any(texel_position < vec2<f32>(0.0)) || any(texel_position >= texels) {
        return background;
    }
    let field = textureLoad(occluder_fields, vec2<i32>(texel_position), layer, 0);
    let grid = scene.occluder_field_grid_m[layer];
    let relative_m = grid.xy + (texel_position - vec2<f32>(0.5)) * grid.zw;
    let half_view_m = 0.5 * scene.camera_and_view_m.zw;
    let outline_m = 1.5 * grid.zw / tile_scale;
    let view_outline = all(abs(relative_m) <= half_view_m + outline_m)
        && any(abs(abs(relative_m) - half_view_m) <= outline_m);
    return select(
        occluder_field_inspection_color(layer, field),
        vec3<f32>(1.0),
        view_outline,
    );
}
