struct CloudShadowCacheUniforms {
    viewport_size_px: vec2<u32>,
    atlas_size_px: vec2<u32>,
    table_mask: u32,
    maximum_tiles: u32,
    atlas_columns: u32,
    tile_texels: u32,
    padding_0: u32,
    padding_1: u32,
    padding_2: u32,
    padding_3: u32,
};

struct CloudShadowCacheEntry {
    identity: u32,
    slot: u32,
};

@group(2) @binding(0)
var<uniform> cloud_shadow_cache: CloudShadowCacheUniforms;

@group(2) @binding(1)
var<storage, read> cloud_shadow_cache_entries: array<CloudShadowCacheEntry>;

@group(2) @binding(2)
var cloud_shadow_cache_atlas: texture_2d<f32>;

@group(2) @binding(3)
var cloud_shadow_cache_sampler: sampler;

const CLOUD_SHADOW_CACHE_SLOT_ALLOCATED: u32 = 0x80000000u;
const CLOUD_SHADOW_CACHE_SLOT_VALUE_MASK: u32 = 0x7fffffffu;
// Keep this packing synchronized with cloud-shadow-cache-generate.wgsl. One
// atomic identity avoids compound publication; out-of-range projections use
// the exact direct-sampling fallback.
const CLOUD_SHADOW_CACHE_KEY_MIN: f32 = -16384.0;
const CLOUD_SHADOW_CACHE_KEY_MAX: f32 = 16383.0;

fn cloud_shadow_cache_tile_key(position_m: vec2<f32>) -> vec2<f32> {
    let sample_position = camera_relative_position_m(position_m)
        / visual_pixel_footprint_m;
    return floor(
        (sample_position - vec2<f32>(0.5))
            / f32(shadow_cache_tile_texels()),
    );
}

fn cloud_shadow_cache_key_identity(key: vec2<f32>, layer: u32) -> u32 {
    if layer > 2u
        || any(key < vec2<f32>(CLOUD_SHADOW_CACHE_KEY_MIN))
        || any(key > vec2<f32>(CLOUD_SHADOW_CACHE_KEY_MAX))
    {
        return 0u;
    }
    let coordinates = vec2<u32>(
        vec2<i32>(key) - vec2<i32>(i32(CLOUD_SHADOW_CACHE_KEY_MIN)),
    );
    return 1u + (
        coordinates.x
            | (coordinates.y << 15u)
            | (layer << 30u)
    );
}

fn cloud_shadow_cache_hash(identity: u32) -> u32 {
    var value = identity;
    value = value ^ (value >> 16u);
    value = value * 0x7feb352du;
    value = value ^ (value >> 15u);
    return value * 0x846ca68bu;
}

fn cloud_shadow_cache_slot(key: vec2<f32>, layer: u32) -> u32 {
    let identity = cloud_shadow_cache_key_identity(key, layer);
    if identity == 0u {
        return cloud_shadow_cache.maximum_tiles;
    }
    var table_index = cloud_shadow_cache_hash(identity)
        & cloud_shadow_cache.table_mask;
    for (var probe = 0u; probe < 16u; probe++) {
        let entry = cloud_shadow_cache_entries[table_index];
        if entry.identity == 0u {
            break;
        }
        if entry.identity == identity {
            if (entry.slot & CLOUD_SHADOW_CACHE_SLOT_ALLOCATED) != 0u {
                return entry.slot & CLOUD_SHADOW_CACHE_SLOT_VALUE_MASK;
            }
            return cloud_shadow_cache.maximum_tiles;
        }
        table_index = (table_index + 1u) & cloud_shadow_cache.table_mask;
    }
    return cloud_shadow_cache.maximum_tiles;
}

