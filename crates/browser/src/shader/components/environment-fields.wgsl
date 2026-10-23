// Environment noise evaluated once per frame on the lattice in
// `scene.environment_field_grid_m`: the first texel centre relative to the
// camera, then the texel spacing. `scene.environment_field_texels.xy` counts
// the texels written this frame; lookups clamp to them. Three later layers
// hold the cumulus and deck drivers and the ground-sky bounce conditions on
// the cloud weather lattice, which `scene.cloud_weather_grid_m` places the
// same way. Layers are rgba32float
// because pressure potentials exceed half-float range and temperature-noise
// gradients fall into half-float subnormals. WebGPU does not guarantee filtering of 32-bit float
// textures, so reconstruction blends four loaded texels.
@group(0) @binding(1)
var environment_fields: texture_2d_array<f32>;

// The scene lattice's layers, then the cloud weather lattice's; keep the total
// synchronized with ENVIRONMENT_FIELD_LAYERS in field_grids.rs.
const ENVIRONMENT_FIELD_LAYERS: u32 = 5u;
const CUMULUS_DRIVER_FIELD: u32 = 5u;
const DECK_DRIVER_FIELD: u32 = 6u;
const GROUND_BOUNCE_FIELD: u32 = 7u;

struct EnvironmentFieldSample {
    atmosphere: AtmosphereNoiseSample,
    upper_air_temperature: vec2<f32>,
    tide: SurfaceTideSample,
}

fn procedural_environment_field_sample(position_m: vec2<f32>) -> EnvironmentFieldSample {
    return EnvironmentFieldSample(
        atmosphere_noise_sample_at(position_m),
        upper_air_temperature_500_hpa_noise_at(position_m),
        surface_tide_sample_at(position_m),
    );
}

fn environment_field_layers(
    sample: EnvironmentFieldSample,
) -> array<vec4<f32>, ENVIRONMENT_FIELD_LAYERS> {
    let pressure = sample.atmosphere.pressure;
    let temperature = sample.atmosphere.temperature;
    return array<vec4<f32>, ENVIRONMENT_FIELD_LAYERS>(
        vec4<f32>(
            pressure.potential_m2_s,
            pressure.gradient_m_s,
            pressure.climate_potential_m2_s,
        ),
        vec4<f32>(pressure.synoptic_gradient_m_s, sample.upper_air_temperature),
        vec4<f32>(
            temperature.value,
            temperature.gradient,
            sample.atmosphere.broad_temperature,
        ),
        vec4<f32>(
            sample.atmosphere.water_vapor,
            sample.tide.response.value,
            sample.tide.lag.value,
        ),
        vec4<f32>(sample.tide.response.gradient, sample.tide.lag.gradient),
    );
}

fn environment_field_sample_from_layers(
    layers: array<vec4<f32>, ENVIRONMENT_FIELD_LAYERS>,
) -> EnvironmentFieldSample {
    return EnvironmentFieldSample(
        AtmosphereNoiseSample(
            // The stored pressure omits the surface-gust band and the
            // broad-synoptic gradient and per-band potentials, which only
            // diagnostics read, through direct evaluation.
            PressureFieldSample(
                layers[0].x,
                layers[0].yz,
                layers[1].xy,
                vec2<f32>(0.0),
                layers[0].w,
                0.0,
                0.0,
                0.0,
            ),
            TerrainNoiseSample(layers[2].x, layers[2].yz),
            layers[2].w,
            layers[3].xy,
        ),
        layers[1].zw,
        SurfaceTideSample(
            TerrainNoiseSample(layers[3].z, layers[4].xy),
            TerrainNoiseSample(layers[3].w, layers[4].zw),
        ),
    );
}

fn environment_field_layer(
    texel: vec2<i32>,
    weight: vec2<f32>,
    layer: u32,
) -> vec4<f32> {
    let lower = mix(
        textureLoad(environment_fields, texel, layer, 0),
        textureLoad(environment_fields, texel + vec2<i32>(1, 0), layer, 0),
        weight.x,
    );
    let upper = mix(
        textureLoad(environment_fields, texel + vec2<i32>(0, 1), layer, 0),
        textureLoad(environment_fields, texel + vec2<i32>(1, 1), layer, 0),
        weight.x,
    );
    return mix(lower, upper, weight.y);
}

