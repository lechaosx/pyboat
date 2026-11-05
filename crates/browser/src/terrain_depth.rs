use std::borrow::Cow;

use crate::{FieldGrids, TERRAIN_DEPTH_SHADER};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The depth of the first broad landform or sea surface each scene pixel's
/// eye ray meets, rasterized from a triangle grid on the landform layer's
/// texels. The scene reads `view` at binding 8 of its first group, through
/// `SCENE_LAYOUT_ENTRY`, and shades the terrain the camera sees there.
pub struct TerrainDepth {
    input_layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    pub view: wgpu::TextureView,
}

impl TerrainDepth {
    pub const SCENE_LAYOUT_ENTRY: wgpu::BindGroupLayoutEntry = wgpu::BindGroupLayoutEntry {
        binding: 8,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    };

    /// `size` is the scene's render size.
    pub fn new(device: &wgpu::Device, size: (u32, u32)) -> Self {
        let input_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Terrain depth input layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Terrain depth shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(TERRAIN_DEPTH_SHADER)),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Terrain depth pipeline layout"),
            bind_group_layouts: &[Some(&input_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Terrain depth pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_terrain_depth"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: None,
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let texture = depth_texture(device, size);
        Self {
            input_layout,
            pipeline,
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture,
        }
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// Reallocates the depth at a new scene size; the scene's bind group must
    /// then take the new `view`. Returns whether it did.
    pub fn resize(&mut self, device: &wgpu::Device, size: (u32, u32)) -> bool {
        if (self.texture.width(), self.texture.height()) == size {
            return false;
        }
        self.texture = depth_texture(device, size);
        self.view = self
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        true
    }

    /// The scene uniforms and the occluder field layers the pass reads.
    pub fn input_bind_group(
        &self,
        device: &wgpu::Device,
        uniform_buffer: &wgpu::Buffer,
        occluder_fields: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Terrain depth input"),
            layout: &self.input_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(occluder_fields),
                },
            ],
        })
    }

    /// Rasterizes the landform quads that hold the view, after the occluder
    /// pass has written the landform layer that `grids` describes.
    /// Timestamps, when given as a query set and a beginning index, mark the
    /// start of the pass.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        input: &wgpu::BindGroup,
        grids: &FieldGrids,
        timestamps: Option<(&wgpu::QuerySet, u32)>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Terrain depth pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.view,
                depth_ops: Some(wgpu::Operations {
                    // Sea level, where every eye ray ends.
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: timestamps.map(|(query_set, beginning)| {
                wgpu::RenderPassTimestampWrites {
                    query_set,
                    beginning_of_pass_write_index: Some(beginning),
                    end_of_pass_write_index: None,
                }
            }),
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let [columns, rows] = grids.landform_quads();
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, input, &[]);
        pass.draw(6 * columns.start..6 * columns.end, rows);
    }
}

fn depth_texture(device: &wgpu::Device, (width, height): (u32, u32)) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Terrain depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
