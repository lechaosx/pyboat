struct RegionalCanopyConditions {
    sky_fraction: f32,
    mean_optical_thickness: f32,
}

fn regional_canopy_conditions_from_drivers(
    drivers: RegionalCloudDrivers,
) -> RegionalCanopyConditions {
    if environment_feature_visibility().w <= 0.0 {
        return RegionalCanopyConditions(0.0, 0.0);
    }
    let condensation = drivers.condensation;
    let pressure_lift = drivers.pressure_lift;
    let instability = drivers.instability;
    let cloud_potential = 0.55 * condensation
        + 0.25 * pressure_lift
        + 0.20 * condensation * instability;
    let sky_fraction = smoothstep(0.30, 0.88, cloud_potential);
    let storm_depth = smoothstep(
        0.15,
        0.45,
        condensation * pressure_lift * instability,
    );
    return RegionalCanopyConditions(
        sky_fraction,
        0.18
            + 0.50 * condensation
            + 0.70 * pressure_lift
            + 0.45 * condensation * instability
            + 3.6 * storm_depth,
    );
}

fn regional_canopy_conditions(weather: CloudWeatherSample) -> RegionalCanopyConditions {
    return regional_canopy_conditions_from_drivers(weather.regional);
}

// Every cloud above the camera is one field, seen only through its shadows.
// Each cloud type sits at its own height, takes its cover from the weather
// under it, and rides its own wind. Cloud shadow grids hold the field's
// transmittance along each light from a plane above every shadow receiver, so
// one tap per light reads every cloud type.
const CUMULUS_LOWEST_HEIGHT_M: f32 = 1000.0;
const CUMULUS_HIGHEST_HEIGHT_M: f32 = 3000.0;
// A cell scale that followed the weather would warp the noise domain far from
// the origin, so each type's is fixed.
const CUMULUS_WAVELENGTH_M: f32 = 1200.0;
const DECK_HEIGHT_M: f32 = WEATHER_CANOPY_HEIGHT_M;
const DECK_WAVELENGTH_M: f32 = 4000.0;
const CIRRUS_HEIGHT_M: f32 = 9000.0;
const CIRRUS_WAVELENGTH_M: f32 = 6000.0;
// Cirrus rides the upper wind faster than the deck below it.
const CIRRUS_TRANSPORT_RATIO: f32 = 1.5;
// Spread of one spatiotemporal octave's value; with a fractal's spread ratio
// this sets the logistic scale that makes its latent rank about uniform.
const CLOUD_OCTAVE_RANK_SCALE: f32 = 0.1786;
// Turbulent eddies turn over in a time that grows with their size to the 2/3
// power, so each halving of the wavelength evolves 2^(2/3) times faster.
const CLOUD_OCTAVE_TIME_RATIO: f32 = 1.5874;
// Root-mean-square slope of one spatiotemporal octave across the ground, per
// unit of its domain: 1.456 along each domain axis over 400 000 uniform points.
const CLOUD_OCTAVE_SLOPE: f32 = 2.06;

struct CloudLatent {
    // Normalized value, its gradient per metre, its spread relative to one
    // octave's, and its root-mean-square slope per metre.
    value: f32,
    gradient: vec2<f32>,
    spread: f32,
    typical_slope: f32,
};

