fn rotate_left(vector: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(-vector.y, vector.x);
}

struct PressureFieldSample {
    potential_m2_s: f32,
    gradient_m_s: vec2<f32>,
    synoptic_gradient_m_s: vec2<f32>,
    broad_synoptic_gradient_m_s: vec2<f32>,
    climate_potential_m2_s: f32,
    synoptic_potential_m2_s: f32,
    local_potential_m2_s: f32,
    surface_gust_signal: f32,
}

struct SurfaceClimateSample {
    latitude_degrees: f32,
    seasonal_latitude_factor: f32,
    annual_mean_air_temperature_c: f32,
    seasonal_temperature_range_c: f32,
    diurnal_temperature_range_c: f32,
    local_solar_phase: f32,
}

struct SurfaceAtmosphereSample {
    pressure: PressureFieldSample,
    temperature_anomaly: TerrainNoiseSample,
    water_vapor_g_kg: f32,
    climate: SurfaceClimateSample,
}

struct CloudWeatherFieldSample {
    atmosphere: SurfaceAtmosphereSample,
    weather: CloudWeatherSample,
}

struct SpatiotemporalNoiseDomain {
    point: vec4<f32>,
    longitude: f32,
    latitude_sign: f32,
};

fn spatiotemporal_noise_domain(
    field_seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
    time_coordinate: f32,
) -> SpatiotemporalNoiseDomain {
    let orientation = terrain_hash(
        field_seed ^ vec2<u32>(0xf3bcc909u, 0x6a09e667u),
    ).x;
    let longitude_offset = TAU * f32(orientation & 0xffu) / 256.0;
    let latitude_sign = select(1.0, -1.0, (orientation & 0x100u) != 0u);
    let latitude_offset = f32((orientation >> 9u) & 0xffu) / 16.0;
    let time_offset = f32((orientation >> 17u) & 0x7fffu) / 64.0;
    let longitude = TAU * canonical_world_position(position_m).x
        * scene.cloud_anchor_phase_and_inverse_world_size_m.z
        + longitude_offset;
    let circle_radius = scene.world_size_m.x / (TAU * wavelength_m);
    return SpatiotemporalNoiseDomain(
        vec4<f32>(
            circle_radius * cos(longitude),
            circle_radius * sin(longitude),
            latitude_sign * position_m.y / wavelength_m + latitude_offset,
            time_coordinate + time_offset,
        ),
        longitude,
        latitude_sign,
    );
}

fn spatiotemporal_noise_sample(
    field_seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
    time_coordinate: f32,
) -> TerrainNoiseSample {
    let domain = spatiotemporal_noise_domain(
        field_seed,
        position_m,
        wavelength_m,
        time_coordinate,
    );
    let simplex = simplex_noise_sample_4d(field_seed, domain.point);
    return TerrainNoiseSample(
        simplex.value,
        vec2<f32>(
            (-sin(domain.longitude) * simplex.gradient.x
                + cos(domain.longitude) * simplex.gradient.y) / wavelength_m,
            domain.latitude_sign * simplex.gradient.z / wavelength_m,
        ),
    );
}

fn spatiotemporal_noise_value(
    field_seed: vec2<u32>,
    position_m: vec2<f32>,
    wavelength_m: f32,
    time_coordinate: f32,
) -> f32 {
    let domain = spatiotemporal_noise_domain(
        field_seed,
        position_m,
        wavelength_m,
        time_coordinate,
    );
    return simplex_noise_value_4d(field_seed, domain.point);
}

fn spatiotemporal_pressure_noise_sample(
    field_seed: vec2<u32>,
    position_m: vec2<f32>,
    zonal_wavelength_m: f32,
    meridional_wavelength_m: f32,
    time_coordinate: f32,
    zonal_aspect: f32,
    zonal_aspect_gradient_per_m: f32,
) -> TerrainNoiseSample {
    let orientation = terrain_hash(
        field_seed ^ vec2<u32>(0xf3bcc909u, 0x6a09e667u),
    ).x;
    let longitude_offset = TAU * f32(orientation & 0xffu) / 256.0;
    let latitude_sign = select(1.0, -1.0, (orientation & 0x100u) != 0u);
    let latitude_offset = f32((orientation >> 9u) & 0xffu) / 16.0;
    let time_offset = f32((orientation >> 17u) & 0x7fffu) / 64.0;
    let longitude = TAU * canonical_world_position(position_m).x
        * scene.cloud_anchor_phase_and_inverse_world_size_m.z
        + longitude_offset;
    let circle_radius = scene.world_size_m.x
        / (TAU * zonal_wavelength_m * zonal_aspect);
    let circle_radius_gradient_per_m =
        -circle_radius * zonal_aspect_gradient_per_m / zonal_aspect;
    let simplex = simplex_noise_sample_4d(
        field_seed,
        vec4<f32>(
            circle_radius * cos(longitude),
            circle_radius * sin(longitude),
            latitude_sign * position_m.y / meridional_wavelength_m + latitude_offset,
            time_coordinate + time_offset,
        ),
    );
    return TerrainNoiseSample(
        simplex.value,
        vec2<f32>(
            (-sin(longitude) * simplex.gradient.x
                + cos(longitude) * simplex.gradient.y) / (zonal_wavelength_m * zonal_aspect),
            circle_radius_gradient_per_m
                * (cos(longitude) * simplex.gradient.x
                    + sin(longitude) * simplex.gradient.y)
                + latitude_sign * simplex.gradient.z / meridional_wavelength_m,
        ),
    );
}

