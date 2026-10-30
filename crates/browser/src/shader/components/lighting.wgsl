// Directions are geometric. Atmospheric refraction, which lifts a light at
// the horizon by about 0.57 degrees (Saemundsson 1986, Sky Telesc. 72) and
// delays sea-level sunset by about two minutes at the equator, is left out:
// a compromise that keeps one direction for light, shadow geometry and its
// grid windows, tides and eclipses. game::celestial_direction mirrors it.
fn celestial_direction_at_position(
    position_m: vec2<f32>,
    declination: f32,
    hour_angle: f32,
) -> vec3<f32> {
    let latitude = radians(surface_latitude_phase_degrees_at(position_m));
    return vec3<f32>(
        -cos(declination) * sin(hour_angle),
        sin(declination) * cos(latitude)
            - cos(declination) * sin(latitude) * cos(hour_angle),
        sin(latitude) * sin(declination)
            + cos(latitude) * cos(declination) * cos(hour_angle),
    );
}

fn topocentric_moon_direction_for_geocentric(
    height_m: f32,
    geocentric_direction: vec3<f32>,
) -> vec3<f32> {
    let world_radius_m = scene.world_size_m.x / TAU;
    let observer_radius_planet_radii = (
        world_radius_m + height_m
    ) / world_radius_m;
    return normalize(
        LUNAR_DISTANCE_PLANET_RADII * geocentric_direction
            - vec3<f32>(0.0, 0.0, observer_radius_planet_radii),
    );
}

fn angular_separation(left: vec3<f32>, right: vec3<f32>) -> f32 {
    return atan2(length(cross(left, right)), dot(left, right));
}

fn circle_overlap_fraction(
    separation: f32,
    target_radius: f32,
    occluder_radius: f32,
) -> f32 {
    if separation >= target_radius + occluder_radius {
        return 0.0;
    }
    if separation <= abs(target_radius - occluder_radius) {
        if occluder_radius >= target_radius {
            return 1.0;
        }
        return occluder_radius * occluder_radius / (target_radius * target_radius);
    }
    let target_angle = acos(clamp(
        (separation * separation + target_radius * target_radius
            - occluder_radius * occluder_radius)
            / (2.0 * separation * target_radius),
        -1.0,
        1.0,
    ));
    let occluder_angle = acos(clamp(
        (separation * separation + occluder_radius * occluder_radius
            - target_radius * target_radius)
            / (2.0 * separation * occluder_radius),
        -1.0,
        1.0,
    ));
    let lens = 0.5 * sqrt(
        (-separation + target_radius + occluder_radius)
            * (separation + target_radius - occluder_radius)
            * (separation - target_radius + occluder_radius)
            * (separation + target_radius + occluder_radius),
    );
    return (
        target_radius * target_radius * target_angle
            + occluder_radius * occluder_radius * occluder_angle
            - lens
    ) / (PI * target_radius * target_radius);
}

fn initialize_celestial_lighting() {
    let hour_angle = TAU * (surface_local_solar_phase() - 0.5);
    sun_light_direction = celestial_direction_at_position(
        surface_position_m,
        scene.celestial_orbit_angles.x,
        hour_angle,
    );
    geocentric_moon_light_direction = celestial_direction_at_position(
        surface_position_m,
        scene.celestial_orbit_angles.z,
        hour_angle
            + scene.celestial_orbit_angles.y
            - scene.celestial_orbit_angles.w,
    );
    lunar_penumbra_fraction = scene.celestial_orbit_light.y;
    lunar_umbra_fraction = scene.celestial_orbit_light.z;
}

fn sun_direction() -> vec3<f32> {
    return sun_light_direction;
}

struct CelestialReceiverLight {
    position_m: vec2<f32>,
    height_m: f32,
    // The horizon the receiver's height has on Earth, whose air sets each
    // light's attenuation and visibility. The small world's lower horizon
    // bounds only shadow reach and grid windows.
    horizon_z: f32,
    sun_direction: vec3<f32>,
    moon_direction: vec3<f32>,
    solar_eclipse_fraction: f32,
    sun_amount: f32,
    moon_amount: f32,
};

struct CelestialSourceLight {
    direction: vec3<f32>,
    amount: f32,
    angular_radius_rad: f32,
};

fn celestial_source_light(
    receiver: CelestialReceiverLight,
    source_index: u32,
) -> CelestialSourceLight {
    let is_sun = source_index == 0u;
    return CelestialSourceLight(
        select(receiver.moon_direction, receiver.sun_direction, is_sun),
        select(receiver.moon_amount, receiver.sun_amount, is_sun),
        select(
            MOON_ANGULAR_RADIUS_RAD,
            CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
            is_sun,
        ),
    );
}

fn celestial_horizon_z_at_height(height_m: f32) -> f32 {
    let world_radius_m = scene.world_size_m.x / TAU;
    let observer_radius_m = world_radius_m + max(height_m, 0.0);
    let radius_ratio = world_radius_m / observer_radius_m;
    return -sqrt(max(1.0 - radius_ratio * radius_ratio, 0.0));
}

fn celestial_disc_visibility_at_height(
    height_m: f32,
    light: vec3<f32>,
    angular_radius_rad: f32,
) -> f32 {
    let horizon_z = celestial_horizon_z_at_height(height_m);
    return celestial_disc_visibility_at_horizon(
        horizon_z,
        light,
        angular_radius_rad,
    );
}

fn celestial_disc_visibility_at_horizon(
    horizon_z: f32,
    light: vec3<f32>,
    angular_radius_rad: f32,
) -> f32 {
    let horizon_cosine = sqrt(max(1.0 - horizon_z * horizon_z, 0.0));
    let disc_sine = sin(angular_radius_rad);
    let disc_cosine = cos(angular_radius_rad);
    // The disc's centre stands `centre` radii above the horizon, which hides
    // the circular segment below it.
    let centre = clamp(
        (light.z - horizon_z * disc_cosine) / (horizon_cosine * disc_sine),
        -1.0,
        1.0,
    );
    return select(disc_cover(centre), select(0.0, 1.0, centre > 0.0), abs(centre) >= 1.0);
}

