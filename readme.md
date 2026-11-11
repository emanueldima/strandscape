# Strandscape

Strandscape is a software project for visualizations of people and ideas evolving in time and space. It brings information from multiple sources into a shared visual context to reveal connections, movements, and change.

## Concept

Timelines show when things happened; geographic views show where. Together, they help trace a person's journey, follow the spread of an idea, or explore how different histories intersect.

Events connect these stories to dates, places, people, ideas, and supporting media. Information from multiple sources can be explored together, with its origins preserved.

## Examples

- A biography connecting places lived, people encountered, work produced, and archival material.
- The history of an idea across publications, correspondence, people, and places.
- A trip connecting stops and experiences with photos, videos, and other records.

## Development

The first iteration renders a plane and shaped text through wgpu. Rust owns the
scene, font processing, and GPU atlas. The browser supplies font bytes and redraws
when the canvas size or display scale changes. wgpu selects WebGPU when available
and falls back to WebGL2.

Install the browser build tools:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.126 --locked
```

Build and serve:

```sh
make web
make serve
```

Open http://localhost:8080. The page downloads ABeeZee from Google Fonts at runtime.
Supply another direct TTF/OTF URL using `?font=<encoded-url>`. Cross-origin servers
must allow fetching the font through CORS. Fonts are never bundled.

Check the crate with `make check`. To include the font integration test, download
a font outside the repository and provide its path:

```sh
curl -fL https://raw.githubusercontent.com/google/fonts/main/ofl/abeezee/ABeeZee-Regular.ttf -o /tmp/strandscape-font.ttf
STRANDSCAPE_TEST_FONT=/tmp/strandscape-font.ttf cargo test --lib -- --include-ignored
```
