use std::{
    borrow::Cow,
    f64::consts::{SQRT_2, TAU},
    ops::Range,
    sync::LazyLock,
};

use bytemuck::{Pod, Zeroable};
use rapier2d_f64::glamx::Vec2 as Vector;
use simulation::{TICKS_PER_SECOND, WorldGeometry};

use crate::WAVE_TILE_SHADER;

// Keep synchronized with crates/browser/src/shader/components/wave-tile-state.wgsl.
const WAVE_TILE_LAYERS: usize = 5;
const TEXELS: u32 = 512;
// Wavenumbers from -WAVE_TILE_REACH to WAVE_TILE_REACH along x.
const COLUMNS: u32 = 93;
const PACKED_BYTES: u32 = TEXELS * TEXELS * WAVE_TILE_LAYERS as u32 * 4;
const COLUMN_BYTES: u32 = COLUMNS * TEXELS * WAVE_TILE_LAYERS as u32 * 8;
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;

/// The short waves' longest octave, as `DECORATIVE_WATER_WAVELENGTH_M` in
/// ocean-waves.wgsl; each layer carries two octaves, halving from it.
const LONGEST_OCTAVE_M: f64 = 18.0;
/// A layer's tile holds this many of its longer octave's wavelengths, so its
/// shorter octave's shortest waves span 11 texels, where bilinear
/// interpolation loses at most 5% of their slope variance on average.
const TILE_WAVELENGTHS: f64 = 16.0;
/// The shortest gravity wave, as `GRAVITY_CAPILLARY_WAVELENGTH_M`.
const SHORTEST_GRAVITY_WAVE_M: f64 = 0.017;
/// Wave frequencies round to whole cycles over this loop of the clock, so
/// the tiles evolve in closed form from a clock reduced to the loop.
const LOOP_SECONDS: u64 = 1000;
/// A layer whose longer octave keeps less of its slope variance than this
/// at the pixel's footprint is not drawn.
const DRAWN_SHARE: f64 = 1.0e-3;

#[repr(C)]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct WaveTileLayer {
    view: [f32; 4],
    lattice: [i32; 4],
    tile: [f32; 4],
    blend: [f32; 4],
}

/// The wave tiles' per-frame state, which their generation and the scene read.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct WaveTileUniforms {
    layers: [WaveTileLayer; WAVE_TILE_LAYERS],
    clock: [f32; 4],
    sea: [f32; 4],
    octaves: [f32; 4],
    window: [u32; 4],
}

impl WaveTileUniforms {
    fn drawn_layers(&self) -> Range<u32> {
        self.window[0]..self.window[0] + self.window[1]
    }

    /// Workgroups of the transform's column pass, then of its texel-row
    /// pass, then the layers each transforms.
    pub fn workgroups(&self) -> [u32; 3] {
        [COLUMNS, TEXELS, self.drawn_layers().len() as u32]
    }
}

/// The texture's width, height and layers, then the bytes of its packed
/// texels, of the transform's columns, and of `WaveTileUniforms`.
pub fn wave_tile_buffer_sizes() -> [u32; 6] {
    [
        TEXELS,
        TEXELS,
        WAVE_TILE_LAYERS as u32,
        PACKED_BYTES,
        COLUMN_BYTES,
        size_of::<WaveTileUniforms>() as u32,
    ]
}

/// The share of slope variance along the wind of a sech²(βθ) spreading.
fn sech_squared_share(beta: f64) -> f64 {
    // β is at most 2.28, where 16 midpoints leave under 5e-4 of the share.
    const STEPS: u32 = 16;
    let (density, along) = (0..STEPS).fold((0.0, 0.0), |(density, along), step| {
        let angle = TAU * ((f64::from(step) + 0.5) / f64::from(STEPS) - 0.5);
        let weight = (beta * angle).cosh().powi(-2);
        (density + weight, along + weight * angle.cos().powi(2))
    });
    along / density
}

/// The share of slope variance along the wind under Donelan et al.'s
/// spreading at the spectrum's peak.
static PEAK_ALONG_WIND_SHARE: LazyLock<f64> = LazyLock::new(|| sech_squared_share(2.28));