// Octaves fade from full weight at twice `shortest_m` to none at it. The loop
// stops at `loop_shortest_m`, which forced evaluation lowers to the shortest
// wavelength any weather can keep.
fn band_limited_cloud_latent(
    field: u32,
    position_m: vec2<f32>,
    initial_wavelength_m: f32,
    initial_time_coordinate: f32,
    persistence: f32,
    shortest_m: f32,
    loop_shortest_m: f32,
) -> CloudLatent {
    var value = 0.0;
    var gradient = vec2<f32>(0.0);
    var amplitude = 1.0;
    var amplitude_sum = 0.0;
    var squared_sum = 0.0;
    var squared_slope_sum = 0.0;
    var wavelength_m = initial_wavelength_m;
    var time_coordinate = initial_time_coordinate;
    for (var octave = 0u; octave < 9u; octave++) {
        if wavelength_m <= loop_shortest_m {
            break;
        }
        let octave_amplitude = amplitude * clamp(log2(wavelength_m / shortest_m), 0.0, 1.0);
        let sample = spatiotemporal_noise_sample(
            noise_key(field, octave),
            position_m,
            wavelength_m,
            time_coordinate,
        );
        value += octave_amplitude * sample.value;
        gradient += octave_amplitude * sample.gradient;
        amplitude_sum += octave_amplitude;
        squared_sum += octave_amplitude * octave_amplitude;
        squared_slope_sum += octave_amplitude * octave_amplitude / (wavelength_m * wavelength_m);
        amplitude *= persistence;
        wavelength_m *= 0.5;
        time_coordinate *= CLOUD_OCTAVE_TIME_RATIO;
    }
    let normalization = max(amplitude_sum, 1e-6);
    return CloudLatent(
        value / normalization,
        gradient / normalization,
        sqrt(squared_sum) / normalization,
        CLOUD_OCTAVE_SLOPE * sqrt(squared_slope_sum) / normalization,
    );
}

fn cloud_latent_rank_scale(latent: CloudLatent) -> f32 {
    return CLOUD_OCTAVE_RANK_SCALE * max(latent.spread, 1e-6);
}

fn cloud_latent_rank(latent: CloudLatent) -> f32 {
    return 1.0 / (1.0 + exp(-latent.value / cloud_latent_rank_scale(latent)));
}

// The rank's gradient, at least as steep as the latent's typical slope. Octaves
// near the disc's width bend within it, so a shallower local gradient would
// misjudge the rim's distance.
fn cloud_latent_rank_gradient(latent: CloudLatent, rank: f32) -> vec2<f32> {
    let local_slope = length(latent.gradient);
    let slope = max(local_slope, latent.typical_slope);
    let direction = select(vec2<f32>(1.0, 0.0), latent.gradient / local_slope, local_slope > 0.0);
    return rank * (1.0 - rank) / cloud_latent_rank_scale(latent) * slope * direction;
}

struct CloudEdge {
    // The rim's distance from the disc's centre on the cloud side, in half
    // widths of the disc, and the edge value halfway into the covered part.
    rim: f32,
    inner_value: f32,
};

// Share of the light's disc that a cloud covers when its rim lies `rim` half
// widths from the disc's centre on the cloud side: the disc's chord.
fn disc_cover(rim: f32) -> f32 {
    let chord = clamp(rim, -1.0, 1.0);
    return 0.5 + (chord * sqrt(1.0 - chord * chord) + asin(chord)) / PI;
}

// The light's disc across a cloud rim, which lies where `value` falls to
// `rim_value`. Within the disc the field is nearly linear along its
// gradient, so the covered share follows the disc's chord at the rim's
// distance. The disc spans `diameter_m` across the light where the ray meets
// the cloud and stretches along it by 1 / sin(elevation) there.
fn disc_cloud_edge(
    value: f32,
    rim_value: f32,
    gradient: vec2<f32>,
    ray: CloudRay,
    light_direction: vec2<f32>,
    diameter_m: f32,
) -> CloudEdge {
    let slope = max(length(gradient), 1e-9);
    let along = dot(gradient, light_direction) / slope;
    let half_width_m = max(
        0.5 * diameter_m * sqrt(
            1.0 + along * along * (1.0 / (ray.sine_elevation * ray.sine_elevation) - 1.0),
        ),
        1e-3,
    );
    let distance_m = (value - rim_value) / slope;
    let inner_distance_m = max(distance_m, 0.5 * (distance_m + half_width_m));
    return CloudEdge(distance_m / half_width_m, rim_value + max(inner_distance_m, 0.0) * slope);
}

// A cloud shadow texel: the cumulus rim distance in disc half widths, which
// varies smoothly where the transmittance turns sharply, so the scene applies
// the disc's chord after reconstruction; the cumulus transmittance halfway into
// the covered part; and the deck's and cirrus's transmittance.
struct CloudShadowTexel {
    cumulus_rim: f32,
    cumulus_inner_transmittance: f32,
    other_transmittance: f32,
};

