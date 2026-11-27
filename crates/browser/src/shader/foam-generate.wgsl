// Whitecap events rendered into the foam grid each frame. Keep the laws
// synchronized with crates/browser/src/foam.rs, whose tests check them.

@group(1) @binding(0)
var<uniform> foam_frame: FoamUniforms;
@group(1) @binding(1)
var foam_forcing_output: texture_storage_2d_array<rgba16float, write>;
@group(1) @binding(2)
var foam_forcing_input: texture_2d_array<f32>;
@group(1) @binding(3)
var foam_forcing_sampler: sampler;
@group(1) @binding(4)
var foam_cell_output: texture_storage_2d_array<rg32uint, write>;
@group(1) @binding(5)
var<storage, read_write> foam_record_output: array<FoamEvent>;
@group(1) @binding(6)
var<storage, read_write> foam_record_count: atomic<u32>;
@group(1) @binding(7)
var foam_cells: texture_2d_array<u32>;
@group(1) @binding(14)
var<storage, read> foam_records: array<FoamEvent>;
// Each live event's reach: the centre and radius of the disc its foam
// stays within this frame, which tiles cull against before reading it.
@group(1) @binding(15)
var<storage, read_write> foam_reach_output: array<vec4<f32>>;
@group(1) @binding(16)
var<storage, read> foam_reaches: array<vec4<f32>>;
@group(1) @binding(8)
var foam_grid_output: texture_storage_2d<rgba16float, write>;
@group(1) @binding(9)
var foam_tile_output: texture_storage_2d<r32uint, write>;
@group(1) @binding(10)
var foam_grid_input: texture_2d<f32>;
@group(1) @binding(11)
var foam_tile_input: texture_2d<u32>;
@group(1) @binding(12)
var foam_shadow_output: texture_storage_2d<rgba8unorm, write>;
@group(1) @binding(13)
var foam_scene_tile_output: texture_storage_2d<r32uint, write>;

const FOAM_GRAVITY_M_S2: f32 = 9.81;
const FOAM_BREAKING_SPEED_RATIO: f32 = 0.85;
const FOAM_SPREAD_PER_FRICTION_VELOCITY: f32 = 4.6;
const FOAM_CUT_SPREADS: f32 = 3.0;
const FOAM_COVERAGE_PLATEAU: f32 = 0.04;
const FOAM_AREA_GROWTH_PER_S: f32 = 0.25;
const FOAM_SEGMENT_MEDIAN_M: f32 = 2.2045;
const FOAM_SEGMENT_LOG_DEVIATION: f32 = 0.6205;
const FOAM_LONGEST_SEGMENT_M: f32 = 15.0;
const FOAM_FIRST_ROW_CLASS: u32 = 3u;
const FOAM_ROW_LOG_DEVIATION: f32 = 0.5;
const FOAM_LONGEST_ROW_WAVELENGTHS: f32 = 3.0;
const FOAM_SEGMENT_SPACING_M: f32 = 5.4;
const FOAM_ROLLER_WAVELENGTHS: f32 = 0.275;
const FOAM_ACTIVE_MODULATION: f32 = 22.9;
const FOAM_ACTIVE_LAG_RAD: f32 = 0.24609142;
// Width of the soft margin about each candidate's acceptance, relative to it.
const FOAM_MARGIN: f32 = 0.25;
// The forced evaluation's wind, the strongest the coverage law is checked at.
const FOAM_FORCED_WIND_M_S: f32 = 36.0;
// Foam stands about 2 cm thick when fresh, within Reul and Chapron's (2003,
// J. Geophys. Res. 108) 1–4 cm, in proportion to its optical depth.
const FOAM_FRESH_THICKNESS_M: f32 = 0.02;
// The hole noise's finest octave.
const FOAM_FINEST_LACE_M: f32 = 0.02;
// A tile loads this many of the events reaching it at once. Compromises: a
// tile lists at most `FOAM_TILE_LIST` events, six times the most measured
// on any scene, and a frame lists at most `foam_frame.lace.z` live events.
const FOAM_EVENTS_PER_TILE: u32 = 128u;
const FOAM_TILE_LIST: u32 = 2048u;

// Gauss–Legendre nodes on [0, 1] and their weights.
const FOAM_ACTIVE_NODES = array<vec2<f32>, 8>(
    vec2<f32>(0.019855072, 0.050614268),
    vec2<f32>(0.101666761, 0.111190517),
    vec2<f32>(0.237233795, 0.156853323),
    vec2<f32>(0.408282679, 0.181341892),
    vec2<f32>(0.591717321, 0.181341892),
    vec2<f32>(0.762766205, 0.156853323),
    vec2<f32>(0.898333239, 0.111190517),
    vec2<f32>(0.980144928, 0.050614268),
);

fn foam_unit(hash: u32) -> f32 {
    return f32(hash >> 8u) / 16777216.0;
}

fn foam_friction_velocity_m_s(wind_m_s: f32) -> f32 {
    if wind_m_s <= 0.0 {
        return 0.0;
    }
    let constant = log(10.0 * FOAM_GRAVITY_M_S2 / (0.011 * 0.41 * 0.41 * wind_m_s * wind_m_s));
    var log_height = 10.0;
    for (var iteration = 0u; iteration < 4u; iteration++) {
        log_height = constant + 2.0 * log(log_height);
    }
    return 0.41 * wind_m_s / log_height;
}

fn foam_whitecap_coverage(wind_m_s: f32) -> f32 {
    if wind_m_s <= 0.0 {
        return 0.0;
    }
    return FOAM_COVERAGE_PLATEAU * tanh(3.84e-6 * pow(wind_m_s, 3.41) / FOAM_COVERAGE_PLATEAU);
}

fn foam_slowest_speed_m_s(kind: u32) -> f32 {
    return foam_frame.classes[kind].timing.w;
}

// The fastest breaking speed under a sustained wind: Kleiss and Melville's
// Rayleigh width, cut at three of it.
fn foam_fastest_speed_m_s(wind_m_s: f32) -> f32 {
    return FOAM_CUT_SPREADS * FOAM_SPREAD_PER_FRICTION_VELOCITY * foam_friction_velocity_m_s(wind_m_s);
}

// A class's means over events breaking from its slowest speed up to
// `top_m_s`, from its table, as `BinShape::means` gives their sums; zero
// where the wind breaks none of them.
fn foam_class_sums(kind: u32, top_m_s: f32) -> array<f32, 6> {
    var sums = array<f32, 6>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let slowest = foam_slowest_speed_m_s(kind);
    let octaves = log2(min(top_m_s, 2.0 * slowest) / slowest);
    if octaves <= 1e-6 {
        return sums;
    }
    let position = octaves * f32(FOAM_SHAPE_POINTS - 1u);
    let index = min(u32(position), FOAM_SHAPE_POINTS - 2u);
    let fraction = position - f32(index);
    for (var sum = 0u; sum < 6u; sum++) {
        let lower = foam_table(kind, 6u * index + sum);
        let upper = foam_table(kind, 6u * (index + 1u) + sum);
        sums[sum] = (lower + fraction * (upper - lower)) / octaves;
    }
    return sums;
}

fn foam_table(kind: u32, entry: u32) -> f32 {
    let row = foam_frame.classes[kind].table[entry / 4u];
    return row[entry % 4u];
}

