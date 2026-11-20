@group(1) @binding(0)
var<uniform> wave_tile_generation: WaveTileUniforms;
@group(1) @binding(1)
var<storage, read_write> wave_tile_columns: array<vec2<f32>>;
// The tiles' texels, layer by layer and row by row, each slope's two
// components as half floats, which a copy then moves into the scene's
// texture: WebGPU stores no two-channel half-float texture.
@group(1) @binding(2)
var<storage, read_write> wave_tile_packed_slopes: array<u32>;

const WAVE_TILE_COLUMNS: u32 = u32(2 * WAVE_TILE_REACH + 1);
// One radix-4 butterfly per invocation for the longest tile.
const WAVE_TILE_WORKGROUP: u32 = 128u;
const GRAVITY_M_S2: f32 = 9.81;
// Seawater's surface tension over its density, 0.0728 N/m over 1025 kg/m³.
const CAPILLARY_M3_S2: f32 = 7.1e-5;

// Two halves, each one FFT's length, that the stages pass the transform
// between.
var<workgroup> wave_tile_fft: array<vec2<f32>, 1024>;

fn complex_product(left: vec2<f32>, right: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(left.x * right.x - left.y * right.y, left.x * right.y + left.y * right.x);
}

// Directional spreading at `angle_rad` from the wind for waves of
// `frequency_ratio` times the peak's angular frequency: Donelan, Hamilton
// and Hui's (1985, Phil. Trans. R. Soc. A 315) sech²(βθ), with Banner's (1990,
// J. Phys. Oceanogr. 20) β above 1.6 times the peak, which levels off at
// 10^-0.4 for the shortest waves, normalized over the circle. Keep
// synchronized with `along_wind_share` in crates/browser/src/wave_tiles.rs.
fn wave_tile_spreading(angle_rad: f32, frequency_ratio: f32) -> f32 {
    let banner = exp2(
        3.3219281 * (-0.4 + 0.8393 * exp(-0.567 * log(frequency_ratio * frequency_ratio))),
    );
    let beta = select(banner, 2.28 * pow(frequency_ratio, -1.3), frequency_ratio < 1.6);
    let sech = 1.0 / cosh(beta * angle_rad);
    return 0.5 * beta / tanh(beta * PI) * sech * sech;
}

// A layer's tile spans this many texels, which the texture repeats.
fn wave_tile_texels(layer: u32) -> u32 {
    return u32(wave_tile_generation.layers[layer].lattice.w);
}

// The complex amplitude at the clock of the wave of wavenumber `cycles`, in
// cycles over the tile, in `layer`, whose two bands each hold an octave of
// unit slope variance: Phillips' saturation range, a slope density of
// D(θ) / (ln 2 k²) over wavenumber, filtered by the footprint and raised by
// the mean loss of bilinear interpolation between texels. Each wave travels
// at its capillary-gravity frequency, rounded to whole cycles of the clock's
// loop. Compromises: the spreading and the peak are those of the sustained
// wind at the view centre, whose own wind sets the tiles' amplitude and the
// slopes they leave unresolved; one sea-level footprint filters the whole
// frame; the interpolation loss is made up on average, so texel centres
// carry slightly more slope variance than the points between them; and the
// short waves repeat every loop.
fn wave_tile_component(layer: u32, cycles: vec2<i32>, key: u32) -> vec2<f32> {
    let tile = wave_tile_generation.layers[layer].tile;
    let spacing = TAU / tile.x;
    let wavenumber = vec2<f32>(cycles) * spacing;
    let k = length(wavenumber);
    let texels = wave_tile_texels(layer);
    let nyquist = i32(texels / 2u);
    // Each octave reaches half an octave either side of its wavelength.
    let lowest = TAU * f32(1u << (2u * layer)) / (1.4142135 * DECORATIVE_WATER_WAVELENGTH_M);
    if k < lowest || k >= 4.0 * lowest || max(abs(cycles.x), abs(cycles.y)) >= nyquist {
        return vec2<f32>(0.0);
    }
    let sea = wave_tile_generation.sea;
    let clock = wave_tile_generation.clock;
    let frequency = sqrt(GRAVITY_M_S2 * k + CAPILLARY_M3_S2 * k * k * k);
    let angle = atan2(
        sea.x * wavenumber.y - sea.y * wavenumber.x,
        dot(sea.xy, wavenumber),
    );
    let texel_phase = TAU / f32(texels) * vec2<f32>(cycles);
    let interpolation = (2.0 + cos(texel_phase.x)) * (2.0 + cos(texel_phase.y)) / 9.0;
    let filtered = k * clock.z;
    let slope_variance = wave_tile_spreading(angle, frequency / sea.z) / (0.6931472 * k * k)
        * spacing * spacing * exp(-filtered * filtered) / interpolation;
    let hashed = hash32(
        key ^ (u32(cycles.x) * LATTICE_KEY_MULTIPLIERS[0]) ^ (u32(cycles.y) * LATTICE_KEY_MULTIPLIERS[1]),
    );
    let loop_cycles = round(frequency * clock.y / TAU);
    let phase = TAU * fract(f32(hashed) * 2.3283064e-10 - loop_cycles * clock.x / clock.y);
    return sqrt(2.0 * slope_variance) / k * vec2<f32>(cos(phase), sin(phase));
}