fn celestial_visible_disc_sample_direction_at_horizon(
    horizon_z: f32,
    light: vec3<f32>,
    angular_radius_rad: f32,
) -> vec3<f32> {
    let light_horizontal = length(light.xy);
    if light_horizontal <= 0.001 {
        return light;
    }
    let disc_sine = sin(angular_radius_rad);
    let disc_cosine = cos(angular_radius_rad);
    let lower_limb_z = light.z * disc_cosine - light_horizontal * disc_sine;
    let upper_limb_z = light.z * disc_cosine + light_horizontal * disc_sine;
    if lower_limb_z >= horizon_z || upper_limb_z <= horizon_z {
        return light;
    }
    // The centre ray enters the planet during the lower half of partial visibility.
    // One visible-slice sample keeps the whole transition on the sunward path.
    let sampled_z = 0.5 * (horizon_z + upper_limb_z);
    let sampled_horizontal = sqrt(max(1.0 - sampled_z * sampled_z, 0.0));
    return vec3<f32>(sampled_horizontal * light.xy / light_horizontal, sampled_z);
}

fn solar_eclipse_fraction_for_directions(sun: vec3<f32>, moon: vec3<f32>) -> f32 {
    let disc_sum = CELESTIAL_DISC_ANGULAR_RADIUS_RAD + MOON_ANGULAR_RADIUS_RAD;
    if dot(sun, moon) <= cos(disc_sum) {
        return 0.0;
    }
    return circle_overlap_fraction(
        angular_separation(sun, moon),
        CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
        MOON_ANGULAR_RADIUS_RAD,
    );
}

fn sun_amount_at_horizon_with_eclipse_for_direction(
    horizon_z: f32,
    air_column: f32,
    eclipse_fraction: f32,
    sun: vec3<f32>,
) -> f32 {
    let elevation = clamp(
        (sun.z - horizon_z) / (1.0 - horizon_z),
        0.0,
        1.0,
    );
    return (1.0 - eclipse_fraction)
        * celestial_disc_visibility_at_horizon(
            horizon_z,
            sun,
            CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
        )
        * clear_sky_beam_transmittance(elevation, air_column);
}

fn sun_amount_at_horizon_with_eclipse(
    horizon_z: f32,
    air_column: f32,
    eclipse_fraction: f32,
) -> f32 {
    return sun_amount_at_horizon_with_eclipse_for_direction(
        horizon_z,
        air_column,
        eclipse_fraction,
        sun_direction(),
    );
}

fn celestial_receiver_light_for_directions(
    position_m: vec2<f32>,
    height_m: f32,
    sun: vec3<f32>,
    geocentric_moon: vec3<f32>,
) -> CelestialReceiverLight {
    let horizon_z = earth_horizon_z_at_height(height_m);
    let moon = topocentric_moon_direction_for_geocentric(height_m, geocentric_moon);
    let eclipse_fraction = solar_eclipse_fraction_for_directions(sun, moon);
    let air_column = air_column_at_height(height_m);
    let sun_amount = sun_amount_at_horizon_with_eclipse_for_direction(
        horizon_z,
        air_column,
        eclipse_fraction,
        sun,
    );
    let moon_elevation = clamp(
        (moon.z - horizon_z) / (1.0 - horizon_z),
        0.0,
        1.0,
    );
    let moon_amount = lunar_light_scale()
        * celestial_disc_visibility_at_horizon(
            horizon_z,
            moon,
            MOON_ANGULAR_RADIUS_RAD,
        )
        * clear_sky_beam_transmittance(moon_elevation, air_column);
    return CelestialReceiverLight(
        position_m,
        height_m,
        horizon_z,
        sun,
        moon,
        eclipse_fraction,
        sun_amount,
        moon_amount,
    );
}

fn celestial_receiver_light_at_height(height_m: f32) -> CelestialReceiverLight {
    return celestial_receiver_light_for_directions(
        surface_position_m,
        height_m,
        sun_light_direction,
        geocentric_moon_light_direction,
    );
}

fn celestial_receiver_light_at_position_and_height(
    position_m: vec2<f32>,
    height_m: f32,
) -> CelestialReceiverLight {
    let hour_angle = TAU * (surface_local_solar_phase_at(position_m) - 0.5);
    let sun = celestial_direction_at_position(
        position_m,
        scene.celestial_orbit_angles.x,
        hour_angle,
    );
    let geocentric_moon = celestial_direction_at_position(
        position_m,
        scene.celestial_orbit_angles.z,
        hour_angle + scene.celestial_orbit_angles.y - scene.celestial_orbit_angles.w,
    );
    return celestial_receiver_light_for_directions(
        position_m,
        height_m,
        sun,
        geocentric_moon,
    );
}

// Scene light is clear-sky daylight from a zenith Sun on level ground; these
// mirror game's constants of the same names, which give their provenance and
// compromises.
const PLANET_RADIUS_SCALE_HEIGHTS: f32 = 2.0 / (PI * 0.025 * 0.025);
const SCATTERED_TWILIGHT_RADIUS_SCALE_HEIGHTS: f32 = 312.5;
const SCATTERED_TWILIGHT_SHARE: f32 = 0.0146;
const FULL_MOON_SUNLIGHT_RATIO: f32 = 2.512e-6;
const TOTALITY_LIGHT_SHARE: f32 = 1.0 / 11460.0;
const REFERENCE_AEROSOL_OPTICAL_DEPTH: f32 = 0.103;
const REFERENCE_LINKE_TURBIDITY: f32 = 2.35 + 7.79 * REFERENCE_AEROSOL_OPTICAL_DEPTH;
// Earth's mean radius (Moritz 2000, Geodetic Reference System 1980).
const EARTH_RADIUS_M: f32 = 6.371e6;

