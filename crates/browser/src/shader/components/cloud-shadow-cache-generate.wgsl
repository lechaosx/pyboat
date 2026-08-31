struct CloudShadowCacheUniforms {
    viewport_size_px: vec2<u32>,
    atlas_size_px: vec2<u32>,
    table_mask: u32,
    maximum_tiles: u32,
    atlas_columns: u32,
    padding: u32,
};

struct CloudShadowCacheHeader {
    vertex_count: u32,
    instance_count: atomic<u32>,
    first_vertex: u32,
    first_instance: u32,
};

struct CloudShadowCacheEntry {
    state: atomic<u32>,
    key_x: u32,
    key_y: u32,
    layer: u32,
    slot: u32,
};

struct CloudShadowCacheTile {
    key_x: u32,
    key_y: u32,
    layer: u32,
    padding: u32,
};

@group(2) @binding(0)
var<uniform> cloud_shadow_cache: CloudShadowCacheUniforms;

@group(2) @binding(1)
var<storage, read_write> cloud_shadow_cache_header: CloudShadowCacheHeader;

@group(2) @binding(2)
var<storage, read_write> cloud_shadow_cache_entries: array<CloudShadowCacheEntry>;

@group(2) @binding(3)
var<storage, read_write> cloud_shadow_cache_tiles: array<CloudShadowCacheTile>;

@group(2) @binding(4)
var<storage, read> cloud_shadow_cache_tiles_for_rendering: array<CloudShadowCacheTile>;

const CLOUD_SHADOW_CACHE_TILE_TEXELS: f32 = 128.0;
const CLOUD_SHADOW_CACHE_PHYSICAL_TILE_TEXELS: u32 = 130u;
const CLOUD_SHADOW_CACHE_ENTRY_BUSY: u32 = 0xffffffffu;
const CLOUD_SHADOW_CACHE_DISCOVERY_STRIDE_PX: u32 = 16u;

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

fn insert_cloud_shadow_cache_tile_key(key: vec2<f32>, layer: u32) {
    let tag = cloud_shadow_cache_hash(key, layer);
    var table_index = tag & cloud_shadow_cache.table_mask;
    for (var probe = 0u; probe < 16u; probe++) {
        let claim = atomicCompareExchangeWeak(
            &cloud_shadow_cache_entries[table_index].state,
            0u,
            CLOUD_SHADOW_CACHE_ENTRY_BUSY,
        );
        if claim.exchanged {
            var slot = cloud_shadow_cache.maximum_tiles;
            var tile_count = atomicLoad(&cloud_shadow_cache_header.instance_count);
            loop {
                if tile_count >= cloud_shadow_cache.maximum_tiles {
                    break;
                }
                let allocation = atomicCompareExchangeWeak(
                    &cloud_shadow_cache_header.instance_count,
                    tile_count,
                    tile_count + 1u,
                );
                if allocation.exchanged {
                    slot = tile_count;
                    break;
                }
                tile_count = allocation.old_value;
            }
            cloud_shadow_cache_entries[table_index].key_x = bitcast<u32>(key.x);
            cloud_shadow_cache_entries[table_index].key_y = bitcast<u32>(key.y);
            cloud_shadow_cache_entries[table_index].layer = layer;
            cloud_shadow_cache_entries[table_index].slot = slot;
            if slot < cloud_shadow_cache.maximum_tiles {
                cloud_shadow_cache_tiles[slot] = CloudShadowCacheTile(
                    bitcast<u32>(key.x),
                    bitcast<u32>(key.y),
                    layer,
                    0u,
                );
            }
            atomicStore(&cloud_shadow_cache_entries[table_index].state, tag);
            return;
        }
        if claim.old_value == CLOUD_SHADOW_CACHE_ENTRY_BUSY {
            return;
        }
        if claim.old_value == tag
            && cloud_shadow_cache_entries[table_index].key_x == bitcast<u32>(key.x)
            && cloud_shadow_cache_entries[table_index].key_y == bitcast<u32>(key.y)
            && cloud_shadow_cache_entries[table_index].layer == layer
        {
            return;
        }
        table_index = (table_index + 1u) & cloud_shadow_cache.table_mask;
    }
}

