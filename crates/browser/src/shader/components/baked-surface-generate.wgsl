// Keep synchronized with `FillRect` and `MAX_FILL_RECTS` in baked_surface.rs.
struct BakedSurfaceFillRect {
    // Column within one longitude lap, then unbounded row, on the level's lattice.
    first_texel: vec2<i32>,
    texels: vec2<u32>,
    first_slot: vec2<u32>,
    spacing_m: vec2<f32>,
    first_invocation: u32,
    level: u32,
    lap_columns: u32,
    padding: u32,
};

struct BakedSurfaceFill {
    rect_count: u32,
    padding: u32,
    more_padding: vec2<u32>,
    rects: array<BakedSurfaceFillRect, 16>,
};

@group(2) @binding(0)
var<uniform> baked_surface_fill: BakedSurfaceFill;
@group(2) @binding(1)
var baked_height_output: texture_storage_2d_array<rgba16float, write>;
@group(2) @binding(2)
var baked_color_output: texture_storage_2d_array<rgba8unorm, write>;

struct BakedSurfaceTexel {
    valid: bool,
    level: u32,
    position_m: vec2<f32>,
    spacing_m: f32,
    slot: vec2<u32>,
};

// One invocation per scheduled texel; rects take consecutive invocations.
fn baked_surface_texel(invocation: u32) -> BakedSurfaceTexel {
    var rect = baked_surface_fill.rects[0];
    for (var index = 1u; index < baked_surface_fill.rect_count; index++) {
        if invocation >= baked_surface_fill.rects[index].first_invocation {
            rect = baked_surface_fill.rects[index];
        }
    }
    var texel: BakedSurfaceTexel;
    let local = invocation - rect.first_invocation;
    texel.valid = baked_surface_fill.rect_count > 0u && local < rect.texels.x * rect.texels.y;
    let offset = vec2<u32>(local % rect.texels.x, local / rect.texels.x);
    let column = (u32(rect.first_texel.x) + offset.x) % rect.lap_columns;
    let row = rect.first_texel.y + i32(offset.y);
    texel.position_m = canonical_world_position(vec2<f32>(f32(column), f32(row)) * rect.spacing_m);
    texel.spacing_m = max(rect.spacing_m.x, rect.spacing_m.y);
    texel.level = rect.level;
    texel.slot = (rect.first_slot + offset) % textureDimensions(baked_height_output).xy;
    return texel;
}

struct BakedSurfaceValues {
    // Elevation, its gradient, and bed reflectance.
    height: vec4<f32>,
    color: vec3<f32>,
    unresolved_slope: f32,
    detail_gradient: vec2<f32>,
    wet_fraction: f32,
    surface_grade: f32,
    autumn_color: vec3<f32>,
    snow_detail_scale: f32,
    dormancy_color: vec3<f32>,
    land_ice_amount: f32,
    // Premultiplied by the land ice amount, so it interpolates as a share.
    land_ice: LandIceDetail,
    sea_ice: SeaIceDetail,
};