// Clear-sky light on level ground from a light at the given sine of its
// height above the horizon, under `air_column` of sea level's air, per unit of
// a zenith light's at sea level, as game::clear_sky_light.
fn clear_sky_light(sine_height: f32, air_column: f32) -> f32 {
    let beam = clamp(sine_height, 0.0, 1.0) * clear_sky_beam_luminance(
        sine_height,
        REFERENCE_AEROSOL_OPTICAL_DEPTH,
        air_column,
    );
    let diffuse = clear_sky_diffuse_light(sine_height, REFERENCE_LINKE_TURBIDITY);
    return (beam + diffuse) / zenith_clear_sky_light();
}

// Clear-sky light on level ground from a zenith light at sea level, per unit
// of the light above the atmosphere, as game::zenith_clear_sky_light.
fn zenith_clear_sky_light() -> f32 {
    return clear_sky_beam_luminance(1.0, REFERENCE_AEROSOL_OPTICAL_DEPTH, 1.0)
        + clear_sky_diffuse_light(1.0, REFERENCE_LINKE_TURBIDITY);
}

// The beam's clear-sky transmittance along its path under `air_column` of sea
// level's air, per unit of a zenith beam's at sea level, as
// game::clear_sky_beam_transmittance.
fn clear_sky_beam_transmittance(sine_height: f32, air_column: f32) -> f32 {
    return clear_sky_beam_luminance(sine_height, REFERENCE_AEROSOL_OPTICAL_DEPTH, air_column)
        / clear_sky_beam_luminance(1.0, REFERENCE_AEROSOL_OPTICAL_DEPTH, 1.0);
}

// The Moon's light relative to the Sun's, through its phase and Earth's shadow,
// as game::celestial_lighting composes it. The eclipse dimming is compressed
// from a totality's 1e-4 of the full Moon so that totality stays visible.
fn lunar_light_scale() -> f32 {
    let phase_angle_degrees = degrees(acos(clamp(2.0 * scene.celestial_orbit_light.x - 1.0, -1.0, 1.0)));
    // game::lunar_phase_brightness gives this phase law's provenance.
    let phase_magnitudes = 0.026 * phase_angle_degrees + 4e-9 * pow(phase_angle_degrees, 4.0);
    return max(1.0 - 0.18 * lunar_penumbra_fraction - 0.78 * lunar_umbra_fraction, 0.04)
        * FULL_MOON_SUNLIGHT_RATIO
        * pow(10.0, -0.4 * phase_magnitudes);
}

// Height of the planet's shadow above a receiver, in planet radii, for a
// light at the given sine of its height above the local horizontal: R(1/cos θ
// - 1) for a light θ below it, and none above.
fn planet_shadow_height_radii(direction_z: f32) -> f32 {
    let z = min(direction_z, 0.0);
    return inverseSqrt(max(1.0 - z * z, 1e-12)) - 1.0;
}

// A light's clear-sky light on level ground as a share of its clear-sky light
// overhead, through twilight, as game::celestial_lighting composes it. It
// follows the light's height above the local horizontal, as on Earth, whose
// horizon dips little at hill and cloud heights; the small world's dip would
// light raised ground several times more than sea level under a low light.
// Below the horizon the sky keeps its light at sunset as far as the planet's
// shadow leaves its air lit, exp(-h/H) of an exponential atmosphere's column,
// plus light scattered more than once into the shadow.
fn twilight_clear_sky_light(direction_z: f32, height_m: f32) -> f32 {
    let shadow_height_radii = planet_shadow_height_radii(direction_z);
    return clear_sky_light(direction_z, air_column_at_height(height_m)) * mix(
        exp(-PLANET_RADIUS_SCALE_HEIGHTS * shadow_height_radii),
        exp(-SCATTERED_TWILIGHT_RADIUS_SCALE_HEIGHTS * shadow_height_radii),
        SCATTERED_TWILIGHT_SHARE,
    );
}

fn moon_atmospheric_amount(receiver: CelestialReceiverLight) -> f32 {
    return lunar_light_scale() * twilight_clear_sky_light(
        receiver.moon_direction.z,
        receiver.height_m,
    );
}

// Share of the Sun's clear-sky light that a solar eclipse covering
// `eclipse_fraction` of its disc leaves, as game::solar_eclipse_sky_share,
// which gives its provenance and compromise.
fn solar_eclipse_sky_share(eclipse_fraction: f32) -> f32 {
    return max(1.0 - eclipse_fraction, TOTALITY_LIGHT_SHARE);
}

// Clear-sky light on level ground from the night sky, the Sun, and the Moon,
// as game::celestial_lighting's ambient intensity.
fn clear_sky_intensity(receiver: CelestialReceiverLight) -> f32 {
    return min(
        1.0,
        NIGHT_SKY_LIGHT
            + solar_eclipse_sky_share(receiver.solar_eclipse_fraction)
                * twilight_clear_sky_light(receiver.sun_direction.z, receiver.height_m)
            + moon_atmospheric_amount(receiver),
    );
}

// Diffuse light that a non-absorbing cloud layer of optical depth `tau` and
// regional cover `cover` sends down from each beam, for lights at `z`, the sines
// of their elevations above the layer's level, by the two-stream form along
// the slant path that measured transmittance under uniform cloud follows
// against measured optical depth (Ezhova et al. 2020, Atmos. Meas. Tech. 13).
// A light above the level lights the layer's top with `top`, its direct light
// on level ground, and the layer passes its slant transmittance less the
// unscattered beam, which reaches the ground through the upper field's deck
// shadows. A light below the level but above the canopy's horizon lights the
// layer's underside with `normal`, its beam's light on a surface facing it at
// the layer's height. The layer reflects its slant reflectance of that beam,
// projected on it, down. The beam crossed the layer's level once already on
// the far side of its tangent point, at the same depth.
fn canopy_scattered_beams(
    tau: f32,
    cover: f32,
    z: vec2<f32>,
    top: vec2<f32>,
    normal: vec2<f32>,
) -> vec2<f32> {
    let slant_depth = tau / max(abs(z), vec2<f32>(0.001));
    let scaled = (1.0 - CLOUD_ASYMMETRY) * slant_depth;
    let reflectance = vec2<f32>(two_stream_reflectance(scaled.x), two_stream_reflectance(scaled.y));
    let unscattered = exp(-slant_depth);
    let from_above = top * max(1.0 - reflectance - unscattered, vec2<f32>(0.0));
    let from_below = normal * abs(z) * reflectance * (1.0 - cover + cover * unscattered);
    return select(from_below, from_above, z > vec2<f32>(0.0));
}

