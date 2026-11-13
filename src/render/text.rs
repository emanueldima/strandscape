use {
    super::{RenderError, TextBatch},
    crate::text::GlyphKey,
    bytemuck::{Pod, Zeroable},
    glam::Vec4,
    std::collections::HashMap,
    wgpu::util::DeviceExt,
};

pub(super) struct TextRenderer {
    atlas: wgpu::Texture,
    atlas_bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
    entries: HashMap<GlyphKey, AtlasEntry>,
    cursor: [u32; 2],
    row_height: u32,
    vertices: Option<wgpu::Buffer>,
    vertex_count: u32,
}

impl TextRenderer {
    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph atlas"),
            size: wgpu::Extent3d {
                width: ATLAS_SIZE,
                height: ATLAS_SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // Reserve the corner texel for solid panes; glyphs start after padding.
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &atlas,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[u8::MAX],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Text"),
            source: wgpu::ShaderSource::Wgsl(include_str!("text.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Text"), layout: None,
            vertex: wgpu::VertexState {
                module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader, entry_point: Some("fs_main"), compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format, depth_write_enabled: Some(false), depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(), bias: Default::default(),
            }),
            multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glyph atlas"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &atlas.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self {
            atlas,
            atlas_bind_group,
            pipeline,
            entries: HashMap::new(),
            cursor: [PADDING; 2],
            row_height: 0,
            vertices: None,
            vertex_count: 0,
        }
    }

    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &TextBatch,
    ) -> Result<(), RenderError> {
        // Atlas positions remain stable; upload each glyph only once.
        for (key, image) in &batch.images {
            if image.width == 0 || image.height == 0 || self.entries.contains_key(key) {
                continue;
            }
            if image.width + PADDING * 2 > ATLAS_SIZE || image.height + PADDING * 2 > ATLAS_SIZE {
                return Err(RenderError::AtlasFull);
            }
            if self.cursor[0] + image.width + PADDING > ATLAS_SIZE {
                self.cursor = [PADDING, self.cursor[1] + self.row_height + PADDING];
                self.row_height = 0;
            }
            if self.cursor[1] + image.height + PADDING > ATLAS_SIZE {
                return Err(RenderError::AtlasFull);
            }

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: self.cursor[0],
                        y: self.cursor[1],
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &image.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(image.width),
                    rows_per_image: Some(image.height),
                },
                wgpu::Extent3d {
                    width: image.width,
                    height: image.height,
                    depth_or_array_layers: 1,
                },
            );
            self.entries.insert(
                *key,
                AtlasEntry {
                    origin: self.cursor,
                },
            );
            self.cursor[0] += image.width + PADDING;
            self.row_height = self.row_height.max(image.height);
        }

        let vertices = vertices(batch, &self.entries);
        self.vertex_count = vertices.len() as u32;
        self.vertices = if vertices.is_empty() {
            None
        } else {
            Some(
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Text vertices"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            )
        };
        Ok(())
    }

    pub(super) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(vertices) = &self.vertices else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.atlas_bind_group, &[]);
        pass.set_vertex_buffer(0, vertices.slice(..));
        pass.draw(0..self.vertex_count, 0..1);
    }
}

const ATLAS_SIZE: u32 = 1024;
const PADDING: u32 = 1;
const PANE_PADDING: f32 = 4.0;
const SOLID_UV: [f32; 2] = [0.5 / ATLAS_SIZE as f32; 2];
const QUAD: [[f32; 2]; 6] = [
    [0.0, 0.0],
    [1.0, 0.0],
    [1.0, 1.0],
    [0.0, 0.0],
    [1.0, 1.0],
    [0.0, 1.0],
];

struct AtlasEntry {
    origin: [u32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 4],
    uv: [f32; 2],
    color: [f32; 4],
}

fn vertices(batch: &TextBatch, entries: &HashMap<GlyphKey, AtlasEntry>) -> Vec<Vertex> {
    let mut vertices = Vec::new();
    for run in &batch.runs {
        if let Some(color) = run.background {
            // Match the shaped text bounds, then draw its glyphs over the pane.
            for [u, v] in QUAD {
                let x = u * (run.layout.bounds[0] + PANE_PADDING * 2.0) - PANE_PADDING;
                let y = v * (run.layout.bounds[1] + PANE_PADDING * 2.0) - PANE_PADDING;
                vertices.push(Vertex {
                    position: (run.clip_from_local * Vec4::new(x, -y, 0.0, 1.0)).to_array(),
                    uv: SOLID_UV,
                    color,
                });
            }
        }
        for glyph in run.layout.glyphs.iter() {
            let Some(entry) = entries.get(&glyph.key) else {
                continue;
            };
            let image = &batch.images[&glyph.key];
            for [u, v] in QUAD {
                let x = glyph.position[0] + image.offset[0] as f32 + u * image.width as f32;
                let y = glyph.position[1] + image.offset[1] as f32 + v * image.height as f32;
                vertices.push(Vertex {
                    position: (run.clip_from_local * Vec4::new(x, -y, 0.0, 1.0)).to_array(),
                    uv: [
                        (entry.origin[0] as f32 + u * image.width as f32) / ATLAS_SIZE as f32,
                        (entry.origin[1] as f32 + v * image.height as f32) / ATLAS_SIZE as f32,
                    ],
                    color: run.color,
                });
            }
        }
    }
    vertices
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{render::TextDraw, scene::Space, text::TextLayout},
        glam::{Mat4, Vec3},
    };

    #[test]
    fn panes_fit_text_bounds() {
        for (bounds, transform) in [
            ([100.0, 27.0], Mat4::IDENTITY),
            (
                [800.0, 54.0],
                Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2)
                    * Mat4::from_scale(Vec3::splat(0.009)),
            ),
        ] {
            for background in [None, Some([0.0, 0.0, 0.0, 0.5]), Some([0.0, 0.0, 0.0, 1.0])] {
                let batch = TextBatch {
                    runs: vec![TextDraw {
                        layout: TextLayout {
                            bounds,
                            glyphs: Vec::new().into(),
                        },
                        clip_from_local: transform,
                        color: [1.0; 4],
                        background,
                        space: Space::World,
                    }],
                    images: HashMap::new(),
                };
                let pane = vertices(&batch, &HashMap::new());
                let Some(color) = background else {
                    assert!(pane.is_empty());
                    continue;
                };
                assert_eq!(pane.len(), 6);
                let top_left = transform.inverse() * Vec4::from_array(pane[0].position);
                let bottom_right = transform.inverse() * Vec4::from_array(pane[2].position);
                assert!(top_left.abs_diff_eq(Vec4::new(-4.0, 4.0, 0.0, 1.0), 0.0001));
                assert!(bottom_right.abs_diff_eq(
                    Vec4::new(bounds[0] + 4.0, -bounds[1] - 4.0, 0.0, 1.0),
                    0.0001,
                ));
                assert!(
                    pane.iter()
                        .all(|vertex| vertex.color == color && vertex.uv == SOLID_UV)
                );
            }
        }
    }
}