fn procedural_pressure_sample_filtered_bands(
    position_m: vec2<f32>,
    pixel_footprint_m: f32,
    band_count: u32,
) -> PressureFieldSample {
    let zonal_wavelengths_m = array<f32, 4>(19200.0, 3200.0, 960.0, 256.0);
    let fixed_meridional_wavelengths_m = array<f32, 4>(0.0, 3200.0, 960.0, 256.0);
    let meridional_world_fractions = array<f32, 4>(0.30, 0.0, 0.0, 0.0);
    let variances = array<f32, 4>(0.30, 0.54, 0.08, 0.02);
    let midlatitude_variance_transfers = array<f32, 4>(-0.15, 0.15, 0.0, 0.0);
    let midlatitude_zonal_aspects = array<f32, 4>(1.25, 1.4, 1.0, 1.0);
    let salts = array<vec2<u32>, 4>(
        vec2<u32>(0xade682d1u, 0x510e527fu),
        vec2<u32>(0x2b3e6c1fu, 0x9b05688cu),
        vec2<u32>(0xfb41bd6bu, 0x1f83d9abu),
        vec2<u32>(0x137e2179u, 0x5be0cd19u),
    );
    let synoptic_coupling = array<f32, 4>(1.0, 1.0, 0.20, 0.0);
    let latitude_rad = radians(surface_latitude_degrees_at(position_m));
    let midlatitude_sine = sin(2.0 * latitude_rad);
    let midlatitude = midlatitude_sine * midlatitude_sine;
    let latitude_gradient_rad_per_m = select(
        0.0,
        PI * scene.cloud_anchor_phase_and_inverse_world_size_m.w,
        abs(position_m.y) < 0.5 * scene.world_size_m.y,
    );
    let midlatitude_gradient_per_m =
        2.0 * sin(4.0 * latitude_rad) * latitude_gradient_rad_per_m;
    var sample: PressureFieldSample;
    sample.potential_m2_s = 0.0;
    sample.gradient_m_s = vec2<f32>(0.0);
    sample.synoptic_gradient_m_s = vec2<f32>(0.0);
    sample.broad_synoptic_gradient_m_s = vec2<f32>(0.0);
    sample.climate_potential_m2_s = 0.0;
    sample.synoptic_potential_m2_s = 0.0;
    sample.local_potential_m2_s = 0.0;
    sample.surface_gust_signal = 0.0;
    for (var band = 0u; band < band_count; band++) {
        let zonal_wavelength_m = zonal_wavelengths_m[band];
        let meridional_wavelength_m = select(
            fixed_meridional_wavelengths_m[band],
            meridional_world_fractions[band] * scene.world_size_m.y,
            meridional_world_fractions[band] > 0.0,
        );
        let variance_fraction = variances[band]
            + midlatitude_variance_transfers[band] * midlatitude;
        let variance_scale = sqrt(variance_fraction);
        let variance_scale_gradient_per_m = 0.5
            * midlatitude_variance_transfers[band]
            * midlatitude_gradient_per_m / variance_scale;
        let zonal_aspect = 1.0
            + (midlatitude_zonal_aspects[band] - 1.0) * midlatitude;
        let zonal_aspect_gradient_per_m =
            (midlatitude_zonal_aspects[band] - 1.0) * midlatitude_gradient_per_m;
        let noise = spatiotemporal_pressure_noise_sample(
            scene.match_seed ^ salts[band],
            position_m,
            zonal_wavelength_m,
            meridional_wavelength_m,
            scene.pressure_time_coordinate[band],
            zonal_aspect,
            zonal_aspect_gradient_per_m,
        );
        if band == 3u {
            sample.surface_gust_signal = smoothstep(0.05, 0.65, noise.value);
            continue;
        }
        let attenuation_wavelength_m = min(zonal_wavelength_m, meridional_wavelength_m);
        let attenuation = exp(
            -0.5 * pow(16.0 * pixel_footprint_m / attenuation_wavelength_m, 2.0),
        );
        let total_zonal_aspect = zonal_wavelength_m * zonal_aspect
            / meridional_wavelength_m;
        let total_zonal_aspect_gradient_per_m = zonal_wavelength_m
            * zonal_aspect_gradient_per_m / meridional_wavelength_m;
        let zonal_aspect_squared = total_zonal_aspect * total_zonal_aspect;
        let anisotropy_normalization = sqrt(
            2.0 / (1.0 + 1.0 / zonal_aspect_squared),
        );
        let anisotropy_normalization_gradient_per_m = anisotropy_normalization
            * total_zonal_aspect_gradient_per_m
            / (total_zonal_aspect * (zonal_aspect_squared + 1.0));
        let base_potential_scale_m2_s =
            attenuation * 0.495 * 9.0 * meridional_wavelength_m;
        let potential_scale_m2_s =
            base_potential_scale_m2_s * variance_scale * anisotropy_normalization;
        let potential_scale_gradient_m_s = base_potential_scale_m2_s
            * (variance_scale_gradient_per_m * anisotropy_normalization
                + variance_scale * anisotropy_normalization_gradient_per_m);
        let gradient_m_s = potential_scale_m2_s * noise.gradient
            + vec2<f32>(0.0, potential_scale_gradient_m_s * noise.value);
        let potential_m2_s = potential_scale_m2_s * noise.value;
        sample.potential_m2_s += potential_m2_s;
        sample.gradient_m_s += gradient_m_s;
        sample.synoptic_gradient_m_s += synoptic_coupling[band] * gradient_m_s;
        if band <= 1u { sample.broad_synoptic_gradient_m_s += gradient_m_s; }
        if band == 0u { sample.climate_potential_m2_s = potential_m2_s; }
        if band == 1u { sample.synoptic_potential_m2_s = potential_m2_s; }
        if band == 2u { sample.local_potential_m2_s = potential_m2_s; }
    }
    let zonal_phase = 6.0 * latitude_rad;
    let zonal_potential_scale_m2_s = 3.1 * scene.world_size_m.y / (6.0 * PI);
    let zonal_gradient_m_s = vec2<f32>(0.0, 3.1 * sin(zonal_phase));
    sample.potential_m2_s -= zonal_potential_scale_m2_s * cos(zonal_phase);
    sample.climate_potential_m2_s -= zonal_potential_scale_m2_s * cos(zonal_phase);
    sample.gradient_m_s += zonal_gradient_m_s;
    sample.synoptic_gradient_m_s += zonal_gradient_m_s;
    sample.broad_synoptic_gradient_m_s += zonal_gradient_m_s;
    return sample;
}