// Each class's events per area and time under a sustained wind, as
// `event_rates_per_m2_s` in foam.rs gives them: the Rayleigh share of
// breaking-crest length over each event's crest length and duration,
// normalized to the coverage.
fn foam_rates(wind_m_s: f32) -> array<f32, FOAM_CLASSES> {
    let fastest = foam_fastest_speed_m_s(wind_m_s);
    let spread = fastest / FOAM_CUT_SPREADS;
    var rates: array<f32, FOAM_CLASSES>;
    var counted = 0.0;
    for (var kind = 0u; kind < FOAM_CLASSES; kind++) {
        let slow = min(foam_slowest_speed_m_s(kind), fastest);
        let fast = min(2.0 * foam_slowest_speed_m_s(kind), fastest);
        let share = select(
            0.0,
            exp(-0.5 * pow(slow / spread, 2.0)) - exp(-0.5 * pow(fast / spread, 2.0)),
            spread > 0.0,
        );
        let sums = foam_class_sums(kind, fastest);
        rates[kind] = select(0.0, share / sums[0], sums[0] > 0.0);
        counted += rates[kind] * sums[1];
    }
    let coverage = foam_whitecap_coverage(wind_m_s);
    for (var kind = 0u; kind < FOAM_CLASSES; kind++) {
        rates[kind] = select(0.0, coverage * rates[kind] / counted, counted > 0.0);
    }
    return rates;
}

struct FoamBreaker {
    speed_m_s: f32,
    wavelength_m: f32,
    period_s: f32,
}

fn foam_breaker(speed_m_s: f32) -> FoamBreaker {
    let phase_speed = speed_m_s / FOAM_BREAKING_SPEED_RATIO;
    return FoamBreaker(
        speed_m_s,
        TAU * phase_speed * phase_speed / FOAM_GRAVITY_M_S2,
        TAU * phase_speed / FOAM_GRAVITY_M_S2,
    );
}

fn foam_area_growth(age_s: f32) -> f32 {
    return 1.0 - exp(-FOAM_AREA_GROWTH_PER_S * age_s);
}

fn foam_dilation(age_s: f32, period_s: f32) -> f32 {
    return select(1.0, sqrt(foam_area_growth(age_s) / foam_area_growth(period_s)), age_s > period_s);
}

// The active patch's area after `u` of its breaking, and its first moment
// about the patch's rear, per unit of band times crest: a crest of fixed
// length sweeping a rectangle.
fn foam_swept_area(u: f32) -> f32 {
    return clamp(u, 0.0, 1.0);
}

fn foam_swept_moment(u: f32) -> f32 {
    let swept = clamp(u, 0.0, 1.0);
    return 0.5 * swept * swept;
}

// A candidate's draws: its acceptance threshold, birth within the slot,
// place in its cell, breaking speed within the class, then two for its
// crest length and its direction about the wind, and a spare. Compromise:
// a candidate's speed takes the fastest the wind breaks now, so a change
// of wind resizes a live event; the wind changes over minutes, the event
// lives under one.
fn foam_candidate_units(cell_hash: u32, candidate: u32) -> array<f32, 8> {
    var hash = hash32(cell_hash ^ (candidate * 0x27d4eb2fu));
    var units: array<f32, 8>;
    for (var index = 0u; index < 8u; index++) {
        units[index] = foam_unit(hash);
        hash = hash32(hash);
    }
    return units;
}

fn foam_cell_hash(kind: u32, cell: vec2<i32>, slot: i32) -> u32 {
    return hash32(
        noise_key(FOAM_NOISE_FIELD, kind)
            ^ (bitcast<u32>(cell.x) * 0x9e3779b1u)
            ^ (bitcast<u32>(cell.y) * 0x85ebca77u)
            ^ (bitcast<u32>(slot) * 0xc2b2ae3du),
    );
}

// Two independent standard normal draws from two uniform ones, by the
// Box–Muller transform.
fn foam_normals(first: f32, second: f32) -> vec2<f32> {
    let radius = sqrt(-2.0 * log(1.0 - first));
    return radius * vec2<f32>(cos(TAU * second), sin(TAU * second));
}

// A breaking segment's length from a standard normal draw, as
// `segment_length_m` in foam.rs: log-normal, held at the longest Gemmrich
// et al. saw.
fn foam_segment_m(normal: f32) -> f32 {
    return min(FOAM_SEGMENT_MEDIAN_M * exp(FOAM_SEGMENT_LOG_DEVIATION * normal), FOAM_LONGEST_SEGMENT_M);
}

// An event's crest: one breaking segment, or for the fastest classes the
// row of segments along its dominant crest, the wave's length times a
// log-normal factor of median 1, held at three wavelengths.
fn foam_crest_m(kind: u32, normal: f32, wavelength_m: f32) -> f32 {
    if kind < FOAM_FIRST_ROW_CLASS {
        return foam_segment_m(normal);
    }
    return min(exp(FOAM_ROW_LOG_DEVIATION * normal), FOAM_LONGEST_ROW_WAVELENGTHS) * wavelength_m;
}

// A breaker's direction about the wind, from a standard normal draw: the
// root-mean-square spread Kleiss and Melville (2010, J. Phys. Oceanogr.
// 40, p. 2585) measured of the breaking-crest length distribution, nearly
// 40° for small-scale breakers to 20° at the larger scales. Compromises:
// normal in angle, and the spread falls linearly in the logarithm of the
// breaking speed from 40° at 1 m/s to 20° at 10 m/s, the GOTEX range of
// their speeds.
fn foam_spread_rad(normal: f32, speed_m_s: f32) -> f32 {
    let scale = clamp(log(speed_m_s) / log(10.0), 0.0, 1.0);
    return radians(40.0 - 20.0 * scale) * normal;
}

// The forced evaluation's stand-in for every class drawn.
fn foam_class_drawn(kind: u32) -> bool {
    return foam_frame.classes[kind].place.w > 0.0 || foam_frame.flags.y != 0u;
}

// The sustained surface wind as the scene's waves take it, evaluated from
// the procedural atmosphere: the forcing grid reaches past the environment
// grid's window, as far as the cells it serves.
fn foam_wind_m_s(position_m: vec2<f32>, atmosphere: SurfaceAtmosphereSample) -> vec2<f32> {
    let wind = resolved_surface_wind_m_s_from_pressure(position_m, atmosphere.pressure);
    if foam_frame.flags.y == 0u {
        return wind;
    }
    let speed = length(wind);
    return select(vec2<f32>(FOAM_FORCED_WIND_M_S, 0.0), wind * FOAM_FORCED_WIND_M_S / speed, speed > 0.0);
}

// The surface water's drift beside the current, as `surface_drift_m_s` in
// foam.rs gives it.
fn foam_surface_drift_m_s(wind_m_s: vec2<f32>, latitude_degrees: f32) -> vec2<f32> {
    let speed = length(wind_m_s);
    if speed <= 0.0 {
        return vec2<f32>(0.0);
    }
    let direction = wind_m_s / speed;
    let peak_frequency = sqrt(FOAM_GRAVITY_M_S2 * wind_sea(speed).peak_wavenumber_per_m);
    let stokes = 2.0 * 0.0081 * FOAM_GRAVITY_M_S2 * 0.8572 / peak_frequency;
    let friction_rate = 0.018 / 1000.0;
    let rotation = 7.292e-5;
    let turn = atan2(2.0 * rotation * sin(radians(latitude_degrees)), friction_rate);
    let midlatitude = atan2(2.0 * rotation * sin(radians(45.0)), friction_rate);
    let angle = -radians(55.0) * turn / midlatitude;
    let drift = 0.006 * speed * vec2<f32>(
        cos(angle) * direction.x - sin(angle) * direction.y,
        sin(angle) * direction.x + cos(angle) * direction.y,
    );
    return stokes * direction + drift;
}