@compute @workgroup_size(8, 8)
fn discover_cloud_shadow_cache_tiles(
    @builtin(global_invocation_id) invocation_id: vec3<u32>,
) {
    let pixel_origin = invocation_id.xy * CLOUD_SHADOW_CACHE_DISCOVERY_STRIDE_PX;
    if any(pixel_origin >= cloud_shadow_cache.viewport_size_px) {
        return;
    }
    let pixel_position = min(
        pixel_origin + vec2<u32>(CLOUD_SHADOW_CACHE_DISCOVERY_STRIDE_PX / 2u),
        cloud_shadow_cache.viewport_size_px - vec2<u32>(1u),
    );
    let screen_position = (vec2<f32>(pixel_position) + vec2<f32>(0.5))
        / vec2<f32>(cloud_shadow_cache.viewport_size_px);
    let clip_position = vec2<f32>(
        2.0 * screen_position.x - 1.0,
        1.0 - 2.0 * screen_position.y,
    );
    let raw_position_m = scene.camera_and_view_m.xy
        + 0.5 * clip_position * scene.camera_and_view_m.zw;
    let position_m = canonical_world_position(raw_position_m);
    initialize_cloud_shadow_cache_discovery(position_m);
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let is_sun = source_index == 0u;
        let light = select(moon_direction(), sun_direction(), is_sun);
        let light_amount = select(direct_moon_amount(), direct_sun_amount(), is_sun);
        if light_amount > 0.0 {
            let projection = cloud_shadow_projection_at(position_m, 0.0, light);
            insert_cloud_shadow_cache_tile_key(
                cloud_shadow_cache_tile_key(projection.low_position_m),
                0u,
            );
            insert_cloud_shadow_cache_tile_key(
                cloud_shadow_cache_tile_key(projection.upper_position_m),
                1u,
            );
        }
    }
}

struct CloudShadowCacheVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position_m: vec2<f32>,
    @location(1) @interpolate(flat) layer: u32,
};

@vertex
fn cloud_shadow_cache_vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> CloudShadowCacheVertexOutput {
    initialize_cloud_shadow_cache_generation(scene.camera_and_view_m.xy);
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
    );
    let tile = cloud_shadow_cache_tiles_for_rendering[instance_index];
    let atlas_tile = vec2<u32>(
        instance_index % cloud_shadow_cache.atlas_columns,
        instance_index / cloud_shadow_cache.atlas_columns,
    );
    let local_position_px = corners[vertex_index]
        * f32(CLOUD_SHADOW_CACHE_PHYSICAL_TILE_TEXELS);
    let atlas_position_px = vec2<f32>(
        atlas_tile * CLOUD_SHADOW_CACHE_PHYSICAL_TILE_TEXELS,
    ) + local_position_px;
    let atlas_position = atlas_position_px / vec2<f32>(cloud_shadow_cache.atlas_size_px);
    let key = vec2<f32>(bitcast<f32>(tile.key_x), bitcast<f32>(tile.key_y));
    let relative_position_m = (
        key * CLOUD_SHADOW_CACHE_TILE_TEXELS
            + local_position_px
            - vec2<f32>(1.0)
    ) * visual_pixel_footprint_m;
    var output: CloudShadowCacheVertexOutput;
    output.position = vec4<f32>(
        2.0 * atlas_position.x - 1.0,
        1.0 - 2.0 * atlas_position.y,
        0.0,
        1.0,
    );
    output.world_position_m = canonical_world_position(
        scene.camera_and_view_m.xy + relative_position_m,
    );
    output.layer = tile.layer;
    return output;
}

@fragment
fn cloud_shadow_cache_fragment(
    input: CloudShadowCacheVertexOutput,
) -> @location(0) f32 {
    initialize_cloud_shadow_cache_generation(input.world_position_m);
    if input.layer == 0u {
        let weather = cloud_weather_conditions_at(input.world_position_m);
        return procedural_low_cloud_optical_depth(input.world_position_m, weather);
    }
    let canopy = regional_canopy_conditions_at(input.world_position_m);
    return procedural_upper_cloud_optical_depth(input.world_position_m, canopy);
}