fn procedural_pressure_sample_filtered(
    position_m: vec2<f32>,
    pixel_footprint_m: f32,
) -> PressureFieldSample {
    return procedural_pressure_sample_filtered_bands(position_m, pixel_footprint_m, 4u);
}

fn procedural_pressure_sample_without_surface_gust(
    position_m: vec2<f32>,
) -> PressureFieldSample {
    return procedural_pressure_sample_filtered_bands(position_m, 0.0, 3u);
}

fn procedural_pressure_sample_with_surface_gust(position_m: vec2<f32>) -> PressureFieldSample {
    return procedural_pressure_sample_filtered(position_m, 0.0);
}

fn pressure_at(position_m: vec2<f32>) -> vec3<f32> {
    let pressure = procedural_pressure_sample_without_surface_gust(position_m);
    return vec3<f32>(
        1013.25 + pressure.potential_m2_s / 6000.0,
        pressure.gradient_m_s / 6000.0,
    );
}

fn wind_from_pressure_gradient(
    pressure_gradient_hpa_per_m: vec2<f32>,
    latitude_degrees: f32,
) -> vec2<f32> {
    let gradient_length = length(pressure_gradient_hpa_per_m);
    if gradient_length == 0.0 {
        return vec2<f32>(0.0);
    }
    let latitude_sine = sin(radians(latitude_degrees));
    let normalized_coriolis = clamp((abs(latitude_sine) - 0.02) / 0.23, 0.0, 1.0);
    let geostrophic_amount = normalized_coriolis * normalized_coriolis
        * (3.0 - 2.0 * normalized_coriolis);
    let hemisphere = select(1.0, -1.0, latitude_sine < 0.0);
    let along_isobar = hemisphere * rotate_left(pressure_gradient_hpa_per_m);
    let toward_low = -pressure_gradient_hpa_per_m;
    let cross_isobar_fraction = 0.12 * atmosphere_model.y;
    let flow = geostrophic_amount * along_isobar
        + (1.0 - geostrophic_amount * (1.0 - cross_isobar_fraction)) * toward_low;
    return 6000.0 * gradient_length * normalize(flow);
}

fn resolved_surface_wind_m_s_from_pressure(
    position_m: vec2<f32>,
    pressure: PressureFieldSample,
) -> vec2<f32> {
    return atmosphere_model.x * wind_from_pressure_gradient(
        pressure.gradient_m_s / 6000.0,
        surface_latitude_degrees_at(position_m),
    );
}

fn resolved_surface_wind_m_s_at(position_m: vec2<f32>) -> vec2<f32> {
    return resolved_surface_wind_m_s_from_pressure(
        position_m,
        procedural_pressure_sample_without_surface_gust(position_m),
    );
}

fn surface_wind_m_s_at(position_m: vec2<f32>) -> vec2<f32> {
    let pressure = procedural_pressure_sample_with_surface_gust(position_m);
    let resolved_surface_wind_m_s = atmosphere_model.x * wind_from_pressure_gradient(
        pressure.gradient_m_s / 6000.0,
        surface_latitude_degrees_at(position_m),
    );
    let upper_wind_m_s = upper_wind_m_s_at(position_m);
    return resolved_surface_wind_m_s
        + surface_gust_velocity_m_s_at(
            position_m,
            pressure,
            resolved_surface_wind_m_s,
            upper_wind_m_s,
        );
}

fn surface_wind_m_s() -> vec2<f32> {
    return surface_wind_m_s_at(surface_position_m);
}

fn upper_wind_m_s_from_atmosphere(
    position_m: vec2<f32>,
    atmosphere: SurfaceAtmosphereSample,
) -> vec2<f32> {
    let pressure_wind = atmosphere_model.x * wind_from_pressure_gradient(
        atmosphere.pressure.synoptic_gradient_m_s / 6000.0,
        atmosphere.climate.latitude_degrees,
    );
    return 1.15 * pressure_wind
        + 1600.0 * atmosphere_model.z
            * rotate_left(atmosphere.temperature_anomaly.gradient);
}

fn upper_wind_m_s_at(position_m: vec2<f32>) -> vec2<f32> {
    return upper_wind_m_s_from_atmosphere(
        position_m,
        surface_atmosphere_sample_at(position_m),
    );
}

fn upper_wind_m_s() -> vec2<f32> {
    return upper_wind_m_s_at(surface_position_m);
}

fn surface_latitude_degrees_at(position_m: vec2<f32>) -> f32 {
    return clamp(
        180.0 * position_m.y * scene.cloud_anchor_phase_and_inverse_world_size_m.w,
        -90.0,
        90.0,
    );
}

fn surface_latitude_degrees() -> f32 {
    return surface_latitude_degrees_at(surface_position_m);
}

fn surface_local_solar_phase_at(position_m: vec2<f32>) -> f32 {
    return fract(
        scene.world_phases_and_pixel_footprint_m.x
            + position_m.x * scene.cloud_anchor_phase_and_inverse_world_size_m.z,
    );
}

fn surface_local_solar_phase() -> f32 {
    return surface_local_solar_phase_at(surface_position_m);
}

// Mirrors the 1991–2020 latitude-band climate and probability calibration in
// `game/src/lib.rs`; both paths must derive the same climate at a world point.
const CLIMATE_ANNUAL_MEAN_C: array<f32, 10> = array<f32, 10>(
    27.4, 27.9, 25.0, 21.9, 16.9, 8.0, -0.2, -7.9, -11.7, -15.0,
);
const CLIMATE_SEASONAL_RANGE_C: array<f32, 10> = array<f32, 10>(
    2.0, 3.1, 5.8, 15.3, 18.5, 21.3, 31.0, 31.6, 25.9, 20.0,
);
const CLIMATE_DIURNAL_RANGE_C: array<f32, 10> = array<f32, 10>(
    4.4, 5.6, 5.2, 9.5, 9.6, 6.1, 6.3, 4.5, 3.2, 2.5,
);
const TEMPERATURE_ANOMALY_P99_C: array<f32, 10> = array<f32, 10>(
    9.5, 5.4, 5.5, 13.7, 14.0, 16.2, 12.4, 13.3, 6.6, 6.0,
);
const TEMPERATURE_ANOMALY_SCALE_C: array<f32, 10> = array<f32, 10>(
    19.5, 11.1, 11.3, 28.1, 28.7, 33.2, 25.4, 27.3, 13.5, 12.3,
);

