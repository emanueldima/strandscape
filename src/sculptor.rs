use {
    crate::{
        camera::{Camera, CameraConfig},
        scene::{Item, Plane, Scene, SceneConfig, Text, Transform},
        text::{FontId, TextSpec},
    },
    glam::{Quat, Vec2, Vec3},
    std::collections::HashMap,
    thiserror::Error,
};

pub(crate) struct Output {
    pub(crate) scene: Scene,
    pub(crate) bindings: HashMap<usize, Target>,
}

pub(crate) enum Target {
    Entry(String),
    Place(String),
}

pub(crate) struct Composition {
    pub(crate) title: String,
    pub(crate) reference: String,
    pub(crate) timeline: Option<Vec<Entry>>,
    pub(crate) map: Option<Map>,
}

pub(crate) struct Entry {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) date: String,
    pub(crate) time: f64,
    pub(crate) emphasis: Emphasis,
}

pub(crate) struct Map {
    pub(crate) markers: Vec<Marker>,
}

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
    pub(crate) aspect: f32,
}

#[derive(Debug, Error)]
pub(crate) enum BuildError {
    #[error("Invalid scene aspect ratio")]
    InvalidAspect,
}

pub(crate) fn build(composition: &Composition, config: &Config) -> Result<Output, BuildError> {
    if !config.aspect.is_finite() || config.aspect <= 0.0 {
        return Err(BuildError::InvalidAspect);
    }
    let mut items = Vec::new();
    let mut bindings = HashMap::new();
    text(
        &mut items,
        &composition.title,
        Vec2::new(-7.4, 4.8),
        14.8,
        28.0,
        config.font,
        INK,
    );
    text(
        &mut items,
        &format!("Reference: {}", composition.reference),
        Vec2::new(-7.4, 4.3),
        14.8,
        18.0,
        config.font,
        MUTED,
    );

    if let Some(entries) = &composition.timeline {
        plane(
            &mut items,
            Vec2::new(-3.9, -0.25),
            Vec2::new(7.8, 8.4),
            0.0,
            PANEL,
        );
        text(
            &mut items,
            "Timeline",
            Vec2::new(-7.45, 3.6),
            7.0,
            24.0,
            config.font,
            INK,
        );
        if let (Some(first), Some(last)) = (entries.first(), entries.last()) {
            let start = first.time;
            let span = (last.time - start).max(1.0);
            line(
                &mut items,
                Vec2::new(-7.2, 3.0),
                Vec2::new(-7.2, -3.8),
                MUTED,
            );
            for (index, entry) in entries.iter().enumerate() {
                let time_y = 3.0 - ((entry.time - start) / span) as f32 * 6.8;
                let row_y =
                    3.0 - index as f32 / entries.len().saturating_sub(1).max(1) as f32 * 6.8;
                let color = if entry.emphasis == Emphasis::Selected {
                    ACCENT
                } else {
                    MUTED
                };
                line(
                    &mut items,
                    Vec2::new(-7.2, time_y),
                    Vec2::new(-6.85, row_y),
                    color,
                );
                plane(
                    &mut items,
                    Vec2::new(-7.2, time_y),
                    Vec2::splat(0.07),
                    MARKER_DEPTH,
                    color,
                );
                bindings.insert(items.len(), Target::Entry(entry.id.clone()));
                plane(
                    &mut items,
                    Vec2::new(-3.6, row_y - 0.05),
                    Vec2::new(6.4, 0.36),
                    CONTENT_DEPTH,
                    if entry.emphasis == Emphasis::Selected {
                        HIGHLIGHT
                    } else {
                        PANEL
                    },
                );
                text(
                    &mut items,
                    &entry.date,
                    Vec2::new(-6.7, row_y + 0.08),
                    1.2,
                    16.0,
                    config.font,
                    color,
                );
                text(
                    &mut items,
                    &entry.label,
                    Vec2::new(-5.45, row_y + 0.08),
                    5.05,
                    18.0,
                    config.font,
                    INK,
                );
            }
        }
    }

    if let Some(map) = &composition.map {
        plane(
            &mut items,
            Vec2::new(4.0, -0.25),
            Vec2::new(7.6, 8.4),
            0.0,
            PANEL,
        );
        text(
            &mut items,
            "Places · schematic",
            Vec2::new(0.55, 3.6),
            6.8,
            24.0,
            config.font,
            INK,
        );
        text(
            &mut items,
            "Approximate city positions",
            Vec2::new(0.55, 3.1),
            6.8,
            16.0,
            config.font,
            MUTED,
        );
        if !map.markers.is_empty() {
            let west = map
                .markers
                .iter()
                .map(|p| p.longitude)
                .fold(f64::INFINITY, f64::min);
            let east = map
                .markers
                .iter()
                .map(|p| p.longitude)
                .fold(f64::NEG_INFINITY, f64::max);
            let south = map
                .markers
                .iter()
                .map(|p| p.latitude)
                .fold(f64::INFINITY, f64::min);
            let north = map
                .markers
                .iter()
                .map(|p| p.latitude)
                .fold(f64::NEG_INFINITY, f64::max);
            let mid_lat = (south + north) * 0.5;
            let cos_lat = mid_lat.to_radians().cos().max(0.01);
            let scale =
                (3.4 / ((east - west) * cos_lat).max(1.0)).min(4.6 / (north - south).max(1.0));
            let mut markers: Vec<_> = map.markers.iter().collect();
            markers.sort_by(|a, b| {
                b.latitude
                    .total_cmp(&a.latitude)
                    .then_with(|| a.id.cmp(&b.id))
            });

            // Callouts keep nearby cities readable without moving their map positions.
            for (index, marker) in markers.iter().enumerate() {
                let point = Vec2::new(
                    2.4 + ((marker.longitude - (west + east) * 0.5) * cos_lat * scale) as f32,
                    -0.4 + ((marker.latitude - mid_lat) * scale) as f32,
                );
                let label_y = 2.5 - index as f32 * 0.48;
                let color = if marker.emphasis == Emphasis::Selected {
                    ACCENT
                } else {
                    MUTED
                };
                line(&mut items, point, Vec2::new(4.5, label_y), color);
                bindings.insert(items.len(), Target::Place(marker.id.clone()));
                plane(&mut items, point, Vec2::splat(0.1), MARKER_DEPTH, color);
                text(
                    &mut items,
                    &marker.label,
                    Vec2::new(4.65, label_y + 0.08),
                    2.7,
                    20.0,
                    config.font,
                    color,
                );
            }
        }
    }

    let fov = 45.0_f32.to_radians();
    let distance = 5.5_f32.max(8.3 / config.aspect) / (fov * 0.5).tan();
    Ok(Output {
        scene: Scene::new(SceneConfig {
            camera: Camera::new(CameraConfig {
                position: Vec3::new(0.0, 0.0, distance),
                focus: Vec3::ZERO,
                vertical_fov: fov,
                near: 0.1,
                far: 100.0,
            }),
            items,
        }),
        bindings,
    })
}

