// The static surface kept between frames in a toroidal clipmap on
// world-anchored lattices. Each level holds, per texel, a half-precision layer
// of elevation with land ice, its gradient, and bed reflectance, then
// eight-bit layers of the dry, snowless, unseasoned land colour and roughness;
// the land detail gradient, wetness fraction, and surface grade; the autumn
// colour change, or on texels with land ice its colour grade and relief, and
// the snow detail scale; the dormancy colour
// change and land ice amount; and the sea-ice plate noises with their relief
// gradient, wherever the coldest season freezes the water. The scene applies
// tide, sea-ice cover, slabs, and fractures, snow, seasons, water, and
// lighting per pixel.
//
// `scene.baked_surface_grid_m` is the read level's first valid texel centre
// relative to the camera, then its spacing; `scene.baked_surface_texels` is
// that texel's toroidal slot, then the valid extent; `scene.baked_surface_level.x`
// is the level. Positions outside the valid extent compose the surface directly.
@group(0) @binding(5)
var baked_height: texture_2d_array<f32>;
@group(0) @binding(6)
var baked_color: texture_2d_array<f32>;

struct BakedSurfaceCell {
    valid: bool,
    lower: vec2<u32>,
    upper: vec2<u32>,
    fraction: vec2<f32>,
    spacing_m: vec2<f32>,
};

fn baked_surface_cell(position_m: vec2<f32>) -> BakedSurfaceCell {
    let grid = scene.baked_surface_grid_m;
    let extent = vec2<f32>(scene.baked_surface_texels.zw);
    let size = textureDimensions(baked_height).xy;
    let texel_position = (camera_relative_position_m(position_m) - grid.xy) / grid.zw;
    let texel = floor(texel_position);
    let cell = vec2<u32>(clamp(texel, vec2<f32>(0.0), max(extent - 2.0, vec2<f32>(0.0))));
    let lower = (scene.baked_surface_texels.xy + cell) % size;
    return BakedSurfaceCell(
        all(texel >= vec2<f32>(0.0)) && all(texel + 1.0 < extent),
        lower,
        (lower + 1u) % size,
        clamp(texel_position - texel, vec2<f32>(0.0), vec2<f32>(1.0)),
        grid.zw,
    );
}

struct BakedCorners {
    lower_left: vec4<f32>,
    lower_right: vec4<f32>,
    upper_left: vec4<f32>,
    upper_right: vec4<f32>,
};

fn baked_corners(texture: texture_2d_array<f32>, cell: BakedSurfaceCell, layer: u32) -> BakedCorners {
    return BakedCorners(
        textureLoad(texture, cell.lower, layer, 0),
        textureLoad(texture, vec2<u32>(cell.upper.x, cell.lower.y), layer, 0),
        textureLoad(texture, vec2<u32>(cell.lower.x, cell.upper.y), layer, 0),
        textureLoad(texture, cell.upper, layer, 0),
    );
}

fn baked_bilinear(corners: BakedCorners, fraction: vec2<f32>) -> vec4<f32> {
    return mix(
        mix(corners.lower_left, corners.lower_right, fraction.x),
        mix(corners.upper_left, corners.upper_right, fraction.x),
        fraction.y,
    );
}

struct BakedHeight {
    elevation: TerrainNoiseSample,
    bottom_reflectance: f32,
};

// Elevation by bicubic Hermite reconstruction from the baked slopes, with the
// cross derivative estimated from their differences across the cell.
fn baked_height_at(cell: BakedSurfaceCell) -> BakedHeight {
    let height = baked_corners(baked_height, cell, scene.baked_surface_level.x);
    let cross = 0.25 * (
        (height.upper_left.y - height.lower_left.y + height.upper_right.y - height.lower_right.y)
            / cell.spacing_m.y
            + (height.lower_right.z - height.lower_left.z + height.upper_right.z
                - height.upper_left.z) / cell.spacing_m.x
    );
    var elevation = terrain_cache_hermite(
        vec4<f32>(height.lower_left.xyz, cross),
        vec4<f32>(height.lower_right.xyz, cross),
        vec4<f32>(height.upper_left.xyz, cross),
        vec4<f32>(height.upper_right.xyz, cross),
        cell.fraction,
        cell.spacing_m,
    );
    let interpolated = baked_bilinear(height, cell.fraction);
    // Land ice thickens only bedrock above sea level, so a cell across an ice
    // shore spans a cliff that no pixel shows; its slopes light the land.
    let lowest = min(
        min(height.lower_left.x, height.lower_right.x),
        min(height.upper_left.x, height.upper_right.x),
    );
    let highest = max(
        max(height.lower_left.x, height.lower_right.x),
        max(height.upper_left.x, height.upper_right.x),
    );
    if lowest < 0.0 && highest >= 0.0 {
        elevation.gradient = interpolated.yz;
    }
    return BakedHeight(elevation, interpolated.w);
}

struct BakedLand {
    color: vec3<f32>,
    roughness: f32,
    detail_gradient: vec2<f32>,
    wet_fraction: f32,
    surface_grade: f32,
    autumn_color: vec3<f32>,
    snow_detail_scale: f32,
    dormancy_color: vec3<f32>,
    land_ice_amount: f32,
    land_ice: LandIceDetail,
};

