use {
    crate::{
        camera::{Camera, CameraConfig},
        config,
        scene::{Dot, Item, Mesh, Plane, Scene, SceneConfig, Space, Sphere, Text, Transform, Tube},
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
    pub(crate) coastlines: Vec<[[f64; 3]; 2]>,
    pub(crate) land: Vec<[[f64; 3]; 3]>,
    pub(crate) lakes: Vec<[[f64; 3]; 3]>,
    pub(crate) rivers: Vec<[[f64; 3]; 2]>,
    pub(crate) contours: Vec<[[f64; 3]; 2]>,
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
    pub(crate) elevation: f64,
    pub(crate) emphasis: Emphasis,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Emphasis {
    Normal,
    Selected,
}

pub(crate) struct Config<'a> {
    pub(crate) settings: &'a config::Config,
    pub(crate) font: FontId,
    pub(crate) viewport: Vec2,
    pub(crate) timeline_offset: f32,
}

#[derive(Debug, Error)]
pub(crate) enum BuildError {
    #[error("Invalid scene viewport")]
    InvalidViewport,
}

pub(crate) fn build(composition: &Composition, config: &Config<'_>) -> Result<Output, BuildError> {
    if !config.viewport.is_finite()
        || config.viewport.min_element() <= 0.0
        || !config.timeline_offset.is_finite()
    {
        return Err(BuildError::InvalidViewport);
    }
    let settings = config.settings;
    let layout = &settings.layout;
    let colors = &settings.theme.colors;
    let opacity = &settings.theme.opacity;

    let mut items = Vec::new();
    let mut bindings = HashMap::new();
    text(
        &mut items,
        &composition.title,
        Transform {
            space: Space::Hud,
            translation: Vec3::new(layout.timeline.margin, -layout.timeline.margin, 0.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        None,
        config,
        colors.ink.linear,
        None,
    );
    let floor_rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let projection = composition.map.as_ref().map(|map| {
        let mid_lat = (map.bounds.south + map.bounds.north) * 0.5;
        let cos_lat = mid_lat.to_radians().cos().max(MIN_COS_LAT);
        let longitude_span = (map.bounds.east - map.bounds.west).max(MIN_GEO_SPAN) * cos_lat;
        let latitude_span = (map.bounds.north - map.bounds.south).max(MIN_GEO_SPAN);
        let scale = (layout.map.width / longitude_span).min(layout.map.depth / latitude_span);
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
        plane(
            &mut items,
            Vec3::ZERO,
            size,
            floor_rotation,
            colors.water.linear,
        );

        // Keep a compressed real surface; quantized face lighting batches existing meshes.
        let elevation_scale = layout.map.terrain.elevation_scale;
        let light = Vec3::new(-1.0, 2.0, -1.0).normalize();
        for (triangles, color, height) in [
            (&map.land, colors.land.linear, LAND_OFFSET),
            (&map.lakes, colors.water.linear, LAKE_OFFSET),
        ] {
            let mut shades = vec![Vec::new(); SHADE_STEPS * 2 + 1];
            for triangle in triangles {
                let points = triangle.map(|[longitude, latitude, elevation]| {
                    projection.project(latitude, longitude)
                        + Vec3::Y * (elevation as f32 * elevation_scale + height)
                });
                let [a, b, c] = points;
                let mut normal = (b - a).cross(c - a).normalize_or_zero();
                if normal.y < 0.0 {
                    normal = -normal;
                }
                let shade = if height == LAND_OFFSET {
                    ((normal.dot(light) / light.y - 1.0).clamp(-1.0, 1.0) * SHADE_STEPS as f32)
                        .round() as isize
                } else {
                    0
                };
                shades[(shade + SHADE_STEPS as isize) as usize].push(points);
            }
            for (shade, triangles) in shades.into_iter().enumerate() {
                if triangles.is_empty() {
                    continue;
                }
                let factor =
                    1.0 + layout.map.terrain.shading * (shade as f32 / SHADE_STEPS as f32 - 1.0);
                items.push(Item::Mesh(Mesh {
                    triangles,
                    color: [
                        (color[0] * factor).clamp(0.0, 1.0),
                        (color[1] * factor).clamp(0.0, 1.0),
                        (color[2] * factor).clamp(0.0, 1.0),
                        color[3],
                    ],
                }));
            }
        }

        // All map layers share the event projection so locations stay aligned.
        for (segments, color, width, offset) in [
            (
                &map.coastlines,
                colors.coastline.linear,
                layout.map.coastline_width,
                SURFACE_OFFSET,
            ),
            (
                &map.rivers,
                colors.river.linear,
                layout.map.river_width,
                SURFACE_OFFSET,
            ),
            (
                &map.contours,
                colors.contour.linear,
                layout.map.terrain.contour_width,
                LAND_OFFSET + CONTOUR_OFFSET,
            ),
        ] {
            for segment in segments {
                let points = segment.map(|[longitude, latitude, elevation]| {
                    projection.project(latitude, longitude)
                        + Vec3::Y * (elevation as f32 * elevation_scale + offset)
                });
                line(&mut items, points[0], points[1], width, color, Vec3::Y);

                // Coplanar endpoint disks fill outer bends and round exposed ends.
                for point in points {
                    items.push(Item::Dot(Dot {
                        transform: Transform {
                            space: Space::World,
                            translation: point,
                            rotation: floor_rotation,
                            scale: Vec3::ONE,
                        },
                        radius: width * 0.5,
                        color,
                    }));
                }
            }
        }
        for marker in &map.markers {
            let point = projection.project(marker.latitude, marker.longitude)
                + Vec3::Y * (marker.elevation as f32 * elevation_scale + SURFACE_OFFSET);
            let color = if marker.emphasis == Emphasis::Selected {
                colors.accent.linear
            } else {
                colors.muted.linear
            };
            bindings.insert(items.len(), Target::Place(marker.id.clone(), point));
            items.push(Item::Dot(Dot {
                transform: Transform {
                    space: Space::Overlay,
                    translation: point,
                    rotation: floor_rotation,
                    scale: Vec3::ONE,
                },
                radius: layout.map.marker_size * 0.5,
                color: [color[0], color[1], color[2], color[3] * opacity.places],
            }));
            // Keep names close to the right of their discs, visible above the terrain.
            bindings.insert(items.len(), Target::Place(marker.id.clone(), point));
            text(
                &mut items,
                &marker.label,
                Transform {
                    space: Space::Overlay,
                    translation: point
                        + Vec3::new(
                            layout.map.marker_size * 0.5 + layout.text.label_gap,
                            SURFACE_OFFSET,
                            -layout.map.marker_size * 0.5,
                        ),
                    rotation: floor_rotation,
                    scale: Vec3::splat(layout.text.scale),
                },
                None,
                config,
                if marker.emphasis == Emphasis::Selected {
                    color
                } else {
                    [color[0], color[1], color[2], opacity.labels]
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
        let height = layout.events.height_base
            + ((entry.time - start) / span) as f32 * layout.events.height_span;
        let ground = entry
            .place
            .as_ref()
            .zip(projection.as_ref())
            .map(|(place, projection)| {
                projection.project(place.latitude, place.longitude)
                    + Vec3::Y * (place.elevation as f32 * layout.map.terrain.elevation_scale)
            });
        let color = if entry.emphasis == Emphasis::Selected {
            colors.selected_event.linear
        } else {
            colors.event.linear
        };
        let Some(ground) = ground else {
            // Keep unknown events on the HUD while joining the surrounding located dots.
            continue;
        };
        // Terrain moves the stem's foot; event height continues to encode only time.
        let point = Vec3::new(ground.x, height, ground.z);
        tube(&mut items, ground, point, colors.stem.linear, settings);
        if let Some(previous) = previous {
            tube(&mut items, previous, point, colors.accent.linear, settings);
        }
        previous = Some(point);
        bindings.insert(items.len(), Target::Entry(entry.id.clone()));
        items.push(Item::Sphere(Sphere {
            center: point,
            radius: layout.events.radius
                * if entry.emphasis == Emphasis::Selected {
                    layout.selected_scale
                } else {
                    1.0
                },
            color,
        }));
    }

    // Reserve space beyond the last dot for its angled label.
    let width =
        (config.viewport.x - layout.timeline.margin * 2.0 - layout.timeline.label_room).max(0.0);
    let positions = timeline_positions(&composition.strand, width, &layout.timeline);
    let max_offset = (positions.last().copied().unwrap_or(0.0) - width).max(0.0);
    let offset = config.timeline_offset.clamp(0.0, max_offset);
    let baseline = -config.viewport.y + layout.timeline.margin;
    let top = (config.viewport.y - layout.timeline.height).max(0.0);
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
                translation: Vec3::new(layout.timeline.margin + line_width * 0.5, baseline, 0.0),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            size: Vec2::new(line_width, layout.timeline.line_width),
            color: colors.muted.linear,
        }));
    }
    for (entry, x) in composition.strand.iter().zip(positions) {
        let x = x - offset;
        if x < 0.0 || x > width {
            continue;
        }
        let position = Vec3::new(layout.timeline.margin + x, baseline, 0.0);
        bindings.insert(items.len(), Target::Entry(entry.id.clone()));
        items.push(Item::Dot(Dot {
            transform: Transform {
                space: Space::Hud,
                translation: position,
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            radius: layout.timeline.dot_radius
                * if entry.emphasis == Emphasis::Selected {
                    layout.selected_scale
                } else {
                    1.0
                },
            color: if entry.emphasis == Emphasis::Selected {
                colors.selected_event.linear
            } else {
                colors.event.linear
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
            size: Vec2::splat(layout.timeline.min_gap),
            color: [0.0; 4],
        }));
        let label = format!("{} · {}", entry.date, entry.label);
        let mut chars = label.chars();
        let mut content: String = chars.by_ref().take(layout.timeline.label_limit).collect();
        if chars.next().is_some() {
            content.pop();
            content.push('…');
        }
        bindings.insert(items.len(), Target::Entry(entry.id.clone()));
        items.push(Item::Text(Text {
            transform: Transform {
                space: Space::Hud,
                translation: position + Vec3::Y * layout.timeline.min_gap,
                rotation: Quat::from_rotation_z(layout.timeline.label_angle.to_radians()),
                scale: Vec3::ONE,
            },
            spec: TextSpec {
                content,
                font: config.font,
                size: layout.timeline.label_size,
                max_width: None,
            },
            // Keep the selected label visible and match its highlighted dot.
            color: if entry.emphasis == Emphasis::Selected {
                colors.selected_event.linear
            } else {
                colors.muted.linear
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
                    layout.timeline.margin,
                    baseline + layout.timeline.height - layout.timeline.line_height,
                    0.0,
                ),
                rotation: Quat::IDENTITY,
                scale: Vec3::ONE,
            },
            None,
            config,
            colors.accent.linear,
            Some(colors.panel.linear),
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
    let fov = layout.field_of_view.to_radians();
    let focal_length = config.viewport.y / (2.0 * (fov * 0.5).tan());
    let map_top = layout.timeline.margin + layout.timeline.line_height;
    let map_bottom = (top - layout.timeline.margin).max(map_top + 1.0);
    let top_slope = (config.viewport.y * 0.5 - map_top) / focal_length;
    let bottom_slope = (map_bottom - config.viewport.y * 0.5) / focal_length;
    let height = layout.events.height_base + layout.events.height_span;
    let bottom_height = if bottom_slope >= 0.0 { height } else { 0.0 };
    let distance = ((2.0 * half_size.y + top_slope * height + bottom_slope * bottom_height)
        / (top_slope + bottom_slope))
        .max(
            half_size.x * 2.0 * focal_length
                / (config.viewport.x - 2.0 * layout.timeline.margin).max(1.0)
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

const MIN_COS_LAT: f64 = 0.01;
const MIN_GEO_SPAN: f64 = 0.001;
const MIN_TIME_SPAN: f64 = 1.0;
const LAND_OFFSET: f32 = 0.005;
const LAKE_OFFSET: f32 = 0.01;
const SURFACE_OFFSET: f32 = 0.015;
const CONTOUR_OFFSET: f32 = 0.001;
const SHADE_STEPS: usize = 32;
const LAYOUT_STEPS: usize = 32;

fn timeline_positions(entries: &[Entry], width: f32, style: &config::Timeline) -> Vec<f32> {
    let Some(first) = entries.first() else {
        return Vec::new();
    };
    let span = entries.last().unwrap().time - first.time;
    let mut scale = if span > 0.0 { width as f64 / span } else { 0.0 };

    // Preserve proportional time gaps; clamp only crowded gaps to the minimum.
    // When they cannot fit, retain that scale and let the timeline scroll.
    if entries.len().saturating_sub(1) as f32 * style.min_gap <= width {
        let mut low = 0.0;
        let mut high = scale;
        for _ in 0..LAYOUT_STEPS {
            scale = (low + high) * 0.5;
            let length: f64 = entries
                .windows(2)
                .map(|pair| ((pair[1].time - pair[0].time) * scale).max(style.min_gap as f64))
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
        x += ((pair[1].time - pair[0].time) * scale).max(style.min_gap as f64);
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

fn line(items: &mut Vec<Item>, start: Vec3, end: Vec3, width: f32, color: [f32; 4], normal: Vec3) {
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
        Vec2::new(length, width),
        Quat::from_mat3(&Mat3::from_cols(x, y, x.cross(y))),
        color,
    );
}

fn tube(items: &mut Vec<Item>, start: Vec3, end: Vec3, color: [f32; 4], settings: &config::Config) {
    if start.distance_squared(end) <= f32::EPSILON * f32::EPSILON {
        return;
    }

    // Use the line's half-width as radius to retain its diameter when shading it.
    items.push(Item::Tube(Tube {
        start,
        end,
        radius: settings.layout.line_width * 0.5,
        color: [
            color[0],
            color[1],
            color[2],
            color[3] * settings.theme.opacity.tubes,
        ],
    }));
}

fn text(
    items: &mut Vec<Item>,
    content: &str,
    transform: Transform,
    width: Option<f32>,
    config: &Config<'_>,
    color: [f32; 4],
    background: Option<[f32; 4]>,
) {
    items.push(Item::Text(Text {
        transform,
        spec: TextSpec {
            content: content.into(),
            font: config.font,
            size: config.settings.layout.text.size,
            max_width: width.map(|width| width / config.settings.layout.text.scale),
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
    fn map_layers_round_and_align() {
        let settings = config::load(config::Kind::Dark).unwrap();
        let layout = &settings.layout;
        let colors = &settings.theme.colors;
        assert_eq!(layout.map.coastline_width, layout.line_width * 0.5);

        let viewport = Vec2::new(800.0, 600.0);
        let output = build(
            &Composition {
                title: String::new(),
                strand: Vec::new(),
                map: Some(Map {
                    bounds: Bounds {
                        west: -1.0,
                        east: 1.0,
                        south: -1.0,
                        north: 1.0,
                    },
                    markers: Vec::new(),
                    coastlines: vec![
                        [[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
                        [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    ],
                    land: vec![[[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]],
                    lakes: vec![[[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]],
                    rivers: vec![
                        [[-1.0, 0.0, 0.0], [0.0, 0.0, 0.0]],
                        [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                    ],
                    contours: Vec::new(),
                }),
            },
            &Config {
                settings: &settings,
                font: FontId(0),
                viewport,
                timeline_offset: 0.0,
            },
        )
        .unwrap();
        let clip_from_world = output
            .scene
            .camera
            .clip_from_world(viewport.x / viewport.y)
            .unwrap();

        // Cover the outer bend without extending into the corner of a square join.
        for (color, width) in [
            (colors.coastline.linear, layout.map.coastline_width),
            (colors.river.linear, layout.map.river_width),
        ] {
            for (offset, covered) in [(0.3, true), (0.45, false)] {
                let point = Vec3::new(offset * width, SURFACE_OFFSET, offset * width);
                let hit = output.scene.pick(
                    clip_from_world.project_point3(point).truncate(),
                    viewport,
                    |index| match &output.scene.items[index] {
                        Item::Plane(plane) => plane.color == color,
                        Item::Dot(dot) => dot.color == color,
                        _ => false,
                    },
                );
                assert_eq!(hit.is_some(), covered, "map line coverage at {point:?}");
            }
        }

        let fills: Vec<_> = output
            .scene
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let Item::Mesh(mesh) = item else { return None };
                assert!(!output.bindings.contains_key(&index));
                Some(mesh)
            })
            .collect();
        assert_eq!(fills.len(), 2);
        for (fill, (height, color)) in fills.iter().zip([
            (LAND_OFFSET, colors.land.linear),
            (LAKE_OFFSET, colors.water.linear),
        ]) {
            assert_eq!(fill.color, color);
            assert_eq!(
                fill.triangles,
                [[
                    Vec3::new(-4.0, height, 0.0),
                    Vec3::new(0.0, height, 0.0),
                    Vec3::new(0.0, height, -4.0),
                ]]
            );
        }
    }

    #[test]
    fn timeline_preserves_time_and_gap() {
        let settings = config::load(config::Kind::Dark).unwrap();
        let layout = &settings.layout;

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
            let positions = timeline_positions(&entries, width, &layout.timeline);
            let gaps: Vec<_> = positions.windows(2).map(|pair| pair[1] - pair[0]).collect();
            assert_eq!(positions.len(), entries.len());
            assert!(
                gaps.iter()
                    .all(|gap| *gap >= layout.timeline.min_gap - 0.001)
            );
            assert!((gaps[0] - layout.timeline.min_gap).abs() < 0.001);
            if width >= 200.0 {
                assert!((positions.last().unwrap() - width).abs() < 0.001);
                assert!((gaps[4] / gaps[3] - 30.0 / 9.0).abs() < 0.001);
            } else {
                assert!(*positions.last().unwrap() > width);
                assert!(gaps[4] > gaps[3]);
            }
        }
        assert!(timeline_positions(&[], 100.0, &layout.timeline).is_empty());
        assert_eq!(
            timeline_positions(&entries[..1], 100.0, &layout.timeline),
            [0.0]
        );
        assert_eq!(
            timeline_positions(&entries[..2], 100.0, &layout.timeline),
            [0.0, layout.timeline.min_gap]
        );
    }

    #[test]
    fn time_separates_same_place() {
        let settings = config::load(config::Kind::Dark).unwrap();
        let layout = &settings.layout;
        let colors = &settings.theme.colors;
        let opacity = &settings.theme.opacity;

        let place = Marker {
            id: "bern".into(),
            label: "Bern".into(),
            latitude: 46.95,
            longitude: 7.45,
            elevation: 0.0,
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
                land: Vec::new(),
                lakes: Vec::new(),
                rivers: Vec::new(),
                contours: Vec::new(),
            }),
        };
        let output = build(
            &composition,
            &Config {
                settings: &settings,
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
                Item::Tube(tube) if tube.color[..3] == colors.accent.linear[..3] => Some(tube),
                _ => None,
            })
            .collect();
        assert_eq!(segments.len(), points.len() - 1);
        for (segment, pair) in segments.iter().zip(points.windows(2)) {
            assert!(segment.start.abs_diff_eq(pair[0], 0.001));
            assert!(segment.end.abs_diff_eq(pair[1], 0.001));
            assert_eq!(segment.radius, layout.line_width * 0.5);
            assert_eq!(segment.color[3], opacity.tubes);
        }

        let stems: Vec<_> = output
            .scene
            .items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                let Item::Tube(tube) = item else { return None };
                assert!(!output.bindings.contains_key(&index));
                (tube.color[..3] == colors.stem.linear[..3]).then_some(tube)
            })
            .collect();
        assert_eq!(stems.len(), points.len());
        for (stem, point) in stems.iter().zip(&points) {
            assert_eq!(stem.start, Vec3::new(point.x, 0.0, point.z));
            assert_eq!(stem.end, *point);
            assert_eq!(stem.color[3], opacity.tubes);
        }

        composition.strand[1].place = None;
        let output = build(
            &composition,
            &Config {
                settings: &settings,
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
                Item::Tube(tube) if tube.color[..3] == colors.accent.linear[..3] => Some(tube),
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
        let settings = config::load(config::Kind::Dark).unwrap();

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
                settings: &settings,
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
