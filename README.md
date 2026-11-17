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
| `geography` | Parse coastline GeoJSON and clip paths to geographic bounds. |
| `composer` | Infer map context and compose event strands. |
| `sculptor` | Translate compositions into scene primitives. |
| `scene` | World/HUD primitives and hit testing. |
| `camera` | Projection, orbit, focus, and zoom. |
| `text` | Shape text and rasterize glyphs. |
| `render` | Convert scenes into drawing data. |
| `render::gpu` | Manage wgpu resources and frame submission. |
| `render::sphere` | Build billboard quads and render shaded sphere surfaces with depth. |
| `render::text` | Maintain the glyph atlas and draw text. |
| `web` | Expose the application through Wasm. |
| `web/app.js` | Download fonts, handle input, and schedule redraws. |

The example embeds `data/einstein.json`. Scene construction and rendering are separate calls:

```text
web/app.js
├── WebApp::create(...)
│   ├── events::Dataset::parse(json) → Dataset
│   ├── geography::Dataset::parse(geojson) → geography
│   └── composer::build(dataset, config) → scene + selection bindings
│       ├── geography::build(geography, bounds) → coastline segments
│       └── sculptor::build(composition, config)
│           └── scene::Scene::new(config)
└── WebApp::draw()
    └── render::Renderer::draw(scene)
        ├── camera::Camera::clip_from_world(...)
        ├── scene::Transform::clip_from_local(...) → world or HUD projection
        ├── text::FontSystem::{layout, rasterize}(...)
        └── render::gpu::Gpu::draw(draw_list)
            ├── render::text::TextRenderer::{prepare, draw}(...)
            └── wgpu → canvas
```

The current experiment uses one map with a chronological polyline above it.
Its physical baseline uses bundled present-day coastlines from
[Natural Earth 1:110m](https://www.naturalearthdata.com/downloads/110m-physical-vectors/),
[release v5.1.2](https://github.com/nvkelso/natural-earth-vector/blob/v5.1.2/geojson/ne_110m_coastline.geojson).
The data is [public domain](https://www.naturalearthdata.com/about/terms-of-use/)
and stored unchanged in `data/coastlines.geojson`. No modern political borders or
background settlements are included. These coastlines are a baseline, not a
reconstruction of historical shorelines.
The initial view looks straight down, with north at the top, and fits all timeline
locations with padding above the HUD timeline.
Longitude and latitude determine horizontal position; elapsed time determines
height across a shallow 0.9-unit span. Place names sit on the map; 3D events use
shaded sphere impostors with surface depth and ray–sphere picking, without labels.
Only events, place markers, and place labels respond to 3D clicks; other clicks
leave the view unchanged.
A bottom HUD timeline uses flat dots for every event, with time-proportional gaps
clamped to a 24 CSS-pixel minimum. Its 12-pixel labels rise at 45°, showing the date
and a shortened description. Scroll over its bottom strip to move an overflowing
timeline and its labels horizontally;
the rest of the HUD stays fixed. Selecting an event enlarges and highlights its HUD
and 3D dots and its timeline label, and shows its date and description in an opaque
HUD pane. Event selection preserves the camera; clicking a map place still recenters it.
The dataset title stays on the HUD. The 3D line connects successive located events,
skipping unknown locations.
All events, including Princeton, remain available.
Historical geography is deferred.

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