// The tile's slope along x plus i times its slope along y at `cycles`: each
// wave's height, made real by pairing it with the wave of opposite
// wavenumber, times i k.
fn wave_tile_spectrum(layer: u32, cycles: vec2<i32>, key: u32) -> vec2<f32> {
    let forward = wave_tile_component(layer, cycles, key);
    let backward = wave_tile_component(layer, -cycles, key);
    let height = 0.5 * (forward + vec2<f32>(backward.x, -backward.y));
    let wavenumber = vec2<f32>(cycles) * TAU / wave_tile_generation.layers[layer].tile.x;
    return vec2<f32>(
        -wavenumber.x * height.y - wavenumber.y * height.x,
        wavenumber.x * height.x - wavenumber.y * height.y,
    );
}

// The inverse transform of the first `texels` of `wave_tile_fft`'s first
// half by Stockham's autosort, in radix-4 stages and a last radix-2 stage
// when `texels` is an odd power of two, each invocation taking one
// butterfly a stage; returns the half that holds the result, in natural
// order.
fn wave_tile_inverse_fft(invocation: u32, texels: u32) -> u32 {
    var source = 0u;
    var span = texels;
    var stride = 1u;
    let stages = countTrailingZeros(texels);
    for (var stage = 0u; stage < stages / 2u; stage++) {
        let quarter = span / 4u;
        let block = invocation / stride;
        let lane = invocation % stride;
        let destination = WAVE_TILE_TEXELS - source;
        if invocation < texels / 4u {
            let read = source + lane + stride * block;
            let first = wave_tile_fft[read];
            let second = wave_tile_fft[read + stride * quarter];
            let third = wave_tile_fft[read + 2u * stride * quarter];
            let fourth = wave_tile_fft[read + 3u * stride * quarter];
            let even_sum = first + third;
            let even_difference = first - third;
            let odd_sum = second + fourth;
            let odd_difference = second - fourth;
            let rotated = vec2<f32>(-odd_difference.y, odd_difference.x);
            let angle = TAU * f32(block) / f32(span);
            let twiddle = vec2<f32>(cos(angle), sin(angle));
            let twiddle_squared = complex_product(twiddle, twiddle);
            let write = destination + lane + 4u * stride * block;
            wave_tile_fft[write] = even_sum + odd_sum;
            wave_tile_fft[write + stride] = complex_product(even_difference + rotated, twiddle);
            wave_tile_fft[write + 2u * stride] = complex_product(even_sum - odd_sum, twiddle_squared);
            wave_tile_fft[write + 3u * stride] = complex_product(
                even_difference - rotated,
                complex_product(twiddle_squared, twiddle),
            );
        }
        workgroupBarrier();
        source = destination;
        span = quarter;
        stride *= 4u;
    }
    if stages % 2u == 1u {
        let destination = WAVE_TILE_TEXELS - source;
        for (var lane = invocation; lane < stride; lane += WAVE_TILE_WORKGROUP) {
            let first = wave_tile_fft[source + lane];
            let second = wave_tile_fft[source + lane + stride];
            wave_tile_fft[destination + lane] = first + second;
            wave_tile_fft[destination + lane + stride] = first - second;
        }
        workgroupBarrier();
        source = destination;
    }
    return source;
}

