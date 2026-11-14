use {
    super::{QUAD, WorldView},
    crate::scene::Sphere,
    bytemuck::{Pod, Zeroable},
    glam::{Mat4, Vec2},
    wgpu::util::DeviceExt,
};

pub(super) struct SphereRenderer {
    pipeline: wgpu::RenderPipeline,
    view: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertices: Option<wgpu::Buffer>,
    vertex_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Vertex {
    position: [f32; 2],
    ray_point: [f32; 3],
    center_radius: [f32; 4],
    color: [f32; 4],
}

pub(super) fn build(sphere: &Sphere, clip_from_world: Mat4) -> [Vertex; 6] {
    let center = clip_from_world * sphere.center.extend(1.0);
    let extent = (clip_from_world.x_axis.abs()
        + clip_from_world.y_axis.abs()
        + clip_from_world.z_axis.abs())
        * sphere.radius;
    let mut min = -Vec2::ONE;
    let mut max = Vec2::ONE;

    // Bound the projected sphere conservatively; near-plane crossings use the viewport.
    if center.w > extent.w && center.z > extent.z {
        let low = (center - extent).truncate().truncate();
        let high = (center + extent).truncate().truncate();
        min = (low / (center.w - extent.w))
            .min(low / (center.w + extent.w))
            .clamp(-Vec2::ONE, Vec2::ONE);
        max = (high / (center.w - extent.w))
            .max(high / (center.w + extent.w))
            .clamp(-Vec2::ONE, Vec2::ONE);
    }

    let world_from_clip = clip_from_world.inverse();
    QUAD.map(|[x, y]| {
        let position = min + (Vec2::new(x, y) + Vec2::splat(0.5)) * (max - min);
        Vertex {
            position: position.to_array(),
            ray_point: world_from_clip
                .project_point3(position.extend(1.0))
                .to_array(),
            center_radius: sphere.center.extend(sphere.radius).to_array(),
            color: sphere.color,
        }
    })
}

impl SphereRenderer {
    pub(super) fn new(
        device: &wgpu::Device,
        color: wgpu::TextureFormat,
        depth: wgpu::TextureFormat,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Sphere"),
            source: wgpu::ShaderSource::Wgsl(include_str!("sphere.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Sphere"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x3, 2 => Float32x4, 3 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let view = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sphere view"),
            size: std::mem::size_of::<WorldView>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sphere view"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            view,
            bind_group,
            vertices: None,
            vertex_count: 0,
        }
    }

    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[Vertex],
        view: &WorldView,
    ) {
        self.vertex_count = vertices.len() as u32;
        self.vertices = None;
        if vertices.is_empty() {
            return;
        }
        queue.write_buffer(&self.view, 0, bytemuck::bytes_of(view));
        self.vertices = Some(
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Sphere vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            }),
        );
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(vertices) = &self.vertices else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use {super::*, glam::Vec3};

    #[test]
    fn quads_cover_projected_spheres() {
        let projection = Mat4::perspective_rh(45.0_f32.to_radians(), 1.6, 0.1, 200.0);
        for (center, radius) in [
            (Vec3::new(0.0, 0.0, -5.0), 0.08),
            (Vec3::new(2.0, 1.0, -5.0), 0.5),
            (Vec3::new(-2.0, -1.0, -5.0), 0.5),
            (Vec3::new(0.0, 0.0, -0.11), 0.08),
            (Vec3::ZERO, 1.0),
        ] {
            let sphere = Sphere {
                center,
                radius,
                color: [1.0; 4],
            };
            let vertices = build(&sphere, projection);
            let min = Vec2::from_array(vertices[0].position);
            let max = Vec2::from_array(vertices[2].position);
            for vertex in vertices {
                let projected = projection.project_point3(Vec3::from_array(vertex.ray_point));
                assert!(
                    projected
                        .truncate()
                        .abs_diff_eq(Vec2::from_array(vertex.position), 0.0001)
                );
            }

            // Sample the sphere surface, including silhouettes and near-plane crossings.
            for latitude in 0..=16 {
                let pitch = latitude as f32 / 16.0 * std::f32::consts::PI;
                for longitude in 0..32 {
                    let yaw = longitude as f32 / 32.0 * std::f32::consts::TAU;
                    let normal = Vec3::new(
                        pitch.sin() * yaw.cos(),
                        pitch.cos(),
                        pitch.sin() * yaw.sin(),
                    );
                    let clip = projection * (center + normal * radius).extend(1.0);
                    if clip.w <= 0.0 || clip.z < 0.0 || clip.z > clip.w {
                        continue;
                    }
                    let point = clip.truncate().truncate() / clip.w;
                    if point.abs().max_element() > 1.0 {
                        continue;
                    }
                    assert!(point.cmpge(min - Vec2::splat(0.0001)).all());
                    assert!(point.cmple(max + Vec2::splat(0.0001)).all());
                }
            }
        }
    }
}
