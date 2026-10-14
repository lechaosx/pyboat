// Copyright (c) 2021 Advanced Micro Devices, Inc. All rights reserved.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
// THE SOFTWARE.

// Spatial upscaling ported from AMD FidelityFX Super Resolution 1: EASU
// edge-adaptive upsampling, then RCAS sharpening at output resolution.
// Unfiltered loads accept every sampleable color format. Sources hold
// display-encoded values, which is the space both filters are tuned for.

@group(0) @binding(0) var source: texture_2d<f32>;

struct UpscaleVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) source_uv: vec2<f32>,
};

@vertex
fn fullscreen_vertex(@builtin(vertex_index) index: u32) -> UpscaleVertex {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var output: UpscaleVertex;
    output.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    output.source_uv = uv;
    return output;
}

fn load_source(texel: vec2<i32>) -> vec3<f32> {
    let last = vec2<i32>(textureDimensions(source)) - 1;
    return textureLoad(source, clamp(texel, vec2<i32>(0), last), 0).rgb;
}

fn easu_luma(color: vec3<f32>) -> f32 {
    return color.b * 0.5 + (color.r * 0.5 + color.g);
}

// Weighted gradient direction and edge strength around tap c:
//   a
// b c d
//   e
fn easu_edge(weight: f32, a: f32, b: f32, c: f32, d: f32, e: f32) -> vec3<f32> {
    let direction_x = d - b;
    let length_x = saturate(abs(direction_x) / max(max(abs(d - c), abs(c - b)), 1.0e-12));
    let direction_y = e - a;
    let length_y = saturate(abs(direction_y) / max(max(abs(e - c), abs(c - a)), 1.0e-12));
    return vec3<f32>(direction_x, direction_y, length_x * length_x + length_y * length_y) * weight;
}

// Rotated, stretched Lanczos-2 approximation; returns the weighted color and weight.
fn easu_tap(
    color: vec3<f32>,
    offset: vec2<f32>,
    direction: vec2<f32>,
    stretch: vec2<f32>,
    lobe: f32,
    clip: f32,
) -> vec4<f32> {
    let rotated = vec2<f32>(
        dot(offset, direction),
        dot(offset, vec2<f32>(-direction.y, direction.x)),
    ) * stretch;
    let distance_squared = min(dot(rotated, rotated), clip);
    let base = 0.4 * distance_squared - 1.0;
    let window = lobe * distance_squared - 1.0;
    let weight = (1.5625 * base * base - 0.5625) * window * window;
    return vec4<f32>(color * weight, weight);
}