/// The share of slope variance along the wind of waves of `wavenumber_ratio`
/// times the peak's wavenumber, under the directional spreading
/// `wave_tile_spreading` in wave-tile-generate.wgsl gives them with
/// `aligned` as its alignment beyond the measurements; keep the two
/// synchronized. The peak's floor keeps the ratio above 1.
fn along_wind_share(wavenumber_ratio: f64, aligned: f64) -> f64 {
    let lobe = 82.0f64.to_radians()
        * (1.0 - 10.0f64.powf(-0.039 * (wavenumber_ratio - 5.0)))
            .max(0.0)
            .sqrt();
    let blend = if lobe > 0.0 {
        (wavenumber_ratio - 5.0).min(1.0)
    } else {
        0.0
    };
    let donelan_banner = if blend < 1.0 {
        sech_squared_share(if wavenumber_ratio < 2.56 {
            2.28 * wavenumber_ratio.powf(-0.65)
        } else {
            10.0f64.powf(-0.4 + 0.8393 * wavenumber_ratio.powf(-0.567))
        })
    } else {
        0.0
    };
    let lobes = if lobe > 0.0 {
        let ratio = 0.34 * wavenumber_ratio.min(22.0) - 0.45;
        let width_squared = lobe * lobe / (2.0 * (2.0 * ratio).ln());
        // A wrapped Gaussian lobe's mean of cos²θ, ½ (1 + cos 2μ e^(−2σ²)).
        0.5 * (1.0 + (2.0 * lobe).cos() * (-2.0 * width_squared).exp())
    } else {
        0.0
    };
    let measured = donelan_banner + (lobes - donelan_banner) * blend;
    let alignment = aligned * (1.0 - 45.0 / wavenumber_ratio).max(0.0);
    measured + (*PEAK_ALONG_WIND_SHARE - measured) * alignment
}

/// The alignment beyond the measured spreading at which the octaves the
/// tiles carry and the shorter ones down to the shortest gravity wave, at
/// equal slope variance each, split along and across the wind as what
/// `modes_slope_variance`, the wave modes' slope variance along and across
/// it, leaves of Cox and Munk's slope variance for a sustained wind of
/// `wind_speed_m_s`, as `cox_munk_slope_variance` in ocean-waves.wgsl gives
/// it. Each share is linear in the alignment.
fn cox_munk_alignment(peak_per_m: f64, wind_speed_m_s: f64, modes_slope_variance: [f64; 2]) -> f64 {
    // 128 midpoints in log wavenumber, about twelve to an octave, keep the
    // octaves' share within 3e-4 of its target.
    const STEPS: u32 = 128;
    let [longest, shortest] = [
        TAU / (SQRT_2 * LONGEST_OCTAVE_M),
        TAU / SHORTEST_GRAVITY_WAVE_M,
    ]
    .map(|wavenumber| wavenumber / peak_per_m);
    let [unaligned, aligned] = [0.0, 1.0].map(|alignment| {
        (0..STEPS)
            .map(|step| {
                let ratio =
                    longest * (shortest / longest).powf((f64::from(step) + 0.5) / f64::from(STEPS));
                along_wind_share(ratio, alignment)
            })
            .sum::<f64>()
            / f64::from(STEPS)
    });
    let [along, across] = [3.16e-3, 1.92e-3].map(|slope| 0.0015 + slope * wind_speed_m_s);
    let [modes_along, modes_across] = modes_slope_variance;
    let target = (along - modes_along) / (along + across - modes_along - modes_across);
    (target - unaligned) / (aligned - unaligned)
}

