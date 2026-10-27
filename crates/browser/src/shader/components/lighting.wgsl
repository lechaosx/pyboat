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
    return smoothstep(
        horizon_z * disc_cosine - horizon_cosine * disc_sine,
        horizon_z * disc_cosine + horizon_cosine * disc_sine,
        light.z,
    );
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
        * (0.35 + 0.65 * sqrt(elevation));
}

fn sun_amount_at_horizon_with_eclipse(
    horizon_z: f32,
    eclipse_fraction: f32,
) -> f32 {
    return sun_amount_at_horizon_with_eclipse_for_direction(
        horizon_z,
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
    let horizon_z = celestial_horizon_z_at_height(height_m);
    let moon = topocentric_moon_direction_for_geocentric(height_m, geocentric_moon);
    let eclipse_fraction = solar_eclipse_fraction_for_directions(sun, moon);
    let sun_amount = sun_amount_at_horizon_with_eclipse_for_direction(
        horizon_z,
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
        * (0.35 + 0.65 * sqrt(moon_elevation));
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

// Calibrated clear-sky light on level ground in daylight, for a light at the
// given sine of its height above the horizon.
fn daylight_light(height: f32) -> f32 {
    return 0.45 + 0.515 * sqrt(height);
}

// The Moon's light relative to the Sun's, through its phase and Earth's shadow,
// both compressed into the exposure range: 0.12 stands in for the full Moon's
// 2.5e-6 of sunlight, and the eclipse dimming for a totality's 1e-4 of it.
fn lunar_light_scale() -> f32 {
    return max(1.0 - 0.18 * lunar_penumbra_fraction - 0.78 * lunar_umbra_fraction, 0.04)
        * 0.12
        * scene.celestial_orbit_light.x;
}

fn moon_atmospheric_amount(receiver: CelestialReceiverLight) -> f32 {
    // These atmospheric horizon thresholds must stay synchronized with game::celestial_lighting.
    let daylight = smoothstep(
        -0.0348995,
        0.1391731,
        receiver.moon_direction.z,
    );
    let lower_twilight = smoothstep(
        -0.1564345,
        0.0174524,
        receiver.moon_direction.z,
    );
    let upper_afterglow = smoothstep(
        -0.309017,
        -0.1391731,
        receiver.moon_direction.z,
    ) * (1.0 - smoothstep(
        -0.1391731,
        -0.0348995,
        receiver.moon_direction.z,
    ));
    let moon_elevation = clamp(
        (receiver.moon_direction.z - receiver.horizon_z)
            / (1.0 - receiver.horizon_z),
        0.0,
        1.0,
    );
    let lunar_ambient = daylight * daylight_light(moon_elevation)
        + 0.30 * (1.0 - daylight) * lower_twilight
        + 0.065 * upper_afterglow;
    return lunar_light_scale() * lunar_ambient;
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
// layer's underside with `normal`, its beam across its path: the calibrated
// direct light at the zenith by the light's clear-sky beam share above the
// canopy's horizon. The layer reflects its slant reflectance of that beam,
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
    let sun = sun_direction();
    let receiver_sun_z = clamp(
        (sun.z - receiver.horizon_z) / (1.0 - receiver.horizon_z),
        0.0,
        1.0,
    );
    // These sine-of-elevation thresholds must stay synchronized with game::celestial_lighting.
    let daylight = smoothstep(-0.0348995, 0.1391731, sun.z);
    let lower_twilight = smoothstep(-0.1564345, 0.0174524, sun.z);
    let upper_afterglow = smoothstep(-0.309017, -0.1391731, sun.z)
        * (1.0 - smoothstep(-0.1391731, -0.0348995, sun.z));
    let solar_ambient = daylight * daylight_light(receiver_sun_z)
        + 0.30 * (1.0 - daylight) * lower_twilight
        + 0.065 * upper_afterglow;
    let moon_ambient_amount = moon_atmospheric_amount(receiver);
    let lunar_ambient = CLEAR_SKY_LUNAR_WEIGHT * moon_ambient_amount;
    let clear_intensity = min(
        1.0,
        // Starlight and airglow, compressed from about 1e-8 of daylight.
        0.035
            + (1.0 - 0.85 * receiver.solar_eclipse_fraction)
                * solar_ambient
            + lunar_ambient,
    );
    let split = celestial_light_split(
        receiver,
        solar_ambient,
        lunar_ambient,
        clear_intensity,
    );
    // Each light's sky takes that light's colour through the clear sky's
    // diffuse transfer at its height; starlight and airglow are white.
    let solar_sky = (1.0 - 0.85 * receiver.solar_eclipse_fraction) * solar_ambient;
    var lunar_sky = vec3<f32>(0.0);
    if lunar_ambient > 0.0 || EVALUATE_ALL_BRANCHES {
        let moon_color = moon_own_color();
        lunar_sky = lunar_ambient * dot(moon_color, LUMINANCE_WEIGHTS) * unit_luminance(
            moon_color * clear_sky_light_color(
                regional_aerosol_optical_depth,
                receiver.moon_direction.z,
            ),
        );
    }
    let sky_color = (
        vec3<f32>(0.035)
            + solar_sky * clear_sky_light_color(
                regional_aerosol_optical_depth,
                receiver.sun_direction.z,
            )
            + lunar_sky
    ) / (0.035 + solar_sky + lunar_ambient);
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
    // it. The canopy's own height sees each light above its much lower horizon,
    // which sets the beam that reaches the layer's underside.
    let canopy_receiver = celestial_receiver_light_at_height(WEATHER_CANOPY_HEIGHT_M);
    let daylight_beams = daylight_light(1.0) * vec2<f32>(1.0, CLEAR_SKY_LUNAR_WEIGHT * lunar_light_scale());
    let scattered_beams = canopy_scattered_beams(
        optical_thickness,
        canopy.sky_fraction,
        vec2<f32>(canopy_receiver.sun_direction.z, canopy_receiver.moon_direction.z),
        split.beam_shares * vec2<f32>(solar_ambient, lunar_ambient),
        celestial_light_split(canopy_receiver, 1.0, 1.0, 1.0).beam_shares * daylight_beams,
    );
    let transmitted_light = diffuse_transmittance * split.diffuse_share * clear_sky_light;
    let scattered_light = scattered_beams.x * beam_light_color(
            regional_aerosol_optical_depth,
            canopy_receiver.sun_direction.z,
        )
        + scattered_beams.y * moon_light_color(
            regional_aerosol_optical_depth,
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
// wavelength of its sRGB primary: 611, 549 and 464 nm.
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
// heights: light that reaches raised receivers and clouds after sea-level
// sunset grazes the surface and arrives fully reddened.
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

// Colour of a beam that reaches the ground through Rayleigh scattering,
// aerosol, and ozone, for the 550 nm aerosol optical depth and the sine of the
// light's height above the local horizontal.
fn beam_light_color(aerosol_optical_depth: f32, sine_height: f32) -> vec3<f32> {
    let optical_depth = relative_air_mass(sine_height)
        * (RAYLEIGH_OPTICAL_DEPTHS + aerosol_optical_depth * AEROSOL_SPECTRAL_SCALES)
        + ozone_air_mass(sine_height) * OZONE_OPTICAL_DEPTHS;
    return unit_luminance(exp(-optical_depth));
}

// Colour of the clear sky's light on level ground from a light at the given
// sine of its height above the local horizontal, by Bird and Riordan's (1986) diffuse
// irradiance: half of what Rayleigh scattering takes from the beam, with their
// exponent of 0.95 for multiple scattering, and the forward share of what
// aerosol scatters after 1.5 air masses of Rayleigh extinction, both under
// the ozone the beam crosses. Marine aerosol scatters nearly all it
// extinguishes, and its asymmetry parameter is 0.7.
fn clear_sky_light_color(aerosol_optical_depth: f32, sine_height: f32) -> vec3<f32> {
    let sine = clamp(sine_height, 0.0, 1.0);
    let air_mass = relative_air_mass(sine);
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
// (Cox 2000, Allen's Astrophysical Quantities), at unit luminance. Earth's
// umbra turns it orange-red.
fn moon_own_color() -> vec3<f32> {
    return mix(
        vec3<f32>(1.1108, 0.9855, 0.8175),
        vec3<f32>(1.20, 0.28, 0.06),
        smoothstep(0.05, 0.90, lunar_umbra_fraction),
    );
}

// Moonlight's colour after the atmosphere, keeping the luminance of the
// Moon's own colour.
fn moon_light_color(aerosol_optical_depth: f32, sine_height: f32) -> vec3<f32> {
    let own = moon_own_color();
    return dot(own, LUMINANCE_WEIGHTS)
        * unit_luminance(own * beam_light_color(aerosol_optical_depth, sine_height));
}

fn sun_direct_light_color(receiver: CelestialReceiverLight) -> vec3<f32> {
    return beam_light_color(local_aerosol_optical_depth, receiver.sun_direction.z);
}

// Share of a clear sky's light on level ground that arrives diffuse, for a
// light at the given sine of its height above the horizon, from the ESRA
// clear-sky model (Rigollier, Bauer and Wald 2000, Solar Energy 68): the beam
// crosses the relative air mass with Kasten's (1996) broadband Rayleigh optical
// depth, and diffuse light follows the model's Linke turbidity fits.
// For turbidity 3 it is 0.116 at 60 degrees, 0.183 at 30, 0.359 at 10 and
// 0.667 at 3, reaching 1 at the horizon as air mass extinguishes the beam.
// field_grids.rs mirrors this, `regional_linke_turbidity` and
// `MOON_DIRECT_SCALE` in its Moon cloud-shadow estimate; keep them synchronized.
fn clear_sky_diffuse_fraction(sine_height: f32, linke_turbidity: f32) -> f32 {
    let sine = clamp(sine_height, 0.0, 1.0);
    let air_mass = relative_air_mass(sine);
    let rayleigh_optical_depth = select(
        1.0 / (6.6296 + air_mass * (1.7513 + air_mass
            * (-0.1202 + air_mass * (0.0065 - 0.00013 * air_mass)))),
        1.0 / (10.4 + 0.718 * air_mass),
        air_mass > 20.0,
    );
    let beam = sine * exp(-0.8662 * linke_turbidity * air_mass * rayleigh_optical_depth);
    let turbidity = linke_turbidity;
    let diffuse_transmission = -0.015843 + turbidity * (0.030543 + 0.0003797 * turbidity);
    let a0 = max(
        0.26463 + turbidity * (-0.061581 + 0.0031408 * turbidity),
        0.002 / diffuse_transmission,
    );
    let a1 = 2.04020 + turbidity * (0.018945 - 0.011161 * turbidity);
    let a2 = -1.3025 + turbidity * (0.039231 + 0.0085079 * turbidity);
    let diffuse = diffuse_transmission * (a0 + sine * (a1 + a2 * sine));
    return diffuse / max(diffuse + beam, 1e-6);
}

// Linke turbidity of regional air from its aerosol optical depth, by Ineichen's
// (2008, Solar Energy 82) conversion at sea level with 1.5 cm of precipitable water.
fn regional_linke_turbidity() -> f32 {
    return 2.35 + 7.79 * regional_aerosol_optical_depth;
}

// Direct moonlight per unit of lunar light it takes from diffuse light,
// calibrated so full-Moon scenes keep the mean brightness night exposure and
// colour were tuned for, within 10%.
const MOON_DIRECT_SCALE: f32 = 1.5;
// Weight of the Moon's atmospheric light in calibrated clear-sky light;
// field_grids.rs mirrors it in its Moon cloud-shadow estimate.
const CLEAR_SKY_LUNAR_WEIGHT: f32 = 0.85;

// Share of a light's clear-sky light on level ground that arrives direct.
fn celestial_beam_share(
    receiver: CelestialReceiverLight,
    direction: vec3<f32>,
    angular_radius_rad: f32,
    linke_turbidity: f32,
) -> f32 {
    let visibility = celestial_disc_visibility_at_horizon(
        receiver.horizon_z,
        direction,
        angular_radius_rad,
    );
    if visibility <= 0.0 && !EVALUATE_ALL_BRANCHES {
        return 0.0;
    }
    return visibility * (1.0 - clear_sky_diffuse_fraction(
        light_height_above_horizon(direction.z, receiver.horizon_z),
        linke_turbidity,
    ));
}

struct CelestialLightSplit {
    // Sun and Moon direct light on level ground, and each per unit of the
    // light's clear-sky amount.
    direct: vec2<f32>,
    beam_shares: vec2<f32>,
    // Share of clear-sky light left diffuse.
    diffuse_share: f32,
};

// Splits the calibrated solar and lunar parts of clear-sky light into direct
// light on level ground and diffuse light by each light's clear-sky diffuse
// fraction. Under the Sun every height keeps its calibrated total; the Moon's
// direct light carries `MOON_DIRECT_SCALE` times the beam it takes from
// diffuse light.
fn celestial_light_split(
    receiver: CelestialReceiverLight,
    solar_ambient: f32,
    lunar_ambient: f32,
    clear_intensity: f32,
) -> CelestialLightSplit {
    let visibility = direct_light_visibility();
    let turbidity = regional_linke_turbidity();
    let beam_shares = visibility * vec2<f32>(
        (1.0 - receiver.solar_eclipse_fraction) * celestial_beam_share(
            receiver,
            receiver.sun_direction,
            CELESTIAL_DISC_ANGULAR_RADIUS_RAD,
            turbidity,
        ),
        celestial_beam_share(
            receiver,
            receiver.moon_direction,
            MOON_ANGULAR_RADIUS_RAD,
            turbidity,
        ),
    );
    let beams = beam_shares * vec2<f32>(solar_ambient, lunar_ambient);
    return CelestialLightSplit(
        beams * vec2<f32>(1.0, MOON_DIRECT_SCALE),
        beam_shares,
        max(1.0 - (beams.x + beams.y) / max(clear_intensity, 1e-6), 0.0),
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
    return moon_light_color(local_aerosol_optical_depth, receiver.moon_direction.z);
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
    return moon / max(sun + moon + clear_sky, 1e-9);
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