fn latitude_curve_value(knots: array<f32, 10>, latitude_degrees: f32) -> f32 {
    let knot_position = clamp(abs(latitude_degrees) / 10.0, 0.0, 9.0);
    let lower = min(u32(floor(knot_position)), 8u);
    let fraction = fract(knot_position);
    let amount = fraction * fraction * (3.0 - 2.0 * fraction);
    return mix(knots[lower], knots[lower + 1u], amount);
}

fn surface_seasonal_latitude_factor_at(position_m: vec2<f32>) -> f32 {
    let latitude = surface_latitude_degrees_at(position_m);
    let magnitude = smoothstep(0.0, 45.0, abs(latitude));
    return select(magnitude, -magnitude, latitude < 0.0);
}

fn surface_seasonal_latitude_factor() -> f32 {
    return surface_seasonal_latitude_factor_at(surface_position_m);
}

fn surface_polar_fraction_at(position_m: vec2<f32>) -> f32 {
    return pow(clamp(abs(surface_latitude_degrees_at(position_m)) / 90.0, 0.0, 1.0), 1.5);
}

fn surface_annual_mean_air_temperature_c_at(position_m: vec2<f32>) -> f32 {
    return latitude_curve_value(CLIMATE_ANNUAL_MEAN_C, surface_latitude_degrees_at(position_m));
}

fn surface_annual_mean_air_temperature_c() -> f32 {
    return surface_annual_mean_air_temperature_c_at(surface_position_m);
}

fn surface_seasonal_temperature_range_c_at(position_m: vec2<f32>) -> f32 {
    return latitude_curve_value(CLIMATE_SEASONAL_RANGE_C, surface_latitude_degrees_at(position_m));
}

fn surface_seasonal_temperature_range_c() -> f32 {
    return surface_seasonal_temperature_range_c_at(surface_position_m);
}

fn surface_diurnal_temperature_range_c_at(position_m: vec2<f32>) -> f32 {
    return latitude_curve_value(CLIMATE_DIURNAL_RANGE_C, surface_latitude_degrees_at(position_m));
}

fn surface_diurnal_temperature_range_c() -> f32 {
    return surface_diurnal_temperature_range_c_at(surface_position_m);
}

fn surface_climate_sample_at(position_m: vec2<f32>) -> SurfaceClimateSample {
    let latitude_degrees = surface_latitude_degrees_at(position_m);
    let seasonal_magnitude = smoothstep(0.0, 45.0, abs(latitude_degrees));
    return SurfaceClimateSample(
        latitude_degrees,
        select(seasonal_magnitude, -seasonal_magnitude, latitude_degrees < 0.0),
        latitude_curve_value(CLIMATE_ANNUAL_MEAN_C, latitude_degrees),
        latitude_curve_value(CLIMATE_SEASONAL_RANGE_C, latitude_degrees),
        latitude_curve_value(CLIMATE_DIURNAL_RANGE_C, latitude_degrees),
        surface_local_solar_phase_at(position_m),
    );
}

fn surface_air_temperature_anomaly_c_at(position_m: vec2<f32>) -> f32 {
    return surface_air_temperature_anomaly_c_from_climate(
        position_m,
        surface_climate_sample_at(position_m),
    );
}

fn surface_air_temperature_anomaly_c_from_climate(
    position_m: vec2<f32>,
    climate: SurfaceClimateSample,
) -> f32 {
    let scale_c = latitude_curve_value(
        TEMPERATURE_ANOMALY_SCALE_C,
        climate.latitude_degrees,
    );
    return scale_c * 0.85 * spatiotemporal_fractal_value(
        vec2<u32>(0x38d01377u, 0x452821e6u),
        position_m,
        65536.0,
        scene.scalar_field_time_coordinates.x,
        4u,
        0.5,
    );
}

fn surface_air_temperature_anomaly_sample_at(position_m: vec2<f32>) -> TerrainNoiseSample {
    return surface_air_temperature_anomaly_sample_from_climate(
        position_m,
        surface_climate_sample_at(position_m),
    );
}

fn surface_air_temperature_anomaly_sample_from_climate(
    position_m: vec2<f32>,
    climate: SurfaceClimateSample,
) -> TerrainNoiseSample {
    let scale_c = latitude_curve_value(
        TEMPERATURE_ANOMALY_SCALE_C,
        climate.latitude_degrees,
    );
    let noise = spatiotemporal_fractal_sample(
        vec2<u32>(0x38d01377u, 0x452821e6u),
        position_m,
        65536.0,
        scene.scalar_field_time_coordinates.x,
        4u,
        0.5,
    );
    return TerrainNoiseSample(
        scale_c * 0.85 * noise.value,
        scale_c * 0.85 * noise.gradient,
    );
}

fn surface_atmosphere_sample_at(position_m: vec2<f32>) -> SurfaceAtmosphereSample {
    let climate = surface_climate_sample_at(position_m);
    var pressure = PressureFieldSample(
        0.0,
        vec2<f32>(0.0),
        vec2<f32>(0.0),
        vec2<f32>(0.0),
        0.0,
        0.0,
        0.0,
        0.0,
    );
    if environment_feature_visibility().x > 0.0 {
        pressure = procedural_pressure_sample_without_surface_gust(position_m);
    }
    var temperature_anomaly = TerrainNoiseSample(0.0, vec2<f32>(0.0));
    if environment_feature_visibility().y > 0.0 {
        temperature_anomaly = surface_air_temperature_anomaly_sample_from_climate(
            position_m,
            climate,
        );
    }
    var water_vapor_g_kg = 0.0;
    if environment_feature_visibility().z > 0.0 {
        water_vapor_g_kg = surface_water_vapor_g_kg_from_climate(position_m, climate);
    }
    return SurfaceAtmosphereSample(
        pressure,
        temperature_anomaly,
        water_vapor_g_kg,
        climate,
    );
}