// Rim distances only near the disc matter; clamped ones interpolate across a
// texel wider than the disc as a rim about that wide.
const CLOUD_SHADOW_RIM_LIMIT: f32 = 2.0;

fn cloud_shadow_texel_field(texel: CloudShadowTexel) -> vec4<f32> {
    return vec4<f32>(
        clamp(texel.cumulus_rim, -CLOUD_SHADOW_RIM_LIMIT, CLOUD_SHADOW_RIM_LIMIT),
        texel.cumulus_inner_transmittance,
        texel.other_transmittance,
        0.0,
    );
}

fn cloud_shadow_field_transmittance(field: vec4<f32>) -> f32 {
    return (1.0 - disc_cover(field.x) * (1.0 - field.y)) * field.z;
}

struct CumulusShadow {
    rim: f32,
    inner_transmittance: f32,
};

fn cloud_shadow_extinction(ray: CloudRay) -> f32 {
    return tuning.cloud_shadows.x * tuning.cloud_shadows.y / ray.sine_elevation;
}

// Share of the sky that convection covers with cumulus: unstable air over
// land in the afternoon, under whatever sunlight the deck lets through, or
// over water warmer than the air; heavy rain belongs to the deck. Convective
// cumulus seldom cover more than about half of the sky.
fn cumulus_cover(weather: CloudWeatherSample) -> f32 {
    let convection = smoothstep(0.12, 0.68, weather.instability)
        * (1.0 - 0.90 * weather.stratiform);
    let canopy = regional_canopy_conditions(weather);
    let canopy_sunlight = mix(
        1.0,
        exp(-0.5 * canopy.mean_optical_thickness),
        canopy.sky_fraction,
    );
    let surface_heating = mix(
        weather.sea_warming,
        weather.land_heating * canopy_sunlight,
        weather.land_fraction,
    );
    let storm_clearing = 1.0 - smoothstep(0.30, 0.70, weather.rain);
    return min(
        0.7 * weather.low_coverage * convection * surface_heating * storm_clearing,
        0.5,
    );
}

fn cumulus_depth_amount(weather: CloudWeatherSample) -> f32 {
    return smoothstep(0.12, 0.68, weather.instability)
        * smoothstep(0.75, 1.0, weather.relative_humidity);
}

// Mid-cloud height: the lifting condensation level, about 2.5 km per unit of
// humidity deficit, plus half a depth that grows with moist instability.
fn cumulus_height_m(weather: CloudWeatherSample) -> f32 {
    let base_m = 2500.0 * max(1.0 - weather.relative_humidity, 0.0);
    let depth_m = mix(500.0, 2500.0, cumulus_depth_amount(weather));
    return clamp(
        base_m + 0.5 * depth_m,
        CUMULUS_LOWEST_HEIGHT_M,
        CUMULUS_HIGHEST_HEIGHT_M,
    );
}

// Threshold on mix(rank, |2 rank - 1|, character) with `cover` of uniform ranks
// above it; both terms are piecewise linear in the rank.
fn cumulus_shape_threshold(cover: f32, character: f32) -> f32 {
    let upper_ranks_only = 1.0 - cover * (1.0 + character);
    if character <= 1.0 / 3.0 || upper_ranks_only >= character {
        return upper_ranks_only;
    }
    return (
        character * character + 4.0 * character - 1.0
            - cover * (1.0 + character) * (3.0 * character - 1.0)
    ) / (4.0 * character);
}

// What the weather under a cloud sets for the cumulus there: cover, the share
// of moist instability that deepens them, their mid-cloud height, and their
// convective character.
fn cumulus_drivers(weather: CloudWeatherSample) -> vec4<f32> {
    return vec4<f32>(
        cumulus_cover(weather),
        cumulus_depth_amount(weather),
        cumulus_height_m(weather),
        smoothstep(0.12, 0.68, weather.instability),
    );
}

