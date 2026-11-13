use {
    crate::{
        composer::{self, ContextLayer, Target as Subject},
        events::Dataset,
        render::{RenderConfig, Renderer, Target, Viewport},
        text::{FontConfig, FontId, FontSystem},
    },
    glam::{Vec2, Vec3},
    wasm_bindgen::{JsError, prelude::wasm_bindgen},
    web_sys::HtmlCanvasElement,
};

#[wasm_bindgen]
pub struct WebApp {
    dataset: Dataset,
    output: composer::Output,
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

        let dataset = Dataset::parse(include_bytes!("../data/einstein.json"))?;
        let output = composer::build(
            &dataset,
            &composer::Config {
                layers: &[ContextLayer::Timeline, ContextLayer::Map],
                selected: None,
                reference: &dataset.events[0].date,
                font: FONT,
                aspect: width.max(1) as f32 / height.max(1) as f32,
            },
        )?;
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
        Ok(Self {
            dataset,
            output,
            renderer,
        })
    }

    pub fn draw(&mut self) -> Result<(), JsError> {
        self.renderer.draw(&self.output.scene)?;
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
        self.output.scene.camera.orbit(Vec2::new(x, y));
    }

    pub fn pan(&mut self, x: f32, y: f32) {
        self.output.scene.camera.pan(Vec2::new(x, y));
    }

    pub fn focus(&mut self, x: f32, y: f32, aspect: f32) -> Result<Option<String>, JsError> {
        let Some(hit) = self.output.scene.pick(Vec2::new(x, y), aspect) else {
            self.output.scene.camera.focus(Vec3::ZERO);
            return Ok(None);
        };
        self.output.scene.camera.focus(hit.point);
        match self.output.bindings.get(&hit.item) {
            Some(Subject::Entry(id)) => {
                let output = composer::build(
                    &self.dataset,
                    &composer::Config {
                        layers: &[ContextLayer::Timeline, ContextLayer::Map],
                        selected: Some(id),
                        reference: &self.dataset.events[0].date,
                        font: FONT,
                        aspect,
                    },
                )?;
                let title = self
                    .dataset
                    .events
                    .iter()
                    .find(|event| &event.id == id)
                    .map(|event| event.title.clone());
                // Replace presentation data while preserving the user's camera.
                self.output.scene.items = output.scene.items;
                self.output.bindings = output.bindings;
                Ok(title)
            }
            Some(Subject::Place(id)) => Ok(self
                .dataset
                .events
                .iter()
                .filter_map(|event| event.place.as_ref())
                .find(|place| &place.id == id)
                .map(|place| place.name.clone())),
            None => Ok(None),
        }
    }

    pub fn zoom(&mut self, scroll: f32) {
        self.output.scene.camera.zoom(scroll);
    }
}

const FONT: FontId = FontId(0);