struct AtmosphericAmbientLight {
    // Clear-sky light over the whole sky, then its part where no canopy covers it.
    clear_sky_light: vec3<f32>,
    clear_sky: vec3<f32>,
    // Diffuse light the canopy passes down from the clear sky above it, then
    // from the beams it scatters, in total and per light.
    canopy: vec3<f32>,
    canopy_scattered: vec3<f32>,
    canopy_scattered_beams: vec2<f32>,
    split: CelestialLightSplit,
    // Regional mean share of each light's beam that reaches the ground through
    // the canopy's gaps and body.
    beam_transmittance: vec2<f32>,
};

fn calculate_atmospheric_ambient_light(
    canopy: RegionalCanopyConditions,
    receiver: CelestialReceiverLight,
) -> AtmosphericAmbientLight {
    let solar_ambient = twilight_clear_sky_light(receiver.sun_direction.z, receiver.height_m);
    let lunar_ambient = moon_atmospheric_amount(receiver);
    let clear_intensity = clear_sky_intensity(receiver);
    let split = celestial_light_split(receiver, clear_intensity);
    // Each light's sky takes that light's colour through the clear sky's
    // diffuse transfer at its height; starlight and airglow are white.
    let solar_sky = solar_eclipse_sky_share(receiver.solar_eclipse_fraction) * solar_ambient;
    var lunar_sky = vec3<f32>(0.0);
    if lunar_ambient > 0.0 || EVALUATE_ALL_BRANCHES {
        let moon_color = moon_own_color();
        lunar_sky = lunar_ambient * dot(moon_color, LUMINANCE_WEIGHTS) * unit_luminance(
            moon_color * clear_sky_light_color(
                regional_aerosol_optical_depth,
                receiver.moon_direction.z,
                receiver.height_m,
            ),
        );
    }
    let sky_color = (
        vec3<f32>(NIGHT_SKY_LIGHT)
            + solar_sky * clear_sky_light_color(
                regional_aerosol_optical_depth,
                receiver.sun_direction.z,
                receiver.height_m,
            )
            + lunar_sky
    ) / (NIGHT_SKY_LIGHT + solar_sky + lunar_ambient);
    let clear_sky_light = clear_intensity * sky_color;

    let optical_thickness = max(canopy.mean_optical_thickness, 0.0);
    let clear_contribution = (1.0 - canopy.sky_fraction) * clear_sky_light;
    if canopy.sky_fraction <= 0.0 && !EVALUATE_ALL_BRANCHES {
        return AtmosphericAmbientLight(
            clear_sky_light,
            clear_contribution,
            vec3<f32>(0.0),
            vec3<f32>(0.0),
            vec2<f32>(0.0),
            split,
            vec2<f32>(1.0),
        );
    }
    let diffuse_transmittance = 1.0 - two_stream_reflectance(
        (1.0 - CLOUD_ASYMMETRY) * optical_thickness,
    );
    // The canopy's level face takes the ground's split of clear-sky light: the
    // direct part follows the light's height above that level and vanishes at
    // it. Below that level the layer's underside takes the beam a receiver at
    // its height does.
    let canopy_receiver = celestial_receiver_light_at_height(WEATHER_CANOPY_HEIGHT_M);
    let scattered_beams = canopy_scattered_beams(
        optical_thickness,
        canopy.sky_fraction,
        vec2<f32>(canopy_receiver.sun_direction.z, canopy_receiver.moon_direction.z),
        split.direct,
        celestial_light_split(canopy_receiver, 1.0).normal,
    );
    let transmitted_light = diffuse_transmittance * split.diffuse_share * clear_sky_light;
    let scattered_light = scattered_beams.x * beam_light_color(
            regional_aerosol_optical_depth,
            WEATHER_CANOPY_HEIGHT_M,
            canopy_receiver.sun_direction.z,
        )
        + scattered_beams.y * moon_light_color(
            regional_aerosol_optical_depth,
            WEATHER_CANOPY_HEIGHT_M,
            canopy_receiver.moon_direction.z,
        );
    let ground_heights = max(
        vec2<f32>(
            light_height_above_horizon(receiver.sun_direction.z, receiver.horizon_z),
            light_height_above_horizon(receiver.moon_direction.z, receiver.horizon_z),
        ),
        vec2<f32>(0.001),
    );
    let intercepted = 1.0 - exp(-optical_thickness / ground_heights);

    let absent = canopy.sky_fraction <= 0.0;
    return AtmosphericAmbientLight(
        clear_sky_light,
        clear_contribution,
        select(canopy.sky_fraction * transmitted_light, vec3<f32>(0.0), absent),
        select(canopy.sky_fraction * scattered_light, vec3<f32>(0.0), absent),
        select(canopy.sky_fraction * scattered_beams, vec2<f32>(0.0), absent),
        split,
        1.0 - canopy.sky_fraction * intercepted,
    );
}

// Aerosol optical depth at 550 nm in air of the given relative humidity, before
// washout: a dry depth grown by water uptake as (1 - RH)^-γ (Hänel 1976, Adv.
// Geophys. 19). γ = 0.5 gives the 1.5-3 times dry scattering that marine sites
// measure at 85% humidity (Burgos et al. 2019, Sci. Data 6). The dry depth is
// the 0.11 mean oceanic depth at 500 nm (Smirnov et al. 2009, J. Geophys. Res.
// 114) dried from the marine boundary layer's typical 80% humidity and taken to
// 550 nm by its Ångström exponent of 0.6.
// The law holds for subsaturated air, and humidified nephelometers fit it only
// up to 95% humidity (Burgos et al. 2019). Growth is held there in nearly
// saturated and supersaturated air, standing in for haze activating into the
// mist and cloud droplets that the canopy carries.
fn humid_aerosol_optical_depth(relative_humidity: f32) -> f32 {
    return 0.046 / sqrt(1.0 - clamp(relative_humidity, 0.0, 0.95));
}

