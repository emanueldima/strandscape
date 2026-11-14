# Changelog

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
