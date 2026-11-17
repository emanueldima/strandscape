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
| `app` | Own application state, configuration, input actions, and scene rebuilds. |
| `config` | Load shared layout settings and named dark/light themes from bundled JSON. |
| `config::theme` | Validate theme types, colors, and opacity; convert sRGB hex to linear RGB. |
| `events` | Parse and validate event data. |
| `geography` | Parse geographic layers, triangulate polygons, and clip fills and lines to bounds. |
| `geography::terrain` | Read elevation grids, build relief, drape geographic layers, and extract contours. |
| `composer` | Infer map context and compose event strands. |
| `sculptor` | Translate compositions into scene primitives. |
| `scene` | World/HUD primitives and hit testing. |
| `camera` | Projection, orbit, focus, and zoom. |
| `text` | Shape text and rasterize glyphs. |
| `render` | Convert scenes into drawing data. |
| `render::gpu` | Manage wgpu resources and frame submission. |
| `render::sphere` | Build billboard quads and render shaded sphere surfaces with depth. |
| `render::text` | Maintain the glyph atlas and draw text. |
| `web` | Adapt browser values to the application through a thin Wasm bridge. |
| `web/app.js` | Download fonts, forward browser theme preference and input, and schedule redraws. |

The example embeds `data/einstein.json`. Scene construction and rendering are separate calls:

```text
web/app.js
├── WebApp::create(...) → app::App::create(...)
│   ├── config::load(browser preference) → layout + named theme
│   ├── events::Dataset::parse(json) → Dataset
│   ├── geography::Dataset::parse(sources) → geography
│   └── composer::build(dataset, config) → scene + selection bindings
│       ├── geography::build(geography, bounds) → clipped fills and line segments
│       └── sculptor::build(composition, config)
│           └── scene::Scene::new(config)
└── WebApp::draw() → app::App::draw()
    └── render::Renderer::draw(scene)
        ├── camera::Camera::clip_from_world(...)
        ├── scene::Transform::clip_from_local(...) → world or HUD projection
        ├── text::FontSystem::{layout, rasterize}(...)
        └── render::gpu::Gpu::draw(draw_list)
            ├── render::text::TextRenderer::{prepare, draw}(...)
            └── wgpu → canvas
```

The current experiment uses one map with a chronological polyline above it.
Its physical baseline uses bundled present-day coastlines, land, lakes, and major rivers from
[Natural Earth 1:110m](https://www.naturalearthdata.com/downloads/110m-physical-vectors/),
[release v5.1.2](https://github.com/nvkelso/natural-earth-vector/tree/v5.1.2/geojson).
The data is [public domain](https://www.naturalearthdata.com/about/terms-of-use/)
and stored unchanged in `data/coastlines.geojson`, `data/land.geojson`,
`data/lakes.geojson`, and `data/rivers.geojson` (upstream `ne_110m_coastline`,
`ne_110m_land`, `ne_110m_lakes`, and `ne_110m_rivers_lake_centerlines`).
Muted green land and ocean-colored lakes use triangle meshes; coastlines and thin
blue rivers have round joins. `earcutr` triangulates polygon interiors and holes
once at load time; geography intersects these with an elevation grid before sculptor
projects them. Land has real, compressed 3D relief with subtle northwest lighting.
Coastlines, lakes, rivers, place markers, and stem feet follow the same surface;
event heights continue to encode time independently of terrain.
Place markers are 50%-opacity discs with names close to their right. Their world-positioned
overlay layer draws above terrain/events and below the HUD, so relief cannot hide them.
Coastlines use half the event-line width by default.
Thin, unlabeled contours mark every 1,000 meters above sea level. Marching triangles
interpolates crossings on the rendered faces; a small offset avoids depth flicker.
Contours sit below lake fills. Terrain is always visible, without a toggle.
Geography caches the last bounds and contour interval across scene rebuilds.

`data/terrain.bin` derives from GMT's
[10-arc-minute, gridline-registered Earth Relief grid](https://oceania.generic-mapping-tools.org/server/earth/earth_relief/earth_relief_10m_g.grd),
based on [SRTM15+ v2.7](https://topex.ucsd.edu/pub/srtm15_plus/README_V2.7.txt)
([Tozer et al., 2019](https://doi.org/10.1029/2019EA000658)).
GMT applies a [52.4 km Gaussian filter](https://www.generic-mapping-tools.org/remote-datasets/earth-relief.html);
this is broad geographic context, not detailed local topography or exact summit heights.
Samples below sea level are clamped to zero when building the surface.
The 2161 × 1081 grid is stored south-to-north as little-endian signed 16-bit whole
meters. Its 40-byte header contains `STRDEM01`, two `u32` dimensions, then `f64`
west longitude, south latitude, and degree spacing. To reproduce it, install Python
`netCDF4` and `numpy`, download the source grid, and run:

```sh
python3 tools/prepare-terrain.py earth_relief_10m_g.grd data/terrain.bin
```

Source SHA-256: `4b64b4dd46d26d07c8542a46edec205c08a7b248ac8ab53610407ae4ebf2af57`.

No modern political borders or background settlements are included. These layers
are a baseline, not a reconstruction of historical geography.
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

## Configuration

`config/config.json` holds shared map, event, text, and timeline dimensions.
Its `default_themes.dark` and `default_themes.light` values name theme files in
the same directory, without `.json`. The supplied themes are `config/dark.json`
and `config/light.json`; each declares its `kind`, palette, and opacity.
Colors use sRGB `#RRGGBB` or `#RRGGBBAA` notation.
`layout.map.terrain` sets `elevation_scale` (world units per meter), `shading`
(0–1), `contour_interval` (meters), and `contour_width` (world units).
The initial scale is 0.02 world units per kilometer; the bundled relief remains
below the earliest events. Theme `colors.contour` controls contour color.
`layout.map.coastline_width` sets coastline thickness; theme `opacity.places`
sets disc opacity, and `layout.text.label_gap` sets the gap to place names.
Event dots use `colors.event`; selected dots and their timeline label use
`colors.selected_event`. Text uses separate palette colors.

The browser's preferred color scheme selects the corresponding default theme.
Preference changes update the scene, canvas background, and page colors while
preserving the camera, selected event, and timeline offset.
JavaScript forwards only `dark` or `light`; Rust loads and validates configuration.
The configuration and two themes are embedded with explicit `include_str!` entries.
Edits require `make web` followed by a page reload; adding a theme also requires
an entry in `src/config/theme.rs`. Numerical safeguards and depth offsets stay in Rust.

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