// The washout weights stand in for rain scavenging aerosol.
fn regional_aerosol_optical_depth_at(regional: RegionalCloudDrivers) -> f32 {
    let storm = regional.condensation
        * regional.pressure_lift
        * regional.instability;
    return humid_aerosol_optical_depth(regional.relative_humidity)
        * (1.0 - 0.72 * smoothstep(0.25, 0.70, storm));
}

fn local_aerosol_optical_depth_at(weather: CloudWeatherSample) -> f32 {
    return humid_aerosol_optical_depth(weather.relative_humidity)
        * (1.0 - 0.72 * weather.rain);
}

// Light colours are relative to sunlight above the atmosphere, which stands in
// for the display's D65 white, and each channel follows the dominant
// wavelength of its sRGB primary: 611, 549 and 464 nm. field_grids.rs's
// `beam_share` copies these spectral constants.
const LUMINANCE_WEIGHTS: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);
// Rayleigh optical depth at sea level, 0.008735 λ^-4.08 with λ in micrometres
// (Leckner 1978, Solar Energy 20).
const RAYLEIGH_OPTICAL_DEPTHS: vec3<f32> = vec3<f32>(0.06506, 0.10081, 0.19986);
// Aerosol optical depth per unit of its 550 nm depth by Ångström's (1929,
// Geogr. Ann. 11) law with the oceanic exponent 0.6 (Smirnov et al. 2009).
const AEROSOL_SPECTRAL_SCALES: vec3<f32> = vec3<f32>(0.93857, 1.00098, 1.10698);
// Chappuis-band ozone optical depth of a 0.3 atm-cm column, from Bird and
// Riordan's (1986, J. Clim. Appl. Meteorol. 25) absorption coefficients.
const OZONE_OPTICAL_DEPTHS: vec3<f32> = vec3<f32>(0.03542, 0.02523, 0.00219);

fn unit_luminance(color: vec3<f32>) -> vec3<f32> {
    return color / max(dot(color, LUMINANCE_WEIGHTS), 1e-30);
}

// Relative air mass for a light at the given sine of its height above the
// horizon, by Rozenberg's (1966) form, finite at the horizon and within 1% of
// Kasten-Young above 5 degrees. It is Earth's: the small world's own would
// reach only about 2 at the horizon and leave low light unreddened.
// Light colours take it along the light's height above the receiver's local
// horizontal, as on Earth, whose horizon dips little at cloud and hill
// heights.
fn relative_air_mass(sine_height: f32) -> f32 {
    let sine = clamp(sine_height, 0.0, 1.0);
    return 1.0 / (sine + 0.025 * exp(-11.0 * sine));
}

// Air mass through an ozone layer at 22 km over Earth's radius (Bird and
// Riordan 1986).
fn ozone_air_mass(sine_height: f32) -> f32 {
    let sine = clamp(sine_height, 0.0, 1.0);
    let layer_height = 22.0 / 6370.0;
    return (1.0 + layer_height) / sqrt(sine * sine + 2.0 * layer_height);
}

// Spectral transmittance of a beam through Rayleigh scattering, aerosol, and
// ozone, for the 550 nm aerosol optical depth and the beam's relative air mass
// and ozone air mass.
fn beam_transmittance(aerosol_optical_depth: f32, air_mass: f32, ozone_mass: f32) -> vec3<f32> {
    return exp(
        -air_mass * (RAYLEIGH_OPTICAL_DEPTHS + aerosol_optical_depth * AEROSOL_SPECTRAL_SCALES)
            - ozone_mass * OZONE_OPTICAL_DEPTHS,
    );
}

// Share of sea level's air that lies above a receiver `height_m` up, in an
// exponential atmosphere of PLANET_RADIUS_SCALE_HEIGHTS' scale height: the
// Rayleigh and aerosol air mass of every path from the receiver scales by it.
// Aerosol shares the air's scale height so that one path serves both, a
// semi-physical compromise: marine aerosol mostly lies within the lowest
// kilometre, so a raised receiver's aerosol path is somewhat overstated.
// Ozone lies above every receiver and keeps its whole column.
fn air_column_at_height(height_m: f32) -> f32 {
    return exp(-PLANET_RADIUS_SCALE_HEIGHTS * max(height_m, 0.0) / EARTH_RADIUS_M);
}

// Sine of the elevation of the horizon that a receiver `height_m` above sea
// level would have on Earth.
fn earth_horizon_z_at_height(height_m: f32) -> f32 {
    let height = max(height_m, 0.0);
    return -sqrt(height * (2.0 * EARTH_RADIUS_M + height)) / (EARTH_RADIUS_M + height);
}

// Luminous transmittance of a clear sky's beam from a light at the given sine
// of its height above the horizon, under `air_column` of sea level's air, for
// the 550 nm aerosol optical depth: its spectral transmittance's luminance for
// sunlight above the atmosphere. Luminance rather than broadband power sets
// the beam's brightness, since a low light's beam keeps mostly near-infrared
// power, which the eye does not see. Under the reference aerosol it stays
// within 5% of a 10 nm integration of the same extinction over the eye's
// photopic sensitivity (CIE 1924) and a 5778 K solar spectrum from the zenith
// down to 2 degrees, and within 10% at 1 degree.
fn clear_sky_beam_luminance(
    sine_height: f32,
    aerosol_optical_depth: f32,
    air_column: f32,
) -> f32 {
    let sine = clamp(sine_height, 0.0, 1.0);
    return dot(
        beam_transmittance(
            aerosol_optical_depth,
            relative_air_mass(sine) * air_column,
            ozone_air_mass(sine),
        ),
        LUMINANCE_WEIGHTS,
    );
}

