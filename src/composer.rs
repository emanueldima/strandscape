use {
    crate::{
        events::{Dataset, Date},
        sculptor::{self, Composition, Entry, Map, Marker},
        text::FontId,
    },
    std::collections::BTreeMap,
    thiserror::Error,
};

pub(crate) use crate::sculptor::{Output, Target};

pub(crate) struct Config<'a> {
    pub(crate) layers: &'a [ContextLayer],
    pub(crate) selected: Option<&'a str>,
    pub(crate) reference: &'a Date,
    pub(crate) font: FontId,
    pub(crate) aspect: f32,
}

#[derive(PartialEq)]
pub(crate) enum ContextLayer {
    Timeline,
    Map,
}

#[derive(Debug, Error)]
pub(crate) enum ComposeError {
    #[error("Unknown event: {0}")]
    UnknownEvent(String),
    #[error(transparent)]
    Scene(#[from] sculptor::BuildError),
}

pub(crate) fn build(dataset: &Dataset, config: &Config<'_>) -> Result<Output, ComposeError> {
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

    // The planner supplies context; the sculptor receives no dataset or source format.
    let mut entries: Vec<_> = dataset
        .events
        .iter()
        .map(|event| Entry {
            id: event.id.clone(),
            label: event.title.clone(),
            date: event.date.label.clone(),
            time: event.date.value,
            emphasis: if config.selected == Some(event.id.as_str()) {
                sculptor::Emphasis::Selected
            } else {
                sculptor::Emphasis::Normal
            },
        })
        .collect();
    entries.sort_by(|a, b| a.time.total_cmp(&b.time).then_with(|| a.id.cmp(&b.id)));

    let mut places = BTreeMap::new();
    for event in &dataset.events {
        let Some(place) = &event.place else { continue };
        places.entry(&place.id).or_insert(place);
    }
    let map = config.layers.contains(&ContextLayer::Map).then(|| Map {
        markers: places
            .values()
            .map(|place| Marker {
                id: place.id.clone(),
                label: place.name.clone(),
                latitude: place.latitude,
                longitude: place.longitude,
                emphasis: if selected
                    .and_then(|event| event.place.as_ref())
                    .is_some_and(|p| p.id == place.id)
                {
                    sculptor::Emphasis::Selected
                } else {
                    sculptor::Emphasis::Normal
                },
            })
            .collect(),
    });
    let composition = Composition {
        title: dataset.title.clone(),
        reference: selected
            .map_or(config.reference, |event| &event.date)
            .label
            .clone(),
        timeline: config
            .layers
            .contains(&ContextLayer::Timeline)
            .then_some(entries),
        map,
    };
    Ok(sculptor::build(
        &composition,
        &sculptor::Config {
            font: config.font,
            aspect: config.aspect,
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_events_and_places() {
        let data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        let output = build(
            &data,
            &Config {
                layers: &[ContextLayer::Timeline, ContextLayer::Map],
                selected: None,
                reference: &data.events[0].date,
                font: FontId(0),
                aspect: 1.6,
            },
        )
        .unwrap();
        let events: Vec<_> = output
            .bindings
            .values()
            .filter_map(|target| match target {
                Target::Entry(id) => Some(id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), data.events.len());
        assert!(events.contains(&"einstein-nobel"));
        assert_eq!(
            output
                .bindings
                .values()
                .filter(|target| matches!(target, Target::Place(_)))
                .count(),
            9
        );
    }

    #[test]
    fn honors_layers_and_selection() {
        let data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        let mut config = Config {
            layers: &[ContextLayer::Timeline],
            selected: None,
            reference: &data.events[0].date,
            font: FontId(0),
            aspect: 1.6,
        };
        let output = build(&data, &config).unwrap();
        assert!(
            output
                .bindings
                .values()
                .all(|target| matches!(target, Target::Entry(_)))
        );
        config.selected = Some("missing");
        assert!(matches!(
            build(&data, &config),
            Err(ComposeError::UnknownEvent(_))
        ));
    }

    #[test]
    fn selection_keeps_unknown_place() {
        let data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        let output = build(
            &data,
            &Config {
                layers: &[ContextLayer::Timeline, ContextLayer::Map],
                selected: Some("einstein-nobel"),
                reference: &data.events[0].date,
                font: FontId(0),
                aspect: 1.6,
            },
        )
        .unwrap();
        assert!(output.scene.items.iter().any(|item| matches!(item,
            crate::scene::Item::Text(text) if text.spec.content == "Reference: 1922")));
        assert_eq!(
            output
                .bindings
                .values()
                .filter(|target| matches!(target, Target::Place(id) if id == "bern"))
                .count(),
            1
        );
    }
}