const PANEL: [f32; 4] = [0.035, 0.10, 0.13, 1.0];
const INK: [f32; 4] = [0.91, 0.84, 0.66, 1.0];
const MUTED: [f32; 4] = [0.42, 0.61, 0.64, 1.0];
const ACCENT: [f32; 4] = [1.0, 0.72, 0.30, 1.0];
const HIGHLIGHT: [f32; 4] = [0.10, 0.20, 0.24, 1.0];
const CONTENT_DEPTH: f32 = 0.01;
const MARKER_DEPTH: f32 = 0.02;
const TEXT_DEPTH: f32 = 0.03;
const TEXT_SCALE: f32 = 0.009;

fn plane(items: &mut Vec<Item>, position: Vec2, size: Vec2, depth: f32, color: [f32; 4]) {
    items.push(Item::Plane(Plane {
        transform: Transform {
            translation: position.extend(depth),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        },
        size,
        color,
    }));
}

fn line(items: &mut Vec<Item>, start: Vec2, end: Vec2, color: [f32; 4]) {
    let delta = end - start;
    items.push(Item::Plane(Plane {
        transform: Transform {
            translation: ((start + end) * 0.5).extend(CONTENT_DEPTH),
            rotation: Quat::from_rotation_z(delta.y.atan2(delta.x)),
            scale: Vec3::ONE,
        },
        size: Vec2::new(delta.length(), 0.012),
        color,
    }));
}

fn text(
    items: &mut Vec<Item>,
    content: &str,
    position: Vec2,
    width: f32,
    size: f32,
    font: FontId,
    color: [f32; 4],
) {
    items.push(Item::Text(Text {
        transform: Transform {
            translation: position.extend(TEXT_DEPTH),
            rotation: Quat::IDENTITY,
            scale: Vec3::splat(TEXT_SCALE),
        },
        spec: TextSpec {
            content: content.into(),
            font,
            size,
            max_width: Some(width / TEXT_SCALE),
        },
        color,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_points_are_pickable() {
        let composition = Composition {
            title: "Example".into(),
            reference: "1905".into(),
            timeline: Some(vec![Entry {
                id: "entry".into(),
                label: "An event".into(),
                date: "1905".into(),
                time: 1905.0,
                emphasis: Emphasis::Normal,
            }]),
            map: Some(Map {
                markers: vec![Marker {
                    id: "place".into(),
                    label: "A place".into(),
                    latitude: 90.0,
                    longitude: 180.0,
                    emphasis: Emphasis::Normal,
                }],
            }),
        };
        let output = build(
            &composition,
            &Config {
                font: FontId(0),
                aspect: 1.6,
            },
        )
        .unwrap();
        let projection = output.scene.camera.clip_from_world(1.6).unwrap();
        assert_eq!(output.bindings.len(), 2);
        for index in output.bindings.keys() {
            let Item::Plane(plane) = &output.scene.items[*index] else {
                panic!("expected hit surface")
            };
            assert!(plane.transform.matrix().is_finite());
            let point = projection
                .project_point3(plane.transform.translation)
                .truncate();
            assert_eq!(output.scene.pick(point, 1.6).unwrap().item, *index);
        }
    }
}
