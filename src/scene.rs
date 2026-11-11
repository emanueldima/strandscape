use {
    crate::text::TextSpec,
    glam::{Mat4, Quat, Vec2, Vec3},
};

pub(crate) struct Scene {
    pub(crate) camera: Camera,
    pub(crate) items: Vec<Item>,
}

pub(crate) struct SceneConfig {
    pub(crate) camera: Camera,
    pub(crate) items: Vec<Item>,
}

pub(crate) enum Item {
    Plane(Plane),
    Text(Text),
}

pub(crate) struct Camera {
    pub(crate) transform: Transform,
    pub(crate) vertical_fov: f32,
    pub(crate) near: f32,
    pub(crate) far: f32,
}

#[derive(Clone, Copy)]
pub(crate) struct Transform {
    pub(crate) translation: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) scale: Vec3,
}

pub(crate) struct Plane {
    pub(crate) transform: Transform,
    pub(crate) size: Vec2,
    pub(crate) color: [f32; 4],
}

pub(crate) struct Text {
    pub(crate) transform: Transform,
    pub(crate) spec: TextSpec,
    pub(crate) color: [f32; 4],
}

impl Scene {
    pub(crate) fn new(config: SceneConfig) -> Self {
        Self {
            camera: config.camera,
            items: config.items,
        }
    }
}

impl Transform {
    pub(crate) fn matrix(self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }
}
