use {
    crate::{
        camera::{Camera, CameraConfig},
        scene::{Dot, Item, Plane, Scene, SceneConfig, Space, Sphere, Text, Transform, Tube},
        text::{FontId, TextSpec},
    },
    glam::{Mat3, Quat, Vec2, Vec3},
    std::collections::HashMap,
    thiserror::Error,
};

pub(crate) struct Output {
    pub(crate) scene: Scene,
    pub(crate) bindings: HashMap<usize, Target>,
    pub(crate) timeline: Timeline,
}

pub(crate) struct Timeline {
    pub(crate) top: f32,
    pub(crate) offset: f32,
    pub(crate) max_offset: f32,
}

pub(crate) enum Target {
    Entry(String),
    Place(String, Vec3),
}

pub(crate) struct Composition {
    pub(crate) title: String,
    pub(crate) strand: Vec<Entry>,
    pub(crate) map: Option<Map>,
}

pub(crate) struct Entry {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) date: String,
    pub(crate) time: f64,
    pub(crate) place: Option<Marker>,
    pub(crate) emphasis: Emphasis,
}

pub(crate) struct Map {
    pub(crate) bounds: Bounds,
    pub(crate) markers: Vec<Marker>,
    pub(crate) coastlines: Vec<[[f64; 2]; 2]>,
}

pub(crate) struct Bounds {
    pub(crate) west: f64,
    pub(crate) east: f64,
    pub(crate) south: f64,
    pub(crate) north: f64,
}

#[derive(Clone)]
pub(crate) struct Marker {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) latitude: f64,
    pub(crate) longitude: f64,
    pub(crate) emphasis: Emphasis,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Emphasis {
    Normal,
    Selected,
}

pub(crate) struct Config {
    pub(crate) font: FontId,
    pub(crate) viewport: Vec2,
    pub(crate) timeline_offset: f32,
}

#[derive(Debug, Error)]
pub(crate) enum BuildError {
    #[error("Invalid scene viewport")]
    InvalidViewport,
}