// The wind, then the water's surface drift with the current; and the
// sub-pixel foam of the classes the frame leaves out of the discrete
// events: its cover, its mean reflectance and squared diffuse
// transmittance times that cover, so they interpolate between texels as
// the sums they are, and its unscattered depth.
@compute @workgroup_size(8, 8)
fn generate_foam_forcing(@builtin(global_invocation_id) id: vec3<u32>) {
    let texels = u32(foam_frame.forcing.w);
    if any(id.xy >= vec2<u32>(texels)) {
        return;
    }
    let offset_m = foam_frame.forcing.xy + (vec2<f32>(id.xy) + 0.5) * foam_frame.forcing.z;
    let position_m = canonical_world_position(scene.camera_and_view_m.xy + offset_m);
    let fields = procedural_environment_field_sample(position_m);
    let atmosphere = surface_atmosphere_sample_from_noise(position_m, fields.atmosphere);
    let wind_m_s = foam_wind_m_s(position_m, atmosphere);
    let terrain = procedural_terrain_sample(position_m);
    let current_m_s = ocean_current_m_s_from_surface(
        position_m,
        terrain.cluster,
        terrain.elevation_m,
        atmosphere,
        fields.tide,
    );
    let drift_m_s = foam_surface_drift_m_s(wind_m_s, surface_latitude_degrees_at(position_m))
        + current_m_s;
    textureStore(foam_forcing_output, id.xy, 0, vec4<f32>(wind_m_s, drift_m_s));
    let speed = length(wind_m_s);
    let rates = foam_rates(speed);
    let fastest = foam_fastest_speed_m_s(speed);
    var cover = 0.0;
    var mean = vec3<f32>(0.0);
    for (var kind = 0u; kind < FOAM_CLASSES; kind++) {
        let sums = foam_class_sums(kind, fastest);
        let share = rates[kind] * sums[2] * (1.0 - foam_frame.classes[kind].place.w);
        cover += share;
        mean += share * vec3<f32>(sums[3], sums[4], sums[5]) / max(sums[2], 1e-30);
    }
    textureStore(
        foam_forcing_output,
        id.xy,
        1,
        vec4<f32>(
            cover,
            mean.xy,
            -log(max(mean.z / max(cover, 1e-20), 1e-20)) * (1.0 - FOAM_BUBBLE_ASYMMETRY) / 2.0,
        ),
    );
}

fn foam_forcing_at(offset_m: vec2<f32>) -> vec4<f32> {
    let uv = (offset_m - foam_frame.forcing.xy) / (foam_frame.forcing.z * foam_frame.forcing.w);
    return textureSampleLevel(foam_forcing_input, foam_forcing_sampler, uv, 0, 0.0);
}

// The drawn peak band at a cell: the wind sea's modes of the bands centred
// within a factor √2 of its peak wavenumber, deep and unfiltered, as the
// scene's waves weight them. Each mode's wavevector, frequency, steepness
// a k, and phase at the view centre. Compromise: where depth limits the
// waves, their deep-water amplitudes overstate the modulation.
const FOAM_PEAK_MODES: u32 = 18u;
var<workgroup> foam_modes: array<vec4<f32>, FOAM_PEAK_MODES>;
var<workgroup> foam_mode_phases: array<vec2<f32>, FOAM_PEAK_MODES>;
var<workgroup> foam_cell_records: atomic<u32>;
var<workgroup> foam_cell_first: u32;
var<workgroup> foam_cell_sums: array<f32, 12>;
var<workgroup> foam_cell_fastest: f32;
var<workgroup> foam_cell_acceptance: f32;

fn foam_store_peak_mode(index: u32, wind_m_s: vec2<f32>) {
    let speed = length(wind_m_s);
    let sea = wind_sea(speed);
    let peak = sea.peak_wavenumber_per_m;
    let first = i32(ceil(2.0 * log2(peak / WAVE_LOWEST_WAVENUMBER_PER_M) - 1.5));
    let band = first + i32(index / WAVE_VISITED_DIRECTIONS);
    let centre = WAVE_LOWEST_WAVENUMBER_PER_M * exp2((f32(band) + 0.5) / WAVE_BANDS_PER_OCTAVE);
    let in_band = band >= 0 && band < WAVE_BAND_COUNT && speed > 0.0
        && abs(log2(centre / peak)) <= 0.5;
    let window = WAVE_GAMEPLAY_WINDOW * peak;
    let edges = WAVE_LOWEST_WAVENUMBER_PER_M
        * exp2((f32(band) + vec2<f32>(0.0, 1.0)) / WAVE_BANDS_PER_OCTAVE);
    let lower = max(edges.x, window.x);
    let upper = min(edges.y, window.y);
    let ratio = vec2<f32>(lower, upper) / peak;
    let variance = wind_sea_band_variance_m2(sea, ratio);
    let table_band = u32(clamp(band, 0, WAVE_BAND_COUNT - 1));
    let spread = wind_sea_band_spread(sea, ratio);
    let spacing = TAU / f32(WAVE_DIRECTIONS);
    let window_width = 0.5 * f32(WAVE_VISITED_DIRECTIONS) * spacing;
    let rotation = scene.ocean_wave_band_rotations[table_band / 4u][table_band % 4u];
    let direction_position = (atan2(wind_m_s.y, wind_m_s.x) - rotation) / spacing;
    let nearest = floor(direction_position);
    let offset = direction_position - nearest;
    var weight = 0.0;
    var own = 0.0;
    for (var direction = 0u; direction < WAVE_VISITED_DIRECTIONS; direction++) {
        let step = f32(direction) + 1.0 - 0.5 * f32(WAVE_VISITED_DIRECTIONS);
        let angle = (step - offset) * spacing;
        let root = wind_sea_direction_root(angle, spread, window_width);
        weight += root * root;
        if direction == index % WAVE_VISITED_DIRECTIONS {
            own = root;
        }
    }
    let step = f32(index % WAVE_VISITED_DIRECTIONS) + 1.0 - 0.5 * f32(WAVE_VISITED_DIRECTIONS);
    let table_direction = u32(i32(nearest) + i32(step)) & (WAVE_DIRECTIONS - 1u);
    let mode = scene.ocean_wave_table[table_band * WAVE_DIRECTIONS + table_direction];
    let amplitude_m = select(0.0, sqrt(2.0 * variance / max(weight, 1e-20)) * own, in_band);
    foam_modes[index] = vec4<f32>(mode.xy, sqrt(FOAM_GRAVITY_M_S2 * mode.z), amplitude_m * mode.z);
    foam_mode_phases[index] = vec2<f32>(mode.w, 0.0);
}

// The onset modulation's phase and strength that give active coverage
// Dulov et al.'s (2021, Remote Sens. 13, 2825) modulation of active
// whitecaps, Q₀ exp(M ε sin(Φ - Φ₀)) with M = 22.9 and Φ₀ = 14.1° behind
// each dominant crest, for a breaker on the peak band whose
// steepness-weighted mean frequency, wavenumber along the breaker's travel,
// their variances and covariance are `band`. While the crest breaks for a
// period T, its swept band grows and its centroid ξ(t) advances, and each
// dominant wave's phase there falls by ωt - kξ. The active band's
// area-weighted first harmonic of the band's signal there, g e^{-iΔΦ}, with
// each wave's lag spread normally about the mean, sets the onset's phase
// Φ₀ - ΔΦ and strength M/g. Onsets then sit about 33–48° ahead of the
// dominant crest, on its forward face, for active coverage to peak where
// Dulov et al. measured it: this is the game's own kinematics, chosen for
// that observable; in their model the breaking source sits near the crest.
// Compromises: their seas were younger than the fully developed seas here,
// every breaking speed takes the one modulation though they measured
// breakers faster than 1.25 m/s, and the undrawn swell modulates none.
fn foam_onset_modulation(breaker: FoamBreaker, band: array<f32, 5>) -> vec2<f32> {
    let band_m = breaker.speed_m_s * breaker.period_s;
    var weight = 0.0;
    var harmonic = vec2<f32>(0.0);
    for (var node = 0u; node < 8u; node++) {
        let u = FOAM_ACTIVE_NODES[node].x;
        let area = foam_swept_area(u);
        let time_s = breaker.period_s * u;
        let centroid_m = band_m * foam_swept_moment(u) / area;
        let lag = band[0] * time_s - band[1] * centroid_m;
        let spread = band[2] * time_s * time_s + band[3] * centroid_m * centroid_m
            - 2.0 * band[4] * time_s * centroid_m;
        let weighted = FOAM_ACTIVE_NODES[node].y * area;
        weight += weighted;
        harmonic += weighted * exp(-0.5 * spread) * vec2<f32>(cos(lag), sin(lag));
    }
    return vec2<f32>(
        FOAM_ACTIVE_LAG_RAD - atan2(harmonic.y, harmonic.x),
        FOAM_ACTIVE_MODULATION * weight / length(harmonic),
    );
}