fn decode_detail_gradient(encoded: vec2<f32>, range: f32) -> vec2<f32> {
    let signed_gradient = 2.0 * encoded - 1.0;
    return range * sign(signed_gradient) * signed_gradient * signed_gradient;
}

fn baked_land_at(cell: BakedSurfaceCell) -> BakedLand {
    let layer = BAKED_SURFACE_COLOR_LAYERS * scene.baked_surface_level.x;
    let color = baked_bilinear(baked_corners(baked_color, cell, layer), cell.fraction);
    let detail = baked_bilinear(baked_corners(baked_color, cell, layer + 1u), cell.fraction);
    let seasonal_corners = baked_corners(baked_color, cell, layer + 2u);
    let seasonal = baked_bilinear(seasonal_corners, cell.fraction);
    let dormancy_corners = baked_corners(baked_color, cell, layer + 3u);
    let dormancy = baked_bilinear(dormancy_corners, cell.fraction);
    // Texels with land ice hold its detail, premultiplied by its amount, in
    // place of the autumn colour change, so each corner's share goes to one.
    let fraction = cell.fraction;
    let ice_weights = vec4<f32>(
        (1.0 - fraction.x) * (1.0 - fraction.y),
        fraction.x * (1.0 - fraction.y),
        (1.0 - fraction.x) * fraction.y,
        fraction.x * fraction.y,
    ) * vec4<f32>(
        vec4<bool>(
            dormancy_corners.lower_left.a > 0.0,
            dormancy_corners.lower_right.a > 0.0,
            dormancy_corners.upper_left.a > 0.0,
            dormancy_corners.upper_right.a > 0.0,
        ),
    );
    let ice_payload = ice_weights.x * seasonal_corners.lower_left.rgb
        + ice_weights.y * seasonal_corners.lower_right.rgb
        + ice_weights.z * seasonal_corners.upper_left.rgb
        + ice_weights.w * seasonal_corners.upper_right.rgb;
    let seasonal_weight = 1.0 - dot(ice_weights, vec4<f32>(1.0));
    // An encoded 0.5 is no relief.
    let ice_detail = ice_payload + vec3<f32>(0.0, 0.5, 0.5) * seasonal_weight;
    let ice_share = 1.0 / max(dormancy.a, 1e-3);
    return BakedLand(
        color.rgb,
        color.a,
        decode_detail_gradient(detail.xy, BAKED_DETAIL_GRADIENT_RANGE),
        detail.z / 3.0,
        detail.w,
        seasonal.rgb - ice_payload - 0.5 * seasonal_weight,
        2.0 * seasonal.a,
        dormancy.rgb - 0.5,
        dormancy.a,
        LandIceDetail(
            clamp(ice_share * ice_detail.r, 0.0, 1.0),
            ice_share * decode_detail_gradient(ice_detail.gb, BAKED_DETAIL_GRADIENT_RANGE),
        ),
    );
}

fn baked_sea_ice_detail_at(cell: BakedSurfaceCell) -> SeaIceDetail {
    let layer = BAKED_SURFACE_COLOR_LAYERS * scene.baked_surface_level.x + 4u;
    let detail = baked_bilinear(baked_corners(baked_color, cell, layer), cell.fraction);
    return SeaIceDetail(
        2.0 * detail.x - 1.0,
        2.0 * detail.y - 1.0,
        decode_detail_gradient(detail.zw, BAKED_SEA_ICE_GRADIENT_RANGE),
    );
}

// The depth beyond which open water reads nothing of the bed: the vanishing of
// coastal detail, which lies beyond depth-limited waves, and bottom tint.
fn bed_unseen_depth_m() -> f32 {
    return max(32.0, 2.0 + 2.0 * tuning.water_tuning.x);
}