fn upper_air_temperature_500_hpa_c_at(position_m: vec2<f32>) -> f32 {
    return upper_air_temperature_500_hpa_c_from_anomaly(
        position_m,
        surface_air_temperature_anomaly_c_at(position_m),
    );
}

fn upper_air_temperature_500_hpa_c_from_anomaly(
    position_m: vec2<f32>,
    surface_anomaly_c: f32,
) -> f32 {
    return upper_air_temperature_500_hpa_c_from_atmosphere(
        position_m,
        surface_anomaly_c,
        surface_climate_sample_at(position_m),
    );
}

fn upper_air_temperature_500_hpa_c_from_atmosphere(
    position_m: vec2<f32>,
    surface_anomaly_c: f32,
    climate: SurfaceClimateSample,
) -> f32 {
    let climate_temperature_c = climate.annual_mean_air_temperature_c
        - 35.5
        + 0.25 * climate.seasonal_temperature_range_c
            * climate.seasonal_latitude_factor
            * scene.seasonal_phase_factors.x;
    let independent_noise = 0.75 * spatiotemporal_fractal_value(
        vec2<u32>(0x9e0c6c89u, 0xbe5466cfu),
        position_m,
        65536.0,
        scene.scalar_field_time_coordinates.y,
        4u,
        0.5,
    );
    return climate_temperature_c
        + 0.45 * surface_anomaly_c
        + 19.0 * independent_noise;
}

fn water_vapor_saturation_capacity_g_kg(air_temperature_c: f32) -> f32 {
    return 3.5 * exp(0.055 * (air_temperature_c + 5.0));
}

fn surface_water_vapor_g_kg_at(position_m: vec2<f32>) -> f32 {
    return surface_water_vapor_g_kg_from_climate(
        position_m,
        surface_climate_sample_at(position_m),
    );
}

fn surface_water_vapor_g_kg_from_climate(
    position_m: vec2<f32>,
    climate: SurfaceClimateSample,
) -> f32 {
    let climate_temperature_c = climate.annual_mean_air_temperature_c
        + 0.5 * climate.seasonal_temperature_range_c
            * climate.seasonal_latitude_factor
            * scene.seasonal_phase_factors.x;
    let capacity_g_kg = water_vapor_saturation_capacity_g_kg(climate_temperature_c);
    let noise = 0.75 * spatiotemporal_fractal_value(
        vec2<u32>(0xec4e6c89u, 0x082efa98u),
        position_m,
        8192.0,
        scene.scalar_field_time_coordinates.z,
        5u,
        0.5,
    );
    return clamp(capacity_g_kg * (0.72 + 0.65 * noise), 0.5, 25.0);
}

fn surface_water_vapor_g_kg() -> f32 {
    return surface_water_vapor_g_kg_at(surface_position_m);
}

fn surface_current_air_temperature_c_at(position_m: vec2<f32>) -> f32 {
    return surface_current_air_temperature_c_from_anomaly(
        position_m,
        surface_air_temperature_anomaly_c_at(position_m),
    );
}

fn surface_current_air_temperature_c_from_anomaly(
    position_m: vec2<f32>,
    surface_anomaly_c: f32,
) -> f32 {
    return surface_current_air_temperature_c_from_atmosphere(
        surface_anomaly_c,
        surface_climate_sample_at(position_m),
    );
}

fn surface_current_air_temperature_c_from_atmosphere(
    surface_anomaly_c: f32,
    climate: SurfaceClimateSample,
) -> f32 {
    let diurnal_phase = TAU * (climate.local_solar_phase - 0.625);
    return climate.annual_mean_air_temperature_c
        + 0.5 * climate.seasonal_temperature_range_c
            * climate.seasonal_latitude_factor
            * scene.seasonal_phase_factors.x
        + 0.5 * climate.diurnal_temperature_range_c * cos(diurnal_phase)
        + surface_anomaly_c;
}

fn surface_current_air_temperature_c() -> f32 {
    return surface_current_air_temperature_c_at(surface_position_m);
}

fn ground_temperature_c_from_anomaly(
    elevation_m: f32,
    temperature_anomaly_c: f32,
) -> f32 {
    return ground_temperature_c_from_fields(
        elevation_m,
        temperature_anomaly_c,
        surface_climate_sample_at(surface_position_m),
    );
}

fn ground_temperature_c_from_fields(
    elevation_m: f32,
    temperature_anomaly_c: f32,
    climate: SurfaceClimateSample,
) -> f32 {
    let diurnal_phase = TAU * (climate.local_solar_phase - 0.625);
    let sea_level_temperature = climate.annual_mean_air_temperature_c
        + 0.42 * climate.seasonal_temperature_range_c * climate.seasonal_latitude_factor
            * scene.seasonal_phase_factors.y
        + 0.15 * climate.diurnal_temperature_range_c * cos(diurnal_phase);
    return sea_level_temperature
        + 0.35 * temperature_anomaly_c
        - 0.0065 * max(elevation_m, 0.0);
}

fn ground_temperature_c(elevation_m: f32) -> f32 {
    return ground_temperature_c_from_anomaly(
        elevation_m,
        surface_air_temperature_anomaly_c_at(surface_position_m),
    );
}

fn water_temperature_c() -> f32 {
    return water_temperature_c_from_climate(
        surface_climate_sample_at(surface_position_m),
    );
}

