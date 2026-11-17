# Changelog

## 2026-11-17

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