// What the weather under a deck sets for it and the cirrus above: the deck's
// cover, its optical thickness, the softness of its edges, and cirrus cover.
// Cirrus cover follows regional condensation, which varies over far longer
// distances than the gap between the deck and the cirrus along a light.
fn deck_drivers(weather: CloudWeatherSample) -> vec4<f32> {
    let canopy = regional_canopy_conditions(weather);
    return vec4<f32>(
        canopy.sky_fraction,
        canopy.mean_optical_thickness * (1.0 + 2.0 * weather.rain),
        mix(0.08, 0.40, weather.stratiform),
        0.45 * smoothstep(0.25, 0.80, weather.regional.condensation),
    );
}

// Cumulus drivers with the gradient of their cover per metre.
struct CumulusDrivers {
    drivers: vec4<f32>,
    cover_gradient: vec2<f32>,
};

// Rays from a cloud shadow plane position along one light, which stay the
// same for every cloud type, and the deck and cirrus they meet.
struct CloudShadowRays {
    light: vec3<f32>,
    amount: f32,
    // The light's disc width per metre along it.
    disc_per_m: f32,
    // Distance along the light from sea level to the plane.
    below_plane_m: f32,
    deck: CloudRay,
    cirrus: CloudRay,
};

fn cloud_shadow_rays(plane_position_m: vec2<f32>, source_index: u32) -> CloudShadowRays {
    let receiver = celestial_receiver_light_at_position_and_height(
        plane_position_m,
        CLOUD_SHADOW_PLANE_HEIGHT_M,
    );
    let source = celestial_source_light(receiver, source_index);
    return cloud_shadow_rays_for_light(
        plane_position_m,
        cloud_shadow_direction(receiver, source),
        source.amount,
        source.angular_radius_rad,
    );
}

fn cloud_shadow_rays_for_light(
    plane_position_m: vec2<f32>,
    light: vec3<f32>,
    amount: f32,
    angular_radius_rad: f32,
) -> CloudShadowRays {
    let world_radius_m = scene.world_size_m.x / TAU;
    let plane_radius_m = world_radius_m + CLOUD_SHADOW_PLANE_HEIGHT_M;
    let along_m = plane_radius_m * light.z;
    let below_plane_m = max(
        along_m - sqrt(max(
            along_m * along_m - plane_radius_m * plane_radius_m
                + world_radius_m * world_radius_m,
            0.0,
        )),
        0.0,
    );
    return CloudShadowRays(
        light,
        amount,
        2.0 * angular_radius_rad,
        below_plane_m,
        cloud_ray(plane_position_m, CLOUD_SHADOW_PLANE_HEIGHT_M, DECK_HEIGHT_M, light),
        cloud_ray(plane_position_m, CLOUD_SHADOW_PLANE_HEIGHT_M, CIRRUS_HEIGHT_M, light),
    );
}

// Cumulus along a light at `height_m`. The cumulus height under the deck
// along the same light gives a first guess, and the height under that guess
// the cumulus, so a cloud's height barely depends on the light.
fn cumulus_ray(plane_position_m: vec2<f32>, rays: CloudShadowRays, height_m: f32) -> CloudRay {
    return cloud_ray(plane_position_m, CLOUD_SHADOW_PLANE_HEIGHT_M, height_m, rays.light);
}

fn cloud_light_direction(rays: CloudShadowRays) -> vec2<f32> {
    return rays.light.xy / max(length(rays.light.xy), 1e-6);
}