fn scene_surface_at(
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
    water_height_m: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> SceneSurface {
    let climate = atmosphere.climate;
    // The landform grid's broad elevation bounds the bed: offshore the bed
    // deepens from it, and coastal and land detail vanish. Open water over a
    // bed beyond what anything reads needs no bake. The 2 m margin covers the
    // grid's interpolation.
    let broad_depth_m = -sampled_broad_landform_elevation_m(position_m) - 2.0;
    let bed_depth_m = min(shaped_bed_depth_m(broad_depth_m), OCEAN_FLOOR_DEPTH_M);
    let offshore = broad_depth_m > 0.0;
    var cluster = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    var open_water = TerrainNoiseSample(1.0, vec2<f32>(0.0));
    if offshore || EVALUATE_ALL_BRANCHES {
        cluster = terrain_broad_noise(TERRAIN_CLUSTER_NOISE, position_m, circle_position);
        open_water = sea_ice_open_water(climate, cluster);
    }
    if offshore && bed_depth_m > bed_unseen_depth_m() && 1.0 - open_water.value <= 0.5 {
        var bed: TerrainSample;
        bed.elevation_m = -bed_depth_m;
        bed.material_elevation_m = -bed_depth_m;
        bed.bedrock_elevation_m = -bed_depth_m;
        bed.cluster = cluster;
        return composed_scene_surface(
            position_m,
            solid_ice_sample(bed, cluster, climate, position_m),
            SeaIceDetail(0.0, 0.0, vec2<f32>(0.0)),
            water_height_m,
            atmosphere,
        );
    }
    let cell = baked_surface_cell(position_m);
    if !cell.valid {
        let terrain = procedural_terrain_sample_with_climate_from_circle(
            position_m,
            circle_position,
            climate,
        );
        return composed_scene_surface(
            position_m,
            terrain,
            direct_sea_ice_detail(position_m, terrain),
            water_height_m,
            atmosphere,
        );
    }
    let baked = baked_height_at(cell);
    var elevation = baked.elevation;
    // Sea ice forms on submerged bedrock, as in `solid_ice_sample`.
    var water_ice_amount = 0.0;
    let submerged = elevation.value < 0.0;
    if submerged || EVALUATE_ALL_BRANCHES {
        if !offshore && !EVALUATE_ALL_BRANCHES {
            cluster = terrain_broad_noise(TERRAIN_CLUSTER_NOISE, position_m, circle_position);
            open_water = sea_ice_open_water(climate, cluster);
        }
        let cover = 1.0 - open_water.value;
        if submerged && cover > 0.5 {
            water_ice_amount = cover;
            elevation = TerrainNoiseSample(0.25 + 1.75 * cover, -1.75 * open_water.gradient);
        }
    }
    let sea_ice = water_ice_amount > 0.0;
    let is_land = sea_ice || elevation.value >= water_height_m;
    var surface: TerrainSurface;
    if sea_ice || EVALUATE_ALL_BRANCHES {
        surface = sea_ice_surface(
            position_m,
            water_ice_amount,
            baked.elevation.value,
            baked_sea_ice_detail_at(cell),
        );
    }
    if !is_land || EVALUATE_ALL_BRANCHES {
        surface = select_terrain_surface(
            surface,
            water_column_surface(
                position_m,
                baked.elevation.value,
                water_height_m,
                baked.bottom_reflectance,
                atmosphere,
            ),
            !is_land,
        );
    }
    if (is_land && !sea_ice) || EVALUATE_ALL_BRANCHES {
        surface = select_terrain_surface(
            surface,
            baked_land_surface(
                baked_land_at(cell),
                elevation.value,
                water_height_m,
                atmosphere,
            ),
            is_land && !sea_ice,
        );
    }
    return SceneSurface(elevation.value, elevation.gradient, is_land, surface);
}

// The baked land composition with the live tide, seasons, and snow.
fn baked_land_surface(
    baked: BakedLand,
    elevation_m: f32,
    water_height_m: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> TerrainSurface {
    // One minus the exposed dryness of `terrain_surface`.
    let wetness = 1.0 - smoothstep(0.05, 0.8, elevation_m - water_height_m);
    var color = baked.color * (1.0 - wetness * baked.wet_fraction);
    // Wet intertidal and beach cover is 0.08 rougher; 0.29, about the wetness
    // fraction of that cover alone, takes all of it.
    let roughness = baked.roughness + wetness * 0.08 * min(baked.wet_fraction / 0.29, 1.0);
    if climate_visibility.w > 0.0 {
        let autumn = autumn_vegetation_amount_from_anomaly(
            elevation_m,
            atmosphere.temperature_anomaly.value,
            atmosphere.climate,
        );
        let dormancy = vegetation_dormancy_from_anomaly(
            elevation_m,
            atmosphere.temperature_anomaly.value,
            atmosphere.climate,
        );
        color += autumn * baked.autumn_color + dormancy * baked.dormancy_color;
    }
    var land = TerrainSurface(max(color, vec3<f32>(0.0)), baked.detail_gradient, roughness);
    var snow_amount = 0.0;
    if climate_visibility.x > 0.0 {
        snow_amount = smoothstep(
            0.10,
            0.55,
            local_snow_cover_from_atmosphere(elevation_m, atmosphere),
        );
        snow_amount = clamp(tuning.material_coverage.w * snow_amount, 0.0, 1.0);
    }
    // Snow covers only the land share of partial land ice, as in the direct
    // composition, so it blends towards snow beside the ice it leaves bare.
    let snowy = snow_amount > 0.0 && baked.land_ice_amount < 1.0;
    if snowy || EVALUATE_ALL_BRANCHES {
        var cover = TerrainSurface(
            solid_material_color_visibility().y * snow_color(baked.surface_grade),
            baked.snow_detail_scale * baked.detail_gradient,
            SNOW_ROUGHNESS,
        );
        let partial_ice = baked.land_ice_amount > 0.0;
        if partial_ice || EVALUATE_ALL_BRANCHES {
            let ice = land_ice_surface(baked.land_ice);
            cover = select_terrain_surface(
                cover,
                blend_terrain_surface(cover, ice, baked.land_ice_amount),
                partial_ice,
            );
        }
        land = select_terrain_surface(land, blend_terrain_surface(land, cover, snow_amount), snowy);
    }
    return land;
}
