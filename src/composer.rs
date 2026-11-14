use {
    crate::{
        events::Dataset,
        sculptor::{self, Bounds, Composition, Emphasis, Entry, Map, Marker},
        text::FontId,
    },
    glam::Vec2,
    std::collections::BTreeMap,
    thiserror::Error,
};

pub(crate) use crate::sculptor::{Output, Target};

pub(crate) struct Config<'a> {
    pub(crate) selected: Option<&'a str>,
    pub(crate) font: FontId,
    pub(crate) viewport: Vec2,
    pub(crate) timeline_offset: f32,
}

#[derive(Debug, Error)]
pub(crate) enum ComposeError {
    #[error("Unknown event: {0}")]
    UnknownEvent(String),
    #[error(transparent)]
    Scene(#[from] sculptor::BuildError),
}

pub(crate) fn build(dataset: &Dataset, config: &Config<'_>) -> Result<Output, ComposeError> {
    Ok(sculptor::build(
        &compose(dataset, config)?,
        &sculptor::Config {
            font: config.font,
            viewport: config.viewport,
            timeline_offset: config.timeline_offset,
        },
    )?)
}

fn compose(dataset: &Dataset, config: &Config<'_>) -> Result<Composition, ComposeError> {
    let selected = config
        .selected
        .map(|id| {
            dataset
                .events
                .iter()
                .find(|event| event.id == id)
                .ok_or_else(|| ComposeError::UnknownEvent(id.into()))
        })
        .transpose()?;
    let selected_place = selected.and_then(|event| event.place.as_ref());
    let mut places = BTreeMap::new();
    let mut strand: Vec<_> = dataset
        .events
        .iter()
        .map(|event| {
            let place = event.place.as_ref().map(|place| Marker {
                id: place.id.clone(),
                label: place.name.clone(),
                latitude: place.latitude,
                longitude: place.longitude,
                emphasis: if selected_place.is_some_and(|p| p.id == place.id) {
                    Emphasis::Selected
                } else {
                    Emphasis::Normal
                },
            });
            if let Some(place) = &place {
                places
                    .entry(place.id.clone())
                    .or_insert_with(|| place.clone());
            }
            Entry {
                id: event.id.clone(),
                label: event.title.clone(),
                date: event.date.label.clone(),
                time: event.date.value,
                place,
                emphasis: if config.selected == Some(event.id.as_str()) {
                    Emphasis::Selected
                } else {
                    Emphasis::Normal
                },
            }
        })
        .collect();
    strand.sort_by(|a, b| a.time.total_cmp(&b.time).then_with(|| a.id.cmp(&b.id)));

    // Located events imply geographic context; missing locations remain explicit.
    let map = if places.is_empty() {
        None
    } else {
        let mut bounds = Bounds {
            west: f64::INFINITY,
            east: f64::NEG_INFINITY,
            south: f64::INFINITY,
            north: f64::NEG_INFINITY,
        };
        for place in places.values() {
            bounds.west = bounds.west.min(place.longitude);
            bounds.east = bounds.east.max(place.longitude);
            bounds.south = bounds.south.min(place.latitude);
            bounds.north = bounds.north.max(place.latitude);
        }
        let longitude_margin = ((bounds.east - bounds.west) * MAP_MARGIN).max(MIN_MARGIN);
        let latitude_margin = ((bounds.north - bounds.south) * MAP_MARGIN).max(MIN_MARGIN);
        bounds.west = (bounds.west - longitude_margin).max(-180.0);
        bounds.east = (bounds.east + longitude_margin).min(180.0);
        bounds.south = (bounds.south - latitude_margin).max(-90.0);
        bounds.north = (bounds.north + latitude_margin).min(90.0);
        Some(Map {
            bounds,
            markers: places.into_values().collect(),
        })
    };
    Ok(Composition {
        title: dataset.title.clone(),
        strand,
        map,
    })
}

const MAP_MARGIN: f64 = 0.1;
const MIN_MARGIN: f64 = 1.0;

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::scene::{Item, Space},
    };

    #[test]
    fn build_preserves_ids() {
        let data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        let output = build(
            &data,
            &Config {
                selected: None,
                font: FontId(0),
                viewport: Vec2::new(800.0, 500.0),
                timeline_offset: 0.0,
            },
        )
        .unwrap();
        let events: std::collections::BTreeSet<_> = output
            .bindings
            .values()
            .filter_map(|target| match target {
                Target::Entry(id, _) => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), data.events.len());
        assert!(events.contains("einstein-nobel"));
        assert_eq!(
            output
                .bindings
                .values()
                .filter(|target| matches!(target, Target::Place(_, _)))
                .count(),
            18
        );
        for (index, target) in &output.bindings {
            let Target::Place(id, point) = target else {
                continue;
            };
            let place = data
                .events
                .iter()
                .filter_map(|event| event.place.as_ref())
                .find(|place| &place.id == id)
                .unwrap();
            let label_index = if matches!(&output.scene.items[*index], Item::Text(_)) {
                *index
            } else {
                index + 1
            };
            assert!(
                matches!(&output.bindings[&label_index], Target::Place(label_id, label_point)
                if label_id == id && label_point == point)
            );
            let crate::scene::Item::Text(text) = &output.scene.items[label_index] else {
                panic!("expected place label");
            };
            assert_eq!(text.spec.content, place.name);
            assert_eq!(text.background, Some([0.0; 4]));
            assert!(text.transform.translation.y > 0.0 && text.transform.translation.y < 0.1);
            assert!((text.transform.rotation * glam::Vec3::Z).abs_diff_eq(glam::Vec3::Y, 0.001));
            assert_eq!(
                output
                    .scene
                    .items
                    .iter()
                    .filter(|item| matches!(item,
                crate::scene::Item::Text(label) if label.spec.content == place.name))
                    .count(),
                1
            );
        }
    }

    #[test]
    fn place_labels_pick_their_place() {
        let mut data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        data.events
            .retain(|event| event.place.as_ref().is_some_and(|place| place.id == "ulm"));
        let viewport = Vec2::new(1280.0, 800.0);
        let mut output = build(
            &data,
            &Config {
                selected: None,
                font: FontId(0),
                viewport,
                timeline_offset: 0.0,
            },
        )
        .unwrap();
        let (index, label) = output
            .scene
            .items
            .iter_mut()
            .enumerate()
            .find_map(|(index, item)| match item {
                Item::Text(text) if text.transform.space == Space::World => Some((index, text)),
                _ => None,
            })
            .unwrap();
        label.layout = Some(crate::text::TextLayout {
            glyphs: Vec::new().into(),
            bounds: [100.0, 27.0],
        });
        let transform = label.transform;
        let Target::Place(_, place) = output.bindings[&index] else {
            unreachable!()
        };

        for orbit in [Vec2::ZERO, Vec2::new(0.2, 0.25)] {
            output.scene.camera.orbit(orbit);
            let clip = transform.clip_from_local(
                output
                    .scene
                    .camera
                    .clip_from_world(viewport.x / viewport.y)
                    .unwrap(),
                viewport,
            );
            let point = clip
                .project_point3(glam::Vec3::new(50.0, -13.5, 0.0))
                .truncate();
            let hit = output
                .scene
                .pick(point, viewport, |index| {
                    output.bindings.contains_key(&index)
                })
                .unwrap();
            assert_eq!(hit.item, index);
            assert!(
                matches!(&output.bindings[&hit.item], Target::Place(id, center) if id == "ulm" && *center == place)
            );
        }
    }

    #[test]
    fn timeline_labels_bind_events() {
        let data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        for offset in [0.0, f32::MAX] {
            let mut output = build(
                &data,
                &Config {
                    selected: None,
                    font: FontId(0),
                    viewport: Vec2::new(320.0, 500.0),
                    timeline_offset: offset,
                },
            )
            .unwrap();
            let mut labels = Vec::new();
            for (index, item) in output.scene.items.iter_mut().enumerate() {
                let Item::Text(text) = item else {
                    continue;
                };
                if text.transform.space != Space::Hud
                    || text.transform.rotation == glam::Quat::IDENTITY
                {
                    continue;
                }

                let Some(Target::Entry(id, point)) = output.bindings.get(&index) else {
                    panic!("timeline label must select its event");
                };
                let event = data.events.iter().find(|event| &event.id == id).unwrap();
                assert!(text.spec.content.starts_with(&event.date.label));
                assert_eq!(point.is_some(), event.place.is_some());
                assert_eq!(text.background, Some([0.0; 4]));
                text.layout = Some(crate::text::TextLayout {
                    glyphs: Vec::new().into(),
                    bounds: [80.0, 16.0],
                });
                labels.push((
                    index,
                    text.transform
                        .matrix()
                        .transform_point3(glam::Vec3::new(40.0, -8.0, 0.0)),
                ));
            }
            assert!(!labels.is_empty());

            output.scene.camera.orbit(Vec2::new(0.2, 0.1));
            output.scene.camera.pan(Vec2::new(0.1, -0.2));
            output.scene.camera.zoom(-250.0);
            for (index, point) in labels {
                let ndc = Vec2::new(point.x / 320.0 * 2.0 - 1.0, point.y / 500.0 * 2.0 + 1.0);
                assert_eq!(
                    output
                        .scene
                        .pick(ndc, Vec2::new(320.0, 500.0), |_| true)
                        .unwrap()
                        .item,
                    index
                );
            }
        }
    }

    #[test]
    fn selection_highlights_both_views() {
        let data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        let viewport = Vec2::new(800.0, 500.0);
        for selected in [None, Some("einstein-miracle-year"), Some("einstein-nobel")] {
            let mut output = build(
                &data,
                &Config {
                    selected,
                    font: FontId(0),
                    viewport,
                    timeline_offset: 0.0,
                },
            )
            .unwrap();
            // Fixed HUD text is limited to the title and selected-event details.
            assert_eq!(
                output
                    .scene
                    .items
                    .iter()
                    .filter(|item| matches!(item,
                        Item::Text(text) if text.transform.space == Space::Hud
                            && text.transform.rotation == glam::Quat::IDENTITY
                    ))
                    .count(),
                1 + usize::from(selected.is_some())
            );
            assert_eq!(
                output
                    .bindings
                    .iter()
                    .filter(|(index, target)| {
                        matches!(target, Target::Entry(_, _))
                            && matches!(&output.scene.items[**index], Item::Text(_))
                    })
                    .count(),
                data.events.len()
            );
            let mut hud_count = 0;
            let mut world_count = 0;
            let mut selected_dots = Vec::new();
            for (index, target) in &output.bindings {
                let Target::Entry(id, point) = target else {
                    continue;
                };
                let (space, position, radius, color) = match &output.scene.items[*index] {
                    Item::Dot(dot) => (
                        dot.transform.space,
                        dot.transform.translation,
                        dot.radius,
                        dot.color,
                    ),
                    Item::Sphere(sphere) => {
                        (Space::World, sphere.center, sphere.radius, sphere.color)
                    }
                    _ => continue,
                };
                let event = data.events.iter().find(|event| &event.id == id).unwrap();
                if space == Space::World {
                    world_count += 1;
                    assert!(event.place.is_some());
                    assert_eq!(Some(position), *point);
                    assert_eq!(
                        radius,
                        if selected == Some(id.as_str()) {
                            0.06
                        } else {
                            0.04
                        }
                    );
                } else {
                    hud_count += 1;
                    assert_eq!(position.y, -476.0);
                    assert_eq!(point.is_some(), event.place.is_some());
                    assert_eq!(
                        radius,
                        if selected == Some(id.as_str()) {
                            6.0
                        } else {
                            4.0
                        }
                    );
                    let ndc = Vec2::new(
                        position.x / viewport.x * 2.0 - 1.0,
                        position.y / viewport.y * 2.0 + 1.0,
                    );
                    let hit = output.scene.pick(ndc, viewport, |_| true).unwrap();
                    assert!(
                        matches!(&output.bindings[&hit.item], Target::Entry(hit_id, _) if hit_id == id)
                    );
                }
                if selected == Some(id.as_str()) {
                    selected_dots.push(color);
                }
            }
            assert_eq!(hud_count, data.events.len());
            assert_eq!(
                world_count,
                data.events
                    .iter()
                    .filter(|event| event.place.is_some())
                    .count()
            );
            if let Some(id) = selected {
                let event = data.events.iter().find(|event| event.id == id).unwrap();
                assert_eq!(
                    selected_dots.len(),
                    if event.place.is_some() { 2 } else { 1 }
                );
                assert!(selected_dots.iter().all(|color| *color == selected_dots[0]));
                let label = output
                    .bindings
                    .iter()
                    .find_map(
                        |(index, target)| match (target, &output.scene.items[*index]) {
                            (Target::Entry(event_id, _), Item::Text(text)) if event_id == id => {
                                Some(text)
                            }
                            _ => None,
                        },
                    )
                    .unwrap();
                assert_eq!(label.color, selected_dots[0]);
            }
            let panes: Vec<_> = output
                .scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Text(text) if text.background.is_some_and(|color| color[3] > 0.0) => {
                        Some(text)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(panes.len(), usize::from(selected.is_some()));
            if let Some(id) = selected {
                let event = data.events.iter().find(|event| event.id == id).unwrap();
                let pane = panes[0];
                assert_eq!(pane.transform.space, Space::Hud);
                assert_eq!(pane.color[3], 1.0);
                assert_eq!(pane.background.unwrap()[3], 1.0);
                assert_eq!(
                    pane.spec.content,
                    format!(
                        "{} · {}{}",
                        event.date.label,
                        event.title,
                        if event.place.is_none() {
                            " · place unknown"
                        } else {
                            ""
                        }
                    )
                );
            }
            assert_eq!(
                output
                    .scene
                    .items
                    .iter()
                    .filter(|item| matches!(item,
                Item::Text(text) if text.transform.space == Space::World))
                    .count(),
                9
            );

            // HUD dots stay selectable after all camera operations.
            output.scene.camera.orbit(Vec2::new(0.2, 0.1));
            output.scene.camera.pan(Vec2::new(0.1, -0.2));
            output.scene.camera.zoom(-250.0);
            output.scene.camera.focus(glam::Vec3::new(3.0, 2.0, 1.0));
            for (index, target) in &output.bindings {
                let Target::Entry(id, _) = target else {
                    continue;
                };
                let Item::Dot(dot) = &output.scene.items[*index] else {
                    continue;
                };
                if dot.transform.space != Space::Hud {
                    continue;
                }
                let ndc = Vec2::new(
                    dot.transform.translation.x / viewport.x * 2.0 - 1.0,
                    dot.transform.translation.y / viewport.y * 2.0 + 1.0,
                );
                let hit = output.scene.pick(ndc, viewport, |_| true).unwrap();
                assert!(
                    matches!(&output.bindings[&hit.item], Target::Entry(hit_id, _) if hit_id == id)
                );
            }
        }
    }

    #[test]
    fn scrolling_keeps_hud_text_fixed() {
        let data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        for viewport in [Vec2::new(320.0, 500.0), Vec2::new(600.0, 800.0)] {
            let mut config = Config {
                selected: Some("einstein-miracle-year"),
                font: FontId(0),
                viewport,
                timeline_offset: 0.0,
            };
            let first = build(&data, &config).unwrap();
            config.timeline_offset = f32::MAX;
            let last = build(&data, &config).unwrap();
            assert_eq!(last.timeline.offset, last.timeline.max_offset);
            let text: Vec<_> = first
                .scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Text(text)
                        if text.transform.space == Space::World
                            || text.transform.rotation == glam::Quat::IDENTITY =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .collect();
            let scrolled: Vec<_> = last
                .scene
                .items
                .iter()
                .filter_map(|item| match item {
                    Item::Text(text)
                        if text.transform.space == Space::World
                            || text.transform.rotation == glam::Quat::IDENTITY =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(text.len(), scrolled.len());
            for (a, b) in text.iter().zip(scrolled) {
                assert_eq!(a.spec.content, b.spec.content);
                assert_eq!(a.transform.matrix(), b.transform.matrix());
            }
            let first_dot = first
                .scene
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Dot(dot) if dot.transform.space == Space::Hud => Some(dot),
                    _ => None,
                })
                .unwrap();
            assert_eq!(first_dot.transform.translation.y, -viewport.y + 24.0);
            let last_dot = last
                .scene
                .items
                .iter()
                .rev()
                .find_map(|item| match item {
                    Item::Dot(dot) if dot.transform.space == Space::Hud => Some(dot),
                    _ => None,
                })
                .unwrap();
            assert!((last_dot.transform.translation.x - (viewport.x - 184.0)).abs() < 0.001);

            // Scrolling exposes every event, including those without a location.
            let mut seen = std::collections::BTreeSet::new();
            for step in 0..=first.timeline.max_offset.ceil() as usize {
                config.timeline_offset = step as f32;
                let output = build(&data, &config).unwrap();
                for (index, target) in &output.bindings {
                    if let (Target::Entry(id, _), Item::Dot(dot)) =
                        (target, &output.scene.items[*index])
                        && dot.transform.space == Space::Hud
                    {
                        seen.insert(id.clone());
                    }
                }
            }
            assert_eq!(seen.len(), data.events.len());
            let empty = Vec2::new(0.0, 1.0 - (viewport.y - 8.0) / viewport.y * 2.0);
            let hit = first.scene.pick(empty, viewport, |_| true).unwrap();
            assert!(!first.bindings.contains_key(&hit.item));
            assert!(matches!(&first.scene.items[hit.item], Item::Plane(plane)
                if plane.transform.space == Space::Hud));
        }
    }

    #[test]
    fn infers_map_and_orders_strand() {
        let mut data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        data.events.reverse();
        let composition = compose(
            &data,
            &Config {
                selected: None,
                font: FontId(0),
                viewport: Vec2::new(800.0, 500.0),
                timeline_offset: 0.0,
            },
        )
        .unwrap();
        assert_eq!(composition.strand.len(), 18);
        assert!(
            composition
                .strand
                .windows(2)
                .all(|entries| entries[0].time <= entries[1].time)
        );
        let map = composition.map.unwrap();
        assert_eq!(map.markers.len(), 9);
        assert!(map.bounds.west < -74.67 && map.bounds.east > 14.44);
        assert!(
            composition
                .strand
                .iter()
                .any(|entry| entry.id == "einstein-nobel" && entry.place.is_none())
        );
    }

    #[test]
    fn selection_sets_map_context() {
        let data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        let mut config = Config {
            selected: Some("einstein-miracle-year"),
            font: FontId(0),
            viewport: Vec2::new(800.0, 500.0),
            timeline_offset: 0.0,
        };
        let composition = compose(&data, &config).unwrap();
        let map = composition.map.unwrap();
        assert!(
            map.markers
                .iter()
                .any(|place| place.id == "bern" && place.emphasis == Emphasis::Selected)
        );
        config.selected = Some("einstein-nobel");
        let composition = compose(&data, &config).unwrap();
        let map = composition.map.unwrap();
        assert!(
            map.markers
                .iter()
                .all(|place| place.emphasis == Emphasis::Normal)
        );
        config.selected = Some("missing");
        assert!(matches!(
            compose(&data, &config),
            Err(ComposeError::UnknownEvent(_))
        ));
    }

    #[test]
    fn unlocated_events_need_no_map() {
        let mut data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        data.events.retain(|event| event.place.is_none());
        let composition = compose(
            &data,
            &Config {
                selected: None,
                font: FontId(0),
                viewport: Vec2::new(800.0, 500.0),
                timeline_offset: 0.0,
            },
        )
        .unwrap();
        assert!(composition.map.is_none());
        assert_eq!(composition.strand.len(), 1);
    }
}