fn water_temperature_c_from_climate(climate: SurfaceClimateSample) -> f32 {
    let normalized_latitude = clamp(abs(climate.latitude_degrees) / 90.0, 0.0, 1.0);
    let polar_fraction = pow(normalized_latitude, 1.5);
    let annual_temperature_c = 27.0 - 45.0 * polar_fraction;
    let seasonal_temperature_range_c = 3.0 + 41.0 * pow(polar_fraction, 0.9);
    let maritime_moderation_c = 65.0 * (
        pow(normalized_latitude, 1.5) - pow(normalized_latitude, 2.6)
    );
    return annual_temperature_c
        + maritime_moderation_c
        + 0.28 * seasonal_temperature_range_c * climate.seasonal_latitude_factor
            * scene.seasonal_phase_factors.w;
}

fn climate_moisture_support_from_water_vapor(
    air_temperature_c: f32,
    water_vapor_g_kg: f32,
) -> f32 {
    let relative_humidity = clamp(
        water_vapor_g_kg / water_vapor_saturation_capacity_g_kg(air_temperature_c),
        0.0,
        1.5,
    );
    return smoothstep(0.45, 0.95, relative_humidity);
}

fn climate_moisture_support(air_temperature_c: f32) -> f32 {
    return climate_moisture_support_from_water_vapor(
        air_temperature_c,
        surface_water_vapor_g_kg(),
    );
}

fn seasonal_snow_cover_from_fields(
    elevation_m: f32,
    temperature_anomaly_c: f32,
    water_vapor_g_kg: f32,
    climate: SurfaceClimateSample,
) -> f32 {
    let ground_temperature = ground_temperature_c_from_fields(
        elevation_m,
        temperature_anomaly_c,
        climate,
    );
    let air_temperature = surface_current_air_temperature_c_from_atmosphere(
        temperature_anomaly_c,
        climate,
    )
        - 0.0065 * max(elevation_m, 0.0);
    let weather_standard_deviation_c = latitude_curve_value(
        TEMPERATURE_ANOMALY_P99_C,
        climate.latitude_degrees,
    ) / 2.326;
    let freezing_probability = 1.0 / (
        1.0 + exp(1.702 * ground_temperature / weather_standard_deviation_c)
    );
    let snow_retention = 1.0 - pow(1.0 - freezing_probability, 3.0);
    let persistent_snow = snow_retention * (
        0.35
            + 0.65 * climate_moisture_support_from_water_vapor(
                air_temperature,
                water_vapor_g_kg,
            )
    );
    let snowfall_cold = 1.0 - smoothstep(-1.0, 2.0, air_temperature);
    let fresh_snow = surface_cloud_weather.rain * snowfall_cold;
    let snow_before_melt = persistent_snow + fresh_snow * (1.0 - persistent_snow);
    let melt = smoothstep(0.0, 6.0, air_temperature);
    return snow_before_melt * (1.0 - 0.75 * melt);
}

fn seasonal_snow_cover(elevation_m: f32) -> f32 {
    let climate = surface_climate_sample_at(surface_position_m);
    return seasonal_snow_cover_from_fields(
        elevation_m,
        surface_air_temperature_anomaly_c_from_climate(surface_position_m, climate),
        surface_water_vapor_g_kg_from_climate(surface_position_m, climate),
        climate,
    );
}

fn local_snow_cover_from_atmosphere(
    elevation_m: f32,
    atmosphere: SurfaceAtmosphereSample,
) -> f32 {
    return seasonal_snow_cover_from_fields(
        elevation_m,
        atmosphere.temperature_anomaly.value,
        atmosphere.water_vapor_g_kg,
        atmosphere.climate,
    );
}

fn local_snow_cover(elevation_m: f32) -> f32 {
    return seasonal_snow_cover(elevation_m);
}

fn autumn_vegetation_amount_from_anomaly(
    elevation_m: f32,
    temperature_anomaly_c: f32,
    climate: SurfaceClimateSample,
) -> f32 {
    let annual_temperature = climate.annual_mean_air_temperature_c
        - 0.0065 * max(elevation_m, 0.0);
    let temperate = smoothstep(3.0, 10.0, annual_temperature)
        * (1.0 - smoothstep(18.0, 25.0, annual_temperature));
    let cooling = smoothstep(
        0.10,
        0.80,
        -climate.seasonal_latitude_factor * scene.seasonal_phase_factors.z,
    );
    let temperature = ground_temperature_c_from_fields(
        elevation_m,
        temperature_anomaly_c,
        climate,
    );
    let mild = smoothstep(-2.0, 8.0, temperature)
        * (1.0 - smoothstep(14.0, 20.0, temperature));
    return temperate * cooling * mild;
}

fn autumn_vegetation_amount(elevation_m: f32) -> f32 {
    let climate = surface_climate_sample_at(surface_position_m);
    return autumn_vegetation_amount_from_anomaly(
        elevation_m,
        surface_air_temperature_anomaly_c_from_climate(surface_position_m, climate),
        climate,
    );
}

fn vegetation_dormancy_from_anomaly(
    elevation_m: f32,
    temperature_anomaly_c: f32,
    climate: SurfaceClimateSample,
) -> f32 {
    return 1.0 - smoothstep(
        -2.0,
        8.0,
        ground_temperature_c_from_fields(elevation_m, temperature_anomaly_c, climate),
    );
}

fn vegetation_dormancy(elevation_m: f32) -> f32 {
    let climate = surface_climate_sample_at(surface_position_m);
    return vegetation_dormancy_from_anomaly(
        elevation_m,
        surface_air_temperature_anomaly_c_from_climate(surface_position_m, climate),
        climate,
    );
}

fn tide_height_m() -> f32 {
    let solar_hour_angle = TAU * (surface_local_solar_phase() - 0.5);
    let solar_longitude_phase = scene.world_phases_and_pixel_footprint_m.y - 0.25;
    let lunar_phase = scene.world_phases_and_pixel_footprint_m.z - solar_longitude_phase;
    let lunar_hour_angle = solar_hour_angle + TAU * lunar_phase;
    let phase_offset = TAU * surface_tidal_phase_offset;
    let lunar_tide = cos(2.0 * (lunar_hour_angle + phase_offset));
    let solar_tide = 0.46 * cos(2.0 * (solar_hour_angle + phase_offset));
    return climate_visibility.y
        * 0.5 * surface_tidal_range_m * (lunar_tide + solar_tide) / 1.46;
}