/// The shares of an octave's slope variance, from `lowest` to twice that
/// wavenumber, that a pixel of `footprint_m` resolves along and across the
/// wind: Phillips' equal variance per logarithm of wavenumber, filtered by
/// exp(-(k fp)²) as each wave mode's amplitude is by exp(-(k fp)² / 2).
fn resolved_octave_shares(
    lowest_per_m: f64,
    footprint_m: f64,
    peak_per_m: f64,
    aligned: f64,
) -> [f64; 2] {
    // 8 midpoints in log wavenumber leave under 3e-4 of an octave.
    const STEPS: u32 = 8;
    (0..STEPS).fold([0.0; 2], |[along, across], step| {
        let k = lowest_per_m * (2.0f64).powf((f64::from(step) + 0.5) / f64::from(STEPS));
        let resolved = (-(k * footprint_m).powi(2)).exp() / f64::from(STEPS);
        let share = along_wind_share(k / peak_per_m, aligned);
        [along + resolved * share, across + resolved * (1.0 - share)]
    })
}

/// The wave tiles at the world clock for a view centred on `camera_m` whose
/// sea-level pixels span `pixel_footprint_m`, under the sustained surface
/// wind at the view centre, drawing at most `tier_layers` layers, carried by
/// the water's `orbital_displacement_m` at the view centre.
#[allow(clippy::too_many_arguments)]
pub fn wave_tile_uniforms(
    world_geometry: WorldGeometry,
    match_seed: u64,
    world_tick: u64,
    world_tick_fraction: f32,
    camera_m: [f32; 2],
    view_m: [f32; 2],
    pixel_footprint_m: f32,
    surface_wind_m_s: [f32; 2],
    tier_layers: usize,
    orbital_displacement_m: [f64; 2],
) -> WaveTileUniforms {
    let width_m = f64::from(world_geometry.width_m() as f32);
    let footprint_m = f64::from(pixel_footprint_m);
    // The tiles ride the water's orbital displacement at the view centre, so
    // short waves surge with the long waves' orbital motion as floating
    // boats do (Plant 2015, J. Geophys. Res. 120, finds short waves carried
    // by it). Compromise: the whole view takes the centre's displacement,
    // about a·k of strain, 0.1 in a moderate sea, away from it.
    let [camera_x_m, camera_y_m] = [
        f64::from(camera_m[0]) - orbital_displacement_m[0],
        f64::from(camera_m[1]) - orbital_displacement_m[1],
    ];
    let wind = surface_wind_m_s.map(f64::from);
    let speed = wind[0].hypot(wind[1]);
    let direction = if speed > 0.0 {
        wind.map(|component| component / speed)
    } else {
        [1.0, 0.0]
    };
    // The spectrum's peak, no shorter than the decorative octaves reach, as
    // `wave_peak_wavelength_m` in ocean-waves.wgsl.
    let peak_per_m = game::wind_sea(speed)
        .peak_wavenumber_per_m
        .min(TAU / (SQRT_2 * LONGEST_OCTAVE_M));
    // Compromise: the alignment takes the wind sea's modes in deep water and
    // unfiltered by the footprint. In water shallow enough that depth limits
    // the wind sea, the tiles can take up to a quarter more of Cox and Munk's
    // slopes across the wind than the modes leave, and the unresolved facets
    // there keep none across it.
    let modes_slope_variance = game::wind_sea_slope_variance(
        world_geometry,
        match_seed,
        Vector::new(wind[0] as f32, wind[1] as f32),
    );
    let aligned = cox_munk_alignment(peak_per_m, speed, modes_slope_variance);
    let octave_m = |octave: usize| LONGEST_OCTAVE_M / (1u64 << octave) as f64;
    // Each octave reaches half an octave either side of its wavelength.
    let lowest_wavenumber = |octave: usize| TAU / (SQRT_2 * octave_m(octave));
    let shares: [[f64; 2]; 2 * WAVE_TILE_LAYERS] = std::array::from_fn(|octave| {
        resolved_octave_shares(lowest_wavenumber(octave), footprint_m, peak_per_m, aligned)
    });
    // Layers the tier or the footprint leaves out are neither transformed nor
    // copied.
    let resolvable_layers = (0..WAVE_TILE_LAYERS)
        .take_while(|layer| shares[2 * layer][0] + shares[2 * layer][1] >= DRAWN_SHARE)
        .count();
    let tier_layers = WAVE_TILE_LAYERS.min(tier_layers);
    // A tier that draws fewer layers than the footprint resolves draws the
    // finest it resolves at least half of and the longer ones next to it, a
    // compromise: the longer layers it skips blur the unresolved slopes where
    // their waves would tilt the surface.
    let layer_octaves = |layer: usize| {
        shares[2 * layer]
            .iter()
            .chain(&shares[2 * layer + 1])
            .sum::<f64>()
    };
    let finest = (0..resolvable_layers)
        .rev()
        .find(|&layer| layer_octaves(layer) >= 1.0)
        .unwrap_or(0);
    let first = (finest + 1).saturating_sub(tier_layers);
    let drawn_layers = first..resolvable_layers.min(first + tier_layers);
    let drawn_shares = |octave: usize| {
        if drawn_layers.contains(&(octave / 2)) {
            shares[octave]
        } else {
            [0.0; 2]
        }
    };
    let drawn_share = |octave: usize| drawn_shares(octave)[0] + drawn_shares(octave)[1];
    // The share of an octave a pixel resolves, drawn or skipped by the tier;
    // the rest, and the layers the footprint leaves out, keep their full
    // share with the unresolved slopes.
    let resolvable_share = |octave: usize| {
        if octave / 2 < resolvable_layers {
            shares[octave][0] + shares[octave][1]
        } else {
            0.0
        }
    };
    let drawn_octaves: f64 = (0..2 * WAVE_TILE_LAYERS).map(drawn_share).sum();
    let skipped_octaves: f64 = (0..2 * WAVE_TILE_LAYERS)
        .map(|octave| resolvable_share(octave) - drawn_share(octave))
        .sum();
    let shortest_m = octave_m(2 * WAVE_TILE_LAYERS - 1) / SQRT_2;
    let unresolved = (shortest_m / SHORTEST_GRAVITY_WAVE_M).log2()
        + (0..2 * WAVE_TILE_LAYERS)
            .map(|octave| 1.0 - resolvable_share(octave))
            .sum::<f64>();
    let tile_m =
        |layer: usize| width_m / (width_m / (TILE_WAVELENGTHS * octave_m(2 * layer))).round();
    // A tile longer than twice the view's diagonal never shows two copies of
    // a wave; the tiling blends in as it shortens to one diagonal.
    let tiling = |layer: usize| {
        let ratio = (tile_m(layer) / f64::from(view_m[0].hypot(view_m[1])) - 1.0).clamp(0.0, 1.0);
        1.0 - ratio * ratio * (3.0 - 2.0 * ratio)
    };
    // The forced evaluation reads as many layers as the tier draws, from the
    // first it draws. Tiles shorten fourfold from layer to layer, so a zoom
    // blends one layer at a time; the forced evaluation blends the one at the
    // edge between read and tiled layers, the frame's worst case.
    let forced_blend = (first..first + tier_layers)
        .rev()
        .find(|&layer| tiling(layer) < 1.0)
        .unwrap_or(first);
    let layers = std::array::from_fn(|layer| {
        let tile_m = tile_m(layer);
        let spacing_m = 0.5 * tile_m;
        let skewed = [
            (camera_x_m - camera_y_m / 3.0f64.sqrt()) / spacing_m,
            2.0 * camera_y_m / (3.0f64.sqrt() * spacing_m),
        ];
        let base = skewed.map(f64::floor);
        // Texels no wider than the footprint, which leaves the waves past
        // their Nyquist wavenumber under 1% of their amplitude, and no
        // narrower than half of it, so neighbouring pixels read neighbouring
        // texels, up to the texture's size. A drawn layer keeps at least
        // `DRAWN_SHARE` of its longer octave, so its tile spans more than 32
        // footprints and as many texels.
        let texels = ((tile_m / footprint_m).ceil() as u32)
            .next_power_of_two()
            .min(TEXELS);
        WaveTileLayer {
            view: [
                (camera_x_m / tile_m).rem_euclid(1.0) as f32,
                (camera_y_m / tile_m).rem_euclid(1.0) as f32,
                (skewed[0] - base[0]) as f32,
                (skewed[1] - base[1]) as f32,
            ],
            lattice: [
                base[0] as i32,
                base[1] as i32,
                (width_m / spacing_m).round() as i32,
                texels as i32,
            ],
            blend: [
                tiling(layer) as f32,
                f32::from(u8::from(layer == forced_blend)),
                0.0,
                0.0,
            ],
            tile: [
                tile_m as f32,
                (drawn_shares(2 * layer)[0] + drawn_shares(2 * layer + 1)[0]) as f32,
                // A layer drawn only to time the forced evaluation reads
                // texels about a footprint apart, as drawn layers do.
                (if drawn_layers.contains(&layer) {
                    f64::from(texels)
                } else {
                    (tile_m / footprint_m).min(f64::from(texels))
                } / f64::from(TEXELS)) as f32,
                (drawn_shares(2 * layer)[1] + drawn_shares(2 * layer + 1)[1]) as f32,
            ],
        }
    });
    let loop_ticks = LOOP_SECONDS * TICKS_PER_SECOND;
    let loop_seconds = ((world_tick % loop_ticks) as f64 + f64::from(world_tick_fraction))
        / TICKS_PER_SECOND as f64;
    WaveTileUniforms {
        layers,
        clock: [
            loop_seconds as f32,
            LOOP_SECONDS as f32,
            pixel_footprint_m,
            0.0,
        ],
        sea: [
            direction[0] as f32,
            direction[1] as f32,
            peak_per_m as f32,
            aligned as f32,
        ],
        octaves: [
            unresolved as f32,
            drawn_octaves as f32,
            skipped_octaves as f32,
            0.0,
        ],
        window: [
            first as u32,
            drawn_layers.len() as u32,
            tier_layers as u32,
            0,
        ],
    }
}

