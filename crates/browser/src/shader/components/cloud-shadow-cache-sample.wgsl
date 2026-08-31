struct CloudShadowCacheUniforms {
    viewport_size_px: vec2<u32>,
    atlas_size_px: vec2<u32>,
    table_mask: u32,
    maximum_tiles: u32,
    atlas_columns: u32,
    padding: u32,
};

struct CloudShadowCacheEntry {
    state: u32,
    key_x: u32,
    key_y: u32,
    layer: u32,
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

const CLOUD_SHADOW_CACHE_TILE_TEXELS: f32 = 128.0;
const CLOUD_SHADOW_CACHE_PHYSICAL_TILE_TEXELS: u32 = 130u;
const CLOUD_SHADOW_CACHE_ENTRY_BUSY: u32 = 0xffffffffu;

fn cloud_shadow_cache_relative_position(position_m: vec2<f32>) -> vec2<f32> {
    let longitude_delta_m = position_m.x - scene.camera_and_view_m.x;
    let wrapped_longitude_delta_m = longitude_delta_m
        - round(longitude_delta_m * scene.inverse_world_size_m.x)
            * scene.world_size_m.x;
    return vec2<f32>(
        wrapped_longitude_delta_m,
        position_m.y - scene.camera_and_view_m.y,
    );
}

fn cloud_shadow_cache_tile_key(position_m: vec2<f32>) -> vec2<f32> {
    let sample_position = cloud_shadow_cache_relative_position(position_m)
        / visual_pixel_footprint_m;
    return floor(
        (sample_position - vec2<f32>(0.5))
            / CLOUD_SHADOW_CACHE_TILE_TEXELS,
    );
}

fn cloud_shadow_cache_hash(key: vec2<f32>, layer: u32) -> u32 {
    var value = bitcast<u32>(key.x) * 0x9e3779b9u;
    value = value ^ (bitcast<u32>(key.y) * 0x85ebca6bu);
    value = value ^ (layer * 0xc2b2ae35u);
    value = value ^ (value >> 16u);
    value = value * 0x7feb352du;
    value = value ^ (value >> 15u);
    let tag = (value * 0x846ca68bu) | 1u;
    return select(tag, tag ^ 0x80000000u, tag == CLOUD_SHADOW_CACHE_ENTRY_BUSY);
}

fn cloud_shadow_cache_slot(key: vec2<f32>, layer: u32) -> u32 {
    let tag = cloud_shadow_cache_hash(key, layer);
    var table_index = tag & cloud_shadow_cache.table_mask;
    for (var probe = 0u; probe < 16u; probe++) {
        let entry = cloud_shadow_cache_entries[table_index];
        if entry.state == 0u {
            break;
        }
        if entry.state == tag
            && entry.key_x == bitcast<u32>(key.x)
            && entry.key_y == bitcast<u32>(key.y)
            && entry.layer == layer
        {
            return entry.slot;
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
    if slot >= cloud_shadow_cache.maximum_tiles {
        return -1.0;
    }
    let slot_origin_px = vec2<u32>(
        slot % cloud_shadow_cache.atlas_columns,
        slot / cloud_shadow_cache.atlas_columns,
    ) * CLOUD_SHADOW_CACHE_PHYSICAL_TILE_TEXELS;
    let sample_position = cloud_shadow_cache_relative_position(position_m)
        / visual_pixel_footprint_m;
    let local_position_px = sample_position
        - key * CLOUD_SHADOW_CACHE_TILE_TEXELS
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
    if cached_depth >= 0.0 {
        return cached_depth;
    }
    let canopy = regional_canopy_conditions_at(position_m);
    return procedural_upper_cloud_optical_depth(position_m, canopy);
}

fn sampled_cloud_shadow_depths_at(
    projection: CloudShadowProjection,
) -> CloudShadowDepths {
    var depths = CloudShadowDepths(0.0, 0.0);
    if layer_visibility.y > 0.0 {
        var low_depth = cached_cloud_shadow_optical_depth(projection.low_position_m, 0u);
        if low_depth < 0.0 {
            let weather = cloud_weather_conditions_at(projection.low_position_m);
            low_depth = procedural_low_cloud_optical_depth(
                projection.low_position_m,
                weather,
            );
        }
        depths.low = layer_visibility.y * tuning.cloud_shadows.x * low_depth;
    }
    if layer_visibility.z > 0.0 {
        let upper_depth = sampled_upper_cloud_optical_depth_at(projection.upper_position_m);
        depths.upper = layer_visibility.z * tuning.cloud_shadows.y * upper_depth;
    }
    return depths;
}
