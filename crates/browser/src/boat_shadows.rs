use bytemuck::{Pod, Zeroable};
use rapier2d_f64::glamx::{Mat4, Vec2, Vec3, Vec4};

pub const SHADOW_LIGHTS: usize = 6;
pub const SHADOW_TEXELS: u32 = 1024;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct BoatShadowUniforms {
    pub matrices: [[[f32; 4]; 4]; SHADOW_LIGHTS],
    pub active: [[f32; 4]; SHADOW_LIGHTS],
}

pub fn boat_shadow_uniforms(
    positions: &[[f32; 3]],
    directions: [[f32; 3]; 2],
    lightning: [[f32; 4]; 4],
    world_size_m: [f32; 2],
) -> BoatShadowUniforms {
    let mut frame = BoatShadowUniforms::zeroed();
    if positions.is_empty() {
        return frame;
    }
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    let mut angular_extent = 0.0_f32;
    for &position in positions {
        let point = Vec3::from_array(position);
        minimum = minimum.min(point);
        maximum = maximum.max(point);
        angular_extent = angular_extent.max(
            std::f32::consts::TAU * point.x.abs() / world_size_m[0]
                + std::f32::consts::PI * point.y.abs() / world_size_m[1],
        );
    }
    let highest = maximum.z.max(game::BROAD_LANDFORM_HIGHEST_M as f32);
    let horizon =
        game::earth_horizon_z_at_height(f64::from(highest)) as f32 - 0.00465 - angular_extent;
    for index in 0..SHADOW_LIGHTS {
        let finite = index >= 2;
        let light = if finite {
            Vec3::from_slice(&lightning[index - 2][..3])
        } else {
            Vec3::from_array(directions[index])
        };
        let active = if finite {
            lightning[index - 2][3] > 0.0
        } else {
            light.z > horizon
        };
        if active {
            let (mut lower, mut upper) = (minimum, maximum);
            if finite {
                lower = Vec3::splat(f32::INFINITY);
                upper = Vec3::splat(f32::NEG_INFINITY);
                for &position in positions {
                    let mut point = Vec3::from_array(position);
                    point.x -= ((point.x - light.x) / world_size_m[0]).round() * world_size_m[0];
                    lower = lower.min(point);
                    upper = upper.max(point);
                }
            }
            let corners: [Vec3; 8] = std::array::from_fn(|corner| {
                Vec3::new(
                    if corner & 1 == 0 { lower.x } else { upper.x },
                    if corner & 2 == 0 { lower.y } else { upper.y },
                    if corner & 4 == 0 { lower.z } else { upper.z },
                )
            });
            frame.matrices[index] = light_projection(&corners, light, finite).to_cols_array_2d();
            frame.active[index] = [1.0, world_size_m[0], light.x, f32::from(finite)];
        }
    }
    frame
}

fn light_projection(points: &[Vec3], light: Vec3, finite: bool) -> Mat4 {
    let forward = if finite { -Vec3::Z } else { -light.normalize() };
    let reference = if forward.z.abs() < 0.99 {
        Vec3::Z
    } else {
        Vec3::Y
    };
    let right = forward.cross(reference).normalize();
    let up = right.cross(forward);
    let origin = if finite { light } else { Vec3::ZERO };
    let row = |axis: Vec3| axis.extend(-axis.dot(origin));
    let x = row(right);
    let y = row(up);
    let depth = row(forward);
    let w = if finite { depth } else { Vec4::W };
    let mut minimum = Vec3::splat(f32::INFINITY);
    let mut maximum = Vec3::splat(f32::NEG_INFINITY);
    for point in points {
        let p = point.extend(1.0);
        let scale = w.dot(p);
        let projected = Vec3::new(x.dot(p) / scale, y.dot(p) / scale, depth.dot(p));
        minimum = minimum.min(projected);
        maximum = maximum.max(projected);
    }
    // One border texel keeps the fitted silhouette inside the PCF footprint.
    let span = (maximum.truncate() - minimum.truncate()).max(Vec2::splat(1e-4));
    let padding = span / (SHADOW_TEXELS - 2) as f32;
    minimum -= padding.extend(0.0);
    maximum += padding.extend(0.0);
    let span = maximum - minimum;
    let x_clip = (x * 2.0 - w * (maximum.x + minimum.x)) / span.x;
    let y_clip = (y * 2.0 - w * (maximum.y + minimum.y)) / span.y;
    let z_clip = if finite {
        let near = (minimum.z - 1.0).max(minimum.z * 0.5);
        let far = maximum.z + 1.0;
        (depth - Vec4::W * near) * (far / (far - near))
    } else {
        (depth - Vec4::W * (minimum.z - 1.0)) / (span.z + 2.0)
    };
    Mat4::from_cols(x_clip, y_clip, z_clip, w).transpose()
}

pub struct BoatShadowMaps {
    pub view: wgpu::TextureView,
    pub uniforms: wgpu::Buffer,
    texture: wgpu::Texture,
    layers: Vec<wgpu::TextureView>,
    projections: wgpu::Buffer,
    input: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    active: [bool; SHADOW_LIGHTS],
    occupied: bool,
}