/// Periodic tiles of the short waves' slopes, two octaves to a layer,
/// transformed each frame from their spectrum at the clock. The scene reads
/// them through `SCENE_LAYOUT_ENTRIES` at bindings 11 to 13 of its first
/// group.
pub struct WaveTiles {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    _columns: wgpu::Buffer,
    texels: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    columns_pipeline: wgpu::ComputePipeline,
    texel_rows_pipeline: wgpu::ComputePipeline,
}

impl WaveTiles {
    pub const SCENE_LAYOUT_ENTRIES: [wgpu::BindGroupLayoutEntry; 3] = [
        wgpu::BindGroupLayoutEntry {
            binding: 11,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 12,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 13,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ];

    /// `scene_uniform_layout` holds the scene uniforms, which the generation
    /// reads for the match seed.
    pub fn new(device: &wgpu::Device, scene_uniform_layout: &wgpu::BindGroupLayout) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Wave tiles"),
            size: wgpu::Extent3d {
                width: TEXELS,
                height: TEXELS,
                depth_or_array_layers: WAVE_TILE_LAYERS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Wave tile uniforms"),
            size: size_of::<WaveTileUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let columns = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Wave tile columns"),
            size: u64::from(COLUMN_BYTES),
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let texels = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Wave tile texels"),
            size: u64::from(PACKED_BYTES),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Wave tile generation layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Wave tile generation"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: columns.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: texels.as_entire_binding(),
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Wave tile shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(WAVE_TILE_SHADER)),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Wave tile pipeline layout"),
            bind_group_layouts: &[Some(scene_uniform_layout), Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry_point| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Wave tile pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader,
                entry_point: Some(entry_point),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            columns_pipeline: pipeline("generate_wave_tile_columns"),
            texel_rows_pipeline: pipeline("generate_wave_tile_texel_rows"),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Wave tile sampler"),
                address_mode_u: wgpu::AddressMode::Repeat,
                address_mode_v: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            texture,
            view,
            uniforms,
            _columns: columns,
            texels,
            bind_group,
        }
    }