fn tide_reference_range_m() -> f32 {
    return surface_tidal_range_m;
}

struct SurfaceTideSample {
    response: TerrainNoiseSample,
    lag: TerrainNoiseSample,
}

var<private> surface_tide_sample: SurfaceTideSample;

fn surface_tide_sample_from_circle(
    position_m: vec2<f32>,
    circle_position: vec4<f32>,
) -> SurfaceTideSample {
    return SurfaceTideSample(
        spatial_weather_fractal_sample_from_circle(
            vec2<u32>(0xf3bcc909u, 0x6a09e667u),
            position_m,
            circle_position,
            131072.0,
            3u,
            3u,
        ),
        spatial_weather_fractal_sample_from_circle(
            vec2<u32>(0x84caa73bu, 0xbb67ae85u),
            position_m,
            circle_position,
            131072.0,
            3u,
            3u,
        ),
    );
}

fn surface_tide_sample_at(position_m: vec2<f32>) -> SurfaceTideSample {
    return surface_tide_sample_from_circle(
        position_m,
        periodic_noise_circle_position(position_m),
    );
}

fn initialize_surface_tide_from_circle(circle_position: vec4<f32>) {
    surface_tide_sample = surface_tide_sample_from_circle(
        surface_position_m,
        circle_position,
    );
    surface_tidal_range_m = 1.25 + 0.75 * surface_tide_sample.response.value;
    surface_tidal_phase_offset = 0.08 * surface_tide_sample.lag.value;
}

fn initialize_surface_tide() {
    initialize_surface_tide_from_circle(
        periodic_noise_circle_position(surface_position_m),
    );
}

fn initialize_surface_tide_value() {
    let circle_position = periodic_noise_circle_position(surface_position_m);
    surface_tidal_range_m = 1.25 + 0.75 * spatial_weather_fractal_from_circle(
        vec2<u32>(0xf3bcc909u, 0x6a09e667u),
        surface_position_m,
        circle_position,
        131072.0,
        3u,
        3u,
    );
    surface_tidal_phase_offset = 0.08 * spatial_weather_fractal_from_circle(
        vec2<u32>(0x84caa73bu, 0xbb67ae85u),
        surface_position_m,
        circle_position,
        131072.0,
        3u,
        3u,
    );
}

fn cloud_weather_from_atmosphere(
    position_m: vec2<f32>,
    atmosphere: SurfaceAtmosphereSample,
    include_low_wind: bool,
    include_upper_wind: bool,
) -> CloudWeatherSample {
    if environment_feature_visibility().w <= 0.0 {
        return CloudWeatherSample(
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            vec2<f32>(0.0),
            vec2<f32>(0.0),
        );
    }
    let pressure_sample = atmosphere.pressure;
    let pressure = vec3<f32>(
        1013.25 + pressure_sample.potential_m2_s / 6000.0,
        pressure_sample.gradient_m_s / 6000.0,
    );
    let surface_anomaly = atmosphere.temperature_anomaly;
    let air_temperature_c = surface_current_air_temperature_c_from_atmosphere(
        surface_anomaly.value,
        atmosphere.climate,
    );
    let saturation_capacity_g_kg = water_vapor_saturation_capacity_g_kg(air_temperature_c);
    let relative_humidity = clamp(
        atmosphere.water_vapor_g_kg / saturation_capacity_g_kg,
        0.0,
        1.5,
    );
    let upper_air_temperature_c = upper_air_temperature_500_hpa_c_from_atmosphere(
        position_m,
        surface_anomaly.value,
        atmosphere.climate,
    );
    let temperature_gradient = surface_anomaly.gradient;
    let instability = smoothstep(
        28.0,
        48.0,
        air_temperature_c - upper_air_temperature_c,
    );
    let pressure_lift = 1.0 - smoothstep(990.0, 1012.0, pressure.x);
    let gradient_lift = smoothstep(0.0002, 0.0015, length(pressure.yz));
    let frontal_lift = smoothstep(
        0.00005,
        0.0005,
        length(temperature_gradient),
    );
    let lift = clamp(
        0.25 * pressure_lift + 0.55 * gradient_lift + 0.20 * frontal_lift,
        0.0,
        1.0,
    );
    let condensation = smoothstep(0.45, 0.95, relative_humidity);
    let signal = 0.68 * condensation
        + 0.22 * lift
        + 0.10 * instability * condensation;
    let low_coverage = smoothstep(0.50, 1.01, signal);
    let upper_coverage = smoothstep(
        0.68,
        0.90,
        0.55 * condensation + 0.30 * lift + 0.15 * frontal_lift,
    );
    let rain_signal = smoothstep(0.65, 0.75, low_coverage)
        * low_coverage
        * (0.45 + 0.20 * instability + 0.45 * lift);
    let rain = smoothstep(0.55, 0.90, rain_signal);

    var weather: CloudWeatherSample;
    weather.relative_humidity = relative_humidity;
    weather.instability = instability;
    weather.lift = lift;
    weather.low_coverage = low_coverage;
    weather.upper_coverage = upper_coverage;
    weather.rain = rain;
    weather.stratiform = clamp(
        smoothstep(0.78, 1.05, relative_humidity)
            * (1.0 - instability)
            * smoothstep(0.18, 0.75, lift),
        0.0,
        1.0,
    );
    weather.convective = clamp(
        instability
            * smoothstep(0.35, 0.85, lift)
            * mix(0.45, 1.0, rain),
        0.0,
        1.0,
    );
    let latitude_degrees = atmosphere.climate.latitude_degrees;
    weather.low_advection_wind_m_s = vec2<f32>(0.0);
    if include_low_wind {
        // Broad flow keeps finite cloud-anchor maps coherent instead of folding them
        // around gust-scale pressure features.
        weather.low_advection_wind_m_s = atmosphere_model.x * wind_from_pressure_gradient(
            pressure_sample.broad_synoptic_gradient_m_s / 6000.0,
            latitude_degrees,
        );
    }
    weather.upper_advection_wind_m_s = vec2<f32>(0.0);
    if include_upper_wind {
        weather.upper_advection_wind_m_s = 1.15 * atmosphere_model.x
            * wind_from_pressure_gradient(
            pressure_sample.synoptic_gradient_m_s / 6000.0,
            latitude_degrees,
        )
            + 1600.0 * atmosphere_model.z * rotate_left(surface_anomaly.gradient);
    }
    return weather;
}

