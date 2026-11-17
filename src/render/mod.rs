mod gpu;
mod sphere;
mod text;
mod tube;

use {
    crate::{
        scene::{Item, Scene, Space, Tube},
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
            planes: std::array::from_fn(|_| Vec::new()),
            spheres: Vec::new(),
            tubes: Vec::new(),
            world_view: WorldView {
                clip_from_world: clip_from_world.to_cols_array_2d(),
                eye: scene
                    .camera
                    .ray(Vec2::ZERO, viewport.x / viewport.y)
                    .ok_or(RenderError::InvalidView)?
                    .origin
                    .extend(1.0)
                    .to_array(),
            },
            text: TextBatch {
                runs: Vec::new(),
                images: HashMap::new(),
            },
        };

        let mut tubes = Vec::new();

        // Flatten domain objects into drawing data; only the backend sees wgpu.
        for item in &mut scene.items {
            match item {
                Item::Mesh(mesh) => {
                    for point in mesh.triangles.iter().flatten() {
                        list.planes[Layer::World as usize].push(PlaneVertex {
                            position: (clip_from_world * point.extend(1.0)).to_array(),
                            color: mesh.color,
                        });
                    }
                }
                Item::Plane(plane) => {
                    // Invisible hit areas must not draw or occlude scene geometry.
                    if plane.color[3] == 0.0 {
                        continue;
                    }
                    let clip_from_local =
                        plane.transform.clip_from_local(clip_from_world, viewport);
                    for [x, y] in QUAD {
                        list.planes[Layer::from(plane.transform.space) as usize].push(
                            PlaneVertex {
                                position: (clip_from_local
                                    * Vec3::new(x * plane.size.x, y * plane.size.y, 0.0)
                                        .extend(1.0))
                                .to_array(),
                                color: plane.color,
                            },
                        );
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
                            list.planes[Layer::from(dot.transform.space) as usize].push(
                                PlaneVertex {
                                    position: (clip_from_local * point.extend(1.0)).to_array(),
                                    color: dot.color,
                                },
                            );
                        }
                    }
                }
                Item::Sphere(sphere) => {
                    list.spheres.extend(sphere::build(sphere, clip_from_world));
                }
                Item::Tube(tube) => {
                    tubes.push(&*tube);
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
                        layer: text.transform.space.into(),
                    });
                }
            }
        }

        sort_tubes(&mut tubes, clip_from_world);
        for tube in tubes {
            list.tubes.extend(tube::build(tube, clip_from_world));
        }
        sort_text(&mut list.text.runs);
        self.gpu.draw(&list)
    }

    pub(crate) fn resize(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.gpu.resize(viewport);
    }

    pub(crate) fn set_background(&mut self, color: [f64; 4]) {
        self.gpu.set_background(color);
    }
}

struct DrawList {
    planes: [Vec<PlaneVertex>; 3],
    spheres: Vec<sphere::Vertex>,
    tubes: Vec<tube::Vertex>,
    world_view: WorldView,
    text: TextBatch,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Layer {
    World,
    Overlay,
    Hud,
}

impl From<Space> for Layer {
    fn from(space: Space) -> Self {
        match space {
            Space::World => Self::World,
            Space::Overlay => Self::Overlay,
            Space::Hud => Self::Hud,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct WorldView {
    clip_from_world: [[f32; 4]; 4],
    eye: [f32; 4],
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
    layer: Layer,
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

fn sort_tubes(tubes: &mut [&Tube], clip_from_world: Mat4) {
    // Blend far tubes first, using view depth rather than distance from the eye.
    let depth = |tube: &Tube| (clip_from_world * ((tube.start + tube.end) * 0.5).extend(1.0)).w;
    tubes.sort_by(|a, b| depth(b).total_cmp(&depth(a)));
}

fn sort_text(runs: &mut [TextDraw]) {
    // Blend distant labels first within each layer; HUD stays above world overlays.
    runs.sort_by(|a, b| {
        a.layer.cmp(&b.layer).then_with(|| {
            if a.layer == Layer::Hud {
                return Ordering::Equal;
            }
            let a_depth = (a.clip_from_local * Vec3::ZERO.extend(1.0)).w;
            let b_depth = (b.clip_from_local * Vec3::ZERO.extend(1.0)).w;
            b_depth.total_cmp(&a_depth)
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tube_order_follows_camera() {
        let tubes = [Vec3::X * -2.0, Vec3::new(1.0, 0.0, 1.0), Vec3::Z * -2.0].map(|center| Tube {
            start: center - Vec3::Y,
            end: center + Vec3::Y,
            radius: 0.1,
            color: [1.0; 4],
        });
        for (eye, order) in [(Vec3::Z * 6.0, [2, 0, 1]), (Vec3::X * 6.0, [0, 2, 1])] {
            let clip = Mat4::perspective_rh(45.0_f32.to_radians(), 1.0, 0.1, 100.0)
                * Mat4::look_at_rh(eye, Vec3::ZERO, Vec3::Y);
            let mut sorted: Vec<_> = tubes.iter().collect();
            sort_tubes(&mut sorted, clip);
            for (tube, index) in sorted.into_iter().zip(order) {
                assert_eq!(tube.start, tubes[index].start);
            }
        }
    }

    #[test]
    fn orders_world_overlay_hud() {
        let mut runs: Vec<_> = [
            (Layer::Hud, 0.0),
            (Layer::World, 0.2),
            (Layer::World, 0.9),
            (Layer::Hud, 0.0),
            (Layer::World, 0.5),
            (Layer::Overlay, 0.4),
            (Layer::Overlay, 0.8),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (space, depth))| TextDraw {
            layout: TextLayout {
                bounds: [100.0, 27.0],
                glyphs: Vec::new().into(),
            },
            clip_from_local: Mat4 {
                w_axis: Vec3::ZERO.extend(depth),
                ..Mat4::IDENTITY
            },
            color: [index as f32, 0.0, 0.0, 1.0],
            background: None,
            layer: space,
        })
        .collect();
        sort_text(&mut runs);
        let order: Vec<_> = runs.iter().map(|run| run.color[0] as usize).collect();
        assert_eq!(order, [2, 4, 1, 6, 5, 0, 3]);
    }

    #[test]
    fn validates_shaders() {
        for source in [
            include_str!("plane.wgsl"),
            include_str!("text.wgsl"),
            include_str!("sphere.wgsl"),
            include_str!("tube.wgsl"),
        ] {
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
