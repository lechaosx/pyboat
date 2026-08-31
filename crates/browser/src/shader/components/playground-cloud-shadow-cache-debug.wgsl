struct CloudShadowCacheTile {
    key_x: u32,
    key_y: u32,
    layer: u32,
    padding: u32,
};

struct CloudShadowCacheHeader {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
};

struct CloudShadowCacheDebugUniforms {
    mode: u32,
    padding_0: u32,
    padding_1: u32,
    padding_2: u32,
};

@group(2) @binding(4)
var<storage, read> cloud_shadow_cache_tiles: array<CloudShadowCacheTile>;

@group(2) @binding(5)
var<storage, read> cloud_shadow_cache_header: CloudShadowCacheHeader;

@group(2) @binding(6)
var<uniform> cloud_shadow_cache_debug: CloudShadowCacheDebugUniforms;

var<private> cloud_shadow_cache_debug_hits: vec3<u32>;
var<private> cloud_shadow_cache_debug_misses: u32;

fn initialize_shadow_cache_debug() {
    cloud_shadow_cache_debug_hits = vec3<u32>(0u);
    cloud_shadow_cache_debug_misses = 0u;
}

fn shadow_cache_debug_mode() -> u32 {
    return cloud_shadow_cache_debug.mode;
}

fn shadow_cache_tile_texels() -> u32 {
    return cloud_shadow_cache.tile_texels;
}

fn record_shadow_cache_lookup(layer: u32, hit: bool) {
    if cloud_shadow_cache_debug.mode != 1u {
        return;
    }
    if !hit {
        cloud_shadow_cache_debug_misses += 1u;
    } else if layer == 0u {
        cloud_shadow_cache_debug_hits.x += 1u;
    } else if layer == 1u {
        cloud_shadow_cache_debug_hits.y += 1u;
    } else {
        cloud_shadow_cache_debug_hits.z += 1u;
    }
}

fn shadow_cache_debug_atlas(fragment_position_px: vec2<f32>) -> vec4<f32> {
    let output_uv = fragment_position_px / vec2<f32>(cloud_shadow_cache.viewport_size_px);
    let atlas_position_px = min(
        vec2<u32>(output_uv * vec2<f32>(cloud_shadow_cache.atlas_size_px)),
        cloud_shadow_cache.atlas_size_px - vec2<u32>(1u),
    );
    let value = textureLoad(cloud_shadow_cache_atlas, atlas_position_px, 0).x;
    let physical_tile_texels = shadow_cache_tile_texels() + 2u;
    let atlas_tile = atlas_position_px / vec2<u32>(physical_tile_texels);
    let slot = atlas_tile.y * cloud_shadow_cache.atlas_columns + atlas_tile.x;
    if slot >= atomicLoad(&cloud_shadow_cache_header.instance_count) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let layer = cloud_shadow_cache_tiles[slot].layer;
    if layer == 0u {
        return vec4<f32>(1.0 - exp(-max(value, 0.0)), 0.0, 0.0, 1.0);
    }
    if layer == 1u {
        return vec4<f32>(0.0, 0.0, 1.0 - exp(-max(value, 0.0)), 1.0);
    }
    let elevation = 0.5 + 0.5 * tanh(value / 100.0);
    return vec4<f32>(0.0, elevation, 0.0, 1.0);
}

fn apply_shadow_cache_debug(color: vec4<f32>) -> vec4<f32> {
    if cloud_shadow_cache_debug.mode != 1u {
        return color;
    }
    let hits = vec3<f32>(cloud_shadow_cache_debug_hits);
    let misses = f32(cloud_shadow_cache_debug_misses);
    let hit_count = hits.x + hits.y + hits.z;
    let total = hit_count + misses;
    if total <= 0.0 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let hit_color = vec3<f32>(hits.x, hits.z, hits.y) / max(hit_count, 1.0);
    let fallback_color = vec3<f32>(1.0, 0.35, 0.0);
    return vec4<f32>(mix(hit_color, fallback_color, misses / total), 1.0);
}
