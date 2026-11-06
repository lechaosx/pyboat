use std::{borrow::Cow, mem};

use bytemuck::{Pod, Zeroable};
use rapier2d::prelude::Vector;
use simulation::WorldGeometry;

use crate::{
    BOAT_IMAGES, BoatScene, ClothPose, boat_mesh_with_cloth, boat_sheer_height_m, hull_sprite_mesh,
    sprite_mip_chain, windsock_mesh_with_cloth,
};

/// The scene pass's second target: the height above mean sea level of the
/// surface each pixel shows, which boats hide behind. Half precision steps
/// by 1/1024 to 1/2048 of the height, so it resolves millimetres where boats
/// meet the water and shore, near sea level, and coarsens only on high land.
pub const SURFACE_HEIGHT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;

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
    pub(crate) position: [f32; 3],
    pub(crate) color: [f32; 3],
    pub(crate) is_terrain: f32,
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32];
    const SCENE_ATTRIBUTES: [wgpu::VertexAttribute; 3] = [
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            ..Self::ATTRIBUTES[0]
        },
        Self::ATTRIBUTES[1],
        Self::ATTRIBUTES[2],
    ];

    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::SCENE_ATTRIBUTES,
        }
    }

    fn boat_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            attributes: &Self::ATTRIBUTES,
            ..Self::layout()
        }
    }
}

struct SpriteBatch {
    image: usize,
    sail: bool,
    vertices: std::ops::Range<u32>,
}

/// Hulls, rigging, sails and windsocks, drawn over a finished view with the
/// scene's first bind group.
pub struct BoatSprites {
    sprite_pipeline: wgpu::RenderPipeline,
    sail_depth_pipeline: wgpu::RenderPipeline,
    sail_pipeline: wgpu::RenderPipeline,
    bind_groups: Vec<wgpu::BindGroup>,
    surface_height_layout: wgpu::BindGroupLayout,
    sail_depth: Option<((u32, u32), wgpu::TextureView)>,
    vertex_buffer: wgpu::Buffer,
    vertex_capacity: usize,
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
        for image in BOAT_IMAGES {
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
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                }],
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
        let pipeline = |entry_point, sail, write_mask, depth_stencil| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry_point),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_boat_sprite"),
                    buffers: &[Some(Vertex::boat_layout())],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry_point),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: output_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &[("IS_SAIL", if sail { 1.0 } else { 0.0 })],
                        ..Default::default()
                    },
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
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
        Ok(Self {
            sprite_pipeline: pipeline("fragment_boat_sprite", false, wgpu::ColorWrites::ALL, None),
            sail_depth_pipeline: pipeline(
                "fragment_sail_depth",
                true,
                wgpu::ColorWrites::empty(),
                depth(true, wgpu::CompareFunction::LessEqual),
            ),
            sail_pipeline: pipeline(
                "fragment_boat_sprite",
                true,
                wgpu::ColorWrites::ALL,
                depth(false, wgpu::CompareFunction::Equal),
            ),
            bind_groups,
            surface_height_layout,
            sail_depth: None,
            vertex_buffer: vertex_buffer(device, INITIAL_VERTEX_CAPACITY),
            vertex_capacity: INITIAL_VERTEX_CAPACITY,
        })
    }

    /// Draws `scenes` over `view`, whose size is `output_size`, for a view
    /// `view_height_m` tall centred on `camera_m`, behind the surfaces in
    /// `surface_height`, a `surface_height_texture` the scene pass wrote.
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        output_size: (u32, u32),
        scene_bind_group: &wgpu::BindGroup,
        surface_height: &wgpu::TextureView,
        scenes: &[BoatScene],
        cloth: &[ClothPose],
        world_geometry: WorldGeometry,
        match_seed: u64,
        camera_m: Vector,
        view_height_m: f32,
        time_seconds: f64,
    ) {
        let (vertices, batches) = sprite_vertices(
            scenes,
            cloth,
            world_geometry,
            match_seed,
            camera_m,
            output_size.1,
            view_height_m,
            time_seconds,
        );
        if vertices.is_empty() {
            return;
        }
        let surface_height = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Surface height bind group"),
            layout: &self.surface_height_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(surface_height),
            }],
        });
        if vertices.len() > self.vertex_capacity {
            self.vertex_capacity = vertices.len().next_power_of_two();
            self.vertex_buffer = vertex_buffer(device, self.vertex_capacity);
        }
        queue.write_buffer(&self.vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        if batches.iter().any(|batch| batch.sail)
            && self
                .sail_depth
                .as_ref()
                .is_none_or(|(size, _)| *size != output_size)
        {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Sail depth"),
                size: wgpu::Extent3d {
                    width: output_size.0,
                    height: output_size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.sail_depth = Some((output_size, texture.create_view(&Default::default())));
        }
        let mut batches = batches.iter().peekable();
        while let Some(&batch) = batches.peek() {
            let depth = batch.sail.then(|| wgpu::RenderPassDepthStencilAttachment {
                view: &self.sail_depth.as_ref().unwrap().1,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Discard,
                }),
                stencil_ops: None,
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(if batch.sail { "Sail pass" } else { "Boat pass" }),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: depth,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, scene_bind_group, &[]);
            pass.set_bind_group(2, &surface_height, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            if batch.sail {
                let batch = batches.next().unwrap();
                pass.set_bind_group(1, &self.bind_groups[batch.image], &[]);
                // Resolve the nearest fold before alpha blending its filtered edge.
                pass.set_pipeline(&self.sail_depth_pipeline);
                pass.draw(batch.vertices.clone(), 0..1);
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
    }
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
fn sprite_vertices(
    scenes: &[BoatScene],
    cloth: &[ClothPose],
    world_geometry: WorldGeometry,
    match_seed: u64,
    camera_m: Vector,
    output_height: u32,
    view_height_m: f32,
    time_seconds: f64,
) -> (Vec<Vertex>, Vec<SpriteBatch>) {
    let mut vertices = Vec::new();
    let mut batches: Vec<SpriteBatch> = Vec::new();
    for (scene, &cloth) in scenes.iter().zip(cloth) {
        let state = if scene.wreck {
            1.0
        } else if scene.recently_damaged {
            2.0
        } else {
            0.0
        };
        let center_m = (scene.hull_m[0] + (scene.hull_m[7] + scene.hull_m[8]) * 0.5) * 0.5;
        let sheer_m = boat_sheer_height_m(world_geometry, match_seed, time_seconds, center_m);
        let sock = scene
            .windsock
            .map(|sock| windsock_mesh_with_cloth(sock, time_seconds, cloth))
            .unwrap_or_default();
        for sprite in hull_sprite_mesh(scene)
            .into_iter()
            .chain(boat_mesh_with_cloth(
                scene,
                time_seconds,
                view_height_m / output_height as f32,
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
            let relative_m = sprite.position_m - camera_m;
            vertices.push(Vertex {
                position: [relative_m.x, relative_m.y, sheer_m + sprite.height_m],
                color: [
                    sprite.uv[0],
                    sprite.uv[1],
                    if sprite.image == 0 {
                        state
                    } else {
                        -sprite.shade
                    },
                ],
                is_terrain: sprite.opacity - 1.0,
            });
        }
    }
    (vertices, batches)
}