// The static surface at one position: terrain with land ice, the dry,
// snowless, unseasoned land composition with the factors the scene applies
// for tide, snow, and seasons, the land-ice detail, and the sea-ice detail.
fn baked_surface_values_at(position_m: vec2<f32>, spacing_m: f32) -> BakedSurfaceValues {
    let circle_position = periodic_noise_circle_position_from_canonical(position_m);
    let bedrock = procedural_bedrock_sample_from_circle(position_m, circle_position);
    let climate = surface_climate_sample_at(position_m);
    // No tide exposes land 3 m below sea level, so the scene reads no land
    // composition there, including through a neighbouring texel's bilinear
    // stencil.
    let reach_m = 2.0 * length(bedrock.gradient) * spacing_m;
    let composed = bedrock.elevation_m + reach_m > -3.0;
    // Land ice follows annual climate; sea ice follows the season, per pixel.
    // Submerged texels take the land ice of their bedrock raised to sea level,
    // so colour and ice amount stay continuous across an ice shore while their
    // elevation stays the bedrock's.
    var iced = bedrock;
    if composed || EVALUATE_ALL_BRANCHES {
        var shore = bedrock;
        shore.elevation_m = max(bedrock.elevation_m, 0.0);
        iced = solid_ice_sample(shore, bedrock.cluster, climate, position_m);
    }
    var terrain = bedrock;
    if bedrock.elevation_m >= 0.0 {
        terrain = iced;
    }
    var values: BakedSurfaceValues;
    values.height = vec4<f32>(
        terrain.elevation_m,
        terrain.gradient - terrain.unresolved_gradient,
        bottom_reflectance(
            terrain.coastal_rock_amount,
            terrain.coastal_outcrop_amount,
            terrain.coastal_sediment_amount,
        ),
    );
    values.unresolved_slope = 0.0;
    // Sea ice follows the season per pixel and shows where water is below
    // -1 °C, so texels hold its detail where the coldest season brings water
    // below that anywhere within the bilinear stencil's reach. Coldest water,
    // with its maritime anomaly, changes by at most 3.1 °C per degree of
    // latitude, near 83°.
    let stencil_reach_m = 2.0 * spacing_m;
    let maritime_scale = maritime_temperature_scale_c(climate);
    let coldest_water_c = water_temperature_c_at_season(
        climate,
        -sign(climate.seasonal_latitude_factor),
    )
        + maritime_scale * bedrock.cluster.value;
    let reach_c = stencil_reach_m * (
        maritime_scale * length(bedrock.cluster.gradient)
            + 3.2 * 180.0 * scene.inverse_world_size_m.y
    );
    let holds_sea_ice = coldest_water_c - reach_c < -1.0 || EVALUATE_ALL_BRANCHES;
    let ice_amount = clamp(iced.ice_amount, 0.0, 1.0);
    let holds_land_ice = (ice_amount > 0.0 || EVALUATE_ALL_BRANCHES) && landscape_visibility.z > 0.0;
    var ice_micro = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    if holds_sea_ice || holds_land_ice {
        ice_micro = ice_micro_relief_sample(position_m);
    }
    if holds_sea_ice {
        values.sea_ice = sea_ice_detail_at(position_m, ice_micro);
    }
    if !composed && !EVALUATE_ALL_BRANCHES {
        return values;
    }
    initialize_surface_tide_value();
    let land_surface = material_fractal_value(LAND_SURFACE_NOISE_FIELD, position_m, 96.0, 4u);
    let land_shows = ice_amount < 1.0;
    var ice_detail = LandIceDetail(0.0, vec2<f32>(0.0));
    if holds_land_ice {
        ice_detail = land_ice_detail_at(position_m, land_surface, ice_micro);
    }
    let ice = land_ice_surface(ice_detail);
    values.land_ice = LandIceDetail(ice_amount * ice_detail.grade, ice_amount * ice_detail.gradient);
    var surface = ice;
    if land_shows || EVALUATE_ALL_BRANCHES {
        surface_baking = true;
        var atmosphere: SurfaceAtmosphereSample;
        atmosphere.climate = climate;
        let land = terrain_surface(
            position_m,
            terrain.elevation_m,
            terrain.gradient,
            terrain.material_elevation_m,
            terrain.material_gradient,
            terrain.coastal_rock_amount,
            terrain.coastal_outcrop_amount,
            terrain.coastal_sediment_amount,
            -1.0e6,
            land_surface,
            atmosphere,
        );
        surface_baking = false;
        let factors = surface_bake_factors;
        let kept = 1.0 - ice_amount;
        surface = select_terrain_surface(
            surface,
            blend_terrain_surface(land, ice, ice_amount),
            land_shows,
        );
        values.wet_fraction = kept * factors.wet_fraction;
        values.surface_grade = factors.surface_grade;
        values.autumn_color = kept * factors.autumn_color;
        values.dormancy_color = kept * factors.dormancy_color;
        values.snow_detail_scale = length(factors.snow_detail_gradient)
            / max(length(surface.detail_gradient), 1e-4);
    }
    values.color = surface.color;
    values.unresolved_slope = lit_unresolved_slope(
        terrain.unresolved_slope_variance,
        surface.unresolved_slope,
    );
    values.detail_gradient = surface.detail_gradient;
    values.land_ice_amount = ice_amount;
    return values;
}

// Square-root companding keeps gentle gradients precise in eight bits.
fn encode_detail_gradient(gradient: vec2<f32>, range: f32) -> vec2<f32> {
    let normalized = clamp(gradient / range, vec2<f32>(-1.0), vec2<f32>(1.0));
    return 0.5 + 0.5 * sign(normalized) * sqrt(abs(normalized));
}

@compute @workgroup_size(64)
fn generate_baked_surface(@builtin(global_invocation_id) invocation_id: vec3<u32>) {
    let texel = baked_surface_texel(invocation_id.x);
    if !texel.valid {
        return;
    }
    initialize_baked_surface_generation(texel.position_m, texel.spacing_m);
    let values = baked_surface_values_at(texel.position_m, texel.spacing_m);
    textureStore(baked_height_output, texel.slot, texel.level, values.height);
    let layer = BAKED_SURFACE_COLOR_LAYERS * texel.level;
    textureStore(
        baked_color_output,
        texel.slot,
        layer,
        vec4<f32>(values.color, sqrt(clamp(values.unresolved_slope / BAKED_DETAIL_GRADIENT_RANGE, 0.0, 1.0))),
    );
    // Wetness removes at most a third of the luminance, so its fraction
    // takes the whole eight-bit range.
    textureStore(
        baked_color_output,
        texel.slot,
        layer + 1u,
        vec4<f32>(
            encode_detail_gradient(values.detail_gradient, BAKED_DETAIL_GRADIENT_RANGE),
            3.0 * values.wet_fraction,
            values.surface_grade,
        ),
    );
    // Texels whose land ice amount stores as nonzero hold its detail in place
    // of the autumn colour change, which they give up: land ice forms only
    // 11 °C colder in annual mean than autumn colour shows.
    var seasonal_or_ice = values.autumn_color + 0.5;
    if values.land_ice_amount >= 0.5 / 255.0 {
        seasonal_or_ice = vec3<f32>(
            values.land_ice.grade,
            encode_detail_gradient(values.land_ice.gradient, BAKED_DETAIL_GRADIENT_RANGE),
        );
    }
    textureStore(
        baked_color_output,
        texel.slot,
        layer + 2u,
        vec4<f32>(seasonal_or_ice, 0.5 * values.snow_detail_scale),
    );
    textureStore(
        baked_color_output,
        texel.slot,
        layer + 3u,
        vec4<f32>(values.dormancy_color + 0.5, values.land_ice_amount),
    );
    textureStore(
        baked_color_output,
        texel.slot,
        layer + 4u,
        vec4<f32>(
            0.5 + 0.5 * vec2<f32>(values.sea_ice.broad, values.sea_ice.fine),
            encode_detail_gradient(values.sea_ice.gradient, BAKED_SEA_ICE_GRADIENT_RANGE),
        ),
    );
}
