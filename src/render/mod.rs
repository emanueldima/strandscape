mod gpu;
mod text;

use {
    crate::{
        scene::{Item, Scene},
        text::{FontError, FontSystem, GlyphImage, GlyphKey, TextLayout},
    },
    bytemuck::{Pod, Zeroable},
    glam::{Mat4, Vec3},
    std::collections::HashMap,
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

    pub(crate) fn draw(&mut self, scene: &Scene) -> Result<(), RenderError> {
        if self.viewport.width == 0 || self.viewport.height == 0 {
            return Ok(());
        }

        let clip_from_world = scene
            .camera
            .clip_from_world(self.viewport.width as f32 / self.viewport.height as f32)
            .ok_or(RenderError::InvalidView)?;
        let mut list = DrawList {
            planes: Vec::new(),
            text: TextBatch {
                runs: Vec::new(),
                images: HashMap::new(),
            },
        };

        // Flatten domain objects into drawing data; only the backend sees wgpu.
        for item in &scene.items {
            match item {
                Item::Plane(plane) => {
                    let clip_from_local = clip_from_world * plane.transform.matrix();
                    for [x, y] in QUAD {
                        list.planes.push(PlaneVertex {
                            position: (clip_from_local
                                * Vec3::new(x * plane.size.x, y * plane.size.y, 0.0).extend(1.0))
                            .to_array(),
                            color: plane.color,
                        });
                    }
                }
                Item::Text(text) => {
                    let layout = self.fonts.layout(&text.spec)?;
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
                        clip_from_local: clip_from_world * text.transform.matrix(),
                        color: text.color,
                    });
                }
            }
        }

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

#[cfg(test)]
mod tests {
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
