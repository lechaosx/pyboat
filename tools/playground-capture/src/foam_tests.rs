use super::*;
use browser::{swept_area, swept_moment};
use wgpu::util::DeviceExt;

/// Deep open water whose sustained wind at the view centre is about 10, 20
/// and 27 m/s, with that wind's seconds on the world clock. Across a
/// kilometres-wide view the wind ranges widely about it.
const WINDY_SEAS: [([f32; 2], f64); 3] = [
    ([6000.0, 2000.0], 4_585_200.0),
    ([-30_000.0, 18_000.0], 5_958_960.0),
    ([-6000.0, 14_000.0], 9_164_400.0),
];
/// Frames further apart than any class's slot, so each draws new events.
const FRAME_SECONDS: f64 = 97.0;

/// A live event as the cell pass lists it: `FoamEvent` in
/// foam-generate.wgsl.
#[derive(Clone, Copy, Debug)]
struct Event {
    position_m: [f32; 2],
    direction: [f32; 2],
    age_s: f32,
    speed_m_s: f32,
    period_s: f32,
    crest_m: f32,
    weight: f32,
}

/// What the foam passes left for a frame.
struct FoamFrame {
    uniforms: FoamUniforms,
    grid: Vec<u8>,
    tiles: Vec<u8>,
    forcing: Vec<u8>,
    events: Vec<Event>,
}

struct FoamRig {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    foam: Foam,
}

impl FoamRig {
    fn new() -> Self {
        let (_, device, queue) =
            pollster::block_on(hardware_device(wgpu::Features::empty())).unwrap();
        let layout = uniform_layout(&device, "foam test uniforms");
        let foam = Foam::new(&device, &layout);
        Self {
            device,
            queue,
            layout,
            foam,
        }
    }

    /// The scene's state for `options`, the footprint and the foam's state.
    fn view(&mut self, options: &Options) -> (wgpu::BindGroup, FoamUniforms) {
        let quality = options.quality;
        self.foam.resize(
            &self.device,
            (options.width, options.height),
            quality.foam_texel_pixels(),
        );
        let uniforms = scene_uniforms(
            options,
            &capture_terrain_cache_frame(options, &mut TerrainCache::default()).unwrap(),
            &BakedSurfaceFrame::direct(),
        )
        .unwrap();
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let scene = bind_group(&self.device, &self.layout, &buffer, "foam test scene");
        let footprint_m = visual_pixel_footprint_m(options.view_height_m, options.height);
        let options = Options {
            foam: true,
            ..options.clone()
        };
        let foam = capture_foam_uniforms(&options, footprint_m, &self.foam, false).unwrap();
        (scene, foam)
    }

    fn run(&self, scene: &wgpu::BindGroup, uniforms: FoamUniforms) -> FoamFrame {
        assert!(uniforms.active());
        self.foam.write(&self.queue, &uniforms);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            self.foam.dispatch(&mut pass, scene, &uniforms);
        }
        self.queue.submit([encoder.finish()]);
        let [grid, tiles, forcing] = self.foam.readback_textures().map(|texture| {
            let encoder = self.device.create_command_encoder(&Default::default());
            read_texture(&self.device, &self.queue, encoder, texture).unwrap()
        });
        let [records, count] = self.foam.readback_events().map(|buffer| self.read(buffer));
        let count = u32::from_le_bytes(count[..4].try_into().unwrap()) as usize;
        let float = |bytes: &[u8; 40], index: usize| {
            f32::from_le_bytes(bytes[4 * index..4 * index + 4].try_into().unwrap())
        };
        let events = records
            .as_chunks::<40>()
            .0
            .iter()
            .take(count)
            .map(|record| Event {
                position_m: [float(record, 0), float(record, 1)],
                direction: [float(record, 2), float(record, 3)],
                age_s: float(record, 4),
                speed_m_s: float(record, 5),
                period_s: float(record, 6),
                crest_m: float(record, 7),
                weight: float(record, 8),
            })
            .collect();
        FoamFrame {
            uniforms,
            grid,
            tiles,
            forcing,
            events,
        }
    }

    fn read(&self, buffer: &wgpu::Buffer) -> Vec<u8> {
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: buffer.size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(buffer, 0, &readback, 0, buffer.size());
        self.queue.submit([encoder.finish()]);
        let slice = readback.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| result.unwrap());
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        slice.get_mapped_range().unwrap().to_vec()
    }
}