@fragment
fn easu(input: UpscaleVertex) -> @location(0) vec4<f32> {
    let position = input.source_uv * vec2<f32>(textureDimensions(source)) - 0.5;
    let origin = floor(position);
    let fraction = position - origin;
    let f_texel = vec2<i32>(origin);
    //   b c
    // e f g h
    // i j k l
    //   n o
    let b = load_source(f_texel + vec2<i32>(0, -1));
    let c = load_source(f_texel + vec2<i32>(1, -1));
    let e = load_source(f_texel + vec2<i32>(-1, 0));
    let f = load_source(f_texel);
    let g = load_source(f_texel + vec2<i32>(1, 0));
    let h = load_source(f_texel + vec2<i32>(2, 0));
    let i = load_source(f_texel + vec2<i32>(-1, 1));
    let j = load_source(f_texel + vec2<i32>(0, 1));
    let k = load_source(f_texel + vec2<i32>(1, 1));
    let l = load_source(f_texel + vec2<i32>(2, 1));
    let n = load_source(f_texel + vec2<i32>(0, 2));
    let o = load_source(f_texel + vec2<i32>(1, 2));
    let x = fraction.x;
    let y = fraction.y;
    let edge = easu_edge((1.0 - x) * (1.0 - y), easu_luma(b), easu_luma(e), easu_luma(f), easu_luma(g), easu_luma(j))
        + easu_edge(x * (1.0 - y), easu_luma(c), easu_luma(f), easu_luma(g), easu_luma(h), easu_luma(k))
        + easu_edge((1.0 - x) * y, easu_luma(f), easu_luma(i), easu_luma(j), easu_luma(k), easu_luma(n))
        + easu_edge(x * y, easu_luma(g), easu_luma(j), easu_luma(k), easu_luma(l), easu_luma(o));
    let direction_squared = dot(edge.xy, edge.xy);
    let direction = select(
        edge.xy * inverseSqrt(direction_squared),
        vec2<f32>(1.0, edge.y),
        direction_squared < 1.0 / 32768.0,
    );
    let edge_length = 0.25 * edge.z * edge.z;
    let axis_stretch = dot(direction, direction) / max(abs(direction.x), abs(direction.y));
    let stretch = vec2<f32>(1.0 + (axis_stretch - 1.0) * edge_length, 1.0 - 0.5 * edge_length);
    let lobe = 0.5 + (0.25 - 0.04 - 0.5) * edge_length;
    let clip = 1.0 / lobe;
    let sum = easu_tap(b, vec2<f32>(0.0, -1.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(c, vec2<f32>(1.0, -1.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(e, vec2<f32>(-1.0, 0.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(f, -fraction, direction, stretch, lobe, clip)
        + easu_tap(g, vec2<f32>(1.0, 0.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(h, vec2<f32>(2.0, 0.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(i, vec2<f32>(-1.0, 1.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(j, vec2<f32>(0.0, 1.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(k, vec2<f32>(1.0, 1.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(l, vec2<f32>(2.0, 1.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(n, vec2<f32>(0.0, 2.0) - fraction, direction, stretch, lobe, clip)
        + easu_tap(o, vec2<f32>(1.0, 2.0) - fraction, direction, stretch, lobe, clip);
    // Clamping to the nearest four taps removes ringing.
    let minimum = min(min(f, g), min(j, k));
    let maximum = max(max(f, g), max(j, k));
    return vec4<f32>(clamp(sum.rgb / sum.w, minimum, maximum), 1.0);
}

// Stops below the strongest sharpening: enough to restore edge contrast lost in
// upsampling while limiting how much faint night grain is amplified.
// Pixels whose cross neighbourhood reaches 1 or more pass through unsharpened.
const RCAS_SHARPNESS_STOPS: f32 = 0.2;
const RCAS_LIMIT: f32 = 0.25 - 1.0 / 16.0;

@fragment
fn rcas(input: UpscaleVertex) -> @location(0) vec4<f32> {
    //   b
    // d e f
    //   h
    let center = vec2<i32>(input.position.xy);
    let b = load_source(center + vec2<i32>(0, -1));
    let d = load_source(center + vec2<i32>(-1, 0));
    let e = load_source(center);
    let f = load_source(center + vec2<i32>(1, 0));
    let h = load_source(center + vec2<i32>(0, 1));
    let ring_minimum = min(min(b, d), min(f, h));
    let ring_maximum = max(max(b, d), max(f, h));
    // The largest negative lobe that keeps the result inside [0, 1] per channel.
    let hit_minimum = min(ring_minimum, e) / max(4.0 * ring_maximum, vec3<f32>(1.0e-12));
    let hit_maximum = (1.0 - max(ring_maximum, e)) / min(4.0 * ring_minimum - 4.0, vec3<f32>(-1.0e-12));
    let channel_lobe = max(-hit_minimum, hit_maximum);
    let lobe = max(-RCAS_LIMIT, min(max(channel_lobe.r, max(channel_lobe.g, channel_lobe.b)), 0.0))
        * exp2(-RCAS_SHARPNESS_STOPS);
    return vec4<f32>((lobe * (b + d + f + h) + e) / (4.0 * lobe + 1.0), 1.0);
}
