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
    slot: atomic<u32>,
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

const CLOUD_SHADOW_CACHE_ENTRY_BUSY: u32 = 0xffffffffu;
const CLOUD_SHADOW_CACHE_SLOT_ALLOCATED: u32 = 0x80000000u;
const CLOUD_SHADOW_CACHE_DISCOVERY_STRIDE_PX: u32 = 16u;
// A coarse multiple plus a small residual keeps kilometre-scale elevations
// precise below the shortest land-shadow transition after f16 conversion.
const LAND_ELEVATION_CACHE_COARSE_STEP_M: f32 = 16.0;

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
            / f32(shadow_cache_tile_texels()),
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

fn allocate_cloud_shadow_cache_tile(
    table_index: u32,
    key: vec2<f32>,
    layer: u32,
) {
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
    if slot < cloud_shadow_cache.maximum_tiles {
        cloud_shadow_cache_tiles[slot] = CloudShadowCacheTile(
            bitcast<u32>(key.x),
            bitcast<u32>(key.y),
            layer,
            0u,
        );
    }
    atomicStore(
        &cloud_shadow_cache_entries[table_index].slot,
        CLOUD_SHADOW_CACHE_SLOT_ALLOCATED | slot,
    );
}

fn mark_cloud_shadow_cache_tile_key(
    key: vec2<f32>,
    layer: u32,
    footprint_index: u32,
) {
    let tag = cloud_shadow_cache_hash(key, layer);
    let footprint_bit = 1u << footprint_index;
    var table_index = tag & cloud_shadow_cache.table_mask;
    for (var probe = 0u; probe < 16u; probe++) {
        let claim = atomicCompareExchangeWeak(
            &cloud_shadow_cache_entries[table_index].state,
            0u,
            CLOUD_SHADOW_CACHE_ENTRY_BUSY,
        );
        if claim.exchanged {
            cloud_shadow_cache_entries[table_index].key_x = bitcast<u32>(key.x);
            cloud_shadow_cache_entries[table_index].key_y = bitcast<u32>(key.y);
            cloud_shadow_cache_entries[table_index].layer = layer;
            atomicStore(&cloud_shadow_cache_entries[table_index].slot, footprint_bit);
            atomicStore(&cloud_shadow_cache_entries[table_index].state, tag);
            return;
        }
        if claim.old_value == CLOUD_SHADOW_CACHE_ENTRY_BUSY {
            continue;
        }
        if claim.old_value == tag
            && cloud_shadow_cache_entries[table_index].key_x == bitcast<u32>(key.x)
            && cloud_shadow_cache_entries[table_index].key_y == bitcast<u32>(key.y)
            && cloud_shadow_cache_entries[table_index].layer == layer
        {
            var footprints = atomicLoad(
                &cloud_shadow_cache_entries[table_index].slot,
            );
            loop {
                if (footprints & CLOUD_SHADOW_CACHE_SLOT_ALLOCATED) != 0u {
                    return;
                }
                let marked_footprints = footprints | footprint_bit;
                let mark = atomicCompareExchangeWeak(
                    &cloud_shadow_cache_entries[table_index].slot,
                    footprints,
                    marked_footprints,
                );
                if !mark.exchanged {
                    footprints = mark.old_value;
                    continue;
                }
                if (footprints & ~footprint_bit) == 0u {
                    return;
                }
                var allocation_state = marked_footprints;
                loop {
                    if (allocation_state & CLOUD_SHADOW_CACHE_SLOT_ALLOCATED) != 0u {
                        return;
                    }
                    let allocation = atomicCompareExchangeWeak(
                        &cloud_shadow_cache_entries[table_index].slot,
                        allocation_state,
                        CLOUD_SHADOW_CACHE_SLOT_ALLOCATED,
                    );
                    if allocation.exchanged {
                        allocate_cloud_shadow_cache_tile(table_index, key, layer);
                        return;
                    }
                    allocation_state = allocation.old_value;
                }
            }
        }
        table_index = (table_index + 1u) & cloud_shadow_cache.table_mask;
    }
}

fn landform_shadow_cache_offset(
    source_index: u32,
    sample_index: u32,
    reach_m: f32,
    direct_light: CelestialReceiverLight,
) -> vec3<f32> {
    let is_sun = source_index == 0u;
    let light = select(
        direct_light.moon_direction,
        direct_light.sun_direction,
        is_sun,
    );
    let light_amount = select(
        direct_light.moon_amount,
        direct_light.sun_amount,
        is_sun,
    );
    let horizontal_light = length(light.xy);
    if light_amount <= 0.0 || light.z <= 0.0 || horizontal_light <= 0.001 {
        return vec3<f32>(0.0);
    }
    let distance_m = reach_m * exp2(mix(-4.0, 0.0, f32(sample_index) / 3.0));
    return vec3<f32>(distance_m * light.xy / horizontal_light, 1.0);
}

struct CloudShadowCacheFootprint {
    enabled: bool,
    light: vec3<f32>,
    receiver_height_m: f32,
    cloud_height_m: f32,
};