struct EnvironmentFieldLookup {
    texel: vec2<i32>,
    weight: vec2<f32>,
};

fn environment_field_lookup(
    position_m: vec2<f32>,
    grid_m: vec4<f32>,
    texels: vec4<u32>,
) -> EnvironmentFieldLookup {
    let texel_position = (camera_relative_position_m(position_m) - grid_m.xy) / grid_m.zw;
    let last_texel = vec2<i32>(texels.xy) - vec2<i32>(2);
    let texel = clamp(vec2<i32>(floor(texel_position)), vec2<i32>(0), last_texel);
    return EnvironmentFieldLookup(
        texel,
        clamp(texel_position - vec2<f32>(texel), vec2<f32>(0.0), vec2<f32>(1.0)),
    );
}

fn environment_field_sample(position_m: vec2<f32>) -> EnvironmentFieldSample {
    let lookup = environment_field_lookup(
        position_m,
        scene.environment_field_grid_m,
        scene.environment_field_texels,
    );
    return environment_field_sample_from_layers(array<vec4<f32>, ENVIRONMENT_FIELD_LAYERS>(
        environment_field_layer(lookup.texel, lookup.weight, 0u),
        environment_field_layer(lookup.texel, lookup.weight, 1u),
        environment_field_layer(lookup.texel, lookup.weight, 2u),
        environment_field_layer(lookup.texel, lookup.weight, 3u),
        environment_field_layer(lookup.texel, lookup.weight, 4u),
    ));
}

fn cloud_weather_lookup(position_m: vec2<f32>) -> EnvironmentFieldLookup {
    return environment_field_lookup(
        position_m,
        scene.cloud_weather_grid_m,
        scene.cloud_weather_texels,
    );
}

fn cumulus_drivers_from_cloud_weather_fields(position_m: vec2<f32>) -> CumulusDrivers {
    let lookup = cloud_weather_lookup(position_m);
    let texel = lookup.texel;
    let weight = lookup.weight;
    let corners = array<vec4<f32>, 4>(
        textureLoad(environment_fields, texel, CUMULUS_DRIVER_FIELD, 0),
        textureLoad(environment_fields, texel + vec2<i32>(1, 0), CUMULUS_DRIVER_FIELD, 0),
        textureLoad(environment_fields, texel + vec2<i32>(0, 1), CUMULUS_DRIVER_FIELD, 0),
        textureLoad(environment_fields, texel + vec2<i32>(1, 1), CUMULUS_DRIVER_FIELD, 0),
    );
    let lower = mix(corners[0], corners[1], weight.x);
    let upper = mix(corners[2], corners[3], weight.x);
    return CumulusDrivers(
        mix(lower, upper, weight.y),
        vec2<f32>(
            mix(corners[1].x - corners[0].x, corners[3].x - corners[2].x, weight.y),
            upper.x - lower.x,
        ) / scene.cloud_weather_grid_m.zw,
    );
}

fn deck_drivers_from_cloud_weather_fields(position_m: vec2<f32>) -> vec4<f32> {
    let lookup = cloud_weather_lookup(position_m);
    return environment_field_layer(lookup.texel, lookup.weight, DECK_DRIVER_FIELD);
}

fn cloud_weather_field_from_environment_fields(
    position_m: vec2<f32>,
    fields: EnvironmentFieldSample,
) -> CloudWeatherFieldSample {
    let atmosphere = surface_atmosphere_sample_from_noise(position_m, fields.atmosphere);
    return CloudWeatherFieldSample(
        atmosphere,
        cloud_weather_from_atmosphere_and_upper_air(
            atmosphere,
            upper_air_temperature_500_hpa_field_from_noise(
                atmosphere.temperature_anomaly.value,
                atmosphere.broad_temperature_anomaly_c,
                atmosphere.climate,
                fields.upper_air_temperature,
            ),
        ),
    );
}