// Spectral transmittance of the beam that reaches a receiver `height_m` above
// sea level from a light at the given sine of its height above the receiver's
// horizontal. Above that horizontal the beam takes `relative_air_mass`, whose
// horizon value stands for the horizontal path from the receiver. Below it,
// the ray passes its lowest point a depth d = r(1 - cos θ) under the receiver,
// for a light θ down and the receiver r from the centre of an Earth-sized
// planet, and climbs back up to it. In an exponential atmosphere of scale
// height H that path holds e^(d/H) (1 + erf √(d/H)) times the air of the
// horizontal path (Chapman's grazing incidence function for R/H >> 1), so the
// air mass is continuous at the horizontal. Either way the path holds
// `air_column_at_height` of the air of the same path from sea level. H keeps
// PLANET_RADIUS_SCALE_HEIGHTS' ratio to Earth's radius: 6.25 km rather than
// Earth's 8 km. erf takes Bürmann's series to first order, within 0.007. The
// lowest point stops at sea level, where the part of the disc still above the
// receiver's Earth horizon grazes. The ray crosses the ozone layer once, on
// the light's side, as steeply as a ray as far above the horizontal as the
// grazing ray is below it.
fn beam_transmittance_at_height(
    aerosol_optical_depth: f32,
    height_m: f32,
    sine_height: f32,
) -> vec3<f32> {
    let height = max(height_m, 0.0);
    let z = min(sine_height, 0.0);
    let depth_scale_heights = PLANET_RADIUS_SCALE_HEIGHTS * min(
        (1.0 + height / EARTH_RADIUS_M) * z * z / (1.0 + sqrt(1.0 - z * z)),
        height / EARTH_RADIUS_M,
    );
    let climb = sqrt(1.0 - exp(-4.0 / PI * depth_scale_heights));
    return beam_transmittance(
        aerosol_optical_depth,
        relative_air_mass(sine_height) * exp(depth_scale_heights) * (1.0 + climb)
            * air_column_at_height(height),
        ozone_air_mass(select(
            sine_height,
            min(-sine_height, -earth_horizon_z_at_height(height)),
            sine_height < 0.0,
        )),
    );
}

// Colour of a beam that reaches a receiver `height_m` above sea level, for the
// 550 nm aerosol optical depth and the sine of the light's height above the
// receiver's horizontal.
fn beam_light_color(aerosol_optical_depth: f32, height_m: f32, sine_height: f32) -> vec3<f32> {
    return unit_luminance(beam_transmittance_at_height(
        aerosol_optical_depth,
        height_m,
        sine_height,
    ));
}

// Colour of the clear sky's light on level ground `height_m` above sea level
// from a light at the given sine of its height above the local horizontal,
// through the air above the receiver, by Bird and Riordan's (1986) diffuse
// irradiance: half of what Rayleigh scattering takes from the beam, with their
// exponent of 0.95 for multiple scattering, and the forward share of what
// aerosol scatters after 1.5 air masses of Rayleigh extinction, both under
// the ozone the beam crosses. Marine aerosol scatters nearly all it
// extinguishes, and its asymmetry parameter is 0.7.
fn clear_sky_light_color(aerosol_optical_depth: f32, sine_height: f32, height_m: f32) -> vec3<f32> {
    let sine = clamp(sine_height, 0.0, 1.0);
    let air_mass = relative_air_mass(sine) * air_column_at_height(height_m);
    let rayleigh = air_mass * RAYLEIGH_OPTICAL_DEPTHS;
    let aerosol = exp(-air_mass * aerosol_optical_depth * AEROSOL_SPECTRAL_SCALES);
    // Bird and Riordan's Eqs. 3-11 to 3-14 for asymmetry 0.7.
    let forward = 1.0 - 0.5 * exp((-2.24599 + 0.37656 * sine) * sine);
    let ozone = exp(-ozone_air_mass(sine) * OZONE_OPTICAL_DEPTHS);
    return unit_luminance(ozone * (
        0.5 * (1.0 - exp(-0.95 * rayleigh))
            + exp(-1.5 * rayleigh) * (1.0 - aerosol) * forward
    ));
}

// The Moon's own colour, which game::celestial_lighting mirrors as
// `moon_color`: sunlight reflected by lunar soil, whose reflectance rises as
// λ^1.11 to redden the Sun's B-V colour index of 0.65 to the full Moon's 0.92
// (Cox 2000, Allen's Astrophysical Quantities), at unit luminance. In Earth's
// umbra the sunlight it reflects has been refracted through Earth's limb,
// crossing about twice the horizontal path through the reference air, which
// reddens it; the umbra's share of the disc mixes the two at unit luminance.
fn moon_own_color() -> vec3<f32> {
    let soil = vec3<f32>(1.1108, 0.9855, 0.8175);
    let refracted = unit_luminance(soil * beam_transmittance(
        REFERENCE_AEROSOL_OPTICAL_DEPTH,
        2.0 * relative_air_mass(0.0),
        2.0 * ozone_air_mass(0.0),
    ));
    return mix(soil, refracted, lunar_umbra_fraction);
}

// Moonlight's colour after the atmosphere, keeping the luminance of the
// Moon's own colour.
fn moon_light_color(aerosol_optical_depth: f32, height_m: f32, sine_height: f32) -> vec3<f32> {
    let own = moon_own_color();
    return dot(own, LUMINANCE_WEIGHTS)
        * unit_luminance(own * beam_light_color(aerosol_optical_depth, height_m, sine_height));
}

fn sun_direct_light_color(receiver: CelestialReceiverLight) -> vec3<f32> {
    return beam_light_color(
        local_aerosol_optical_depth,
        receiver.height_m,
        receiver.sun_direction.z,
    );
}

