use {
    crate::app::{App, Target, Viewport},
    wasm_bindgen::{JsError, prelude::wasm_bindgen},
    web_sys::HtmlCanvasElement,
};

#[wasm_bindgen]
pub struct WebApp(App);

#[wasm_bindgen]
impl WebApp {
    pub async fn create(
        canvas: HtmlCanvasElement,
        font_bytes: Vec<u8>,
        width: u32,
        height: u32,
        scale_factor: f64,
        preference: &str,
    ) -> Result<WebApp, JsError> {
        let (app, css) = App::create(
            Target::from(canvas),
            font_bytes,
            Viewport {
                width,
                height,
                scale_factor,
            },
            preference,
        )
        .await?;
        apply_style(&css)?;
        Ok(Self(app))
    }

    pub fn set_theme(&mut self, preference: &str) -> Result<(), JsError> {
        apply_style(&self.0.set_theme(preference)?)
    }

    pub fn draw(&mut self) -> Result<(), JsError> {
        Ok(self.0.draw()?)
    }

    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f64) -> Result<(), JsError> {
        Ok(self.0.resize(Viewport {
            width,
            height,
            scale_factor,
        })?)
    }

    pub fn orbit(&mut self, x: f32, y: f32) {
        self.0.orbit(x, y);
    }

    pub fn pan(&mut self, x: f32, y: f32) {
        self.0.pan(x, y);
    }

    pub fn focus(&mut self, x: f32, y: f32) -> Result<Option<String>, JsError> {
        Ok(self.0.focus(x, y)?)
    }

    pub fn zoom(&mut self, scroll: f32) {
        self.0.zoom(scroll);
    }

    pub fn scroll_timeline(&mut self, x: f32, y: f32, delta: f32) -> Result<bool, JsError> {
        Ok(self.0.scroll_timeline(x, y, delta)?)
    }
}

fn apply_style(css: &str) -> Result<(), JsError> {
    let root = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.document_element())
        .ok_or_else(|| JsError::new("Missing page document"))?;
    root.set_attribute("style", css)
        .map_err(|_| JsError::new("Could not apply page theme"))
}
