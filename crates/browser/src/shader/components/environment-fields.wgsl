// Environment noise evaluated once per frame on the lattice in
// `scene.environment_field_grid_m`: the first texel centre relative to the
// camera, then the texel spacing. `scene.environment_field_texels.xy` counts
// the texels written this frame; lookups clamp to them. Layers are rgba32float
// because pressure potentials exceed half-float range and temperature-noise
// gradients fall into half-float subnormals. WebGPU does not guarantee filtering of 32-bit float
// textures, so reconstruction blends four loaded texels.
@group(0) @binding(1)
var environment_fields: texture_2d_array<f32>;

const ENVIRONMENT_FIELD_LAYERS: u32 = 5u;

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

fn environment_field_sample(position_m: vec2<f32>) -> EnvironmentFieldSample {
    let texel_position = (
        camera_relative_position_m(position_m) - scene.environment_field_grid_m.xy
    ) / scene.environment_field_grid_m.zw;
    let last_texel = vec2<i32>(scene.environment_field_texels.xy) - vec2<i32>(2);
    let texel = clamp(vec2<i32>(floor(texel_position)), vec2<i32>(0), last_texel);
    let weight = clamp(texel_position - vec2<f32>(texel), vec2<f32>(0.0), vec2<f32>(1.0));
    return environment_field_sample_from_layers(array<vec4<f32>, ENVIRONMENT_FIELD_LAYERS>(
        environment_field_layer(texel, weight, 0u),
        environment_field_layer(texel, weight, 1u),
        environment_field_layer(texel, weight, 2u),
        environment_field_layer(texel, weight, 3u),
        environment_field_layer(texel, weight, 4u),
    ));
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

fn cloud_weather_from_environment_fields(position_m: vec2<f32>) -> CloudWeatherSample {
    return cloud_weather_field_from_environment_fields(
        position_m,
        environment_field_sample(position_m),
    ).weather;
}