pub(crate) fn build(composition: &Composition, config: &Config) -> Result<Output, BuildError> {
    if !config.viewport.is_finite()
        || config.viewport.min_element() <= 0.0
        || !config.timeline_offset.is_finite()
    {
        return Err(BuildError::InvalidViewport);
    }
    let mut items = Vec::new();
    let mut bindings = HashMap::new();
    text(
        &mut items,
        &composition.title,
        Transform {
            space: Space::Hud,
            translation: Vec3::new(HUD_MARGIN, -HUD_MARGIN, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        None,
        config.font,
        INK,
        None,
    );
    let floor_rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let projection = composition.map.as_ref().map(|map| {
        let mid_lat = (map.bounds.south + map.bounds.north) * 0.5;
        let cos_lat = mid_lat.to_radians().cos().max(MIN_COS_LAT);
        let longitude_span = (map.bounds.east - map.bounds.west).max(MIN_GEO_SPAN) * cos_lat;
        let latitude_span = (map.bounds.north - map.bounds.south).max(MIN_GEO_SPAN);
        let scale = (MAP_WIDTH / longitude_span).min(MAP_DEPTH / latitude_span);
        let projection = Projection {
            longitude: (map.bounds.west + map.bounds.east) * 0.5,
            latitude: mid_lat,
            cos_lat,
            scale,
        };
        let size = Vec2::new(
            (longitude_span * scale) as f32,
            (latitude_span * scale) as f32,
        );
        plane(&mut items, Vec3::ZERO, size, floor_rotation, PANEL);
        for step in 0..=GRID_STEPS {
            let fraction = step as f32 / GRID_STEPS as f32 - 0.5;
            line(
                &mut items,
                Vec3::new(fraction * size.x, SURFACE_OFFSET, -size.y * 0.5),
                Vec3::new(fraction * size.x, SURFACE_OFFSET, size.y * 0.5),
                GRID,
                Vec3::Y,
            );
            line(
                &mut items,
                Vec3::new(-size.x * 0.5, SURFACE_OFFSET, fraction * size.y),
                Vec3::new(size.x * 0.5, SURFACE_OFFSET, fraction * size.y),
                GRID,
                Vec3::Y,
            );
        }
        // Coastlines share the event projection so map locations stay aligned.
        for [[west, south], [east, north]] in &map.coastlines {
            line(
                &mut items,
                projection.project(*south, *west) + Vec3::Y * SURFACE_OFFSET,
                projection.project(*north, *east) + Vec3::Y * SURFACE_OFFSET,
                COASTLINE,
                Vec3::Y,
            );
        }
        for marker in &map.markers {
            let point =
                projection.project(marker.latitude, marker.longitude) + Vec3::Y * SURFACE_OFFSET;
            let color = if marker.emphasis == Emphasis::Selected {
                ACCENT
            } else {
                MUTED
            };
            bindings.insert(items.len(), Target::Place(marker.id.clone(), point));
            plane(
                &mut items,
                point,
                Vec2::splat(MARKER_SIZE),
                floor_rotation,
                color,
            );
            // Name each place on the map rather than repeating it on event labels.
            bindings.insert(items.len(), Target::Place(marker.id.clone(), point));
            text(
                &mut items,
                &marker.label,
                Transform {
                    space: Space::World,
                    translation: point
                        + Vec3::new(MARKER_SIZE * 0.5 + LABEL_GAP, SURFACE_OFFSET, 0.0),
                    rotation: floor_rotation,
                    scale: Vec3::splat(TEXT_SCALE),
                },
                None,
                config.font,
                if marker.emphasis == Emphasis::Selected {
                    color
                } else {
                    [color[0], color[1], color[2], LABEL_OPACITY]
                },
                Some([0.0; 4]),
            );
        }
        projection
    });

    let start = composition.strand.first().map_or(0.0, |entry| entry.time);
    let span =
        (composition.strand.last().map_or(start, |entry| entry.time) - start).max(MIN_TIME_SPAN);
    let mut previous = None;
    for entry in &composition.strand {
        let height = HEIGHT_BASE + ((entry.time - start) / span) as f32 * HEIGHT_SPAN;
        let ground = entry
            .place
            .as_ref()
            .zip(projection.as_ref())
            .map(|(place, projection)| projection.project(place.latitude, place.longitude));
        let color = if entry.emphasis == Emphasis::Selected {
            SELECTED_EVENT
        } else {
            INK
        };
        let Some(ground) = ground else {
            // Keep unknown events on the HUD while joining the surrounding located dots.
            continue;
        };
        let point = ground + Vec3::Y * height;
        tube(&mut items, ground, point, GRID);
        if let Some(previous) = previous {
            tube(&mut items, previous, point, ACCENT);
        }
        previous = Some(point);
        bindings.insert(items.len(), Target::Entry(entry.id.clone()));
        items.push(Item::Sphere(Sphere {
            center: point,
            radius: EVENT_RADIUS
                * if entry.emphasis == Emphasis::Selected {
                    SELECTED_SCALE
                } else {
                    1.0
                },
            color,
        }));
    }

    // Reserve space beyond the last dot for its angled label.
    let width = (config.viewport.x - HUD_MARGIN * 2.0 - HUD_LABEL_ROOM).max(0.0);
    let positions = timeline_positions(&composition.strand, width);
    let max_offset = (positions.last().copied().unwrap_or(0.0) - width).max(0.0);
    let offset = config.timeline_offset.clamp(0.0, max_offset);
    let baseline = -config.viewport.y + HUD_MARGIN;
    let top = (config.viewport.y - HUD_TIMELINE_HEIGHT).max(0.0);
    // Empty timeline clicks must not select the world beneath the HUD.
    items.push(Item::Plane(Plane {
        transform: Transform {
            space: Space::Hud,
            translation: Vec3::new(
                config.viewport.x * 0.5,
                -(top + config.viewport.y) * 0.5,
                0.0,
            ),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        size: Vec2::new(config.viewport.x, config.viewport.y - top),
        color: [0.0; 4],
    }));
    let line_width = (positions.last().copied().unwrap_or(0.0) - offset).min(width);
    if line_width > 0.0 {
        items.push(Item::Plane(Plane {
            transform: Transform {
                space: Space::Hud,
                translation: Vec3::new(HUD_MARGIN + line_width * 0.5, baseline, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            size: Vec2::new(line_width, 2.0),
            color: MUTED,
        }));
    }
    for (entry, x) in composition.strand.iter().zip(positions) {
        let x = x - offset;
        if x < 0.0 || x > width {
            continue;
        }
        let position = Vec3::new(HUD_MARGIN + x, baseline, 0.0);
        bindings.insert(items.len(), Target::Entry(entry.id.clone()));
        items.push(Item::Dot(Dot {
            transform: Transform {
                space: Space::Hud,
                translation: position,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            radius: HUD_DOT_RADIUS
                * if entry.emphasis == Emphasis::Selected {
                    SELECTED_SCALE
                } else {
                    1.0
                },
            color: if entry.emphasis == Emphasis::Selected {
                SELECTED_EVENT
            } else {
                INK
            },
        }));
        // Keep small dots easy to select without changing their visible spacing.
        bindings.insert(items.len(), Target::Entry(entry.id.clone()));
        items.push(Item::Plane(Plane {
            transform: Transform {
                space: Space::Hud,
                translation: position,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            size: Vec2::splat(HUD_MIN_GAP),
            color: [0.0; 4],
        }));
        let label = format!("{} · {}", entry.date, entry.label);
        let mut chars = label.chars();
        let mut content: String = chars.by_ref().take(HUD_LABEL_LIMIT).collect();
        if chars.next().is_some() {
            content.pop();
            content.push('…');
        }
        bindings.insert(items.len(), Target::Entry(entry.id.clone()));
        items.push(Item::Text(Text {
            transform: Transform {
                space: Space::Hud,
                translation: position + Vec3::Y * HUD_MIN_GAP,
                rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_4),
                scale: Vec3::ONE,
            },
            spec: TextSpec {
                content,
                font: config.font,
                size: HUD_LABEL_SIZE,
                max_width: None,
            },
            // Keep the selected label visible and match its highlighted dot.
            color: if entry.emphasis == Emphasis::Selected {
                SELECTED_EVENT
            } else {
                MUTED
            },
            background: Some([0.0; 4]),
            layout: None,
        }));
    }
    if let Some(entry) = composition
        .strand
        .iter()
        .find(|entry| entry.emphasis == Emphasis::Selected)
    {
        let label = format!(
            "{} · {}{}",
            entry.date,
            entry.label,
            if entry.place.is_none() {
                " · place unknown"
            } else {
                ""
            }
        );
        text(
            &mut items,
            &label,
            Transform {
                space: Space::Hud,
                translation: Vec3::new(
                    HUD_MARGIN,
                    baseline + HUD_TIMELINE_HEIGHT - HUD_LINE_HEIGHT,
                    0.0,
                ),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            None,
            config.font,
            ACCENT,
            Some(PANEL),
        );
    }

    // Fit the map and raised events between the title and timeline, including on wide views.
    let half_size =
        composition
            .map
            .as_ref()
            .zip(projection.as_ref())
            .map_or(Vec2::ONE, |(map, projection)| {
                let corner = projection.project(map.bounds.south, map.bounds.east);
                Vec2::new(corner.x, corner.z)
            });
    let fov = 45.0_f32.to_radians();
    let focal_length = config.viewport.y / (2.0 * (fov * 0.5).tan());
    let map_top = HUD_MARGIN + HUD_LINE_HEIGHT;
    let map_bottom = (top - HUD_MARGIN).max(map_top + 1.0);
    let top_slope = (config.viewport.y * 0.5 - map_top) / focal_length;
    let bottom_slope = (map_bottom - config.viewport.y * 0.5) / focal_length;
    let height = HEIGHT_BASE + HEIGHT_SPAN;
    let bottom_height = if bottom_slope >= 0.0 { height } else { 0.0 };
    let distance = ((2.0 * half_size.y + top_slope * height + bottom_slope * bottom_height)
        / (top_slope + bottom_slope))
        .max(
            half_size.x * 2.0 * focal_length / (config.viewport.x - 2.0 * HUD_MARGIN).max(1.0)
                + height,
        );
    let north = top_slope * (distance - height) - half_size.y;
    let south = half_size.y - bottom_slope * (distance - bottom_height);
    let focus = Vec3::new(0.0, 0.0, (north + south) * 0.5);
    let camera = Camera::new(CameraConfig {
        position: focus + Vec3::Y * distance,
        focus,
        vertical_fov: fov,
        near: 0.1,
        far: (distance + 1.0).max(200.0),
    });

    Ok(Output {
        scene: Scene::new(SceneConfig { camera, items }),
        bindings,
        timeline: Timeline {
            top,
            offset,
            max_offset,
        },
    })
}

struct Projection {
    longitude: f64,
    latitude: f64,
    cos_lat: f64,
    scale: f64,
}

impl Projection {
    fn project(&self, latitude: f64, longitude: f64) -> Vec3 {
        Vec3::new(
            ((longitude - self.longitude) * self.cos_lat * self.scale) as f32,
            0.0,
            -((latitude - self.latitude) * self.scale) as f32,
        )
    }
}

const PANEL: [f32; 4] = [0.035, 0.10, 0.13, 1.0];
const INK: [f32; 4] = [0.91, 0.84, 0.66, 1.0];
const MUTED: [f32; 4] = [0.42, 0.61, 0.64, 1.0];
const ACCENT: [f32; 4] = [1.0, 0.72, 0.30, 1.0];
const SELECTED_EVENT: [f32; 4] = [1.0, 0.15, 0.15, 1.0];
const GRID: [f32; 4] = [0.055, 0.16, 0.19, 1.0];
const COASTLINE: [f32; 4] = [0.25, 0.43, 0.39, 1.0];
const MAP_WIDTH: f64 = 14.0;
const MAP_DEPTH: f64 = 8.0;
const GRID_STEPS: usize = 6;
const MIN_COS_LAT: f64 = 0.01;
const MIN_GEO_SPAN: f64 = 0.001;
const MIN_TIME_SPAN: f64 = 1.0;
const HEIGHT_BASE: f32 = 0.7;
const HEIGHT_SPAN: f32 = 0.9;
const SURFACE_OFFSET: f32 = 0.015;
const MARKER_SIZE: f32 = 0.16;
const EVENT_RADIUS: f32 = 0.04;
const SELECTED_SCALE: f32 = 1.5;
const LINE_WIDTH: f32 = 0.025;
const TUBE_OPACITY: f32 = 0.5;
const TEXT_SCALE: f32 = 0.009;
const LABEL_GAP: f32 = 0.2;
const LABEL_OPACITY: f32 = 0.5;
const HUD_MARGIN: f32 = 24.0;
const HUD_LINE_HEIGHT: f32 = 28.0;
const HUD_DOT_RADIUS: f32 = 4.0;
const HUD_MIN_GAP: f32 = 24.0;
const HUD_TIMELINE_HEIGHT: f32 = 224.0;
const HUD_LABEL_SIZE: f32 = 12.0;
const HUD_LABEL_LIMIT: usize = 28;
const HUD_LABEL_ROOM: f32 = 160.0;
const LAYOUT_STEPS: usize = 32;

fn timeline_positions(entries: &[Entry], width: f32) -> Vec<f32> {
    let Some(first) = entries.first() else {
        return Vec::new();
    };
    let span = entries.last().unwrap().time - first.time;
    let mut scale = if span > 0.0 { width as f64 / span } else { 0.0 };

    // Preserve proportional time gaps; clamp only crowded gaps to the minimum.
    // When they cannot fit, retain that scale and let the timeline scroll.
    if entries.len().saturating_sub(1) as f32 * HUD_MIN_GAP <= width {
        let mut low = 0.0;
        let mut high = scale;
        for _ in 0..LAYOUT_STEPS {
            scale = (low + high) * 0.5;
            let length: f64 = entries
                .windows(2)
                .map(|pair| ((pair[1].time - pair[0].time) * scale).max(HUD_MIN_GAP as f64))
                .sum();
            if length > width as f64 {
                high = scale;
            } else {
                low = scale;
            }
        }
        scale = low;
    }
    let mut positions = vec![0.0];
    let mut x = 0.0;
    for pair in entries.windows(2) {
        x += ((pair[1].time - pair[0].time) * scale).max(HUD_MIN_GAP as f64);
        positions.push(x as f32);
    }
    positions
}

fn plane(items: &mut Vec<Item>, position: Vec3, size: Vec2, rotation: Quat, color: [f32; 4]) {
    items.push(Item::Plane(Plane {
        transform: Transform {
            space: Space::World,
            translation: position,
            rotation,
            scale: Vec3::ONE,
        },
        size,
        color,
    }));
}

fn line(items: &mut Vec<Item>, start: Vec3, end: Vec3, color: [f32; 4], normal: Vec3) {
    let delta = end - start;
    let length = delta.length();
    if length <= f32::EPSILON {
        return;
    }
    let x = delta / length;
    let mut y = normal.cross(x).normalize_or_zero();
    if y == Vec3::ZERO {
        // Retain vertical segments even when they point along the viewing axis.
        y = x.any_orthonormal_vector();
    }
    plane(
        items,
        (start + end) * 0.5,
        Vec2::new(length, LINE_WIDTH),
        Quat::from_mat3(&Mat3::from_cols(x, y, x.cross(y))),
        color,
    );
}

fn tube(items: &mut Vec<Item>, start: Vec3, end: Vec3, color: [f32; 4]) {
    if start.distance_squared(end) <= f32::EPSILON * f32::EPSILON {
        return;
    }

    // Use the line's half-width as radius to retain its diameter when shading it.
    items.push(Item::Tube(Tube {
        start,
        end,
        radius: LINE_WIDTH * 0.5,
        color: [color[0], color[1], color[2], color[3] * TUBE_OPACITY],
    }));
}

fn text(
    items: &mut Vec<Item>,
    content: &str,
    transform: Transform,
    width: Option<f32>,
    font: FontId,
    color: [f32; 4],
    background: Option<[f32; 4]>,
) {
    items.push(Item::Text(Text {
        transform,
        spec: TextSpec {
            content: content.into(),
            font,
            size: 20.0,
            max_width: width.map(|width| width / TEXT_SCALE),
        },
        color,
        background,
        layout: None,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_preserves_time_and_gap() {
        let entries: Vec<_> = [0.0, 0.0, 0.01, 1.0, 10.0, 40.0]
            .into_iter()
            .map(|time| Entry {
                id: time.to_string(),
                label: "Event".into(),
                date: time.to_string(),
                time,
                place: None,
                emphasis: Emphasis::Normal,
            })
            .collect();
        for width in [1000.0, 200.0, 64.0] {
            let positions = timeline_positions(&entries, width);
            let gaps: Vec<_> = positions.windows(2).map(|pair| pair[1] - pair[0]).collect();
            assert_eq!(positions.len(), entries.len());
            assert!(gaps.iter().all(|gap| *gap >= HUD_MIN_GAP - 0.001));
            assert!((gaps[0] - HUD_MIN_GAP).abs() < 0.001);
            if width >= 200.0 {
                assert!((positions.last().unwrap() - width).abs() < 0.001);
                assert!((gaps[4] / gaps[3] - 30.0 / 9.0).abs() < 0.001);
            } else {
                assert!(*positions.last().unwrap() > width);
                assert!(gaps[4] > gaps[3]);
            }
        }
        assert!(timeline_positions(&[], 100.0).is_empty());
        assert_eq!(timeline_positions(&entries[..1], 100.0), [0.0]);
        assert_eq!(timeline_positions(&entries[..2], 100.0), [0.0, HUD_MIN_GAP]);
    }

    #[test]
    fn time_separates_same_place() {
        let place = Marker {
            id: "bern".into(),
            label: "Bern".into(),
            latitude: 46.95,
            longitude: 7.45,
            emphasis: Emphasis::Normal,
        };
        let mut composition = Composition {
            title: "Example".into(),
            strand: [1900.0, 1901.0, 1905.0]
                .into_iter()
                .map(|time| Entry {
                    id: time.to_string(),
                    label: "An event".into(),
                    date: time.to_string(),
                    time,
                    place: Some(place.clone()),
                    emphasis: Emphasis::Normal,
                })
                .collect(),
            map: Some(Map {
                bounds: Bounds {
                    west: 6.0,
                    east: 9.0,
                    south: 46.0,
                    north: 48.0,
                },
                markers: vec![place],
                coastlines: Vec::new(),
            }),
        };
        let output = build(
            &composition,
            &Config {
                font: FontId(0),
                viewport: Vec2::new(800.0, 500.0),
                timeline_offset: 0.0,
            },
        )
        .unwrap();
        let points: Vec<_> = composition
            .strand
            .iter()
            .map(|entry| {
                let index = output
                    .bindings
                    .iter()
                    .filter_map(|(index, target)| match target {
                        Target::Entry(id) if id == &entry.id => Some(*index),
                        _ => None,
                    })
                    .min()
                    .unwrap();
                let Item::Sphere(sphere) = &output.scene.items[index] else {
                    panic!("expected event marker")
                };
                assert!(sphere.center.is_finite());
                sphere.center
            })
            .collect();
        assert_eq!(points[0].x, points[2].x);
        assert_eq!(points[0].z, points[2].z);
        assert!(points[0].y > 0.0 && points[1].y > points[0].y);
        assert!(((points[2].y - points[1].y) / (points[1].y - points[0].y) - 4.0).abs() < 0.001);
        let segments: Vec<_> = output
            .scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Tube(tube) if tube.color[..3] == ACCENT[..3] => Some(tube),
                _ => None,
            })
            .collect();
        assert_eq!(segments.len(), points.len() - 1);
        for (segment, pair) in segments.iter().zip(points.windows(2)) {
            assert!(segment.start.abs_diff_eq(pair[0], 0.001));
            assert!(segment.end.abs_diff_eq(pair[1], 0.001));
            assert_eq!(segment.radius, LINE_WIDTH * 0.5);
            assert_eq!(segment.color[3], TUBE_OPACITY);
        }

        let stems: Vec<_> = output
            .scene
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let Item::Tube(tube) = item else { return None };
                assert!(!output.bindings.contains_key(&index));
                (tube.color[..3] == GRID[..3]).then_some(tube)
            })
            .collect();
        assert_eq!(stems.len(), points.len());
        for (stem, point) in stems.iter().zip(&points) {
            assert_eq!(stem.start, Vec3::new(point.x, 0.0, point.z));
            assert_eq!(stem.end, *point);
            assert_eq!(stem.color[3], TUBE_OPACITY);
        }

        composition.strand[1].place = None;
        let output = build(
            &composition,
            &Config {
                font: FontId(0),
                viewport: Vec2::new(800.0, 500.0),
                timeline_offset: 0.0,
            },
        )
        .unwrap();
        let segments: Vec<_> = output
            .scene
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Tube(tube) if tube.color[..3] == ACCENT[..3] => Some(tube),
                _ => None,
            })
            .collect();
        assert_eq!(segments.len(), 1);
        let segment = segments[0];
        assert!(segment.start.abs_diff_eq(points[0], 0.001));
        assert!(segment.end.abs_diff_eq(points[2], 0.001));
    }

    #[test]
    fn selected_unknown_stays_on_hud() {
        let composition = Composition {
            title: "Example".into(),
            map: None,
            strand: vec![Entry {
                id: "unknown".into(),
                label: "An event".into(),
                date: "1905".into(),
                time: 1905.0,
                place: None,
                emphasis: Emphasis::Selected,
            }],
        };
        let output = build(
            &composition,
            &Config {
                font: FontId(0),
                viewport: Vec2::new(800.0, 500.0),
                timeline_offset: 0.0,
            },
        )
        .unwrap();
        assert!(output.scene.items.iter().any(|item| matches!(item,
            Item::Text(text) if text.spec.content == "1905 · An event · place unknown")));
        for item in &output.scene.items {
            let Item::Text(text) = item else {
                continue;
            };
            assert_eq!(text.transform.space, Space::Hud);
            assert!(
                text.transform.rotation == Quat::IDENTITY
                    || text.transform.rotation
                        == Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)
            );
            assert_eq!(text.transform.scale, Vec3::ONE);
        }
        assert!(
            output
                .bindings
                .values()
                .all(|target| matches!(target, Target::Entry(id) if id == "unknown"))
        );
    }
}
