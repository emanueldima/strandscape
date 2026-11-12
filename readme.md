# Strandscape

Strandscape is a software project for visualizations of people and ideas evolving
in time and space. It brings information from multiple sources into a shared visual
context to reveal connections, movements, and change.

## Concept

Timelines show when things happened; geographic views show where. Together, 
they help trace a person's journey, follow the spread of an idea, or explore
how different histories intersect.

Events connect these stories to dates, places, people, ideas, and supporting media.
Information from multiple sources can be explored together, with its origins preserved.

## Examples

- A biography connecting places lived, people encountered, work produced, and archival material.
- The history of an idea across publications, correspondence, people, and places.
- A trip connecting stops and experiences with photos, videos, and other records.

## Architecture

| Module | Responsibility |
| --- | --- |
| `events` | Parse and validate event data. |
| `composer` | Combine events with requested context. |
| `sculptor` | Translate compositions into scene primitives. |
| `scene` | Scene primitives and hit testing. |
| `camera` | Projection, orbit, focus, and zoom. |
| `text` | Shape text and rasterize glyphs. |
| `render` | Convert scenes into drawing data. |
| `render::gpu` | Manage wgpu resources and frame submission. |
| `render::text` | Maintain the glyph atlas and draw text. |
| `web` | Expose the application through Wasm. |
| `web/app.js` | Download fonts, handle input, and schedule redraws. |

The example embeds `data/einstein.json`. Scene construction and rendering are separate calls:

```text
web/app.js
├── WebApp::create(...)
│   ├── events::Dataset::parse(json) → Dataset
│   └── composer::build(dataset, config) → scene + selection bindings
│       └── sculptor::build(composition, config)
│           └── scene::Scene::new(config)
└── WebApp::draw()
    └── render::Renderer::draw(scene)
        ├── camera::Camera::clip_from_world(...)
        ├── text::FontSystem::{layout, rasterize}(...)
        └── render::gpu::Gpu::draw(draw_list)
            ├── render::text::TextRenderer::{prepare, draw}(...)
            └── wgpu → canvas
```

## Development

Install the browser build tools:

```sh
make install-tools
```

Build and serve:

```sh
make web
make serve
```

Open http://localhost:8080.

Check the crate with `make check`.