// A candidate's acceptance at its rate times its modulation: linear while
// small, saturating smoothly at one. Saturating sooner, as 1 - exp(-x)
// does, would flatten the modulation of the long breakers, whose onsets
// are modulated most strongly.
fn foam_acceptance(x: f32) -> f32 {
    return x * pow(1.0 + x * x * x * x, -0.25);
}

fn foam_acceptance_slope(x: f32) -> f32 {
    return pow(1.0 + x * x * x * x, -1.25);
}

// The soft margin's mean weight for acceptance `q`, and its slope.
fn foam_margin_acceptance(q: f32) -> vec2<f32> {
    let reach = 1.0 + 0.5 * FOAM_MARGIN;
    let excess = reach * q - 1.0;
    if excess <= 0.0 {
        return vec2<f32>(q, 1.0);
    }
    return vec2<f32>(
        q - excess * excess / (2.0 * FOAM_MARGIN * q),
        1.0 - (reach * reach * q * q - 1.0) / (2.0 * FOAM_MARGIN * q * q),
    );
}

// The rate for which candidates kept with `foam_acceptance`, under the soft
// margin, are kept with mean `mean_acceptance` over a log-normal modulation
// of these moments: a three-point Gauss–Hermite mean and two Newton steps.
fn foam_candidate_rate(mean_acceptance: f32, log_mean: f32, log_deviation: f32) -> f32 {
    var rate = mean_acceptance;
    for (var step = 0u; step < 2u; step++) {
        var value = -mean_acceptance;
        var slope = 0.0;
        for (var node = 0u; node < 3u; node++) {
            let z = 1.7320508 * (f32(node) - 1.0);
            let weight = select(1.0 / 6.0, 2.0 / 3.0, node == 1u);
            let modulation = exp(log_mean + log_deviation * z);
            let margin = foam_margin_acceptance(foam_acceptance(rate * modulation));
            value += weight * margin.x;
            slope += weight * margin.y * foam_acceptance_slope(rate * modulation) * modulation;
        }
        rate -= value / slope;
    }
    return rate;
}

