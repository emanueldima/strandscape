use {serde::Deserialize, thiserror::Error};

pub(crate) struct Dataset {
    paths: Vec<Vec<[f64; 2]>>,
}

#[derive(Debug, Error)]
pub(crate) enum DataError {
    #[error("Invalid geography: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid geography: expected coastline paths with longitude/latitude coordinates")]
    InvalidPath,
}

impl Dataset {
    pub(crate) fn parse(json: &[u8]) -> Result<Self, DataError> {
        let Collection::FeatureCollection { features } = serde_json::from_slice(json)?;
        let paths: Vec<_> = features
            .into_iter()
            .flat_map(|feature| match feature.geometry {
                Geometry::LineString(path) => vec![path],
                Geometry::MultiLineString(paths) => paths,
            })
            .collect();
        if paths.is_empty()
            || paths.iter().any(|path| {
                path.len() < 2
                    || path.iter().any(|[longitude, latitude]| {
                        !(-180.0..=180.0).contains(longitude) || !(-90.0..=90.0).contains(latitude)
                    })
            })
        {
            return Err(DataError::InvalidPath);
        }

        Ok(Self { paths })
    }
}

pub(crate) fn build(dataset: &Dataset, bounds: [f64; 4]) -> Vec<[[f64; 2]; 2]> {
    let [west, south, east, north] = bounds;
    dataset
        .paths
        .iter()
        .flat_map(|path| path.windows(2))
        .filter_map(|pair| {
            let [x, y] = pair[0];
            let dx = pair[1][0] - x;
            let dy = pair[1][1] - y;
            let mut start: f64 = 0.0;
            let mut end: f64 = 1.0;

            // Clip each segment, retaining crossings even when both endpoints are outside.
            for (origin, delta, minimum, maximum) in [(x, dx, west, east), (y, dy, south, north)] {
                if delta == 0.0 {
                    if origin < minimum || origin > maximum {
                        return None;
                    }
                    continue;
                }

                let first = (minimum - origin) / delta;
                let last = (maximum - origin) / delta;
                start = start.max(first.min(last));
                end = end.min(first.max(last));
                if start >= end {
                    return None;
                }
            }

            (dx != 0.0 || dy != 0.0).then_some([
                [x + start * dx, y + start * dy],
                [x + end * dx, y + end * dy],
            ])
        })
        .collect()
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum Collection {
    FeatureCollection { features: Vec<Feature> },
}

#[derive(Deserialize)]
struct Feature {
    geometry: Geometry,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "coordinates")]
enum Geometry {
    LineString(Vec<[f64; 2]>),
    MultiLineString(Vec<Vec<[f64; 2]>>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_crossing_paths() {
        let dataset = Dataset::parse(
            br#"{
            "type": "FeatureCollection",
            "features": [{"geometry": {"type": "MultiLineString", "coordinates": [
                [[-2, 0], [2, 0]],
                [[0, 2], [0, -2]],
                [[-2, 2], [2, 2]],
                [[2, 2], [3, 3]],
                [[0, 0], [0, 0]]
            ]}}]
        }"#,
        )
        .unwrap();
        assert_eq!(
            build(&dataset, [-1.0, -1.0, 1.0, 1.0]),
            [[[-1.0, 0.0], [1.0, 0.0]], [[0.0, 1.0], [0.0, -1.0]]]
        );
    }

    #[test]
    fn rejects_invalid_coordinates() {
        for coordinates in ["[[181, 0], [0, 0]]", "[[0, -91], [0, 0]]", "[[0, 0]]"] {
            let json = format!(
                r#"{{"type":"FeatureCollection","features":[{{"geometry":{{"type":"LineString","coordinates":{coordinates}}}}}]}}"#
            );
            assert!(matches!(
                Dataset::parse(json.as_bytes()),
                Err(DataError::InvalidPath)
            ));
        }
    }

    #[test]
    fn reads_bundled_coastlines() {
        let dataset = Dataset::parse(include_bytes!("../data/coastlines.geojson")).unwrap();
        let bounds = [-85.0, 35.0, 25.0, 60.0];
        let segments = build(&dataset, bounds);
        assert!(!segments.is_empty());
        for [longitude, latitude] in segments.into_iter().flatten() {
            assert!((bounds[0]..=bounds[2]).contains(&longitude));
            assert!((bounds[1]..=bounds[3]).contains(&latitude));
        }
    }
}