fn cached_cloud_shadow_optical_depth(
    position_m: vec2<f32>,
    layer: u32,
) -> f32 {
    let key = cloud_shadow_cache_tile_key(position_m);
    let slot = cloud_shadow_cache_slot(key, layer);
    record_shadow_cache_lookup(layer, slot < cloud_shadow_cache.maximum_tiles);
    if slot >= cloud_shadow_cache.maximum_tiles {
        return -1.0;
    }
    let slot_origin_px = vec2<u32>(
        slot % cloud_shadow_cache.atlas_columns,
        slot / cloud_shadow_cache.atlas_columns,
    ) * (shadow_cache_tile_texels() + 2u);
    let sample_position = camera_relative_position_m(position_m)
        / visual_pixel_footprint_m;
    let local_position_px = sample_position
        - key * f32(shadow_cache_tile_texels())
        + vec2<f32>(1.0);
    let atlas_position_px = vec2<f32>(slot_origin_px) + local_position_px;
    let atlas_uv = atlas_position_px / vec2<f32>(cloud_shadow_cache.atlas_size_px);
    return textureSampleLevel(
        cloud_shadow_cache_atlas,
        cloud_shadow_cache_sampler,
        atlas_uv,
        0.0,
    ).x;
}

fn sampled_upper_cloud_optical_depth_at(position_m: vec2<f32>) -> f32 {
    let cached_depth = cached_cloud_shadow_optical_depth(position_m, 1u);
    if cached_depth >= 0.0 && !EVALUATE_ALL_BRANCHES {
        return cached_depth;
    }
    let canopy = regional_canopy_conditions_at(position_m);
    return select(
        procedural_upper_cloud_optical_depth(position_m, canopy),
        cached_depth,
        cached_depth >= 0.0,
    );
}

fn sampled_broad_landform_elevation_m(position_m: vec2<f32>) -> f32 {
    let key = cloud_shadow_cache_tile_key(position_m);
    let slot = cloud_shadow_cache_slot(key, 2u);
    let cached = slot < cloud_shadow_cache.maximum_tiles;
    record_shadow_cache_lookup(2u, cached);
    if !cached && !EVALUATE_ALL_BRANCHES {
        return broad_landform_elevation_m(position_m);
    }
    let slot_origin_px = vec2<u32>(
        slot % cloud_shadow_cache.atlas_columns,
        slot / cloud_shadow_cache.atlas_columns,
    ) * (shadow_cache_tile_texels() + 2u);
    let sample_position = camera_relative_position_m(position_m)
        / visual_pixel_footprint_m;
    let local_position_px = sample_position
        - key * f32(shadow_cache_tile_texels())
        + vec2<f32>(1.0);
    let atlas_position_px = vec2<f32>(slot_origin_px) + local_position_px;
    let atlas_uv = atlas_position_px / vec2<f32>(cloud_shadow_cache.atlas_size_px);
    let cached_elevation_m = textureSampleLevel(
        cloud_shadow_cache_atlas,
        cloud_shadow_cache_sampler,
        atlas_uv,
        0.0,
    ).xy;
    if EVALUATE_ALL_BRANCHES {
        return select(
            broad_landform_elevation_m(position_m),
            cached_elevation_m.x + cached_elevation_m.y,
            cached,
        );
    }
    return cached_elevation_m.x + cached_elevation_m.y;
}

fn sampled_cloud_shadow_depths_at(
    projection: CloudShadowProjection,
) -> CloudShadowDepths {
    var depths = CloudShadowDepths(0.0, 0.0);
    if layer_visibility.y > 0.0 {
        var low_depth = cached_cloud_shadow_optical_depth(projection.low_position_m, 0u);
        if low_depth < 0.0 || EVALUATE_ALL_BRANCHES {
            let weather = cloud_weather_conditions_at(projection.low_position_m);
            low_depth = select(low_depth, procedural_low_cloud_optical_depth(
                projection.low_position_m,
                weather,
            ), low_depth < 0.0);
        }
        depths.low = layer_visibility.y * tuning.cloud_shadows.x * low_depth;
    }
    if layer_visibility.z > 0.0 {
        let upper_depth = sampled_upper_cloud_optical_depth_at(projection.upper_position_m);
        depths.upper = layer_visibility.z * tuning.cloud_shadows.y * upper_depth;
    }
    return depths;
}