fn cloud_weather_field_at_with_winds(
    position_m: vec2<f32>,
    include_low_wind: bool,
    include_upper_wind: bool,
) -> CloudWeatherFieldSample {
    let atmosphere = surface_atmosphere_sample_at(position_m);
    return CloudWeatherFieldSample(
        atmosphere,
        cloud_weather_from_atmosphere(
            position_m,
            atmosphere,
            include_low_wind,
            include_upper_wind,
        ),
    );
}

fn cloud_weather_at_with_winds(
    position_m: vec2<f32>,
    include_low_wind: bool,
    include_upper_wind: bool,
) -> CloudWeatherSample {
    return cloud_weather_field_at_with_winds(
        position_m,
        include_low_wind,
        include_upper_wind,
    ).weather;
}

fn cloud_weather_conditions_at(position_m: vec2<f32>) -> CloudWeatherSample {
    return cloud_weather_at_with_winds(position_m, false, false);
}

fn low_cloud_weather_at(position_m: vec2<f32>) -> CloudWeatherSample {
    return cloud_weather_at_with_winds(position_m, true, false);
}

fn low_cloud_weather_field_at(position_m: vec2<f32>) -> CloudWeatherFieldSample {
    return cloud_weather_field_at_with_winds(position_m, true, false);
}

fn upper_cloud_weather_at(position_m: vec2<f32>) -> CloudWeatherSample {
    return cloud_weather_at_with_winds(position_m, false, true);
}

fn surface_gust_velocity_m_s_at(
    position_m: vec2<f32>,
    pressure: PressureFieldSample,
    resolved_surface_wind_m_s: vec2<f32>,
    upper_wind_m_s: vec2<f32>,
) -> vec2<f32> {
    let weather = cloud_weather_conditions_at(position_m);
    let mechanical_gust_speed_m_s = 0.27
        * (1.0 + 0.35 * weather.instability)
        * length(resolved_surface_wind_m_s);
    let convective_activity = weather.instability
        * smoothstep(0.65, 0.95, weather.relative_humidity)
        * smoothstep(0.35, 0.80, weather.lift);
    let convective_gust_speed_m_s = 0.60
        * convective_activity
        * max(length(upper_wind_m_s) - length(resolved_surface_wind_m_s), 0.0);
    var gust_velocity_m_s = vec2<f32>(0.0);
    if length(resolved_surface_wind_m_s) > 0.0 {
        gust_velocity_m_s += mechanical_gust_speed_m_s
            * normalize(resolved_surface_wind_m_s);
    }
    if length(upper_wind_m_s) > 0.0 {
        gust_velocity_m_s += convective_gust_speed_m_s * normalize(upper_wind_m_s);
    }
    return pressure.surface_gust_signal * gust_velocity_m_s;
}

fn relative_humidity() -> f32 {
    return surface_cloud_weather.relative_humidity;
}

fn low_cloud_coverage() -> f32 {
    return surface_cloud_weather.low_coverage;
}

fn upper_cloud_coverage() -> f32 {
    return surface_cloud_weather.upper_coverage;
}

fn rain_intensity() -> f32 {
    return surface_cloud_weather.rain;
}

fn wind_shear_amount() -> f32 {
    return smoothstep(4.0, 15.0, length(upper_wind_m_s() - surface_wind_m_s()));
}

fn visible_low_cloud_coverage(weather: CloudWeatherSample) -> f32 {
    let fair_weather_lift = smoothstep(0.12, 0.68, weather.instability)
        * (1.0 - smoothstep(0.08, 0.42, weather.rain))
        * (1.0 - 0.90 * weather.stratiform);
    return weather.low_coverage * mix(0.06, 1.0, fair_weather_lift);
}

fn production_tuning() -> SceneTuning {
    return SceneTuning(
        vec4<f32>(
            CLOUD_DETAIL_CHARACTER,
            CLOUD_DETAIL_BLOBINESS,
            CLOUD_DETAIL_CONTRAST,
            CLOUD_DETAIL_EDGE_SOFTNESS,
        ),
        vec4<f32>(
            CLOUD_MAX_WISP_COVERAGE,
            0.0,
            0.0,
            0.0,
        ),
        vec4<f32>(
            CLOUD_OPTICAL_DEPTH,
            CLOUD_OPACITY,
            CLOUD_DARKNESS,
            CLOUD_RELIEF,
        ),
        vec4<f32>(
            TERRAIN_HORIZONTAL_SCALE,
            TERRAIN_ELEVATION_SCALE,
            TERRAIN_CLIFF_SLOPE,
            0.0,
        ),
        vec4<f32>(
            WATER_ATTENUATION_LENGTH_M,
            WATER_BOTTOM_VISIBILITY,
            0.0,
            0.0,
        ),
        vec4<f32>(
            LANDFORM_RELIEF_STRENGTH,
            LANDFORM_SHADOW_STRENGTH,
            LANDFORM_SHADOW_REACH_M,
            0.0,
        ),
        vec4<f32>(0.18, 0.14, 0.0, 0.0),
        vec4<f32>(1.0, 1.0, 0.85, CLOUD_OPACITY),
        vec4<f32>(2.5, 0.20, 1.0, 1.0),
        vec4<f32>(0.18, 0.32, 1.0, 1.0),
        vec4<f32>(192.0, 1.0, 1.0, 1.0),
        vec4<f32>(1.0),
        vec4<f32>(1.0),
        vec4<f32>(1.0),
    );
}