impl BoatShadowMaps {
    pub const SCENE_LAYOUT_ENTRIES: [wgpu::BindGroupLayoutEntry; 2] = [
        wgpu::BindGroupLayoutEntry {
            binding: 9,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 10,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ];

    pub fn new(device: &wgpu::Device) -> Self {
        let image_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Boat shadow image layout"),
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
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Boat shadow frame"),
            size: size_of::<BoatShadowUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let projections = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Boat light projections"),
            size: SHADOW_LIGHTS as u64 * 256,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let input_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Boat light projection layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let input = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Boat light projections"),
            layout: &input_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &projections,
                    offset: 0,
                    size: std::num::NonZeroU64::new(80),
                }),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Boat shadow shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader/boat-shadow.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Boat shadow pipeline layout"),
            bind_group_layouts: &[Some(&input_layout), Some(&image_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Boat shadow"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[Some(crate::boat_sprites::Vertex::layout())],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 1.0,
                    clamp: 0.0,
                },
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let (texture, view, layers) = shadow_texture(device, false);
        Self {
            view,
            uniforms,
            texture,
            layers,
            projections,
            input,
            pipeline,
            active: [false; SHADOW_LIGHTS],
            occupied: false,
        }
    }

    pub fn update(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &BoatShadowUniforms,
    ) -> bool {
        self.active = frame.active.map(|row| row[0] > 0.0);
        let occupied = self.active.contains(&true);
        let resized = self.occupied != occupied;
        if resized {
            self.texture.destroy();
            (self.texture, self.view, self.layers) = shadow_texture(device, occupied);
            self.occupied = occupied;
        }
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(frame));
        let mut rows = [0u8; SHADOW_LIGHTS * 256];
        for (index, matrix) in frame.matrices.iter().enumerate() {
            rows[index * 256..index * 256 + 64].copy_from_slice(bytemuck::bytes_of(matrix));
            rows[index * 256 + 64..index * 256 + 80]
                .copy_from_slice(bytemuck::bytes_of(&frame.active[index]));
        }
        queue.write_buffer(&self.projections, 0, &rows);
        resized
    }

    pub(crate) fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        vertices: &wgpu::Buffer,
        batches: &[crate::boat_sprites::SpriteBatch],
        images: &[wgpu::BindGroup],
    ) {
        for (index, &active) in self.active.iter().enumerate() {
            if !active {
                continue;
            }
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Boat light-space depth"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.layers[index],
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.input, &[index as u32 * 256]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            for batch in batches {
                pass.set_bind_group(1, &images[batch.image], &[]);
                pass.draw(batch.vertices.clone(), 0..if index >= 2 { 3 } else { 1 });
            }
        }
    }

    pub fn scene_entries(&self) -> [wgpu::BindGroupEntry<'_>; 2] {
        [
            wgpu::BindGroupEntry {
                binding: 9,
                resource: wgpu::BindingResource::TextureView(&self.view),
            },
            wgpu::BindGroupEntry {
                binding: 10,
                resource: self.uniforms.as_entire_binding(),
            },
        ]
    }
}

fn shadow_texture(
    device: &wgpu::Device,
    occupied: bool,
) -> (wgpu::Texture, wgpu::TextureView, Vec<wgpu::TextureView>) {
    let texels = if occupied { SHADOW_TEXELS } else { 1 };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Boat light-space depth"),
        size: wgpu::Extent3d {
            width: texels,
            height: texels,
            depth_or_array_layers: SHADOW_LIGHTS as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let layers = (0..SHADOW_LIGHTS)
        .map(|index| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: index as u32,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    (texture, view, layers)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projected(matrix: Mat4, point: Vec3) -> Vec3 {
        let p = matrix * point.extend(1.0);
        p.truncate() / p.w
    }

    #[test]
    fn directional_shadow_depth_orders_points_along_the_same_light_ray() {
        let light = Vec3::new(0.6, 0.0, 0.8);
        let points = [Vec3::new(-2.0, -1.0, 0.0), Vec3::new(2.0, 1.0, 5.0)];
        let matrix = light_projection(&points, light, false);
        let caster = Vec3::new(0.0, 0.0, 3.0);
        let receiver = caster - light * 3.0;
        let a = projected(matrix, caster);
        let b = projected(matrix, receiver);
        assert!((a.truncate() - b.truncate()).length() < 1e-5);
        assert!(a.z < b.z);
        for p in points {
            let p = projected(matrix, p);
            assert!(p.x.abs() <= 1.0 && p.y.abs() <= 1.0 && (0.0..=1.0).contains(&p.z));
        }
    }

    #[test]
    fn finite_shadow_projection_preserves_source_rays_and_translation() {
        let source = Vec3::new(100.0, -40.0, 2000.0);
        let points = [Vec3::new(-2.0, -1.0, 0.0), Vec3::new(2.0, 1.0, 5.0)];
        let matrix = light_projection(&points, source, true);
        let caster = Vec3::new(0.0, 0.0, 3.0);
        let receiver = source + (caster - source) * 1.1;
        let a = projected(matrix, caster);
        let b = projected(matrix, receiver);
        assert!((a.truncate() - b.truncate()).length() < 1e-4);
        assert!(a.z < b.z);
        let offset = Vec3::new(250.0, -100.0, 0.0);
        let translated = points.map(|p| p + offset);
        let moved = light_projection(&translated, source + offset, true);
        assert!((projected(moved, caster + offset) - a).length() < 1e-3);
    }
}
