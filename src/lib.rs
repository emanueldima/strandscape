#![cfg_attr(test, allow(dead_code))]

#[cfg(any(target_arch = "wasm32", test))]
mod render;
#[cfg(any(target_arch = "wasm32", test))]
mod scene;
#[cfg(any(target_arch = "wasm32", test))]
mod text;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

/// Starts the browser scene using a remotely loaded font.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub async fn start(canvas_id: String, font_url: String) -> Result<(), JsValue> {
    web::run(&canvas_id, &font_url)
        .await
        .map_err(|error| JsValue::from_str(&error.to_string()))
}
