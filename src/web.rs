use {
    crate::{
        camera::{Camera, CameraConfig},
        render::{RenderConfig, Renderer, Target, Viewport},
        scene::{Item, Plane, Scene, SceneConfig, Text, Transform},
        text::{FontConfig, FontId, FontSystem, TextSpec},
    },
    glam::{Quat, Vec2, Vec3},
    wasm_bindgen::{JsError, prelude::wasm_bindgen},
    web_sys::HtmlCanvasElement,
};

#[wasm_bindgen]
pub struct WebApp {
    scene: Scene,
    renderer: Renderer,
}

#[wasm_bindgen]
impl WebApp {
    pub async fn create(
        canvas: HtmlCanvasElement,
        font_bytes: Vec<u8>,
        width: u32,
        height: u32,
        scale_factor: f64,
    ) -> Result<WebApp, JsError> {
        let fonts = FontSystem::new(FontConfig {
            fonts: vec![(FONT, font_bytes)],
        })?;

        // Text shares the plane's orientation, with a small offset to avoid z-fighting.
        let rotation = Quat::from_rotation_y(-0.18) * Quat::from_rotation_x(-0.08);
        let scene = Scene::new(SceneConfig {
            camera: Camera::new(CameraConfig {
                position: Vec3::new(0.0, 0.0, 6.0),
                focus: Vec3::ZERO,
                vertical_fov: 45.0_f32.to_radians(),
                near: 0.1,
                far: 100.0,
            }),
            items: vec![
                Item::Plane(Plane {
                    transform: Transform {
                        translation: Vec3::ZERO,
                        rotation,
                        scale: Vec3::ONE,
                    },
                    size: Vec2::new(4.5, 2.4),
                    color: [0.035, 0.10, 0.13, 1.0],
                }),
                Item::Text(Text {
                    transform: Transform {
                        translation: rotation * Vec3::new(-1.8, 0.65, 0.01),
                        rotation,
                        scale: Vec3::splat(0.009),
                    },
                    spec: TextSpec {
                        content: "Strandscape\nPeople, ideas, time and place.".into(),
                        font: FONT,
                        size: 32.0,
                        max_width: Some(400.0),
                    },
                    color: [0.91, 0.84, 0.66, 1.0],
                }),
            ],
        });
        let renderer = Renderer::new(
            Target::from(canvas),
            fonts,
            RenderConfig {
                viewport: Viewport {
                    width,
                    height,
                    scale_factor,
                },
                clear_color: [0.012, 0.023, 0.033, 1.0],
            },
        )
        .await?;
        Ok(Self { scene, renderer })
    }

    pub fn draw(&mut self) -> Result<(), JsError> {
        self.renderer.draw(&self.scene)?;
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f64) {
        self.renderer.resize(Viewport {
            width,
            height,
            scale_factor,
        });
    }

    pub fn orbit(&mut self, x: f32, y: f32) {
        self.scene.camera.orbit(Vec2::new(x, y));
    }

    pub fn focus(&mut self, x: f32, y: f32, aspect: f32) {
        let point = self.scene.pick(Vec2::new(x, y), aspect);
        self.scene.camera.focus(point);
    }

    pub fn zoom(&mut self, scroll: f32) {
        self.scene.camera.zoom(scroll);
    }
}

const FONT: FontId = FontId(0);