fn cumulus_shadow(
    plane_position_m: vec2<f32>,
    rays: CloudShadowRays,
    ray: CloudRay,
    cumulus: CumulusDrivers,
    grid_shortest_m: f32,
) -> CumulusShadow {
    let clear = CumulusShadow(-CLOUD_SHADOW_RIM_LIMIT, 1.0);
    let cover = cumulus.drivers.x;
    if cover <= 0.0 && !EVALUATE_ALL_BRANCHES {
        return clear;
    }
    let diameter_m = rays.disc_per_m * (rays.below_plane_m + ray.distance_m);
    let shortest_m = max(diameter_m, grid_shortest_m);
    let lowest = cloud_ray(
        plane_position_m,
        CLOUD_SHADOW_PLANE_HEIGHT_M,
        CUMULUS_LOWEST_HEIGHT_M,
        rays.light,
    );
    let latent = band_limited_cloud_latent(
        CUMULUS_NOISE_FIELD,
        ray.position_m - scene.view_cloud_motion_m.xy,
        CUMULUS_WAVELENGTH_M,
        scene.cloud_time_coordinates.x,
        0.58,
        shortest_m,
        select(
            shortest_m,
            max(rays.disc_per_m * (rays.below_plane_m + lowest.distance_m), grid_shortest_m),
            EVALUATE_ALL_BRANCHES,
        ),
    );
    let rank = cloud_latent_rank(latent);
    let character = mix(0.2, 0.5, cumulus.drivers.w);
    let shape = mix(rank, abs(2.0 * rank - 1.0), character);
    let shape_per_rank = 1.0 - character + 2.0 * character * sign(2.0 * rank - 1.0);
    let threshold = cumulus_shape_threshold(cover, character);
    // Where cover changes quickly, as at a coast or a rain area, its gradient
    // moves the rim as much as the cells' own shape does.
    let threshold_per_cover = 50.0 * (
        cumulus_shape_threshold(cover + 0.01, character)
            - cumulus_shape_threshold(cover - 0.01, character)
    );
    let edge = disc_cloud_edge(
        shape,
        threshold,
        shape_per_rank * cloud_latent_rank_gradient(latent, rank)
            - threshold_per_cover * cumulus.cover_gradient,
        ray,
        cloud_light_direction(rays),
        diameter_m,
    );
    // Humilis liquid water paths of 20–50 g/m² give optical depths near 3–8;
    // congestus exceeds 30.
    let depth = mix(4.0, 25.0, cumulus.drivers.y);
    // Thickness follows the cells' excess over the threshold, so the small
    // cells of sparse cover stay thin rather than reaching full depth.
    let rise_scale = max(1.0 - threshold, 0.5);
    let rise = clamp((edge.inner_value - threshold) / rise_scale, 0.0, 1.0);
    let extinction = cloud_shadow_extinction(ray);
    // A dome: thickness rises steeply from the rim and flattens at the peak.
    let inner = exp(-extinction * depth * rise * (2.0 - rise));
    // Under a field too coarse for its cells, cumulus leave their mean dimming
    // inside a rim that covers the whole disc: their cover of a dome's mean
    // thickness over the uniform excess of their ranks.
    let highest_rise = (1.0 - threshold) / rise_scale;
    let mean_body = highest_rise - highest_rise * highest_rise / 3.0;
    let resolved = clamp(log2(CUMULUS_WAVELENGTH_M / shortest_m), 0.0, 1.0);
    let shadow = CumulusShadow(
        mix(
            CLOUD_SHADOW_RIM_LIMIT,
            clamp(edge.rim, -CLOUD_SHADOW_RIM_LIMIT, CLOUD_SHADOW_RIM_LIMIT),
            resolved,
        ),
        mix(1.0 - cover * (1.0 - exp(-extinction * depth * mean_body)), inner, resolved),
    );
    if cover <= 0.0 {
        return clear;
    }
    return shadow;
}

