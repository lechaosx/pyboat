use std::{borrow::Cow, mem};

use bytemuck::{Pod, Zeroable};
use rapier2d_f64::glamx::Vec2 as Vector;
use simulation::WorldGeometry;

use crate::{
    BOAT_IMAGES, BoatScene, ClothPose, boat_mesh_with_cloth, drawn_boat_pose, hull_sprite_mesh,
    sprite_mip_chain, tilted_by_deck, windsock_mesh_with_cloth,
};

/// The scene pass's second target: the height above mean sea level of the
/// surface each pixel shows, which boats hide behind. Half precision steps
/// by 1/1024 to 1/2048 of the height, so it resolves millimetres where boats
/// meet the water and shore, near sea level, and coarsens only on high land.
pub const SURFACE_HEIGHT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;

/// Clears a surface height target to the lowest half-precision height, which
/// hides nothing. Keep synchronized with `drawn_surface_height_m` in
/// scene-state.wgsl.
pub const SURFACE_HEIGHT_CLEAR: wgpu::Color = wgpu::Color {
    r: -65504.0,
    ..wgpu::Color::TRANSPARENT
};

/// A surface height target for a scene of `size`.
pub fn surface_height_texture(device: &wgpu::Device, size: (u32, u32)) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Surface height"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SURFACE_HEIGHT_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

const INITIAL_VERTEX_CAPACITY: usize = 32768;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
    coverage: f32,
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32];

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

pub(crate) struct SpriteBatch {
    pub(crate) image: usize,
    pub(crate) sail: bool,
    pub(crate) vertices: std::ops::Range<u32>,
}

/// Hulls, rigging, sails and windsocks, drawn over a finished view with the
/// scene's first bind group.
pub struct BoatSprites {
    pub shadows: crate::boat_shadows::BoatShadowMaps,
    prepared: Option<(Vec<Vertex>, Vec<SpriteBatch>)>,
    sprite_pipeline: wgpu::RenderPipeline,
    sail_depth_pipeline: wgpu::RenderPipeline,
    sail_pipeline: wgpu::RenderPipeline,
    bind_groups: Vec<wgpu::BindGroup>,
    surface_height_layout: wgpu::BindGroupLayout,
    targets: Option<BoatTargets>,
    target_buffer: wgpu::Buffer,
    composite_layout: wgpu::BindGroupLayout,
    composite_pipeline: wgpu::RenderPipeline,
    format: wgpu::TextureFormat,
    vision: bool,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
}

struct BoatTargets {
    size: (u32, u32),
    color: wgpu::TextureView,
    resolved: wgpu::TextureView,
    auxiliary: Option<(wgpu::TextureView, wgpu::TextureView)>,
    depth: wgpu::TextureView,
    sail_depth: wgpu::TextureView,
    composite: wgpu::BindGroup,
}