fn wave_tile_signed_cycles(index: u32, texels: u32) -> i32 {
    return select(i32(index), i32(index) - i32(texels), index >= texels / 2u);
}

// One column of wavenumbers along x, transformed along y.
@compute @workgroup_size(128)
fn generate_wave_tile_columns(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) invocation: u32,
) {
    let layer = workgroup.y;
    let column = workgroup.x;
    let texels = wave_tile_texels(layer);
    if abs(i32(column) - WAVE_TILE_REACH) >= i32(texels / 2u) {
        return;
    }
    let key = noise_key(DECORATIVE_WATER_NOISE_FIELD, layer);
    for (var part = 0u; part < 4u; part++) {
        let index = invocation + part * WAVE_TILE_WORKGROUP;
        if index < texels {
            wave_tile_fft[index] = wave_tile_spectrum(
                layer,
                vec2<i32>(i32(column) - WAVE_TILE_REACH, wave_tile_signed_cycles(index, texels)),
                key,
            );
        }
    }
    workgroupBarrier();
    let result = wave_tile_inverse_fft(invocation, texels);
    for (var part = 0u; part < 4u; part++) {
        let index = invocation + part * WAVE_TILE_WORKGROUP;
        if index < texels {
            wave_tile_columns[(layer * WAVE_TILE_COLUMNS + column) * WAVE_TILE_TEXELS + index] =
                wave_tile_fft[result + index];
        }
    }
}

// One row of texels, the columns transformed along x, written along the row.
@compute @workgroup_size(128)
fn generate_wave_tile_texel_rows(
    @builtin(workgroup_id) workgroup: vec3<u32>,
    @builtin(local_invocation_index) invocation: u32,
) {
    let layer = workgroup.y;
    let row = workgroup.x;
    let texels = wave_tile_texels(layer);
    if row >= texels {
        return;
    }
    for (var part = 0u; part < 4u; part++) {
        let index = invocation + part * WAVE_TILE_WORKGROUP;
        let cycles = wave_tile_signed_cycles(index, texels);
        var value = vec2<f32>(0.0);
        if index < texels && abs(cycles) <= WAVE_TILE_REACH && abs(cycles) < i32(texels / 2u) {
            value = wave_tile_columns[
                (layer * WAVE_TILE_COLUMNS + u32(cycles + WAVE_TILE_REACH)) * WAVE_TILE_TEXELS + row
            ];
        }
        wave_tile_fft[index] = value;
    }
    workgroupBarrier();
    let result = wave_tile_inverse_fft(invocation, texels);
    // The texture repeats the tile, so its texels hold whole tiles at any
    // tile length.
    let copies = WAVE_TILE_TEXELS / texels;
    for (var part = 0u; part < 4u; part++) {
        let index = invocation + part * WAVE_TILE_WORKGROUP;
        if index < texels {
            let slope = pack2x16float(wave_tile_fft[result + index]);
            for (var copy = 0u; copy < copies * copies; copy++) {
                let texel_row = row + texels * (copy / copies);
                wave_tile_packed_slopes[(layer * WAVE_TILE_TEXELS + texel_row) * WAVE_TILE_TEXELS
                    + index + texels * (copy % copies)] = slope;
            }
        }
    }
}
