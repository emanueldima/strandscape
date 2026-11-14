use {
    crate::{
        composer::{self, Target as Subject},
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
    viewport: Vec2,
    selected: Option<String>,
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
        let viewport =
            (Vec2::new(width as f32, height as f32) / scale_factor as f32).max(Vec2::ONE);
        let output = composer::build(
            &dataset,
            &composer::Config {
                selected: None,
                font: FONT,
                viewport,
                timeline_offset: 0.0,
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
            viewport,
            selected: None,
        })
    }

    pub fn draw(&mut self) -> Result<(), JsError> {
        self.renderer.draw(&mut self.output.scene)?;
        Ok(())
    }

    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f64) -> Result<(), JsError> {
        self.viewport =
            (Vec2::new(width as f32, height as f32) / scale_factor as f32).max(Vec2::ONE);
        self.renderer.resize(Viewport {
            width,
            height,
            scale_factor,
        });
        self.rebuild(self.output.timeline.offset)
    }

    pub fn orbit(&mut self, x: f32, y: f32) {
        self.output.scene.camera.orbit(Vec2::new(x, y));
    }

    pub fn pan(&mut self, x: f32, y: f32) {
        self.output.scene.camera.pan(Vec2::new(x, y));
    }

    pub fn focus(&mut self, x: f32, y: f32) -> Result<Option<String>, JsError> {
        let Some(hit) = self
            .output
            .scene
            .pick(Vec2::new(x, y), self.viewport, |index| {
                self.output.bindings.contains_key(&index)
            })
        else {
            return Ok(None);
        };
        let point = match self.output.bindings.get(&hit.item) {
            Some(Subject::Entry(_, point)) => *point,
            Some(Subject::Place(_, point)) => Some(*point),
            None => return Ok(None),
        };
        if let Some(point) = point {
            self.output.scene.camera.focus(point);
        }
        match self.output.bindings.get(&hit.item) {
            Some(Subject::Entry(id, _)) => {
                let title = self
                    .dataset
                    .events
                    .iter()
                    .find(|event| &event.id == id)
                    .map(|event| event.title.clone());
                self.selected = Some(id.clone());
                self.rebuild(self.output.timeline.offset)?;
                Ok(title)
            }
            Some(Subject::Place(id, _)) => Ok(self
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

    pub fn scroll_timeline(&mut self, x: f32, y: f32, delta: f32) -> Result<bool, JsError> {
        if !Vec3::new(x, y, delta).is_finite()
            || !(-1.0..=1.0).contains(&x)
            || !(-1.0..=1.0).contains(&y)
            || (1.0 - y) * self.viewport.y * 0.5 < self.output.timeline.top
        {
            return Ok(false);
        }
        let offset =
            (self.output.timeline.offset + delta).clamp(0.0, self.output.timeline.max_offset);
        if offset != self.output.timeline.offset {
            self.rebuild(offset)?;
        }
        Ok(true)
    }
}

impl WebApp {
    fn rebuild(&mut self, timeline_offset: f32) -> Result<(), JsError> {
        let output = composer::build(
            &self.dataset,
            &composer::Config {
                selected: self.selected.as_deref(),
                font: FONT,
                viewport: self.viewport,
                timeline_offset,
            },
        )?;
        // Resize, selection, and timeline scrolling preserve the user's camera.
        self.output.scene.items = output.scene.items;
        self.output.bindings = output.bindings;
        self.output.timeline = output.timeline;
        Ok(())
    }
}

const FONT: FontId = FontId(0);