// ESRA's clear-sky diffuse light on level ground (Rigollier, Bauer and Wald
// 2000, Solar Energy 68) from a light at the given sine of its height above
// the horizon, per unit of the light above the atmosphere, by the model's
// Linke turbidity fits. game::ZENITH_DAYLIGHT_LUX names its compromises, and
// game::clear_sky_diffuse_light mirrors it; keep the two synchronized.
fn clear_sky_diffuse_light(sine_height: f32, linke_turbidity: f32) -> f32 {
    let sine = clamp(sine_height, 0.0, 1.0);
    let turbidity = linke_turbidity;
    let diffuse_transmission = -0.015843 + turbidity * (0.030543 + 0.0003797 * turbidity);
    let a0 = max(
        0.26463 + turbidity * (-0.061581 + 0.0031408 * turbidity),
        0.002 / diffuse_transmission,
    );
    let a1 = 2.04020 + turbidity * (0.018945 - 0.011161 * turbidity);
    let a2 = -1.3025 + turbidity * (0.039231 + 0.0085079 * turbidity);
    return diffuse_transmission * (a0 + sine * (a1 + a2 * sine));
}

// A beam's light on a surface facing it, per unit of a zenith light's
// clear-sky light on level ground, for a light of `color` above the
// atmosphere at that colour's luminance: the luminance that its path to the
// receiver passes, while its disc stands above the horizon the receiver's
// height has on Earth.
fn celestial_beam_normal_light(
    receiver: CelestialReceiverLight,
    direction: vec3<f32>,
    angular_radius_rad: f32,
    aerosol_optical_depth: f32,
    color: vec3<f32>,
) -> f32 {
    let visibility = celestial_disc_visibility_at_horizon(
        earth_horizon_z_at_height(receiver.height_m),
        direction,
        angular_radius_rad,
    );
    let beam = color * beam_transmittance_at_height(
        aerosol_optical_depth,
        receiver.height_m,
        direction.z,
    );
    return visibility * dot(beam, LUMINANCE_WEIGHTS)
        / (dot(color, LUMINANCE_WEIGHTS) * zenith_clear_sky_light());
}

struct CelestialLightSplit {
    // Sun and Moon direct light on level ground, then each beam's light on a
    // surface facing it.
    direct: vec2<f32>,
    normal: vec2<f32>,
    // Share of clear-sky light left diffuse.
    diffuse_share: f32,
};

// Splits clear-sky light into each beam's direct light on level ground, its
// light on a surface facing it projected on the receiver's horizontal, and
// the diffuse rest. A beam from below that horizontal lights only surfaces
// tilted toward it.
fn celestial_light_split(
    receiver: CelestialReceiverLight,
    clear_intensity: f32,
) -> CelestialLightSplit {
    let visibility = direct_light_visibility();
    let aerosol = regional_aerosol_optical_depth;
    let sun_share = 1.0 - receiver.solar_eclipse_fraction;
    let normal = visibility * vec2<f32>(
        sun_share * celestial_beam_normal_light(
            receiver,
            receiver.sun_direction,
            CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
            aerosol,
            vec3<f32>(1.0),
        ),
        lunar_light_scale() * celestial_beam_normal_light(
            receiver,
            receiver.moon_direction,
            MOON_ANGULAR_RADIUS_RAD,
            aerosol,
            moon_own_color(),
        ),
    );
    let direct = normal * max(
        vec2<f32>(receiver.sun_direction.z, receiver.moon_direction.z),
        vec2<f32>(0.0),
    );
    return CelestialLightSplit(
        direct,
        normal,
        max(1.0 - (direct.x + direct.y) / max(clear_intensity, 1e-30), 0.0),
    );
}

// Reflectance of a non-absorbing layer by the two-stream approximation
// (Bohren 1987, Am. J. Phys. 55), for its optical thickness along the light's
// path scaled by one minus the asymmetry parameter: the layer's own thickness
// for diffuse light, its slant thickness for a beam.
fn two_stream_reflectance(scaled_optical_thickness: f32) -> f32 {
    return scaled_optical_thickness / (2.0 + scaled_optical_thickness);
}

// Asymmetry parameter of water-cloud droplets in visible light (Hansen and
// Travis 1974, Space Sci. Rev. 16).
const CLOUD_ASYMMETRY: f32 = 0.85;

// Clear air returns ground light by isotropic Rayleigh scattering through
// Kasten's 0.12 broadband optical depth at one air mass, and by aerosol with
// asymmetry parameter 0.7.
fn clear_sky_reflectance_from_below(regional: RegionalCloudDrivers) -> f32 {
    return two_stream_reflectance(0.12 + 0.3 * regional_aerosol_optical_depth_at(regional));
}

// Diffuse light that ground and sky exchange before it arrives: a first pass
// of regional ground light, reflected up with regional albedo and returned by
// the sky's reflectance from below, repeats as a geometric series. It follows
// regional means, so it lifts every shadow in a region alike.
fn ground_bounce_light(
    first_pass_light: vec3<f32>,
    sky_reflectance: f32,
    regional_albedo: f32,
) -> vec3<f32> {
    let exchange = regional_albedo * sky_reflectance;
    return first_pass_light * exchange / (1.0 - exchange);
}

fn moon_direct_light_color(receiver: CelestialReceiverLight) -> vec3<f32> {
    return moon_light_color(
        local_aerosol_optical_depth,
        receiver.height_m,
        receiver.moon_direction.z,
    );
}

struct CloudRay {
    position_m: vec2<f32>,
    // Distance along the light from the source height, and the sine of the
    // light's elevation where it meets the cloud.
    distance_m: f32,
    sine_elevation: f32,
};