fn half_to_f32(bits: u16) -> f32 {
    let sign = if bits >> 15 == 1 { -1.0 } else { 1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let mantissa = f32::from(bits & 0x3ff);
    sign * if exponent == 0 {
        mantissa * 2f32.powi(-24)
    } else {
        (1.0 + mantissa / 1024.0) * 2f32.powi(exponent - 15)
    }
}

/// The four channels of an rgba16float texel.
fn half_texel(bytes: &[u8], texel: usize) -> [f64; 4] {
    std::array::from_fn(|channel| {
        let at = 8 * texel + 2 * channel;
        f64::from(half_to_f32(u16::from_le_bytes(
            bytes[at..at + 2].try_into().unwrap(),
        )))
    })
}

impl FoamFrame {
    /// The forcing grid's texels' `values`, interpolated between them as the
    /// scene samples them, `offset_m` from the view centre.
    fn forcing_between(&self, offset_m: [f64; 2], values: impl Fn(usize) -> [f64; 4]) -> [f64; 4] {
        let texels = 64usize;
        let (origin, texel_m) = self.uniforms.grid_and_forcing_m()[1];
        let position = [0, 1].map(|axis| {
            ((offset_m[axis] - f64::from(origin[axis])) / f64::from(texel_m) - 0.5)
                .clamp(0.0, (texels - 1) as f64)
        });
        let low = position.map(|p| (p.floor() as usize).min(texels - 2));
        let fraction = [0, 1].map(|axis| position[axis] - low[axis] as f64);
        let at = |x: usize, y: usize| values(y * texels + x);
        let [a, b, c, d] = [
            at(low[0], low[1]),
            at(low[0] + 1, low[1]),
            at(low[0], low[1] + 1),
            at(low[0] + 1, low[1] + 1),
        ];
        std::array::from_fn(|channel| {
            let bottom = a[channel] + fraction[0] * (b[channel] - a[channel]);
            let top = c[channel] + fraction[0] * (d[channel] - c[channel]);
            bottom + fraction[1] * (top - bottom)
        })
    }

    /// The forcing grid's layer as the scene samples it.
    fn forcing_at(&self, layer: usize, offset_m: [f64; 2]) -> [f64; 4] {
        self.forcing_between(offset_m, |texel| {
            half_texel(&self.forcing, layer * 64 * 64 + texel)
        })
    }

    /// The view's mean reflectance from its foam: the grid's events, where
    /// their tile holds foam, plus the sub-pixel mean of the classes left out
    /// of them. Then the bounds the model's closed form sets on it: for the
    /// events' classes at the forcing's wind at their cells' centres, and
    /// for the rest as the forcing grid holds it, with the events overlapping as if their
    /// filaments added reflectance, and as if they covered the union of
    /// their shares as one layer.
    fn mean_reflectance(&self, capacity: [u32; 2]) -> [f64; 3] {
        let texels = self.uniforms.grid_texels();
        let [(grid_origin, texel_m), _] = self.uniforms.grid_and_forcing_m();
        let tiles_across = capacity[0].div_ceil(32);
        let mut events = 0.0;
        for y in 0..texels[1] {
            for x in 0..texels[0] {
                let tile = (y / 32 * tiles_across + x / 32) as usize;
                if u32::from_le_bytes(self.tiles[4 * tile..4 * tile + 4].try_into().unwrap()) & 1
                    == 0
                {
                    continue;
                }
                let [covered, depth, ..] = half_texel(&self.grid, (y * capacity[0] + x) as usize);
                events += covered * depth / (1.0 + depth);
            }
        }
        events /= f64::from(texels[0] * texels[1]);
        let drawn = self.uniforms.drawn();
        let undrawn = drawn.map(|share| 1.0 - share);
        let cells = self.uniforms.class_cells();
        // The forcing varies over hundreds of metres, so every eighth texel
        // samples its means.
        let (mut unresolved, mut added, mut united, mut samples) = (0.0, 0.0, 0.0, 0.0);
        for y in (0..texels[1]).step_by(8) {
            for x in (0..texels[0]).step_by(8) {
                let offset_m = [x, y].map(f64::from);
                let offset_m = [0, 1].map(|axis| {
                    f64::from(grid_origin[axis]) + (offset_m[axis] + 0.5) * f64::from(texel_m)
                });
                unresolved += self.forcing_at(1, offset_m)[1];
                // The forcing's texels hold the classes left out of the
                // events, between those texels.
                let [left_out, ..] = self.forcing_between(offset_m, |texel| {
                    let [x, y, ..] = half_texel(&self.forcing, texel);
                    [
                        browser::foam_mean_reflectance(x.hypot(y), undrawn),
                        0.0,
                        0.0,
                        0.0,
                    ]
                });
                // Each class's events take the wind at their cell's centre.
                let (mut discrete, mut cover) = (0.0, 0.0);
                for (class, (_, cell_m, place)) in cells.iter().enumerate() {
                    let centre_m = [0, 1].map(|axis| {
                        let position = f64::from(place[axis]) + offset_m[axis] / f64::from(*cell_m);
                        (position.floor() + 0.5 - f64::from(place[axis])) * f64::from(*cell_m)
                    });
                    let [x, y, ..] = self.forcing_at(0, centre_m);
                    let mut only = [0.0; browser::FOAM_BINS];
                    only[class] = drawn[class];
                    discrete += browser::foam_mean_reflectance(x.hypot(y), only);
                    cover += browser::foam_mean_cover(x.hypot(y), only);
                }
                // Poisson events covering `cover` of the sea on average
                // leave 1 - e^-cover of it covered.
                let union = if cover > 0.0 {
                    -(-cover).exp_m1() / cover
                } else {
                    1.0
                };
                added += discrete + left_out;
                united += discrete * union + left_out;
                samples += 1.0;
            }
        }
        [
            events + unresolved / samples,
            added / samples,
            united / samples,
        ]
    }
}

/// The mean residuals of the view's foam from its closed form's upper and
/// lower bounds over frames, their standard error, and the upper bound's
/// mean.
fn mean_reflectance_residual(rig: &mut FoamRig, options: &Options, frames: u32) -> [f64; 4] {
    let (mut upper, mut lower) = (Vec::new(), Vec::new());
    let mut expected_sum = 0.0;
    for frame in 0..frames {
        let options = Options {
            world_time_seconds: options.world_time_seconds + f64::from(frame) * FRAME_SECONDS,
            ..options.clone()
        };
        let (scene, uniforms) = rig.view(&options);
        // A frame that expects too few events to draw holds none.
        if !uniforms.active() {
            upper.push(0.0);
            lower.push(0.0);
            continue;
        }
        let capacity = rig.foam.capacity();
        let [measured, added, united] = rig.run(&scene, uniforms).mean_reflectance(capacity);
        upper.push(measured - added);
        lower.push(measured - united);
        expected_sum += added;
    }
    // The wind changes between frames, so each frame's measure is compared
    // with its own bounds.
    let count = f64::from(frames);
    let mean = |values: &[f64]| values.iter().sum::<f64>() / count;
    let (upper_mean, lower_mean) = (mean(&upper), mean(&lower));
    let error = (upper
        .iter()
        .map(|value| (value - upper_mean).powi(2))
        .sum::<f64>()
        / (count * (count - 1.0)))
        .sqrt();
    [upper_mean, lower_mean, error, expected_sum / count]
}

// The foam of a view, its events' grid plus the sub-pixel mean of the
// classes it leaves out, reflects on average the mean of Monahan and
// O'Muircheartaigh's whitecap coverage the model's closed form gives,
// Σ W fⱼ Aⱼ R̄ⱼ over every class, at the forcing's wind where each class's
// cells take it: in light and
// strong winds, at every tier, and at every zoom, as the classes hand over
// from events to their mean. Where events overlap it lies between the sum
// of their reflectances and the union of their covered shares; the
// strongest winds cover enough of the sea to tell the two apart.
#[test]
fn foam_reflects_its_whitecap_coverage_at_every_wind_tier_and_zoom() {
    let mut rig = FoamRig::new();
    let mut failures = Vec::new();
    // Views, the frames that average out their events, the seas, and the
    // standard error each mean is held to as a share of it: the closest
    // view holds a few heavy-tailed events a frame, so it averages the most
    // frames, is held to a fifth, and only in the strongest wind, whose foam
    // fills it often. Rare large rows carry much of a wide view's foam, so a
    // few frames mostly fall short of the mean and now and then far exceed
    // it; wide views average many.
    let views: [(f32, u32, &[usize], f64); 5] = [
        (15.0, 768, &[2], 0.2),
        (240.0, 36, &[0, 1, 2], 0.1),
        (1000.0, 48, &[0, 1, 2], 0.1),
        (2500.0, 48, &[0, 1, 2], 0.1),
        (6000.0, 48, &[0, 1, 2], 0.1),
    ];
    for (view_height_m, frames, seas, power) in views {
        for &sea in seas {
            let (centre, time_s) = WINDY_SEAS[sea];
            for (quality, size) in [
                (GraphicsQuality::Low, (960, 540)),
                (GraphicsQuality::High, (1920, 1080)),
            ] {
                let options = Options {
                    width: size.0,
                    height: size.1,
                    camera_x_m: centre[0],
                    camera_y_m: centre[1],
                    view_height_m,
                    world_time_seconds: time_s,
                    quality,
                    ..default_options()
                };
                let [above, below, error, expected] =
                    mean_reflectance_residual(&mut rig, &options, frames);
                let line = format!(
                    "{centre:?} {quality:?} {view_height_m} m: {above:.6} above the sum, \
                     {below:.6} above the union, ± {error:.6} of {expected:.6}"
                );
                println!("{line}");
                if above > 3.0 * error || below < -3.0 * error || error > power * expected {
                    failures.push(line);
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

// The foam over a patch of sea is the same however the view's tiles cut
// it: views of one centre and widths a fraction of a tile apart draw the
// same events into the same texels, though each tile gathers the events
// of every cell that reaches it from its own place. A few texels may
// differ where the views' forcing grids interpolate the wind differently.
#[test]
fn foam_is_the_same_however_tiles_cut_the_view() {
    let mut rig = FoamRig::new();
    let (centre, time_s) = WINDY_SEAS[0];
    for view_height_m in [600.0, 800.0, 1000.0] {
        let frames = [960, 1024].map(|width| {
            let options = Options {
                width,
                height: 540,
                camera_x_m: centre[0],
                camera_y_m: centre[1],
                view_height_m,
                world_time_seconds: time_s,
                quality: GraphicsQuality::Low,
                ..default_options()
            };
            let (scene, uniforms) = rig.view(&options);
            (rig.run(&scene, uniforms), rig.foam.capacity())
        });
        let [(narrow, narrow_capacity), (wide, wide_capacity)] = &frames;
        // Wider views may fade a class sooner as its cells outgrow their
        // texture; these zooms draw the same share of each.
        assert_eq!(narrow.uniforms.drawn(), wide.uniforms.drawn());
        let [(narrow_origin, texel_m), _] = narrow.uniforms.grid_and_forcing_m();
        let [(wide_origin, _), _] = wide.uniforms.grid_and_forcing_m();
        let shift =
            [0, 1].map(|axis| ((narrow_origin[axis] - wide_origin[axis]) / texel_m).round() as i64);
        let reflectance = |frame: &FoamFrame, capacity: [u32; 2], x: i64, y: i64| {
            let texels = frame.uniforms.grid_texels();
            if x < 0 || y < 0 || x >= i64::from(texels[0]) || y >= i64::from(texels[1]) {
                return None;
            }
            let depth = half_texel(&frame.grid, (y * i64::from(capacity[0]) + x) as usize)[0];
            Some(depth / (1.0 + depth))
        };
        let (mut foamy, mut differing) = (0, 0);
        let texels = narrow.uniforms.grid_texels();
        for y in 0..i64::from(texels[1]) {
            for x in 0..i64::from(texels[0]) {
                let (Some(here), Some(there)) = (
                    reflectance(narrow, *narrow_capacity, x, y),
                    reflectance(wide, *wide_capacity, x + shift[0], y + shift[1]),
                ) else {
                    continue;
                };
                if here.max(there) > 0.01 {
                    foamy += 1;
                    if (here - there).abs() > 0.02 {
                        differing += 1;
                    }
                }
            }
        }
        println!("{view_height_m} m: {differing} of {foamy} foamy texels differ");
        assert!(
            foamy > 1000 && differing * 1000 <= foamy,
            "{view_height_m} m: {differing} of {foamy} foamy texels differ"
        );
    }
}

/// The drawn dominant waves where a wind blows: the wind sea's modes of the
/// bands centred within a factor √2 of its peak wavenumber, each with its
/// steepness a k, as Dulov et al. band-passed the sea about its peak.
struct DominantWaves(Vec<(game::OceanWaveMode, f64)>);

impl DominantWaves {
    fn new(seed: u64, wind_m_s: Vector) -> Self {
        let peak_per_m = game::wind_sea(f64::from(wind_m_s.length())).peak_wavenumber_per_m;
        let lowest = std::f64::consts::TAU / game::OCEAN_WAVE_LONGEST_M;
        Self(
            game::wind_sea_components(WorldGeometry::default(), seed, wind_m_s)
                .into_iter()
                .filter(|component| {
                    let centre = lowest
                        * 2f64.powf(
                            (component.band as f64 + 0.5)
                                / game::OCEAN_WAVE_BANDS_PER_OCTAVE as f64,
                        );
                    (centre / peak_per_m).log2().abs() <= 0.5
                })
                .map(|component| {
                    (
                        component.mode,
                        component.amplitude_m * component.mode.wave_number_per_m,
                    )
                })
                .collect(),
        )
    }

    /// Σ aₖ e^{iψₖ} at a point and time on the world clock.
    fn signal(&self, position_m: [f64; 2], time_s: f64) -> [f64; 2] {
        self.0
            .iter()
            .fold([0.0; 2], |[real, imaginary], (mode, steepness)| {
                let phase = mode.wave_number_x_per_m * position_m[0]
                    + mode.wave_number_y_per_m * position_m[1]
                    + mode.phase_rad
                    - mode.angular_frequency_rad_s * time_s;
                [
                    real + steepness * phase.cos(),
                    imaginary + steepness * phase.sin(),
                ]
            })
    }
}

/// Dulov et al.'s Eq. 2 fitted as they fitted it: the active area's
/// profile over the dominant wave's phase, relative to how often the sea
/// shows each phase, as ln Q = ln Q₀ + M ε cos(ψ + Φ₀) by least squares
/// over phase bins, with ε the mean steepness. Returns M and Φ₀.
fn phase_profile_fit(active: &[(f64, [f64; 2])], background: &[[f64; 2]]) -> (f64, f64) {
    const BINS: usize = 36;
    let tau = std::f64::consts::TAU;
    let bin = |z: &[f64; 2]| ((z[1].atan2(z[0]) / tau + 0.5) * BINS as f64) as usize % BINS;
    let [mut active_profile, mut background_profile] = [[0.0; BINS]; 2];
    for (weight, z) in active {
        active_profile[bin(z)] += weight;
    }
    for z in background {
        background_profile[bin(z)] += 1.0;
    }
    let steepness =
        background.iter().map(|z| z[0].hypot(z[1])).sum::<f64>() / background.len() as f64;
    // Weighted least squares of ln Q on 1, cos ψ and sin ψ.
    let mut normal = [[0.0; 3]; 3];
    let mut right = [0.0; 3];
    for index in 0..BINS {
        let phase = ((index as f64 + 0.5) / BINS as f64 - 0.5) * tau;
        let basis = [1.0, phase.cos(), phase.sin()];
        let value = (active_profile[index] / background_profile[index]).ln();
        let weight = background_profile[index];
        for row in 0..3 {
            right[row] += weight * basis[row] * value;
            for column in 0..3 {
                normal[row][column] += weight * basis[row] * basis[column];
            }
        }
    }
    // Gaussian elimination of the 3×3 system.
    for pivot in 0..3 {
        let pivot_row = normal[pivot];
        for row in pivot + 1..3 {
            let factor = normal[row][pivot] / pivot_row[pivot];
            for (value, above) in normal[row].iter_mut().zip(pivot_row).skip(pivot) {
                *value -= factor * above;
            }
            right[row] -= factor * right[pivot];
        }
    }
    let mut solution = [0.0; 3];
    for row in (0..3).rev() {
        let known: f64 = (row + 1..3)
            .map(|column| normal[row][column] * solution[column])
            .sum();
        solution[row] = (right[row] - known) / normal[row][row];
    }
    let [_, cosine, sine] = solution;
    (cosine.hypot(sine) / steepness, (-sine).atan2(cosine))
}

// Active whitecaps gather behind the drawn dominant crests with the
// modulation Dulov et al. (2021) measured from individual waves: their
// Eq. 2, fitted as they fitted it to the active area's profile over the
// phase of the dominant waves' k·η at each listed event's active band's
// centroid, from its analytic signal, with ε its mean envelope, gives
// M = 22.9 ± 2.7 and Φ₀ = 14.1° ± 5.0° upwind. The largest events carry
// most of the active area, so the profile pools many frames, far apart in
// time so each shows the sea's dominant waves anew, over light to strong
// winds. Compromise: the waves are those of the wind to within a fifth of
// a metre per second.
#[test]
fn active_whitecaps_gather_behind_dominant_crests_as_measured() {
    const FRAMES: u32 = 96;
    let mut rig = FoamRig::new();
    let geometry = WorldGeometry::default();
    let mut bands = std::collections::HashMap::new();
    let (mut active, mut background) = (Vec::new(), Vec::new());
    for (centre, time_s) in WINDY_SEAS {
        for frame in 0..FRAMES {
            let options = Options {
                width: 1920,
                height: 1080,
                camera_x_m: centre[0],
                camera_y_m: centre[1],
                view_height_m: 1000.0,
                world_time_seconds: time_s + f64::from(frame) * FRAME_SECONDS,
                ..default_options()
            };
            let (scene, uniforms) = rig.view(&options);
            let foam = rig.run(&scene, uniforms);
            let (tick, tick_fraction) = split_tick_seconds(options.world_time_seconds);
            let camera_m = [options.camera_x_m, options.camera_y_m];
            let orbital_m =
                view_orbital_displacement_m(geometry, options.seed, tick, tick_fraction, camera_m);
            let drift_m_s =
                view_surface_drift_m_s(geometry, options.seed, tick, tick_fraction, camera_m);
            // The waves of the wind at `wind_offset_m`, to within a fifth of
            // a metre per second, at `world_m`.
            let mut signal = |wind_offset_m: [f64; 2], world_m: [f64; 2]| {
                let [x, y, ..] = foam.forcing_at(0, wind_offset_m);
                let key = [x, y].map(|component| (component * 5.0).round() as i32);
                bands
                    .entry(key)
                    .or_insert_with(|| {
                        DominantWaves::new(
                            options.seed,
                            Vector::new(key[0] as f32 / 5.0, key[1] as f32 / 5.0),
                        )
                    })
                    .signal(world_m, options.world_time_seconds)
            };
            let cells = foam.uniforms.class_cells();
            // The fastest class breaks on waves at least 0.4 of the peak's
            // length, outside Dulov et al.'s measured breakers, three to
            // four times shorter than the waves modulating them, so the
            // profile leaves it out.
            let fastest_from_m_s = cells[browser::FOAM_BINS - 1].0;
            for event in foam
                .events
                .iter()
                .filter(|event| event.age_s < event.period_s && event.speed_m_s < fastest_from_m_s)
            {
                let age_s = f64::from(event.age_s);
                let u = age_s / f64::from(event.period_s);
                let band_m = f64::from(event.speed_m_s * event.period_s);
                let ahead_m = band_m * swept_moment(u) / swept_area(u);
                // Where it broke on the waves, by the water's slow motion
                // alone, and its active band's centroid now.
                let offset_m = [0, 1].map(|axis| {
                    f64::from(event.position_m[axis]) - orbital_m[axis] - drift_m_s[axis] * age_s
                        + f64::from(event.direction[axis]) * ahead_m
                });
                let world_m = [0, 1].map(|axis| f64::from(camera_m[axis]) + offset_m[axis]);
                // The cell that drew it, whose centre's wind it took.
                let (_, cell_m, place) = *cells
                    .iter()
                    .rev()
                    .find(|(slowest, ..)| event.speed_m_s >= *slowest)
                    .unwrap();
                let centre_m = [0, 1].map(|axis| {
                    let position = place[axis] + event.position_m[axis] / cell_m;
                    f64::from((position.floor() + 0.5 - place[axis]) * cell_m)
                });
                // A row's segments cover its filled share.
                let breaking = if event.speed_m_s >= cells[browser::FIRST_ROW_BIN].0 {
                    browser::foam_row_fill()
                } else {
                    1.0
                };
                let area =
                    breaking * f64::from(event.weight * event.crest_m) * band_m * swept_area(u);
                active.push((area, signal(centre_m, world_m)));
            }
            let [(origin, texel_m), _] = foam.uniforms.grid_and_forcing_m();
            let texels = foam.uniforms.grid_texels();
            for y in (0..texels[1]).step_by(16) {
                for x in (0..texels[0]).step_by(16) {
                    let offset_m =
                        [x, y].map(|texel| (f64::from(texel) + 0.5) * f64::from(texel_m));
                    let offset_m = [0, 1].map(|axis| f64::from(origin[axis]) + offset_m[axis]);
                    let world_m = [0, 1].map(|axis| f64::from(camera_m[axis]) + offset_m[axis]);
                    background.push(signal(offset_m, world_m));
                }
            }
        }
    }
    let (strength, lag) = phase_profile_fit(&active, &background);
    let lag_degrees = lag.to_degrees();
    assert!((strength - 22.9).abs() <= 2.7, "M = {strength}");
    assert!((lag_degrees - 14.1).abs() <= 5.0, "Φ₀ = {lag_degrees}°");
}

// In moderate winds, the most common breaking segment by count is 1 to 2 m
// long, as Gemmrich et al. (2013) measured, and none is longer than the
// 15 m they saw: events born within the last two seconds of frames far
// apart, at a zoom that draws every class. The fastest classes break in
// rows, whose segments the events pass draws.
#[test]
fn most_breaking_crests_are_one_to_two_metres_long() {
    const BORN_WITHIN_S: f32 = 2.0;
    let mut rig = FoamRig::new();
    for (centre, time_s) in [([12136.0, -9457.0], 4_585_200.0), WINDY_SEAS[0]] {
        let mut counts = [0.0; 16];
        for frame in 0..24 {
            let options = Options {
                width: 1920,
                height: 1080,
                camera_x_m: centre[0],
                camera_y_m: centre[1],
                view_height_m: 240.0,
                world_time_seconds: time_s + f64::from(frame) * FRAME_SECONDS,
                ..default_options()
            };
            let (scene, uniforms) = rig.view(&options);
            assert!(uniforms.drawn().iter().all(|&drawn| drawn == 1.0));
            let rows_from_m_s = uniforms.class_cells()[browser::FIRST_ROW_BIN].0;
            for event in rig.run(&scene, uniforms).events {
                if event.speed_m_s >= rows_from_m_s {
                    continue;
                }
                assert!(event.crest_m <= 15.0, "{event:?}");
                if event.age_s < BORN_WITHIN_S {
                    counts[event.crest_m as usize] += f64::from(event.weight);
                }
            }
        }
        let mode = (0..counts.len())
            .max_by(|a, b| counts[*a].total_cmp(&counts[*b]))
            .unwrap();
        println!("{centre:?}: counts by metre {counts:?}");
        assert_eq!(mode, 1, "{centre:?}: counts by metre {counts:?}");
    }
}

/// One event as the events pass reads it: `FoamEvent` in foam-generate.wgsl.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct EventRecord {
    position_m: [f32; 2],
    direction: [f32; 2],
    age_s: f32,
    speed_m_s: f32,
    period_s: f32,
    crest_m: f32,
    weight: f32,
    seed: u32,
}

/// Draws each of `events` over a grid of the view's texels `texels` across
/// whose corner lies `corner_m` from the view centre, summing each event's
/// cover times its reflectance over the grid.
const EVENT_COVER_ENTRY: &str = r"
@group(2) @binding(0)
var<storage, read> foam_test_events: array<FoamEvent>;
@group(2) @binding(1)
var<storage, read_write> foam_test_sums: array<f32>;
@group(2) @binding(2)
var<uniform> foam_test_grid: vec4<f32>;
var<workgroup> foam_test_partial: array<f32, 256>;

@compute @workgroup_size(16, 16)
fn test_foam_event_cover(
    @builtin(workgroup_id) group: vec3<u32>,
    @builtin(num_workgroups) groups: vec3<u32>,
    @builtin(local_invocation_id) local: vec3<u32>,
    @builtin(local_invocation_index) thread: u32,
) {
    let texel = group.xy * 16u + local.xy;
    var value = 0.0;
    if all(texel < vec2<u32>(u32(foam_test_grid.z))) {
        let offset_m = foam_test_grid.xy + (vec2<f32>(texel) + 0.5) * foam_frame.grid.z;
        let cover = foam_event_cover(foam_test_events[group.z], offset_m);
        value = cover.x * cover.y / (1.0 + cover.y);
    }
    foam_test_partial[thread] = value;
    workgroupBarrier();
    if thread == 0u {
        var sum = 0.0;
        for (var index = 0u; index < 256u; index++) {
            sum += foam_test_partial[index];
        }
        foam_test_sums[(group.z * groups.y + group.y) * groups.x + group.x] = sum;
    }
}
";

impl FoamRig {
    /// The reflectance-time one event draws under the frame `uniforms`:
    /// its cover times its reflectance summed over its texels and over its
    /// life at midpoints of `ages` steps.
    fn event_reflectance_time(
        &self,
        uniforms: &FoamUniforms,
        speed_m_s: f32,
        crest_m: f32,
        seed: u32,
        ages: u32,
    ) -> f64 {
        let device = &self.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(
                format!("{}{EVENT_COVER_ENTRY}", browser::FOAM_SHADER).into(),
            ),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty,
            count: None,
        };
        let storage = |read_only| wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let uniform = wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        };
        let layouts = [
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[],
            }),
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[entry(0, uniform)],
            }),
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    entry(0, storage(true)),
                    entry(1, storage(false)),
                    entry(2, uniform),
                ],
            }),
        ];
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&layouts[0]), Some(&layouts[1]), Some(&layouts[2])],
                    immediate_size: 0,
                }),
            ),
            module: &shader,
            entry_point: Some("test_foam_event_cover"),
            compilation_options: Default::default(),
            cache: None,
        });
        let [(_, texel_m), _] = uniforms.grid_and_forcing_m();
        let breaking = speed_m_s / 0.85;
        let period_s = std::f32::consts::TAU * breaking / 9.81;
        let band_m = speed_m_s * period_s;
        let life_s = period_s + browser::foam_faded_age_s() as f32;
        let row = speed_m_s >= uniforms.class_cells()[browser::FIRST_ROW_BIN].0;
        let widest_m = if row {
            crest_m * 1.5
        } else {
            (crest_m * crest_m + 2.0 * 4.1 * life_s).sqrt()
        };
        let extent_m = 1.1 * band_m.max(widest_m) + 1.0 * crest_m.max(band_m) + 4.0 * texel_m;
        let texels = (extent_m / texel_m).ceil() as u32;
        let events: Vec<EventRecord> = (0..ages)
            .map(|age| EventRecord {
                position_m: [-0.5 * band_m, 0.0],
                direction: [1.0, 0.0],
                age_s: (age as f32 + 0.5) / ages as f32 * life_s,
                speed_m_s,
                period_s,
                crest_m,
                weight: 1.0,
                seed,
            })
            .collect();
        let groups = texels.div_ceil(16);
        let sums = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(groups * groups * ages) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let buffer = |contents: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents,
                usage,
            })
        };
        let event_buffer = buffer(bytemuck::cast_slice(&events), wgpu::BufferUsages::STORAGE);
        let grid = [
            -0.5 * texels as f32 * texel_m,
            -0.5 * texels as f32 * texel_m,
            texels as f32,
            0.0,
        ];
        let grid_buffer = buffer(bytemuck::cast_slice(&grid), wgpu::BufferUsages::UNIFORM);
        let uniform_buffer = buffer(bytemuck::bytes_of(uniforms), wgpu::BufferUsages::UNIFORM);
        let groups_bound = [
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layouts[0],
                entries: &[],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layouts[1],
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                }],
            }),
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &layouts[2],
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: event_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: sums.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: grid_buffer.as_entire_binding(),
                    },
                ],
            }),
        ];
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&pipeline);
            for (index, group) in groups_bound.iter().enumerate() {
                pass.set_bind_group(index as u32, group, &[]);
            }
            pass.dispatch_workgroups(groups, groups, ages);
        }
        self.queue.submit([encoder.finish()]);
        let sum: f64 = self
            .read(&sums)
            .as_chunks::<4>()
            .0
            .iter()
            .map(|bytes| f64::from(f32::from_le_bytes(*bytes)))
            .sum();
        sum * f64::from(texel_m * texel_m) * f64::from(life_s) / f64::from(ages)
    }
}

