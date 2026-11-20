use std::{
    borrow::Cow,
    f64::consts::{SQRT_2, TAU},
};

use bytemuck::{Pod, Zeroable};
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
}

impl WaveTileUniforms {
    fn drawn_layers(&self) -> u32 {
        self.octaves[2] as u32
    }

    /// Workgroups of the transform's column pass, then of its texel-row
    /// pass, then the layers each transforms.
    pub fn workgroups(&self) -> [u32; 3] {
        [COLUMNS, TEXELS, self.drawn_layers()]
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

/// The share of slope variance along the wind of waves of `frequency_ratio`
/// times the peak's angular frequency, under the directional spreading
/// `wave_tile_spreading` in wave-tile-generate.wgsl gives them; keep the two
/// synchronized.
fn along_wind_share(frequency_ratio: f64) -> f64 {
    // The peak's floor keeps the ratio above 1, where β is at most 2.28, and
    // 16 midpoints then leave under 5e-4 of the share.
    const STEPS: u32 = 16;
    let beta = if frequency_ratio < 1.6 {
        2.28 * frequency_ratio.powf(-1.3)
    } else {
        10.0f64.powf(-0.4 + 0.8393 * (-0.567 * (frequency_ratio * frequency_ratio).ln()).exp())
    };
    let (density, along) = (0..STEPS).fold((0.0, 0.0), |(density, along), step| {
        let angle = TAU * ((f64::from(step) + 0.5) / f64::from(STEPS) - 0.5);
        let weight = (beta * angle).cosh().powi(-2);
        (density + weight, along + weight * angle.cos().powi(2))
    });
    along / density
}

/// The shares of an octave's slope variance, from `lowest` to twice that
/// wavenumber, that a pixel of `footprint_m` resolves along and across the
/// wind: Phillips' equal variance per logarithm of wavenumber, filtered by
/// exp(-(k fp)²) as each wave mode's amplitude is by exp(-(k fp)² / 2).
fn resolved_octave_shares(lowest_per_m: f64, footprint_m: f64, peak_frequency: f64) -> [f64; 2] {
    // 8 midpoints in log wavenumber leave under 3e-4 of an octave.
    const STEPS: u32 = 8;
    (0..STEPS).fold([0.0; 2], |[along, across], step| {
        let k = lowest_per_m * (2.0f64).powf((f64::from(step) + 0.5) / f64::from(STEPS));
        let resolved = (-(k * footprint_m).powi(2)).exp() / f64::from(STEPS);
        let frequency = (9.81 * k + 7.1e-5 * k.powi(3)).sqrt();
        let share = along_wind_share(frequency / peak_frequency);
        [along + resolved * share, across + resolved * (1.0 - share)]
    })
}

/// The wave tiles at the world clock for a view centred on `camera_m` whose
/// sea-level pixels span `pixel_footprint_m`, under the sustained surface
/// wind at the view centre, drawing at most `tier_layers` layers.
#[allow(clippy::too_many_arguments)]
pub fn wave_tile_uniforms(
    world_geometry: WorldGeometry,
    world_tick: u64,
    world_tick_fraction: f32,
    camera_m: [f32; 2],
    view_m: [f32; 2],
    pixel_footprint_m: f32,
    surface_wind_m_s: [f32; 2],
    tier_layers: usize,
) -> WaveTileUniforms {
    let width_m = f64::from(world_geometry.width_m() as f32);
    let footprint_m = f64::from(pixel_footprint_m);
    let [camera_x_m, camera_y_m] = camera_m.map(f64::from);
    let wind = surface_wind_m_s.map(f64::from);
    let speed = wind[0].hypot(wind[1]);
    let direction = if speed > 0.0 {
        wind.map(|component| component / speed)
    } else {
        [1.0, 0.0]
    };
    // The spectrum's peak, no shorter than the decorative octaves reach, as
    // `wave_peak_wavelength_m` in ocean-waves.wgsl.
    let peak_m = (TAU / game::wind_sea(speed).peak_wavenumber_per_m).max(SQRT_2 * LONGEST_OCTAVE_M);
    let peak_frequency = (9.81 * TAU / peak_m).sqrt();
    let octave_m = |octave: usize| LONGEST_OCTAVE_M / (1u64 << octave) as f64;
    // Each octave reaches half an octave either side of its wavelength.
    let lowest_wavenumber = |octave: usize| TAU / (SQRT_2 * octave_m(octave));
    let shares: [[f64; 2]; 2 * WAVE_TILE_LAYERS] = std::array::from_fn(|octave| {
        resolved_octave_shares(lowest_wavenumber(octave), footprint_m, peak_frequency)
    });
    // Layers the tier or the footprint leaves out keep their share with the
    // unresolved slopes, and are neither transformed nor copied.
    let drawn = (0..WAVE_TILE_LAYERS.min(tier_layers))
        .take_while(|layer| shares[2 * layer][0] + shares[2 * layer][1] >= DRAWN_SHARE)
        .count();
    let drawn_shares = |octave: usize| {
        if octave / 2 < drawn {
            shares[octave]
        } else {
            [0.0; 2]
        }
    };
    let drawn_share = |octave: usize| drawn_shares(octave)[0] + drawn_shares(octave)[1];
    let resolvable: f64 = (0..2 * WAVE_TILE_LAYERS).map(drawn_share).sum();
    let shortest_m = octave_m(2 * WAVE_TILE_LAYERS - 1) / SQRT_2;
    let unresolved = (shortest_m / SHORTEST_GRAVITY_WAVE_M).log2()
        + (0..2 * WAVE_TILE_LAYERS)
            .map(|octave| 1.0 - drawn_share(octave))
            .sum::<f64>();
    let tile_m =
        |layer: usize| width_m / (width_m / (TILE_WAVELENGTHS * octave_m(2 * layer))).round();
    // A tile longer than twice the view's diagonal never shows two copies of
    // a wave; the tiling blends in as it shortens to one diagonal.
    let tiling = |layer: usize| {
        let ratio = (tile_m(layer) / f64::from(view_m[0].hypot(view_m[1])) - 1.0).clamp(0.0, 1.0);
        1.0 - ratio * ratio * (3.0 - 2.0 * ratio)
    };
    // Tiles shorten fourfold from layer to layer, so a zoom blends one layer
    // at a time; the forced evaluation blends the one at the edge between
    // read and tiled layers, the frame's worst case.
    let tier_layers = WAVE_TILE_LAYERS.min(tier_layers);
    let forced_blend = (0..tier_layers)
        .rev()
        .find(|&layer| tiling(layer) < 1.0)
        .unwrap_or(0);
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
                (if layer < drawn {
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
            peak_frequency as f32,
            0.0,
        ],
        octaves: [
            unresolved as f32,
            resolvable as f32,
            drawn as f32,
            tier_layers as f32,
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
        let layers = uniforms.drawn_layers();
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
