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