// Each event draws the reflectance-time its closed form gives it, whatever
// the zoom: every trick of its look moves its foam without changing its
// mean. Each is averaged over many seeds, as a few of its noise's features
// span a whole patch, and a row's segments fill it only on average.
#[test]
fn each_event_draws_the_foam_its_closed_form_gives() {
    let mut rig = FoamRig::new();
    let (centre, time_s) = WINDY_SEAS[0];
    let mut failures = Vec::new();
    for (view_height_m, events) in [
        (15.0, &[(1.0f32, 1.0f32), (1.6, 2.5)][..]),
        (240.0, &[(2.5, 3.0), (5.0, 6.0), (5.0, 15.0)][..]),
        (2000.0, &[(5.0, 6.0), (9.0, 60.0)][..]),
    ] {
        let options = Options {
            width: 1920,
            height: 1080,
            camera_x_m: centre[0],
            camera_y_m: centre[1],
            view_height_m,
            world_time_seconds: time_s,
            ..default_options()
        };
        let (_, uniforms) = rig.view(&options);
        for &(speed_m_s, crest_m) in events {
            let row = speed_m_s >= uniforms.class_cells()[browser::FIRST_ROW_BIN].0;
            let seeds = 32;
            let drawn = (0..seeds)
                .map(|seed| {
                    rig.event_reflectance_time(
                        &uniforms,
                        speed_m_s,
                        crest_m,
                        0x9e37_79b9_u32.wrapping_mul(seed + 1),
                        32,
                    )
                })
                .sum::<f64>()
                / f64::from(seeds);
            let expected =
                browser::foam_event_reflectance_time(f64::from(speed_m_s), f64::from(crest_m), row);
            let line = format!(
                "{view_height_m} m, {speed_m_s} m/s, {crest_m} m: drawn {drawn:.4} of {expected:.4}, {:+.1}%",
                100.0 * (drawn / expected - 1.0)
            );
            println!("{line}");
            if (drawn / expected - 1.0).abs() > 0.03 {
                failures.push(line);
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