fn cloud_shadow_cache_footprint(
    footprint_index: u32,
    layer: u32,
    surface_light: CelestialReceiverLight,
    low_cloud_light: CelestialReceiverLight,
) -> CloudShadowCacheFootprint {
    // Indices 0/1 are Sun/Moon at the surface; 2/3 are Sun/Moon at low clouds.
    let is_sun = footprint_index % 2u == 0u;
    let receives_at_low_cloud = layer == 1u && footprint_index >= 2u;
    var receiver_light = surface_light;
    if receives_at_low_cloud {
        receiver_light = low_cloud_light;
    }
    var light = select(
        receiver_light.moon_direction,
        receiver_light.sun_direction,
        is_sun,
    );
    var light_amount = select(
        receiver_light.moon_amount,
        receiver_light.sun_amount,
        is_sun,
    );
    var enabled = false;
    let cloud_height_m = select(
        WEATHER_CANOPY_HEIGHT_M,
        LOW_CLOUD_BASE_HEIGHT_M,
        layer == 0u,
    );
    if receives_at_low_cloud {
        let angular_radius_rad = select(
            MOON_ANGULAR_RADIUS_RAD,
            CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
            is_sun,
        );
        light = celestial_visible_disc_sample_direction_at_horizon(
            receiver_light.horizon_z,
            light,
            angular_radius_rad,
        );
        enabled = layer_visibility.x > 0.0
            && layer_visibility.w > 0.0
            && light_amount > 0.0;
    } else {
        let layer_active = select(
            layer_visibility.z > 0.0 && tuning.cloud_shadows.y > 0.0,
            layer_visibility.y > 0.0 && tuning.cloud_shadows.x > 0.0,
            layer == 0u,
        );
        enabled = layer_active
            && light_amount > 0.0
            && cloud_shadow_contrast(light) > 0.0;
    }
    return CloudShadowCacheFootprint(
        enabled,
        light,
        receiver_light.height_m,
        cloud_height_m,
    );
}

fn cloud_shadow_cache_footprint_position(
    receiver_position_m: vec2<f32>,
    footprint: CloudShadowCacheFootprint,
) -> vec2<f32> {
    return projected_cloud_position(
        receiver_position_m,
        footprint.receiver_height_m,
        footprint.cloud_height_m,
        footprint.light,
    );
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
    let broad_receiver_elevation_m = broad_landform_elevation_m(position_m);
    let water_height_m = tide_height_m();
    let landform_shadows_visible = tuning.landform_lighting.y
        * landscape_visibility.w * lighting_feature_visibility().y > 0.0
        && broad_receiver_elevation_m >= water_height_m;
    let reach_m = max(tuning.landform_lighting.z, 1.0);
    let surface_receiver_height_m = max(broad_receiver_elevation_m, water_height_m);
    let surface_light = celestial_receiver_light_at_height(surface_receiver_height_m);
    let low_cloud_light = celestial_receiver_light_at_height(LOW_CLOUD_BASE_HEIGHT_M);
    for (var layer = 0u; layer < 2u; layer++) {
        let footprint_count = select(4u, 2u, layer == 0u);
        var footprints: array<CloudShadowCacheFootprint, 4>;
        for (var footprint_index = 0u; footprint_index < footprint_count; footprint_index++) {
            footprints[footprint_index] = cloud_shadow_cache_footprint(
                footprint_index,
                layer,
                surface_light,
                low_cloud_light,
            );
        }
        for (var footprint_index = 0u; footprint_index < footprint_count; footprint_index++) {
            let footprint = footprints[footprint_index];
            if !footprint.enabled {
                continue;
            }
            let projected_position_m = cloud_shadow_cache_footprint_position(
                position_m,
                footprint,
            );
            mark_cloud_shadow_cache_tile_key(
                cloud_shadow_cache_tile_key(projected_position_m),
                layer,
                footprint_index,
            );
        }
    }
    for (var source_index = 0u; source_index < 2u; source_index++) {
        if landform_shadows_visible {
            for (var sample_index = 0u; sample_index < 4u; sample_index++) {
                let offset = landform_shadow_cache_offset(
                    source_index,
                    sample_index,
                    reach_m,
                    surface_light,
                );
                if offset.z > 0.0 {
                    let sample_position_m = canonical_world_position(position_m + offset.xy);
                    mark_cloud_shadow_cache_tile_key(
                        cloud_shadow_cache_tile_key(sample_position_m),
                        2u,
                        4u * source_index + sample_index,
                    );
                }
            }
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
        * f32(shadow_cache_tile_texels() + 2u);
    let atlas_position_px = vec2<f32>(
        atlas_tile * (shadow_cache_tile_texels() + 2u),
    ) + local_position_px;
    let atlas_position = atlas_position_px / vec2<f32>(cloud_shadow_cache.atlas_size_px);
    let key = vec2<f32>(bitcast<f32>(tile.key_x), bitcast<f32>(tile.key_y));
    let relative_position_m = (
        key * f32(shadow_cache_tile_texels())
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
) -> @location(0) vec2<f32> {
    initialize_cloud_shadow_cache_generation(input.world_position_m);
    if input.layer == 0u {
        let weather = cloud_weather_conditions_at(input.world_position_m);
        return vec2<f32>(
            procedural_low_cloud_optical_depth(input.world_position_m, weather),
            0.0,
        );
    }
    if input.layer == 1u {
        let canopy = regional_canopy_conditions_at(input.world_position_m);
        return vec2<f32>(
            procedural_upper_cloud_optical_depth(input.world_position_m, canopy),
            0.0,
        );
    }
    let elevation_m = broad_landform_elevation_m(input.world_position_m);
    let coarse_elevation_m = floor(
        elevation_m / LAND_ELEVATION_CACHE_COARSE_STEP_M,
    ) * LAND_ELEVATION_CACHE_COARSE_STEP_M;
    return vec2<f32>(elevation_m - coarse_elevation_m, coarse_elevation_m);
}