impl BoatSprites {
    /// Pipelines for `output_format` that read the scene's first group
    /// through `scene_layout`.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene_layout: &wgpu::BindGroupLayout,
        output_format: wgpu::TextureFormat,
    ) -> Result<Self, String> {
        Self::with_images(device, queue, scene_layout, output_format, BOAT_IMAGES)
    }

    /// `new` drawing the PNG `images` in place of `BOAT_IMAGES`.
    pub fn with_images(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene_layout: &wgpu::BindGroupLayout,
        output_format: wgpu::TextureFormat,
        images: [&[u8]; 5],
    ) -> Result<Self, String> {
        let vision = output_format == crate::VISION_COLOR_FORMAT;
        let sprite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Boat sprite layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Boat sprite sampler"),
            anisotropy_clamp: 8,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let mut bind_groups = Vec::new();
        for image in images {
            let mut decoder = png::Decoder::new(std::io::Cursor::new(image));
            decoder
                .set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
            let mut reader = decoder.read_info().map_err(|error| error.to_string())?;
            let mut pixels = vec![
                0;
                reader
                    .output_buffer_size()
                    .ok_or("boat sprite is too large")?
            ];
            let info = reader
                .next_frame(&mut pixels)
                .map_err(|error| error.to_string())?;
            let mips = sprite_mip_chain(info.width, info.height, &pixels[..info.buffer_size()]);
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Wooden dinghy sprite"),
                size: wgpu::Extent3d {
                    width: info.width,
                    height: info.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mips.len() as u32,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            for (level, mip) in mips.iter().enumerate() {
                let mut destination = texture.as_image_copy();
                destination.mip_level = level as u32;
                queue.write_texture(
                    destination,
                    &mip.rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(mip.width * 4),
                        rows_per_image: Some(mip.height),
                    },
                    wgpu::Extent3d {
                        width: mip.width,
                        height: mip.height,
                        depth_or_array_layers: 1,
                    },
                );
            }
            let view = texture.create_view(&Default::default());
            bind_groups.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Boat sprite bind group"),
                layout: &sprite_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            }));
        }
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Boat sprite shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(concat!(
                env!("OUT_DIR"),
                "/boat-sprite.wgsl"
            )))),
        });
        let surface_height_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Surface height layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: true,
                        },
                        count: None,
                    },
                ]
                .into_iter()
                .chain([0].map(|binding| wgpu::BindGroupLayoutEntry {
                    binding,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }))
                .collect::<Vec<_>>(),
            });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Boat sprite pipeline layout"),
            bind_group_layouts: &[
                Some(scene_layout),
                Some(&sprite_layout),
                Some(&surface_height_layout),
            ],
            immediate_size: 0,
        });
        let pipeline = |entry_point, sail, write_mask: wgpu::ColorWrites, depth_stencil| {
            // Passes keep the auxiliary slot even without vision, so the
            // pipelines declare it too: native wgpu compares trailing empty
            // slots, which WebGPU ignores.
            let targets = [
                Some(wgpu::ColorTargetState {
                    format: output_format,
                    blend: None,
                    write_mask,
                }),
                vision.then(|| wgpu::TextureFormat::Rg16Float.into()),
            ];
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_boat_sprite"),
                    buffers: &[Some(Vertex::layout())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    targets: if write_mask.is_empty() { &[] } else { &targets },
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[
                            ("IS_SAIL", if sail { 1.0 } else { 0.0 }),
                            ("VISION_INTERMEDIATE", f64::from(vision)),
                        ],
                        ..Default::default()
                    },
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil,
                multisample: wgpu::MultisampleState {
                    count: 4,
                    ..Default::default()
                },
                multiview_mask: None,
                cache: None,
            })
        };
        let depth = |write, compare| {
            Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(write),
                depth_compare: Some(compare),
                stencil: Default::default(),
                bias: Default::default(),
            })
        };
        let target_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Boat target bounds"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Boat composite layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Boat composite shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!(concat!(
                env!("OUT_DIR"),
                "/boat-composite.wgsl"
            )))),
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Boat composite"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&composite_layout)],
                    immediate_size: 0,
                }),
            ),
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: Some("vertex"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: Some(if vision {
                    "fragment_vision"
                } else {
                    "fragment"
                }),
                targets: &[
                    Some(output_format),
                    vision.then_some(wgpu::TextureFormat::Rg16Float),
                ]
                .map(|format| {
                    format.map(|format| wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })
                }),
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        Ok(Self {
            shadows: crate::boat_shadows::BoatShadowMaps::new(device),
            prepared: None,
            sprite_pipeline: pipeline(
                if vision {
                    "fragment_boat_vision"
                } else {
                    "fragment_boat_sprite"
                },
                false,
                wgpu::ColorWrites::ALL,
                depth(true, wgpu::CompareFunction::LessEqual),
            ),
            sail_depth_pipeline: pipeline(
                "fragment_sail_depth",
                true,
                wgpu::ColorWrites::empty(),
                depth(true, wgpu::CompareFunction::LessEqual),
            ),
            sail_pipeline: pipeline(
                if vision {
                    "fragment_sail_vision"
                } else {
                    "fragment_sail"
                },
                true,
                wgpu::ColorWrites::ALL,
                depth(true, wgpu::CompareFunction::LessEqual),
            ),
            bind_groups,
            surface_height_layout,
            targets: None,
            target_buffer,
            composite_layout,
            composite_pipeline,
            format: output_format,
            vision,
            vertex_buffer: vertex_buffer(device, INITIAL_VERTEX_CAPACITY),
            vertex_capacity: INITIAL_VERTEX_CAPACITY,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_shadows(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scenes: &[BoatScene],
        cloth: &[ClothPose],
        world_geometry: WorldGeometry,
        match_seed: u64,
        camera_m: Vector,
        output_size: (u32, u32),
        view_height_m: f32,
        scene_footprint_m: f32,
        time_seconds: f64,
        lightning: [[f32; 4]; 4],
    ) -> bool {
        let (vertices, batches) = sprite_vertices(
            scenes,
            cloth,
            world_geometry,
            match_seed,
            camera_m,
            output_size,
            view_height_m,
            scene_footprint_m,
            time_seconds,
        );
        let conditions = game::world_celestial_conditions(world_geometry, time_seconds, camera_m);
        let directions = game::celestial_directions_at_height(
            world_geometry,
            game::celestial_orbit(conditions),
            conditions,
            0.0,
        );
        let positions: Vec<_> = vertices.iter().map(|v| v.position).collect();
        let frame = crate::boat_shadows::boat_shadow_uniforms(
            &positions,
            [directions.0, directions.1],
            lightning,
            [
                world_geometry.width_m() as f32,
                world_geometry.height_m() as f32,
            ],
        );
        let resized = self.shadows.update(device, queue, &frame);
        if vertices.len() > self.vertex_capacity {
            self.vertex_capacity = vertices.len().next_power_of_two();
            self.vertex_buffer = vertex_buffer(device, self.vertex_capacity);
        }
        if !vertices.is_empty() {
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
        self.shadows
            .encode(encoder, &self.vertex_buffer, &batches, &self.bind_groups);
        self.prepared = Some((vertices, batches));
        resized
    }

    /// Draws `scenes` over `view`, whose size is `output_size`, for a view
    /// `view_height_m` tall centred on `camera_m`, behind the surfaces in
    /// `surface_height`, a `surface_height_texture` the scene pass wrote at
    /// a pixel footprint of `scene_footprint_m`.
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        luminance: Option<&wgpu::TextureView>,
        output_size: (u32, u32),
        scene_bind_group: &wgpu::BindGroup,
        surface_height: &wgpu::TextureView,
        scenes: &[BoatScene],
        cloth: &[ClothPose],
        world_geometry: WorldGeometry,
        match_seed: u64,
        camera_m: Vector,
        view_height_m: f32,
        camera_height_m: f32,
        scene_footprint_m: f32,
        time_seconds: f64,
    ) {
        let prepared = self.prepared.take();
        let uploaded = prepared.is_some();
        let (vertices, batches) = prepared.unwrap_or_else(|| {
            sprite_vertices(
                scenes,
                cloth,
                world_geometry,
                match_seed,
                camera_m,
                output_size,
                view_height_m,
                scene_footprint_m,
                time_seconds,
            )
        });
        let Some(rect) = occupied_rect(&vertices, output_size, view_height_m, camera_height_m)
        else {
            self.targets = None;
            return;
        };
        let required = (rect[2], rect[3]);
        let size = (
            (required.0.div_ceil(64) * 64).min(output_size.0),
            (required.1.div_ceil(64) * 64).min(output_size.1),
        );
        if self.targets.as_ref().is_none_or(|targets| {
            targets.size.0 < required.0
                || targets.size.1 < required.1
                || size.0 < targets.size.0 / 2
                || size.1 < targets.size.1 / 2
                || targets.size.0 > output_size.0
                || targets.size.1 > output_size.1
        }) {
            self.targets = Some(BoatTargets::new(
                device,
                size,
                self.format,
                &self.composite_layout,
                &self.target_buffer,
                self.vision,
            ));
        }
        let targets = self.targets.as_ref().unwrap();
        let (width, height) = (targets.size.0 as f32, targets.size.1 as f32);
        queue.write_buffer(
            &self.target_buffer,
            0,
            bytemuck::cast_slice(&[
                output_size.0 as f32 / width,
                output_size.1 as f32 / height,
                (output_size.0 as f32 - 2.0 * rect[0] as f32) / width - 1.0,
                1.0 - (output_size.1 as f32 - 2.0 * rect[1] as f32) / height,
                rect[0] as f32,
                rect[1] as f32,
                width,
                height,
            ]),
        );
        let surface_group = |fold_depth| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Boat surfaces and target"),
                layout: &self.surface_height_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(surface_height),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.target_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(fold_depth),
                    },
                ],
            })
        };
        let color_surfaces = surface_group(&targets.sail_depth);
        let fold_surfaces = surface_group(&targets.depth);
        if !uploaded {
            if vertices.len() > self.vertex_capacity {
                self.vertex_capacity = vertices.len().next_power_of_two();
                self.vertex_buffer = vertex_buffer(device, self.vertex_capacity);
            }
            queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
        let mut first = true;
        let mut batches = batches.iter().peekable();
        while let Some(&batch) = batches.peek() {
            if batch.sail {
                // Cloth hides its own rear folds even where its silhouette is
                // filtered; those folds must not tint the cloth's front edge.
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Sail nearest fold"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &targets.sail_depth,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_bind_group(0, scene_bind_group, &[]);
                pass.set_bind_group(1, &self.bind_groups[batch.image], &[]);
                pass.set_bind_group(2, &fold_surfaces, &[]);
                pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
                pass.set_pipeline(&self.sail_depth_pipeline);
                pass.draw(batch.vertices.clone(), 0..1);
            }
            let final_pass = if batch.sail {
                batches.len() == 1
            } else {
                batches.clone().all(|b| !b.sail)
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Boat shared depth"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &targets.color,
                        depth_slice: None,
                        resolve_target: final_pass.then_some(&targets.resolved),
                        ops: wgpu::Operations {
                            load: if first {
                                wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                            } else {
                                wgpu::LoadOp::Load
                            },
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    targets.auxiliary.as_ref().map(|(view, resolved)| {
                        wgpu::RenderPassColorAttachment {
                            view,
                            depth_slice: None,
                            resolve_target: final_pass.then_some(resolved),
                            ops: wgpu::Operations {
                                load: if first {
                                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                                } else {
                                    wgpu::LoadOp::Load
                                },
                                store: wgpu::StoreOp::Store,
                            },
                        }
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: if first {
                            wgpu::LoadOp::Clear(1.0)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            first = false;
            pass.set_bind_group(0, scene_bind_group, &[]);
            pass.set_bind_group(2, &color_surfaces, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            if batch.sail {
                let batch = batches.next().unwrap();
                pass.set_bind_group(1, &self.bind_groups[batch.image], &[]);
                pass.set_pipeline(&self.sail_pipeline);
                pass.draw(batch.vertices.clone(), 0..1);
            } else {
                pass.set_pipeline(&self.sprite_pipeline);
                while let Some(batch) = batches.next_if(|batch| !batch.sail) {
                    pass.set_bind_group(1, &self.bind_groups[batch.image], &[]);
                    pass.draw(batch.vertices.clone(), 0..1);
                }
            }
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Boat composite"),
            color_attachments: &[
                Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                }),
                luminance.map(|view| wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                }),
            ],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_scissor_rect(rect[0], rect[1], rect[2], rect[3]);
        pass.set_pipeline(&self.composite_pipeline);
        pass.set_bind_group(0, &targets.composite, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl BoatTargets {
    fn new(
        device: &wgpu::Device,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        layout: &wgpu::BindGroupLayout,
        bounds: &wgpu::Buffer,
        vision: bool,
    ) -> Self {
        let texture = |label, format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let color = texture(
            "Boat coverage",
            format,
            4,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        );
        let resolved = texture(
            "Boat resolved coverage",
            format,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth_usage =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let depth = texture(
            "Boat shared depth",
            wgpu::TextureFormat::Depth32Float,
            4,
            depth_usage,
        );
        let sail_depth = texture(
            "Sail nearest fold",
            wgpu::TextureFormat::Depth32Float,
            4,
            depth_usage,
        );
        let auxiliary = vision.then(|| {
            (
                texture(
                    "Boat auxiliary",
                    wgpu::TextureFormat::Rg16Float,
                    4,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                ),
                texture(
                    "Boat auxiliary resolved",
                    wgpu::TextureFormat::Rg16Float,
                    1,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                ),
            )
        });
        let composite = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Boat composite"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&resolved),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: bounds.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(
                        auxiliary.as_ref().map_or(&resolved, |(_, view)| view),
                    ),
                },
            ],
        });
        Self {
            size,
            color,
            resolved,
            auxiliary,
            depth,
            sail_depth,
            composite,
        }
    }
}

fn occupied_rect(
    vertices: &[Vertex],
    output: (u32, u32),
    view_height_m: f32,
    camera_height_m: f32,
) -> Option<[u32; 4]> {
    let view_width_m = view_height_m * output.0 as f32 / output.1 as f32;
    let mut low = Vector::new(output.0 as f32, output.1 as f32);
    let mut high = Vector::ZERO;
    for vertex in vertices {
        let [x, y, z] = vertex.position;
        let scale = 1.0 - z / camera_height_m;
        if scale <= 0.0 {
            return Some([0, 0, output.0, output.1]);
        }
        let pixel = Vector::new(
            (x / (view_width_m * scale) + 0.5) * output.0 as f32,
            (0.5 - y / (view_height_m * scale)) * output.1 as f32,
        );
        low = low.min(pixel);
        high = high.max(pixel);
    }
    let low = low.floor().max(Vector::ZERO);
    let high = high
        .ceil()
        .min(Vector::new(output.0 as f32, output.1 as f32));
    (high.x > low.x && high.y > low.y).then_some({
        [
            low.x as u32,
            low.y as u32,
            (high.x - low.x) as u32,
            (high.y - low.y) as u32,
        ]
    })
}

fn vertex_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Boat sprite vertices"),
        size: (capacity * mem::size_of::<Vertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sprite_vertices(
    scenes: &[BoatScene],
    cloth: &[ClothPose],
    world_geometry: WorldGeometry,
    match_seed: u64,
    camera_m: Vector,
    output_size: (u32, u32),
    view_height_m: f32,
    scene_footprint_m: f32,
    time_seconds: f64,
) -> (Vec<Vertex>, Vec<SpriteBatch>) {
    let mut vertices = Vec::new();
    let mut batches: Vec<SpriteBatch> = Vec::new();
    let mut ordered: Vec<_> = scenes.iter().zip(cloth).collect();
    // Authored deck details keep their local order; distinct coplanar boats
    // break depth ties by persistent identity.
    ordered.sort_by(|(a, _), (b, _)| a.id.cmp(&b.id));
    for (scene, &cloth) in ordered {
        let state = if scene.wreck {
            1.0
        } else if scene.recently_damaged {
            2.0
        } else {
            0.0
        };
        let stern_m = (scene.hull_m[7] + scene.hull_m[8]) * 0.5;
        let center_m = (scene.hull_m[0] + stern_m) * 0.5;
        let forward = scene.hull_m[0] - stern_m;
        let pose = drawn_boat_pose(
            world_geometry,
            match_seed,
            time_seconds,
            center_m,
            f64::from(forward.y.atan2(forward.x)),
            scene.sea.as_ref(),
            scene_footprint_m,
        );
        let sock = scene
            .windsock
            .map(|sock| windsock_mesh_with_cloth(sock, time_seconds, cloth))
            .unwrap_or_default();
        for sprite in hull_sprite_mesh(scene)
            .into_iter()
            .chain(boat_mesh_with_cloth(
                scene,
                time_seconds,
                view_height_m / output_size.1 as f32,
                cloth,
            ))
            .chain(sock)
        {
            let index = vertices.len() as u32;
            if let Some(batch) = batches
                .last_mut()
                .filter(|batch| batch.image == sprite.image && batch.sail == sprite.sail)
            {
                batch.vertices.end = index + 1;
            } else {
                batches.push(SpriteBatch {
                    image: sprite.image,
                    sail: sprite.sail,
                    vertices: index..index + 1,
                });
            }
            let (offset_m, height_m) = tilted_by_deck(
                sprite.position_m - center_m,
                sprite.height_m,
                pose.deck_slope,
            );
            let mut center_offset_m = center_m - camera_m;
            center_offset_m.x -= (center_offset_m.x / world_geometry.width_m() as f32).round()
                * world_geometry.width_m() as f32;
            let relative_m = center_offset_m + offset_m;
            vertices.push(Vertex {
                position: [relative_m.x, relative_m.y, pose.sheer_m + height_m],
                color: [
                    sprite.uv[0],
                    sprite.uv[1],
                    if sprite.image == 0 {
                        state
                    } else {
                        -sprite.shade
                    },
                ],
                coverage: sprite.opacity,
            });
        }
    }
    (vertices, batches)
}