fn cloud_ray(
    position_m: vec2<f32>,
    source_height_m: f32,
    cloud_height_m: f32,
    light: vec3<f32>,
) -> CloudRay {
    if cloud_height_m <= source_height_m {
        return CloudRay(position_m, 0.0, light.z);
    }
    let world_radius_m = scene.world_size_m.x / TAU;
    let source_radius_m = world_radius_m + source_height_m;
    let cloud_radius_m = world_radius_m + cloud_height_m;
    let origin_along_ray_m = source_radius_m * light.z;
    let ray_distance_m = -origin_along_ray_m + sqrt(
        origin_along_ray_m * origin_along_ray_m
            + cloud_radius_m * cloud_radius_m
            - source_radius_m * source_radius_m,
    );
    let sine_elevation = (origin_along_ray_m + ray_distance_m) / cloud_radius_m;
    let horizontal_light = length(light.xy);
    if horizontal_light <= 0.001 {
        return CloudRay(position_m, ray_distance_m, sine_elevation);
    }
    let angular_distance = atan2(
        ray_distance_m * horizontal_light,
        source_radius_m + ray_distance_m * light.z,
    );
    let tangent_direction = light.xy / horizontal_light;
    return CloudRay(
        position_m + world_radius_m * angular_distance * tangent_direction,
        ray_distance_m,
        sine_elevation,
    );
}

fn projected_cloud_position(
    position_m: vec2<f32>,
    source_height_m: f32,
    cloud_height_m: f32,
    light: vec3<f32>,
) -> vec2<f32> {
    return cloud_ray(position_m, source_height_m, cloud_height_m, light).position_m;
}

// Cloud shadows take each light once per pixel, toward the centre of its
// visible disc.
fn cloud_shadow_direction(
    receiver: CelestialReceiverLight,
    source: CelestialSourceLight,
) -> vec3<f32> {
    return celestial_visible_disc_sample_direction_at_horizon(
        receiver.horizon_z,
        source.direction,
        source.angular_radius_rad,
    );
}

// The Moon's cloud shadows fade out as the share of a receiver's clear-sky
// light that the Moon supplies falls between these, about a just-noticeable
// luminance step. field_grids.rs skips the Moon's cloud shadow grid from its
// own estimate of this share, so keep `moon_clear_sky_share` and
// `MOON_CLOUD_SHADOW_GRID_SHARE` there synchronized with this function.
const MOON_CLOUD_SHADOW_SHARES: vec2<f32> = vec2<f32>(0.02, 0.04);

// The share of a receiver's clear-sky light that the Moon supplies on level
// ground. It counts no ground-sky bounce light, so it bounds the Moon's share
// of the light that arrives from above. Light colours keep their source's
// luminance through the atmosphere, so sunlight counts at unit luminance and
// moonlight at the Moon's own.
fn moon_clear_sky_share(receiver: CelestialReceiverLight, ambient: AtmosphericAmbientLight) -> f32 {
    let luminance = vec3<f32>(0.2126, 0.7152, 0.0722);
    let sun = ambient.split.direct.x;
    let moon = ambient.split.direct.y * dot(moon_own_color(), luminance);
    let clear_sky = ambient.split.diffuse_share * dot(ambient.clear_sky_light, luminance);
    return moon / max(sun + moon + clear_sky, 1e-30);
}

fn moon_cloud_shadow_weight(
    receiver: CelestialReceiverLight,
    ambient: AtmosphericAmbientLight,
) -> f32 {
    if scene.cloud_shadow_grids.z <= 0.0 && !EVALUATE_ALL_BRANCHES {
        return 0.0;
    }
    return scene.cloud_shadow_grids.z * smoothstep(
        MOON_CLOUD_SHADOW_SHARES.x,
        MOON_CLOUD_SHADOW_SHARES.y,
        moon_clear_sky_share(receiver, ambient),
    );
}

// Cloud field transmittance toward the Sun, then the Moon, for the lights
// that shadow this receiver.
fn celestial_cloud_transmittance_at(direct_light: CelestialReceiverLight) -> vec2<f32> {
    var transmittance = vec2<f32>(1.0);
    for (var source_index = 0u; source_index < 2u; source_index++) {
        let source = celestial_source_light(direct_light, source_index);
        let shadowed = source.amount > 0.0
            && (source_index == 0u || scene.cloud_shadow_grids.z > 0.0)
            // The legacy look takes shadows only above a sea-level cutoff.
            && (!LEGACY_SHADOW_LOOK || source.direction.z > 0.04);
        if shadowed || EVALUATE_ALL_BRANCHES {
            let sampled = sampled_cloud_transmittance_at(
                projected_cloud_position(
                    direct_light.position_m,
                    direct_light.height_m,
                    CLOUD_SHADOW_PLANE_HEIGHT_M,
                    cloud_shadow_direction(direct_light, source),
                ),
                source_index,
            );
            transmittance[source_index] = select(1.0, sampled, shadowed);
        }
    }
    return transmittance;
}

fn light_height_above_horizon(light_z: f32, horizon_z: f32) -> f32 {
    return (light_z - horizon_z) / (1.0 - horizon_z);
}

// The cloud field's shadow on the ground, or the legacy look's fade. Where
// `weight` fades the field's shadow out, the light keeps
// `regional_transmittance`, the region's mean share of the beam that cloud
// passes.
fn cloud_shadow_transmittance(
    cloud_transmittance: f32,
    regional_transmittance: f32,
    light: vec3<f32>,
    weight: f32,
) -> f32 {
    var shadow = cloud_transmittance;
    var faded = regional_transmittance;
    if LEGACY_SHADOW_LOOK {
        shadow = pow(max(cloud_transmittance, 1e-30), smoothstep(0.04, 0.22, light.z));
        faded = 1.0;
    }
    return mix(faded, shadow, weight);
}

fn effective_cloud_shadow_amount(
    sun_transmittance: f32,
    moon_transmittance: f32,
    receiver: CelestialReceiverLight,
) -> f32 {
    let visibility = direct_light_visibility();
    return clamp(
        visibility.x * receiver.sun_amount
                * (1.0 - cloud_shadow_transmittance(
                    sun_transmittance,
                    1.0,
                    receiver.sun_direction,
                    1.0,
                ))
            + visibility.y * receiver.moon_amount
                * (1.0 - cloud_shadow_transmittance(
                    moon_transmittance,
                    1.0,
                    receiver.moon_direction,
                    moon_cloud_shadow_weight(
                        receiver,
                        calculate_atmospheric_ambient_light(RegionalCanopyConditions(0.0, 0.0), receiver),
                    ),
                )),
        0.0,
        1.0,
    );
}
