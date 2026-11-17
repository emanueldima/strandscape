# Changelog

## 2026-11-17

- Request: halve coastline thickness, use translucent place discs, bring names closer on their right, and keep places/names above elevations.
- Added a world-positioned overlay layer with matching picking priority. Renderer draws world, alpha-blended places/names, then HUD; text batches follow the same layer order. Coastlines now have independent half-width configuration, place discs use 50% theme opacity, and label spacing is reduced. Added styling, projection, layering, and picking regressions.

- Request: add subtle, real 3D terrain and thin 1 km elevation contours, without a toggle.
- Added a bundled global elevation grid and reproducible NetCDF conversion tool. New `geography::terrain` builds coastline-clipped relief, drapes map layers, and extracts contours from the same triangles. Geography caches the last bounds/interval for scene rebuilds. Composer carries elevations; sculptor batches shaded meshes through the existing renderer and anchors places/stems to terrain while preserving time-only event heights. Added configuration/theme controls and geometry, contour, and integration checks.

- Request: keep selected light-theme events red and make all ordinary event dots blue.
- Added a separate event palette color for 3D and timeline dots, preserving text and dark-theme colors. Restored red selection and added a failing-then-passing regression.

- Request: make light-theme event dots blue.
- Changed selected event dots and their matching timeline label to blue (#2563eb) in the light theme. APIs unchanged.

- Request: remove build.rs.
- Replaced generated theme discovery with explicit dark/light JSON includes in config::theme; removed the build script and updated configuration documentation.

- Request: introduce JSON dark/light themes and general configuration under root `config/`, selected by browser preference through a thin web bridge.
- Added config loaders, named defaults, hex color conversion, validation, and build-time embedding of all theme files. Moved shared layout/sizing out of sculptor and opacity/colors into theme JSON; retained numerical safeguards and depth offsets.
- Moved application orchestration from web to app. Browser preference changes rebuild styled primitives and update page/canvas colors while preserving camera and selection. Added configuration, color, theme geometry, and layout regressions.

- Request: add land fill, lakes, and major rivers.
- Bundled matching Natural Earth v5.1.2 1:110m layers; geography now parses named sources, triangulates polygon holes once with earcutr, and clips fills and lines to map bounds.
- Composer passes layers to sculptor; new decorative scene meshes reuse the existing triangle pipeline. Added muted land, ocean-colored lakes, and thinner blue rivers with round joins, plus geometry and layer regressions.

- Request: remove the map grid.
- Removed grid-line construction and its interval constant from sculptor. APIs unchanged.

- Request: round coastline joins.
- Added coplanar endpoint disks using existing scene dots to fill corner gaps and round exposed ends. APIs unchanged; added a coastline coverage regression.

- Request: make the tubes semitransparent.
- Set event connections and vertical stems to 50% opacity. Enabled alpha blending without depth writes, sorted tubes back-to-front each frame, and drew opaque spheres first. APIs unchanged.

- Request: make the selected event dot red.
- Changed selected map and timeline event dots, plus their matching timeline label, to red.

- Request: add a physical Earth baseline, initially fit all timeline places, and keep the camera still when selecting events.
- Added a pure geography module to parse and clip bundled public-domain Natural Earth coastlines; composer supplies paths and sculptor projects them through the existing scene primitives.
- Fit all timeline locations above the HUD on wide and portrait viewports; event selection preserves the camera while map-place clicks still recenter it. Added coastline and initial-view regressions.
- Removed event camera targets from selection bindings, which now carry only the event ID.

- Request: prevent the initial rotation from flipping the map upside down.
- Orbit derives heading from the camera orientation instead of unstable near-vertical position offsets. Added a first-orbit regression with tiny offsets and drags in every direction; no API changes.

## 2026-11-14

- Request: scroll to zoom, drag to move the map, and Cmd/Ctrl-drag to change the angle.
- Changed browser input routing to existing camera APIs; preserved timeline scrolling.
- Request: make timeline labels pickable using their text background panes, including invisible panes.
- Shared padded pane geometry between text rendering and hit testing; cached shaped layouts on scene text and bound timeline labels to their events.
- Request: reduce HUD clutter and show selected-event text once.
- Removed the subtitle and its unused reference-date fields; hid the selected timeline label and browser status echo while retaining the detail pane and highlighted dots.
- Request: keep the selected timeline label visible with a highlighted color.
- Restored the label and matched its color to the selected event dot; retained the detail pane.

- Request: replace 3D event discs with billboard sphere impostors.
- Added world sphere primitives and ray–sphere picking, plus a dedicated billboard renderer with per-pixel lighting and surface depth. Timeline dots remain flat.

- Request: halve 3D sphere size and pick only map places/labels and events; ignore other clicks.
- Added a world-picking candidate filter, bound map-label panes to their places, and removed camera movement on empty or decorative clicks. Sphere radius is now 0.04; place markers and HUD dots retain their sizes.

- Request: shade event connections and vertical stems like the spheres.
- Added tube primitives and a cylinder-impostor renderer with matching lighting and per-pixel surface depth. Tube diameter retains the previous line width; map grid and HUD remain flat, and tubes are not pickable.