// Stratiform and frontal decks: the regional canopy fraction sets the share of
// sky they cover, stable air softens their edges into overcast, and rain
// thickens them.
fn deck_transmittance(rays: CloudShadowRays, deck: vec4<f32>, grid_shortest_m: f32) -> f32 {
    let cover = deck.x;
    if cover <= 0.0 && !EVALUATE_ALL_BRANCHES {
        return 1.0;
    }
    let ray = rays.deck;
    let diameter_m = rays.disc_per_m * (rays.below_plane_m + ray.distance_m);
    let shortest_m = max(diameter_m, grid_shortest_m);
    let latent = band_limited_cloud_latent(
        DECK_NOISE_FIELD,
        ray.position_m - scene.view_cloud_motion_m.zw,
        DECK_WAVELENGTH_M,
        scene.cloud_time_coordinates.y,
        0.5,
        shortest_m,
        shortest_m,
    );
    let rank = cloud_latent_rank(latent);
    let softness = deck.z;
    let threshold = 1.0 - cover;
    let edge = disc_cloud_edge(
        rank,
        threshold,
        cloud_latent_rank_gradient(latent, rank),
        ray,
        cloud_light_direction(rays),
        diameter_m,
    );
    let amount = smoothstep(threshold, threshold + softness, edge.inner_value);
    // Thickness grows from the deck's edges toward its cores.
    let interior = clamp(
        (edge.inner_value - threshold) / max(1.0 - threshold, 0.05),
        0.0,
        1.0,
    );
    let thickness = deck.y;
    let extinction = cloud_shadow_extinction(ray);
    let detailed = 1.0 - disc_cover(edge.rim)
        * (1.0 - exp(-extinction * amount * mix(0.4, 1.4, interior) * thickness));
    let mean = 1.0 - cover * (1.0 - exp(-extinction * 0.9 * thickness));
    let resolved = clamp(log2(DECK_WAVELENGTH_M / shortest_m), 0.0, 1.0);
    return select(1.0, mix(mean, detailed, resolved), cover > 0.0);
}

// Thin ice-cloud veils in moist upper air.
fn cirrus_transmittance(rays: CloudShadowRays, deck: vec4<f32>, grid_shortest_m: f32) -> f32 {
    let cover = deck.w;
    if cover <= 0.0 && !EVALUATE_ALL_BRANCHES {
        return 1.0;
    }
    let ray = rays.cirrus;
    let diameter_m = rays.disc_per_m * (rays.below_plane_m + ray.distance_m);
    let shortest_m = max(diameter_m, grid_shortest_m);
    let latent = band_limited_cloud_latent(
        CIRRUS_NOISE_FIELD,
        ray.position_m - CIRRUS_TRANSPORT_RATIO * scene.view_cloud_motion_m.zw,
        CIRRUS_WAVELENGTH_M,
        0.5 * scene.cloud_time_coordinates.y,
        0.62,
        shortest_m,
        shortest_m,
    );
    let rank = cloud_latent_rank(latent);
    let rim = 1.0 - cover;
    let edge = disc_cloud_edge(
        rank,
        rim,
        cloud_latent_rank_gradient(latent, rank),
        ray,
        cloud_light_direction(rays),
        diameter_m,
    );
    let amount = smoothstep(rim, 1.0 - 0.6 * cover, edge.inner_value);
    let extinction = cloud_shadow_extinction(ray);
    let detailed = 1.0 - disc_cover(edge.rim) * (1.0 - exp(-extinction * 0.4 * amount));
    let mean = 1.0 - cover * (1.0 - exp(-extinction * 0.2));
    let resolved = clamp(log2(CIRRUS_WAVELENGTH_M / shortest_m), 0.0, 1.0);
    return select(1.0, mix(mean, detailed, resolved), cover > 0.0);
}

// Every cloud type's shadow along one light from a plane position, with the
// drivers under the cumulus and under the deck.
fn cloud_shadow_texel(
    plane_position_m: vec2<f32>,
    rays: CloudShadowRays,
    cumulus: CloudRay,
    cumulus_drivers: CumulusDrivers,
    deck: vec4<f32>,
    grid_shortest_m: f32,
) -> vec4<f32> {
    let cumulus_shadow = cumulus_shadow(
        plane_position_m,
        rays,
        cumulus,
        cumulus_drivers,
        grid_shortest_m,
    );
    return select(
        cloud_shadow_texel_field(CloudShadowTexel(-CLOUD_SHADOW_RIM_LIMIT, 1.0, 1.0)),
        cloud_shadow_texel_field(CloudShadowTexel(
            cumulus_shadow.rim,
            cumulus_shadow.inner_transmittance,
            deck_transmittance(rays, deck, grid_shortest_m)
                * cirrus_transmittance(rays, deck, grid_shortest_m),
        )),
        rays.amount > 0.0,
    );
}
