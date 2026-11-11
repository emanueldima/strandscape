use {
    crate::text::FontBytes, js_sys::Uint8Array, thiserror::Error, wasm_bindgen::JsCast,
    wasm_bindgen_futures::JsFuture, web_sys::Response,
};

pub(super) async fn fetch_font(url: &str) -> Result<FontBytes, FontLoadError> {
    let window =
        web_sys::window().ok_or_else(|| FontLoadError::Fetch("No browser window".into()))?;
    let response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|error| FontLoadError::Fetch(format!("{error:?}")))?
        .dyn_into::<Response>()
        .map_err(|_| FontLoadError::Fetch("Invalid response".into()))?;
    if !response.ok() {
        return Err(FontLoadError::Http(response.status()));
    }

    let bytes = response
        .array_buffer()
        .map_err(|error| FontLoadError::Fetch(format!("{error:?}")))?;
    let bytes = JsFuture::from(bytes)
        .await
        .map_err(|error| FontLoadError::Fetch(format!("{error:?}")))?;
    Ok(Uint8Array::new(&bytes).to_vec())
}

#[derive(Debug, Error)]
pub(super) enum FontLoadError {
    #[error("Font download failed: {0}")]
    Fetch(String),
    #[error("Font download returned HTTP {0}")]
    Http(u16),
}