// Each cell's candidate events of the current and previous slot, each kept
// with a probability set by its class's rate at the cell's wind and thinned
// by where its birth falls on the drawn dominant waves, so a live event
// neither moves nor pops as they advance; and under a soft margin about
// that probability, so a candidate the wind no longer keeps fades over
// seconds. The live ones join the frame's list of events, which the cell's
// texel points into, carried by the water's drift beyond the view centre's.
// Compromise: a real breaker would not un-break; the margin stands in for
// the wind at its birth, which a stateless lookup cannot afford.
@compute @workgroup_size(128)
fn generate_foam_cells(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) candidate: u32,
) {
    let layer = workgroup.z;
    let kind = layer / 2u;
    let previous = layer % 2u;
    let lattice = foam_frame.classes[kind];
    if !foam_class_drawn(kind) || any(vec2<i32>(workgroup.xy) >= lattice.window.zw) {
        return;
    }
    if candidate == 0u {
        atomicStore(&foam_cell_records, 0u);
    }
    let relative = lattice.window.xy + vec2<i32>(workgroup.xy);
    let cell = vec2<i32>(positive_mod(lattice.lattice.x + relative.x, lattice.lattice.z), lattice.lattice.y + relative.y);
    let slot = lattice.lattice.w - i32(previous);
    let cell_m = lattice.place.z;
    let centre_m = (vec2<f32>(relative) + 0.5 - lattice.place.xy) * cell_m;
    let forcing = foam_forcing_at(centre_m);
    let wind_m_s = forcing.xy;
    let speed = length(wind_m_s);
    if candidate < FOAM_PEAK_MODES {
        foam_store_peak_mode(candidate, wind_m_s);
    }
    workgroupBarrier();
    // The band's steepness-weighted moments and its steepness's power sums
    // for the normalization's series, then the cell's mean acceptance and
    // spreading, once for the cell.
    if candidate == 0u {
        var sums = array<f32, 12>(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for (var index = 0u; index < FOAM_PEAK_MODES; index++) {
            let mode = foam_modes[index];
            let a2 = mode.w * mode.w;
            sums[0] += a2 * mode.z;
            sums[1] += a2 * mode.z * mode.z;
            sums[2] += a2 * mode.x;
            sums[3] += a2 * mode.y;
            sums[4] += a2 * mode.x * mode.x;
            sums[5] += a2 * mode.x * mode.y;
            sums[6] += a2 * mode.y * mode.y;
            sums[7] += a2 * mode.z * mode.x;
            sums[8] += a2 * mode.z * mode.y;
            sums[9] += a2;
            sums[10] += a2 * a2;
            sums[11] += a2 * a2 * a2;
        }
        foam_cell_sums = sums;
        foam_cell_fastest = foam_fastest_speed_m_s(speed);
        foam_cell_acceptance = foam_rates(speed)[kind] * cell_m * cell_m * lattice.timing.y
            / f32(FOAM_CANDIDATES);
    }
    let sums = workgroupUniformLoad(&foam_cell_sums);
    let powers = vec3<f32>(sums[9], sums[10], sums[11]);
    let mean_acceptance = foam_cell_acceptance;
    let wind_angle = atan2(wind_m_s.y, wind_m_s.x);
    let units = foam_candidate_units(foam_cell_hash(kind, cell, slot), candidate);
    // Speeds spread evenly in their logarithm from the class's slowest up to
    // the fastest the cell's wind breaks, within the class.
    let slowest = foam_slowest_speed_m_s(kind);
    let top = clamp(foam_cell_fastest, slowest, 2.0 * slowest);
    let breaker = foam_breaker(slowest * pow(top / slowest, units[4]));
    let normals = foam_normals(units[5], units[6]);
    let age_s = lattice.timing.x + f32(previous) * lattice.timing.y - units[1] * lattice.timing.y;
    // A class fading into its unresolved mean keeps that share of its
    // events, each under a soft margin that narrows as the share nears all,
    // so the mean holds through the fade and a fully drawn class keeps
    // every event.
    let seed = hash32(foam_cell_hash(kind, cell, slot) ^ candidate);
    let drawn = select(lattice.place.w, 1.0, foam_frame.flags.y != 0u);
    let margin = max(min(FOAM_MARGIN * drawn, 2.0 * (1.0 - drawn)), 1e-6);
    let fading = clamp((drawn - foam_unit(hash32(seed))) / margin + 0.5, 0.0, 1.0);
    // Candidates unborn, faded, or faded out with their class are not drawn
    // this frame, and need no modulation.
    let alive = fading > 0.0 && age_s >= 0.0 && age_s <= breaker.period_s + foam_frame.grid.w;
    // Compromise: the direction follows the cell's wind as it is now, so a
    // live event turns in place when the wind turns, over minutes against
    // its life of under one.
    let direction_rad = wind_angle + foam_spread_rad(normals.y, breaker.speed_m_s);
    let direction = vec2<f32>(cos(direction_rad), sin(direction_rad));
    let offset_m = (vec2<f32>(relative) + vec2<f32>(units[2], units[3]) - lattice.place.xy) * cell_m;
    var weight = 0.0;
    if alive {
        let total = max(powers.x, 1e-20);
        let frequency = sums[0] / total;
        let along = (sums[2] * direction.x + sums[3] * direction.y) / total;
        let along_squared = (sums[4] * direction.x * direction.x
            + 2.0 * sums[5] * direction.x * direction.y
            + sums[6] * direction.y * direction.y) / total;
        let band = array<f32, 5>(
            frequency,
            along,
            sums[1] / total - frequency * frequency,
            along_squared - along * along,
            (sums[7] * direction.x + sums[8] * direction.y) / total - frequency * along,
        );
        let onset = foam_onset_modulation(breaker, band);
        let strength = select(onset.y, 0.0, powers.x <= 0.0);
        // Σ ln I₀(M aₖ) by its series to the sixth power, within 3e-4 a mode
        // for M a up to 1.
        let strength_squared = strength * strength;
        let log_mean = -(strength_squared * powers.x / 4.0
            - strength_squared * strength_squared * powers.y / 64.0
            + strength_squared * strength_squared * strength_squared * powers.z / 576.0);
        let log_deviation = strength * sqrt(0.5 * powers.x);
        let rate = foam_candidate_rate(mean_acceptance, log_mean, log_deviation);
        // Where the candidate broke on the drawn waves, by the water's slow
        // motion alone: its orbital surge, which swings by about a wave's
        // amplitude each period, would pulse its modulation through its life.
        let birth_m = offset_m - foam_frame.water.xy - foam_frame.water.zw * age_s;
        var projected = 0.0;
        for (var mode_index = 0u; mode_index < FOAM_PEAK_MODES; mode_index++) {
            let mode = foam_modes[mode_index];
            let phase = dot(mode.xy, birth_m) + foam_mode_phases[mode_index].x + mode.z * age_s;
            projected += mode.w * cos(phase + onset.x);
        }
        let q = foam_acceptance(rate * exp(strength * projected + log_mean));
        weight = select(clamp((q - units[0]) / (FOAM_MARGIN * q) + 0.5, 0.0, 1.0), 0.0, q <= 0.0);
    }
    let kept = weight * fading;
    let live = kept > 0.0;
    var event: FoamEvent;
    var index = 0u;
    if live {
        // The water beyond the view centre's carries it, clamped to the
        // cell's margin for drift.
        let drift_m = (forcing.zw - foam_forcing_at(vec2<f32>(0.0)).zw) * age_s;
        let carried_m = drift_m * min(1.0, lattice.timing.z / max(length(drift_m), 1e-6));
        event = FoamEvent(
            offset_m + carried_m,
            direction,
            age_s,
            breaker.speed_m_s,
            breaker.period_s,
            foam_crest_m(kind, normals.x, breaker.wavelength_m),
            kept,
            seed,
        );
        index = atomicAdd(&foam_cell_records, 1u);
    }
    workgroupBarrier();
    if candidate == 0u {
        let count = atomicLoad(&foam_cell_records);
        foam_cell_first = atomicAdd(&foam_record_count, count);
        textureStore(
            foam_cell_output,
            vec2<i32>(workgroup.xy),
            layer,
            vec4<u32>(foam_cell_first, count, 0u, 0u),
        );
    }
    let first = workgroupUniformLoad(&foam_cell_first);
    if live && first + index < u32(foam_frame.lace.z) {
        foam_record_output[first + index] = event;
        let extent = foam_event_extent(event);
        foam_reach_output[first + index] = vec4<f32>(extent.xy, length(extent.zw), 0.0);
    }
}

// A live event as a tile sees it: where it broke now that the water has
// carried it, the direction it travels, its age, breaking speed, period and
// crest length (a row's, for the fastest classes), its weight, and its
// seed for the hole noise and the row's segments.
struct FoamEvent {
    position_m: vec2<f32>,
    direction: vec2<f32>,
    age_s: f32,
    speed_m_s: f32,
    period_s: f32,
    crest_m: f32,
    weight: f32,
    seed: u32,
}

var<workgroup> foam_tile_events: array<FoamEvent, FOAM_EVENTS_PER_TILE>;
var<workgroup> foam_tile_extents: array<vec4<f32>, FOAM_EVENTS_PER_TILE>;
var<workgroup> foam_tile_list: array<u32, FOAM_TILE_LIST>;
var<workgroup> foam_tile_event_count: atomic<u32>;
var<workgroup> foam_tile_foam: atomic<u32>;

// One octave of the hole noise and its gradient: Perlin's gradient noise
// with his quintic fade, on a lattice of half the octave's wavelength,
// turned and shifted by the octave's seed, of standard normal marginal.
fn foam_lace_octave(seed: u32, octave: u32, wavenumber: f32, point: vec2<f32>) -> vec3<f32> {
    var hash = hash32(seed ^ (octave * 0x9e3779b1u));
    let angle = TAU * foam_unit(hash);
    hash = hash32(hash);
    let turn = mat2x2<f32>(cos(angle), sin(angle), -sin(angle), cos(angle));
    let scale = wavenumber / PI;
    let lattice = turn * point * scale + 64.0 * vec2<f32>(foam_unit(hash), foam_unit(hash32(hash)));
    let corner = floor(lattice);
    let inside = lattice - corner;
    var gradients: array<vec2<f32>, 4>;
    var values: array<f32, 4>;
    for (var index = 0u; index < 4u; index++) {
        let step = vec2<f32>(f32(index % 2u), f32(index / 2u));
        let cell = vec2<i32>(corner + step);
        let theta = TAU * foam_unit(hash32(hash ^ (bitcast<u32>(cell.x) * 0x8da6b343u)
            ^ (bitcast<u32>(cell.y) * 0xd8163841u)));
        gradients[index] = vec2<f32>(cos(theta), sin(theta));
        values[index] = dot(gradients[index], inside - step);
    }
    let fade = inside * inside * inside * (inside * (inside * 6.0 - 15.0) + 10.0);
    let fade_slope = 30.0 * inside * inside * (inside - 1.0) * (inside - 1.0);
    let value = mix(mix(values[0], values[1], fade.x), mix(values[2], values[3], fade.x), fade.y);
    let blended = mix(mix(gradients[0], gradients[1], fade.x), mix(gradients[2], gradients[3], fade.x), fade.y);
    let cross = values[0] - values[1] - values[2] + values[3];
    let slope = blended + fade_slope * vec2<f32>(
        values[1] - values[0] + fade.y * cross,
        values[2] - values[0] + fade.x * cross,
    );
    // Mapped to a normal marginal, which the thresholds assume, by a fit
    // to the quantiles of the noise's own, within 0.025 over its central
    // 98%; and the gradient back in the point's frame, per metre.
    let raw = 4.645 * value;
    let raw_squared = raw * raw;
    let normal = raw * (0.944 + raw_squared * (0.00103 + 0.0054 * raw_squared));
    let stretch = 0.944 + raw_squared * (0.00309 + 0.027 * raw_squared);
    return vec3<f32>(normal, 4.645 * stretch * scale * (transpose(turn) * slope));
}

// The scene's pixel footprint, which sets the detail foam draws, so every
// tier draws the same.
fn foam_pixel_m() -> f32 {
    return foam_frame.grid.z / f32(max(foam_frame.flags.z, 1u));
}

// Φ(x) in the logistic form of the hyperbolic tangent, within 2e-4.
fn foam_normal_cdf(x: f32) -> f32 {
    return 0.5 + 0.5 * tanh(0.7978846 * (x + 0.044715 * x * x * x));
}

// Φ⁻¹(p), through Giles's (2010) single-precision approximation of the
// inverse error function, within 4e-7 of erfinv.
fn foam_normal_quantile(p: f32) -> f32 {
    let x = clamp(2.0 * p - 1.0, -0.9999999, 0.9999999);
    var w = -log((1.0 - x) * (1.0 + x));
    var q = 0.0;
    if w < 5.0 {
        w -= 2.5;
        q = 2.81022636e-08;
        q = 3.43273939e-07 + q * w;
        q = -3.5233877e-06 + q * w;
        q = -4.39150654e-06 + q * w;
        q = 0.00021858087 + q * w;
        q = -0.00125372503 + q * w;
        q = -0.00417768164 + q * w;
        q = 0.246640727 + q * w;
        q = 1.50140941 + q * w;
    } else {
        w = sqrt(w) - 3.0;
        q = -0.000200214257;
        q = 0.000100950558 + q * w;
        q = 0.00134934322 + q * w;
        q = -0.00367342844 + q * w;
        q = 0.00573950773 + q * w;
        q = -0.0076224613 + q * w;
        q = 0.00943887047 + q * w;
        q = 1.00167406 + q * w;
        q = 2.83297682 + q * w;
    }
    return 1.41421356 * q * x;
}

// The share of a texel `half_width` either side of `centre` that the span
// from `low` to `high` covers.
fn foam_span_cover(centre: f32, half_width: f32, low: f32, high: f32) -> f32 {
    return max(min(centre + half_width, high) - max(centre - half_width, low), 0.0)
        / (2.0 * half_width);
}

// Named tricks for how an event's foam looks, judged against the varied
// reference set rather than measured; each moves foam within its patch
// without changing its expected cover. The hole noise is stretched this
// many times along the travel, so trails streak.
const FOAM_STREAK_STRETCH: f32 = 2.0;
// A single segment's edges are warped by fractal noise, four octaves from
// one cycle a size, each half the last, the first halved, moving them this
// share of the band along the travel and of the crest across it, and at
// most `FOAM_WARP_REACH` of them. Keep the reach synchronized with
// `WARP_REACH` in foam.rs.
const FOAM_WARP_SHARE: f32 = 0.1;
const FOAM_WARP_REACH: f32 = 0.45;
// A slice's edges soften over up to this share of its half-width as its
// drawn share falls.
const FOAM_LONGEST_TAPER: f32 = 0.6;
// The foam's surface undulates with the hole noise by this many metres per
// unit of the noise, so even solid foam shows relief.
const FOAM_UNDULATION_M: f32 = 0.001;
// Single segments spread across their travel with this diffusivity. Keep
// synchronized with `LATERAL_DIFFUSIVITY_M2_S` in foam.rs.
const FOAM_LATERAL_DIFFUSIVITY_M2_S: f32 = 4.1;

// The integral from 0 to `x` of a profile of height 1 that falls linearly
// to 0 over `ramp` inside `half_width`.
fn foam_profile_integral(x: f32, half_width: f32, ramp: f32) -> f32 {
    let flat = half_width - ramp;
    let reach = abs(x);
    let ramped = clamp(reach - flat, 0.0, ramp);
    return sign(x) * (clamp(reach, 0.0, flat) + ramped - ramped * ramped / (2.0 * max(ramp, 1e-6)));
}

// That profile's mean over a texel `half_texel` either side of `centre`.
fn foam_profile_cover(centre: f32, half_texel: f32, half_width: f32, ramp: f32) -> f32 {
    return (foam_profile_integral(centre + half_texel, half_width, ramp)
        - foam_profile_integral(centre - half_texel, half_width, ramp)) / (2.0 * half_texel);
}

// A slice of a patch breaking at `speed_m_s` for `period_s` and `age_s`
// old, a texel at `along_m` behind its front's start: the share of the
// texel it sweeps, the share of the slice its drawn filaments cover, and
// the foam's age there, from its release.
fn foam_slice(along_m: f32, half_texel: f32, speed_m_s: f32, period_s: f32, age_s: f32) -> vec3<f32> {
    let band_m = speed_m_s * period_s;
    let along_cover = foam_span_cover(along_m, half_texel, 0.0, speed_m_s * min(age_s, period_s));
    let roller_m = FOAM_ROLLER_WAVELENGTHS * band_m / FOAM_BREAKING_SPEED_RATIO;
    let release_s = min((max(along_m, 0.0) + roller_m) / speed_m_s, period_s);
    let point_age_s = max(age_s - release_s, 0.0);
    if along_cover <= 0.0 || point_age_s > foam_frame.grid.w {
        return vec3<f32>(0.0, 0.0, point_age_s);
    }
    let depth = FOAM_FRESH_OPTICAL_DEPTH * exp(-point_age_s / FOAM_OPTICAL_DEPTH_EFOLD_S);
    let fresh = FOAM_FRESH_OPTICAL_DEPTH / (1.0 + FOAM_FRESH_OPTICAL_DEPTH);
    return vec3<f32>(along_cover, depth / (1.0 + depth) / fresh, point_age_s);
}

// The share a slice of a crest `length_m` wide covers over a texel
// `across_m` from its centre: the slice keeps its cover, `drawn` of its
// width, while it spreads across the travel to `width_m` and its edges
// soften as that share falls.
fn foam_slice_share(across_m: f32, half_texel: f32, length_m: f32, width_m: f32, drawn: f32) -> f32 {
    let taper = min(FOAM_LONGEST_TAPER, 1.0 - drawn);
    let half_width = 0.5 * width_m;
    return drawn * length_m / width_m / (1.0 - 0.5 * taper)
        * foam_profile_cover(across_m, half_texel, half_width, taper * half_width);
}

// A breaking segment's length from a uniform draw, through the normal
// quantile rather than Box–Muller, as foam_segment_m takes a normal one.
fn foam_segment_quantile_m(unit: f32) -> f32 {
    return foam_segment_m(foam_normal_quantile(clamp(unit, 1e-4, 1.0 - 1e-4)));
}

// The rectangle beyond which an event draws nothing: its centre, then its
// half-lengths along and across its travel. It holds its swept band, its
// slices' spread or its dilation, and its warped edges.
fn foam_event_extent(event: FoamEvent) -> vec4<f32> {
    let band_m = event.speed_m_s * event.period_s;
    let row = event.speed_m_s >= foam_frame.classes[FOAM_FIRST_ROW_CLASS].timing.w;
    let widest_m = select(
        sqrt(event.crest_m * event.crest_m + 2.0 * FOAM_LATERAL_DIFFUSIVITY_M2_S * event.age_s),
        event.crest_m,
        row,
    );
    let dilation = select(1.0, foam_dilation(event.age_s, event.period_s), row);
    let warp = select(FOAM_WARP_REACH, 0.0, row);
    return vec4<f32>(
        event.position_m + 0.5 * band_m * event.direction,
        dilation * (0.5 + warp) * band_m,
        dilation * (0.5 * widest_m + warp * event.crest_m),
    );
}

// Whether `offset_m` lies beyond an event's `extent`, by more than half a
// texel.
fn foam_beyond(event: FoamEvent, extent: vec4<f32>, offset_m: vec2<f32>) -> bool {
    let relative = offset_m - extent.xy;
    let half_texel = 0.5 * foam_frame.grid.z;
    return abs(dot(relative, event.direction)) > extent.z + half_texel
        || abs(event.direction.x * relative.y - event.direction.y * relative.x) > extent.w + half_texel;
}

// The foam an event leaves at `offset_m` from the view centre: the share
// its filaments cover, their optical depth, and their relief's gradient. A
// crest of fixed length, or a row, sweeps at its breaking speed for one
// period. The roller riding its front, Duncan's breaking region, carries
// fresh foam forward and releases it behind, where it stays and ages from
// its release; at the period's end the roller stops and releases what it
// carries. A single segment's slices spread across the travel by
// diffusion; a row dilates about its centre after breaking as Koepke's
// a(t). Each slice is drawn as thick filaments over the share that keeps
// its mean reflectance; where they lie behind the roller follows a
// threshold of the hole noise, streaked along the travel, set so the
// expected cover holds, its octaves under two pixels left to the closed
// form Φ; the roller itself is solid. A row's roller is its breaking
// segments, of Gemmrich et al.'s lengths, while behind it the segments'
// foam merges into one trail at the row's filled share. Compromises: the
// patch's rear stays where breaking began, though Kleiss and Melville saw
// it ride forward too at first; and segments keep their length while
// breaking, Gemmrich et al.'s mean over it, rather than growing and
// shrinking.
fn foam_event_cover(event: FoamEvent, offset_m: vec2<f32>) -> vec4<f32> {
    if foam_beyond(event, foam_event_extent(event), offset_m) {
        return vec4<f32>(0.0);
    }
    let band_m = event.speed_m_s * event.period_s;
    let relative = offset_m - event.position_m;
    let across = vec2<f32>(-event.direction.y, event.direction.x);
    let row = event.speed_m_s >= foam_frame.classes[FOAM_FIRST_ROW_CLASS].timing.w;
    let dilation = select(1.0, foam_dilation(event.age_s, event.period_s), row);
    var point = vec2<f32>(
        0.5 * band_m + (dot(relative, event.direction) - 0.5 * band_m) / dilation,
        dot(relative, across) / dilation,
    );
    let half_texel = 0.5 * foam_frame.grid.z / dilation;
    // Tear a single segment's edges along the curl of fractal noise in its
    // own scales, which moves foam without gathering or spreading it.
    let scale = vec2<f32>(band_m, event.crest_m);
    let unit_point = point / scale;
    var warp = vec2<f32>(0.0);
    for (var octave = 0u; octave < select(4u, 0u, row); octave++) {
        let wavenumber = TAU * exp2(f32(octave));
        if wavenumber * 0.5 * foam_pixel_m() / dilation > min(scale.x, scale.y) {
            break;
        }
        let noise = foam_lace_octave(event.seed ^ 0x51ed270bu, octave, wavenumber, unit_point);
        let amplitude = select(exp2(-f32(octave)), 0.5, octave == 0u);
        warp += amplitude * vec2<f32>(noise.z, -noise.y) / wavenumber;
    }
    point += FOAM_WARP_SHARE * scale * warp;
    let slice = foam_slice(point.x, half_texel, event.speed_m_s, event.period_s, event.age_s);
    let in_roller = slice.x > 0.0 && slice.z <= 0.0 && event.age_s < event.period_s;
    var share = 0.0;
    var solid = 0.0;
    if row {
        let place_m = point.y + 0.5 * event.crest_m;
        if in_roller && 2.0 * half_texel < FOAM_SEGMENT_SPACING_M {
            // The breaking segments: none, one or two in each cell along the
            // row, so a texel looks back as far as the longest reaches. They
            // cover their union as independent placements would, which is
            // how foam.rs tables the row's filled share.
            var bare = 1.0;
            let first_cell = i32(floor((place_m - half_texel - FOAM_LONGEST_SEGMENT_M) / FOAM_SEGMENT_SPACING_M));
            let last_cell = i32(floor((place_m + half_texel) / FOAM_SEGMENT_SPACING_M));
            for (var cell = first_cell; cell <= last_cell; cell++) {
                var hash = hash32(event.seed ^ (bitcast<u32>(cell) * 0x632be5abu));
                let count = select(select(2u, 1u, foam_unit(hash) < 0.75), 0u, foam_unit(hash) < 0.25);
                for (var index = 0u; index < count; index++) {
                    hash = hash32(hash);
                    let start_m = (f32(cell) + foam_unit(hash)) * FOAM_SEGMENT_SPACING_M;
                    hash = hash32(hash);
                    let length_m = foam_segment_quantile_m(foam_unit(hash));
                    bare *= 1.0 - foam_span_cover(
                        place_m, half_texel, max(start_m, 0.0), min(start_m + length_m, event.crest_m),
                    );
                }
            }
            solid = slice.x * (1.0 - bare);
        } else {
            share = slice.x * slice.y * foam_frame.lace.w
                * foam_span_cover(place_m, half_texel, 0.0, event.crest_m);
            if in_roller {
                solid = share;
                share = 0.0;
            }
        }
    } else {
        let width_m = sqrt(event.crest_m * event.crest_m
            + 2.0 * FOAM_LATERAL_DIFFUSIVITY_M2_S * slice.z);
        share = slice.x * foam_slice_share(point.y, half_texel, event.crest_m, width_m, slice.y);
        if in_roller {
            solid = share;
            share = 0.0;
        }
    }
    if share <= 0.0 && solid <= 0.0 {
        return vec4<f32>(0.0);
    }
    // The hole noise, streaked along the travel.
    let longest_m = min(0.25 * band_m / FOAM_BREAKING_SPEED_RATIO, foam_frame.lace.x);
    let octaves = max(floor(log2(longest_m / FOAM_FINEST_LACE_M)) + 1.0, 1.0);
    let octave_variance = 1.0 / octaves;
    let streaked = vec2<f32>(point.x / FOAM_STREAK_STRETCH, point.y);
    var noise = vec3<f32>(0.0);
    var resolved = 0.0;
    // The three coarsest octaves spanning two pixels are drawn; finer ones
    // join the closed form.
    for (var octave = 0u; octave < 3u; octave++) {
        let wavelength_m = longest_m * exp2(-f32(octave));
        if f32(octave) >= octaves || wavelength_m * dilation < 2.0 * foam_pixel_m() {
            break;
        }
        noise += sqrt(octave_variance) * foam_lace_octave(event.seed, octave, TAU / wavelength_m, streaked);
        resolved += 1.0;
    }
    let spread = sqrt(max((octaves - resolved) * octave_variance, 1e-4));
    // Behind the roller the filaments lie where the noise passes the
    // quantile that gives the expected share.
    var covered = solid;
    var density = 0.0;
    if share > 0.0 {
        let z = (noise.x - foam_normal_quantile(1.0 - min(share, 1.0))) / spread;
        covered = foam_normal_cdf(z);
        density = 0.3989423 * exp(-0.5 * z * z);
    }
    covered *= event.weight;
    let slope = FOAM_FRESH_THICKNESS_M * event.weight * density / spread
        + FOAM_UNDULATION_M * covered;
    let gradient = slope
        * (noise.y / FOAM_STREAK_STRETCH * event.direction + noise.z * across) / dilation;
    return vec4<f32>(covered, FOAM_FRESH_OPTICAL_DEPTH, gradient);
}

// Walks a thread's share of the tile's cells and slots, each class's that
// reach the tile, culling their records against it and listing those that
// reach it.
fn foam_tile_walk(thread: u32, tile_minimum_m: vec2<f32>, tile_maximum_m: vec2<f32>) {
    let texel_m = foam_frame.grid.z;
    for (var kind = 0u; kind < FOAM_CLASSES; kind++) {
        let lattice = foam_frame.classes[kind];
        let first = vec2<i32>(floor(lattice.place.xy + tile_minimum_m / lattice.place.z - 0.5));
        let last = vec2<i32>(floor(lattice.place.xy + tile_maximum_m / lattice.place.z - 0.5)) + 1;
        let span = last - first + 1;
        let entries = select(0u, u32(2 * span.x * span.y), foam_class_drawn(kind));
        for (var entry = thread; entry < entries; entry += 256u) {
            let cell = i32(entry / 2u);
            let window = first + vec2<i32>(cell % span.x, cell / span.x) - lattice.window.xy;
            if all(window >= vec2<i32>(0)) && all(window < lattice.window.zw) {
                let range = textureLoad(foam_cells, window, 2u * kind + entry % 2u, 0).xy;
                let end = min(range.x + range.y, u32(foam_frame.lace.z));
                for (var record = range.x; record < end; record++) {
                    let reach = foam_reaches[record];
                    if distance(clamp(reach.xy, tile_minimum_m, tile_maximum_m), reach.xy)
                        <= reach.z + texel_m {
                        let slot = atomicAdd(&foam_tile_event_count, 1u);
                        if slot < FOAM_TILE_LIST {
                            foam_tile_list[slot] = record;
                        }
                    }
                }
            }
        }
    }
}

// The foam grid's covered share, optical depth and relief at each texel,
// from the events of the cells it looks up; and whether each tile holds
// foam. A tile lists the events that reach it once, then loads them in
// batches, each texel combining theirs: overlapping events cover the union
// of their shares, and where their filaments overlap their optical depths
// add.
@compute @workgroup_size(16, 16)
fn generate_foam_events(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(local_invocation_index) thread: u32,
) {
    if thread == 0u {
        atomicStore(&foam_tile_event_count, 0u);
        atomicStore(&foam_tile_foam, 0u);
    }
    workgroupBarrier();
    let texel_m = foam_frame.grid.z;
    let tile_minimum_m = foam_frame.grid.xy + vec2<f32>(workgroup.xy * FOAM_TILE_TEXELS) * texel_m;
    let tile_maximum_m = tile_minimum_m + f32(FOAM_TILE_TEXELS) * texel_m;
    foam_tile_walk(thread, tile_minimum_m, tile_maximum_m);
    let total = min(workgroupUniformLoad(&foam_tile_event_count), FOAM_TILE_LIST);
    if total == 0u {
        if thread == 0u {
            textureStore(foam_tile_output, vec2<i32>(workgroup.xy), vec4<u32>(0u));
        }
        return;
    }
    // Each thread draws a square of four texels.
    var covers: array<vec4<f32>, 4>;
    for (var corner = 0u; corner < 4u; corner++) {
        covers[corner] = vec4<f32>(1.0, 0.0, 0.0, 0.0);
    }
    for (var batch = 0u; batch < total; batch += FOAM_EVENTS_PER_TILE) {
        let events = min(total - batch, FOAM_EVENTS_PER_TILE);
        if thread < events {
            let event = foam_records[foam_tile_list[batch + thread]];
            foam_tile_events[thread] = event;
            foam_tile_extents[thread] = foam_event_extent(event);
        }
        workgroupBarrier();
        for (var corner = 0u; corner < 4u; corner++) {
            let texel = vec2<i32>(workgroup.xy * FOAM_TILE_TEXELS + 2u * local.xy
                + vec2<u32>(corner % 2u, corner / 2u));
            let offset_m = foam_frame.grid.xy + (vec2<f32>(texel) + 0.5) * texel_m;
            for (var index = 0u; index < events; index++) {
                // Most of a tile's events miss most of its texels.
                if foam_beyond(foam_tile_events[index], foam_tile_extents[index], offset_m) {
                    continue;
                }
                let event = foam_event_cover(foam_tile_events[index], offset_m);
                covers[corner].x *= 1.0 - event.x;
                covers[corner].y += event.x * event.y;
                covers[corner].z += event.z;
                covers[corner].w += event.w;
            }
        }
        workgroupBarrier();
    }
    for (var corner = 0u; corner < 4u; corner++) {
        let texel = vec2<i32>(workgroup.xy * FOAM_TILE_TEXELS + 2u * local.xy
            + vec2<u32>(corner % 2u, corner / 2u));
        let covered = 1.0 - covers[corner].x;
        if all(texel < vec2<i32>(foam_frame.grid_texels.xy)) {
            textureStore(
                foam_grid_output,
                texel,
                // Where the share rounds to nothing, so does the depth.
                vec4<f32>(
                    covered,
                    select(0.0, covers[corner].y / covered, covered > 1e-6),
                    covers[corner].zw,
                ),
            );
            if covered > 0.0 {
                atomicOr(&foam_tile_foam, 1u);
            }
        }
    }
    let foamy = workgroupUniformLoad(&foam_tile_foam);
    if thread == 0u {
        textureStore(foam_tile_output, vec2<i32>(workgroup.xy), vec4<u32>(foamy));
    }
}

var<workgroup> foam_neighbour_tiles: array<u32, 9>;
var<workgroup> foam_neighbour_foam: atomic<u32>;

// The share of a texel foam covers and its filaments' optical depth.
fn foam_cover_at(texel: vec2<i32>, tile: vec2<i32>) -> vec2<f32> {
    let neighbour = texel / i32(FOAM_TILE_TEXELS) - tile + vec2<i32>(1);
    let inside = all(texel >= vec2<i32>(0)) && all(texel < vec2<i32>(foam_frame.grid_texels.xy))
        && all(neighbour >= vec2<i32>(0)) && all(neighbour < vec2<i32>(3));
    if !inside || foam_neighbour_tiles[neighbour.y * 3 + neighbour.x] == 0u {
        return vec2<f32>(0.0);
    }
    return textureLoad(foam_grid_input, texel, 0).xy;
}

// The share of a light's glint that foam within its short shadow passes:
// marching toward the light, a ray from the water's surface runs through
// the share of each texel foam covers where its filaments stand above it,
// as tall as their optical depth, of extinction τ*₀ / ((1 - g) thickness)
// per metre.
fn foam_shadow(texel: vec2<i32>, tile: vec2<i32>, light: vec4<f32>) -> f32 {
    let horizontal = length(light.xy);
    if light.w < 0.5 || horizontal <= 0.0 {
        return 1.0;
    }
    let texel_m = foam_frame.grid.z;
    let toward = light.xy / horizontal;
    let rise = max(light.z, 0.0) / horizontal;
    let extinction = FOAM_FRESH_OPTICAL_DEPTH / ((1.0 - FOAM_BUBBLE_ASYMMETRY) * FOAM_FRESH_THICKNESS_M);
    var path_m = 0.0;
    for (var step = 1u; step <= 4u; step++) {
        let distance_texels = f32(step);
        if distance_texels > light.w + 0.5 {
            break;
        }
        let foam = foam_cover_at(texel + vec2<i32>(round(toward * distance_texels)), tile);
        let height_m = FOAM_FRESH_THICKNESS_M * foam.y / FOAM_FRESH_OPTICAL_DEPTH;
        path_m += select(0.0, foam.x * texel_m, height_m > distance_texels * texel_m * rise);
    }
    return exp(-extinction * path_m);
}

// Which tiles the scene reads foam and shadows from, and the short shadows
// foam casts on the glint beside it, within four texels toward each light,
// where they reach half a texel.
@compute @workgroup_size(16, 16)
fn generate_foam_shadows(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(local_invocation_index) thread: u32,
) {
    let tile = vec2<i32>(workgroup.xy);
    if thread == 0u {
        atomicStore(&foam_neighbour_foam, 0u);
    }
    workgroupBarrier();
    if thread < 9u {
        let neighbour = tile + vec2<i32>(i32(thread % 3u) - 1, i32(thread / 3u) - 1);
        let inside = all(neighbour >= vec2<i32>(0))
            && all(neighbour < vec2<i32>(foam_frame.grid_texels.zw));
        var foamy = 0u;
        if inside {
            foamy = textureLoad(foam_tile_input, neighbour, 0).x;
        }
        foam_neighbour_tiles[thread] = foamy;
        atomicOr(&foam_neighbour_foam, foamy);
    }
    let any_foam = workgroupUniformLoad(&foam_neighbour_foam) != 0u;
    let reaches = max(foam_frame.sun.w, foam_frame.moon.w) >= 0.5 || foam_frame.flags.y != 0u;
    let shadows = any_foam && reaches;
    if thread == 0u {
        textureStore(
            foam_scene_tile_output,
            tile,
            vec4<u32>(foam_neighbour_tiles[4] | select(0u, 2u, shadows)),
        );
    }
    if !shadows {
        return;
    }
    for (var corner = 0u; corner < 4u; corner++) {
        let texel = tile * i32(FOAM_TILE_TEXELS) + 2 * vec2<i32>(local.xy)
            + vec2<i32>(i32(corner % 2u), i32(corner / 2u));
        if all(texel < vec2<i32>(foam_frame.grid_texels.xy)) {
            textureStore(
                foam_shadow_output,
                texel,
                vec4<f32>(
                    foam_shadow(texel, tile, foam_frame.sun),
                    foam_shadow(texel, tile, foam_frame.moon),
                    0.0,
                    1.0,
                ),
            );
        }
    }
}
