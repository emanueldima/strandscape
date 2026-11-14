mod gpu;
mod text;

use {
    crate::{
        scene::{Item, Scene, Space},
        text::{FontError, FontSystem, GlyphImage, GlyphKey, TextLayout},
    },
    bytemuck::{Pod, Zeroable},
    glam::{Mat4, Vec2, Vec3},
    std::{cmp::Ordering, collections::HashMap},
    thiserror::Error,
};

pub(crate) use gpu::Target;

pub(crate) struct Renderer {
    fonts: FontSystem,
    gpu: gpu::Gpu,
    viewport: Viewport,
}

pub(crate) struct RenderConfig {
    pub(crate) viewport: Viewport,
    pub(crate) clear_color: [f64; 4],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Viewport {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) scale_factor: f64,
}

#[derive(Debug, Error)]
pub(crate) enum RenderError {
    #[error(transparent)]
    Font(#[from] FontError),
    #[error("GPU initialization failed: {0}")]
    Initialization(String),
    #[error("GPU surface failed: {0}")]
    Surface(String),
    #[error("The glyph atlas is full")]
    AtlasFull,
    #[error("Invalid camera or viewport")]
    InvalidView,
}

impl Renderer {
    pub(crate) async fn new(
        target: Target,
        fonts: FontSystem,
        config: RenderConfig,
    ) -> Result<Self, RenderError> {
        if !config.viewport.scale_factor.is_finite() || config.viewport.scale_factor <= 0.0 {
            return Err(RenderError::InvalidView);
        }

        let viewport = config.viewport;
        let gpu = gpu::Gpu::new(target, config).await?;
        Ok(Self {
            fonts,
            gpu,
            viewport,
        })
    }

    pub(crate) fn draw(&mut self, scene: &mut Scene) -> Result<(), RenderError> {
        if self.viewport.width == 0 || self.viewport.height == 0 {
            return Ok(());
        }

        let clip_from_world = scene
            .camera
            .clip_from_world(self.viewport.width as f32 / self.viewport.height as f32)
            .ok_or(RenderError::InvalidView)?;
        let viewport = Vec2::new(self.viewport.width as f32, self.viewport.height as f32)
            / self.viewport.scale_factor as f32;
        let mut list = DrawList {
            planes: Vec::new(),
            text: TextBatch {
                runs: Vec::new(),
                images: HashMap::new(),
            },
        };

        // Flatten domain objects into drawing data; only the backend sees wgpu.
        for item in &mut scene.items {
            match item {
                Item::Plane(plane) => {
                    // Invisible hit areas must not draw or occlude scene geometry.
                    if plane.color[3] == 0.0 {
                        continue;
                    }
                    let clip_from_local =
                        plane.transform.clip_from_local(clip_from_world, viewport);
                    for [x, y] in QUAD {
                        list.planes.push(PlaneVertex {
                            position: (clip_from_local
                                * Vec3::new(x * plane.size.x, y * plane.size.y, 0.0).extend(1.0))
                            .to_array(),
                            color: plane.color,
                        });
                    }
                }
                Item::Dot(dot) => {
                    let clip_from_local = dot.transform.clip_from_local(clip_from_world, viewport);
                    // Round markers share the plane pipeline through a triangle fan.
                    for segment in 0..DOT_SEGMENTS {
                        let angle = segment as f32 * std::f32::consts::TAU / DOT_SEGMENTS as f32;
                        let next =
                            (segment + 1) as f32 * std::f32::consts::TAU / DOT_SEGMENTS as f32;
                        for point in [
                            Vec3::ZERO,
                            Vec3::new(angle.cos(), angle.sin(), 0.0) * dot.radius,
                            Vec3::new(next.cos(), next.sin(), 0.0) * dot.radius,
                        ] {
                            list.planes.push(PlaneVertex {
                                position: (clip_from_local * point.extend(1.0)).to_array(),
                                color: dot.color,
                            });
                        }
                    }
                }
                Item::Text(text) => {
                    let layout = self.fonts.layout(&text.spec)?;
                    // Cache the rendered layout so picking uses the same pane bounds.
                    text.layout = Some(layout.clone());
                    if layout.bounds[0] == 0.0 || layout.bounds[1] == 0.0 {
                        continue;
                    }
                    for glyph in layout.glyphs.iter() {
                        if let std::collections::hash_map::Entry::Vacant(entry) =
                            list.text.images.entry(glyph.key)
                        {
                            entry.insert(self.fonts.rasterize(glyph.key)?);
                        }
                    }
                    list.text.runs.push(TextDraw {
                        layout,
                        clip_from_local: text.transform.clip_from_local(clip_from_world, viewport),
                        color: text.color,
                        background: text.background,
                        space: text.transform.space,
                    });
                }
            }
        }

        sort_text(&mut list.text.runs);
        self.gpu.draw(&list)
    }

    pub(crate) fn resize(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.gpu.resize(viewport);
    }
}

struct DrawList {
    planes: Vec<PlaneVertex>,
    text: TextBatch,
}

struct TextBatch {
    runs: Vec<TextDraw>,
    images: HashMap<GlyphKey, GlyphImage>,
}

struct TextDraw {
    layout: TextLayout,
    clip_from_local: Mat4,
    color: [f32; 4],
    background: Option<[f32; 4]>,
    space: Space,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PlaneVertex {
    position: [f32; 4],
    color: [f32; 4],
}

const QUAD: [[f32; 2]; 6] = [
    [-0.5, -0.5],
    [0.5, -0.5],
    [0.5, 0.5],
    [-0.5, -0.5],
    [0.5, 0.5],
    [-0.5, 0.5],
];

const DOT_SEGMENTS: usize = 24;

fn sort_text(runs: &mut [TextDraw]) {
    // Blend distant labels first; keep HUD order stable above the world.
    runs.sort_by(|a, b| {
        (a.space == Space::Hud)
            .cmp(&(b.space == Space::Hud))
            .then_with(|| {
                if a.space == Space::Hud {
                    return Ordering::Equal;
                }
                let a_depth = a.clip_from_local.project_point3(Vec3::ZERO).z;
                let b_depth = b.clip_from_local.project_point3(Vec3::ZERO).z;
                b_depth.total_cmp(&a_depth)
            })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_world_before_hud() {
        let mut runs: Vec<_> = [
            (Space::Hud, 0.0),
            (Space::World, 0.2),
            (Space::World, 0.9),
            (Space::Hud, 0.0),
            (Space::World, 0.5),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (space, depth))| TextDraw {
            layout: TextLayout {
                bounds: [100.0, 27.0],
                glyphs: Vec::new().into(),
            },
            clip_from_local: Mat4::from_translation(Vec3::Z * depth),
            color: [index as f32, 0.0, 0.0, 1.0],
            background: None,
            space,
        })
        .collect();
        sort_text(&mut runs);
        let order: Vec<_> = runs.iter().map(|run| run.color[0] as usize).collect();
        assert_eq!(order, [2, 4, 1, 0, 3]);
    }

    #[test]
    fn validates_shaders() {
        for source in [include_str!("plane.wgsl"), include_str!("text.wgsl")] {
            let module = naga::front::wgsl::parse_str(source).unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::empty(),
            )
            .validate(&module)
            .unwrap();
        }
    }
}