    /// The scene's bindings of the tiles, their sampler, and their state.
    pub fn scene_entries(&self) -> [wgpu::BindGroupEntry<'_>; 3] {
        [
            wgpu::BindGroupEntry {
                binding: 11,
                resource: wgpu::BindingResource::TextureView(&self.view),
            },
            wgpu::BindGroupEntry {
                binding: 12,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
            wgpu::BindGroupEntry {
                binding: 13,
                resource: self.uniforms.as_entire_binding(),
            },
        ]
    }

    /// Holds `uniforms` for the frame's next submission.
    pub fn write(&self, queue: &wgpu::Queue, uniforms: &WaveTileUniforms) {
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(uniforms));
    }

    /// Moves the transformed layers that `uniforms` draws into the scene's
    /// texture, after `dispatch` has run.
    pub fn copy(&self, encoder: &mut wgpu::CommandEncoder, uniforms: &WaveTileUniforms) {
        let layers = uniforms.drawn_layers().len() as u32;
        if layers == 0 {
            return;
        }
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &self.texels,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(TEXELS * 4),
                    rows_per_image: Some(TEXELS),
                },
            },
            self.texture.as_image_copy(),
            wgpu::Extent3d {
                width: TEXELS,
                height: TEXELS,
                depth_or_array_layers: layers,
            },
        );
    }

    /// Transforms the layers that `uniforms`, as written, draws.
    pub fn dispatch(
        &self,
        pass: &mut wgpu::ComputePass<'_>,
        scene_uniforms: &wgpu::BindGroup,
        uniforms: &WaveTileUniforms,
    ) {
        let [columns, rows, layers] = uniforms.workgroups();
        if layers == 0 {
            return;
        }
        pass.set_bind_group(0, scene_uniforms, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_pipeline(&self.columns_pipeline);
        pass.dispatch_workgroups(columns, layers, 1);
        pass.set_pipeline(&self.texel_rows_pipeline);
        pass.dispatch_workgroups(rows, layers, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The drawn tiles and the wave modes together hold no more than Cox and
    // Munk's (1954) slope variance along or across the wind, so the
    // unresolved facets keep slope on both axes, from calm to beyond the
    // strongest sustained wind the weather raises and down to footprints
    // finer than any tile. The drawn octaves take the variance
    // `decorative_water_normal_sample` in ocean-waves.wgsl gives them.
    #[test]
    fn drawn_short_waves_leave_unresolved_slopes_along_and_across_the_wind() {
        for wind_m_s in [0.5, 1.25, 2.0, 3.7, 6.8, 11.6, 20.0, 30.0, 40.0] {
            let cox_munk = [3.16e-3, 1.92e-3].map(|slope| 0.0015 + slope * wind_m_s);
            let modes = game::wind_sea_slope_variance(
                WorldGeometry::default(),
                0x5eed,
                Vector::new(wind_m_s as f32, 0.0),
            );
            for footprint_m in [1.0e-4, 1.0e-3, 0.014, 0.05, 0.22, 1.0] {
                for tier_layers in 1..=WAVE_TILE_LAYERS {
                    let uniforms = wave_tile_uniforms(
                        WorldGeometry::default(),
                        0x5eed,
                        0,
                        0.0,
                        [0.0; 2],
                        [1920.0 * footprint_m, 1080.0 * footprint_m],
                        footprint_m,
                        [wind_m_s as f32, 0.0],
                        tier_layers,
                        [0.0; 2],
                    );
                    let total = cox_munk[0] + cox_munk[1];
                    let peak_m = TAU / f64::from(uniforms.sea[2]);
                    let octave = total / (peak_m / SHORTEST_GRAVITY_WAVE_M).log2();
                    let [unresolved, drawn, skipped] =
                        [0, 1, 2].map(|index| f64::from(uniforms.octaves[index]));
                    let drawn_octave = octave
                        * ((total - modes[0] - modes[1] - octave * unresolved)
                            / (octave * (drawn + skipped)))
                            .clamp(0.0, 1.0);
                    for (axis, tile) in [1, 3].into_iter().enumerate() {
                        let tiles = drawn_octave
                            * uniforms
                                .layers
                                .iter()
                                .map(|layer| f64::from(layer.tile[tile]))
                                .sum::<f64>();
                        assert!(
                            modes[axis] + tiles <= cox_munk[axis],
                            "{wind_m_s} m/s, {footprint_m} m footprint, {tier_layers} layers, axis {axis}: modes {} and tiles {tiles} of {}",
                            modes[axis],
                            cox_munk[axis]
                        );
                    }
                }
            }
        }
    }
}
