mod terrain;

use {serde::Deserialize, std::cell::RefCell, thiserror::Error};

pub(crate) struct Dataset {
    coastlines: Vec<Vec<[f64; 2]>>,
    land: Vec<[[f64; 2]; 3]>,
    lakes: Vec<[[f64; 2]; 3]>,
    rivers: Vec<Vec<[f64; 2]>>,
    terrain: terrain::Dataset,
    cache: RefCell<Option<CachedLayers>>,
}

pub(crate) struct Sources<'a> {
    pub(crate) coastlines: &'a [u8],
    pub(crate) land: &'a [u8],
    pub(crate) lakes: &'a [u8],
    pub(crate) rivers: &'a [u8],
    pub(crate) terrain: &'a [u8],
}

#[derive(Clone)]
pub(crate) struct Layers {
    pub(crate) coastlines: Vec<[[f64; 3]; 2]>,
    pub(crate) land: Vec<[[f64; 3]; 3]>,
    pub(crate) lakes: Vec<[[f64; 3]; 3]>,
    pub(crate) rivers: Vec<[[f64; 3]; 2]>,
    pub(crate) contours: Vec<[[f64; 3]; 2]>,
}

struct CachedLayers {
    bounds: [f64; 4],
    contour_interval: f64,
    layers: Layers,
}

#[derive(Debug, Error)]
pub(crate) enum DataError {
    #[error("Invalid geography: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid geography: expected valid paths or closed rings in longitude/latitude")]
    InvalidPath,
    #[error("Invalid geography: polygon triangulation failed")]
    InvalidPolygon,
    #[error("Invalid terrain: expected a regular elevation grid without missing samples")]
    InvalidTerrain,
}

impl Dataset {
    pub(crate) fn parse(sources: Sources<'_>) -> Result<Self, DataError> {
        Ok(Self {
            coastlines: parse_lines(sources.coastlines)?,
            land: parse_fills(sources.land)?,
            lakes: parse_fills(sources.lakes)?,
            rivers: parse_lines(sources.rivers)?,
            terrain: terrain::Dataset::parse(sources.terrain)?,
            cache: RefCell::new(None),
        })
    }

    pub(crate) fn elevation(&self, point: [f64; 2]) -> f64 {
        self.terrain.elevation(point)
    }
}

pub(crate) fn build(dataset: &Dataset, bounds: [f64; 4], contour_interval: f64) -> Layers {
    // Theme, selection, and timeline changes reuse the same clipped geography.
    if let Some(cache) = dataset.cache.borrow().as_ref()
        && cache.bounds == bounds
        && cache.contour_interval == contour_interval
    {
        return cache.layers.clone();
    }
    let land = dataset.terrain.build(&dataset.land, bounds);
    let layers = Layers {
        contours: terrain::contours(&land, contour_interval),
        land,
        lakes: dataset.terrain.build(&dataset.lakes, bounds),
        coastlines: dataset
            .terrain
            .lines(&clip_lines(&dataset.coastlines, bounds)),
        rivers: dataset.terrain.lines(&clip_lines(&dataset.rivers, bounds)),
    };
    *dataset.cache.borrow_mut() = Some(CachedLayers {
        bounds,
        contour_interval,
        layers: layers.clone(),
    });
    layers
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
    Polygon(Vec<Vec<[f64; 2]>>),
    MultiPolygon(Vec<Vec<Vec<[f64; 2]>>>),
}

const COORDINATE_DIMENSIONS: usize = 2;

fn parse_lines(json: &[u8]) -> Result<Vec<Vec<[f64; 2]>>, DataError> {
    let Collection::FeatureCollection { features } = serde_json::from_slice(json)?;
    let mut paths = Vec::new();
    for feature in features {
        match feature.geometry {
            Geometry::LineString(path) => paths.push(path),
            Geometry::MultiLineString(lines) => paths.extend(lines),
            _ => return Err(DataError::InvalidPath),
        }
    }
    if paths
        .iter()
        .any(|path| path.len() < 2 || !valid_coordinates(path))
    {
        return Err(DataError::InvalidPath);
    }

    Ok(paths)
}

fn parse_fills(json: &[u8]) -> Result<Vec<[[f64; 2]; 3]>, DataError> {
    let Collection::FeatureCollection { features } = serde_json::from_slice(json)?;
    let mut triangles = Vec::new();
    for feature in features {
        let polygons = match feature.geometry {
            Geometry::Polygon(rings) => vec![rings],
            Geometry::MultiPolygon(polygons) => polygons,
            _ => return Err(DataError::InvalidPath),
        };
        for rings in polygons {
            if rings.is_empty()
                || rings.iter().any(|ring| {
                    ring.len() < 4 || ring.first() != ring.last() || !valid_coordinates(ring)
                })
            {
                return Err(DataError::InvalidPath);
            }

            // Triangulate once at load time, preserving interior rings as holes.
            let mut points = Vec::new();
            let mut holes = Vec::new();
            for (index, ring) in rings.into_iter().enumerate() {
                if index > 0 {
                    holes.push(points.len());
                }
                points.extend_from_slice(&ring[..ring.len() - 1]);
            }
            let coordinates: Vec<_> = points.iter().flatten().copied().collect();
            let indices = earcutr::earcut(&coordinates, &holes, COORDINATE_DIMENSIONS)
                .map_err(|_| DataError::InvalidPolygon)?;
            if indices.is_empty() {
                return Err(DataError::InvalidPolygon);
            }
            triangles.extend(
                indices
                    .chunks_exact(3)
                    .map(|indices| [points[indices[0]], points[indices[1]], points[indices[2]]]),
            );
        }
    }

    Ok(triangles)
}

fn valid_coordinates(path: &[[f64; 2]]) -> bool {
    path.iter().all(|[longitude, latitude]| {
        (-180.0..=180.0).contains(longitude) && (-90.0..=90.0).contains(latitude)
    })
}

fn clip_lines(paths: &[Vec<[f64; 2]>], bounds: [f64; 4]) -> Vec<[[f64; 2]; 2]> {
    let [west, south, east, north] = bounds;
    paths
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

fn clip_fills(triangles: &[[[f64; 2]; 3]], bounds: [f64; 4]) -> Vec<[[f64; 2]; 3]> {
    let [west, south, east, north] = bounds;
    let mut clipped = Vec::new();
    for triangle in triangles {
        let mut polygon = triangle.to_vec();

        // Clip convex triangles rather than polygon rings so holes remain empty.
        for (axis, boundary, sign) in [
            (0, west, 1.0),
            (0, east, -1.0),
            (1, south, 1.0),
            (1, north, -1.0),
        ] {
            let Some(&last) = polygon.last() else {
                break;
            };
            let mut previous = last;
            let mut output = Vec::new();
            for point in polygon {
                let previous_distance = sign * (previous[axis] - boundary);
                let distance = sign * (point[axis] - boundary);
                if (previous_distance < 0.0) != (distance < 0.0) {
                    let fraction = previous_distance / (previous_distance - distance);
                    let mut intersection = [
                        previous[0] + fraction * (point[0] - previous[0]),
                        previous[1] + fraction * (point[1] - previous[1]),
                    ];
                    intersection[axis] = boundary;
                    output.push(intersection);
                }
                if distance >= 0.0 {
                    output.push(point);
                }
                previous = point;
            }
            polygon = output;
        }

        for pair in polygon.windows(2).skip(1) {
            let [a, b, c] = [polygon[0], pair[0], pair[1]];
            let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
            if area != 0.0 {
                clipped.push([a, b, c]);
            }
        }
    }

    clipped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_crossing_paths() {
        let paths = parse_lines(
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
            clip_lines(&paths, [-1.0, -1.0, 1.0, 1.0]),
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
                parse_lines(json.as_bytes()),
                Err(DataError::InvalidPath)
            ));
        }
    }

    #[test]
    fn reads_bundled_layers() {
        let dataset = Dataset::parse(Sources {
            coastlines: include_bytes!("../data/coastlines.geojson"),
            land: include_bytes!("../data/land.geojson"),
            lakes: include_bytes!("../data/lakes.geojson"),
            rivers: include_bytes!("../data/rivers.geojson"),
            terrain: include_bytes!("../data/terrain.bin"),
        })
        .unwrap();
        let bounds = [-85.0, 35.0, 25.0, 60.0];
        let layers = build(&dataset, bounds, 1000.0);
        for segments in [&layers.coastlines, &layers.rivers, &layers.contours] {
            assert!(!segments.is_empty());
            for &[longitude, latitude, elevation] in segments.iter().flatten() {
                assert!(longitude >= bounds[0] - 1e-9 && longitude <= bounds[2] + 1e-9);
                assert!(latitude >= bounds[1] - 1e-9 && latitude <= bounds[3] + 1e-9);
                assert!((elevation - dataset.elevation([longitude, latitude])).abs() < 1e-6);
            }
        }
        for triangles in [&layers.land, &layers.lakes] {
            assert!(!triangles.is_empty());
            for &[longitude, latitude, elevation] in triangles.iter().flatten() {
                assert!(longitude >= bounds[0] - 1e-9 && longitude <= bounds[2] + 1e-9);
                assert!(latitude >= bounds[1] - 1e-9 && latitude <= bounds[3] + 1e-9);
                assert!((elevation - dataset.elevation([longitude, latitude])).abs() < 1e-6);
            }
        }

        // Changing either cache key must refresh geometry and contour spacing.
        assert!(build(&dataset, bounds, 10_000.0).contours.is_empty());
        assert!(!build(&dataset, bounds, 1000.0).contours.is_empty());
        let cropped = build(&dataset, [6.0, 45.0, 10.0, 48.0], 1000.0);
        assert!(!cropped.land.is_empty());
        for &[longitude, latitude, _] in cropped.land.iter().flatten() {
            assert!((6.0 - 1e-9..=10.0 + 1e-9).contains(&longitude));
            assert!((45.0 - 1e-9..=48.0 + 1e-9).contains(&latitude));
        }
    }

    #[test]
    fn preserves_holes_when_clipping() {
        let triangles = parse_fills(
            br#"{"type":"FeatureCollection","features":[{"geometry":{
                "type":"Polygon","coordinates":[
                    [[-3,-3],[3,-3],[3,3],[-3,3],[-3,-3]],
                    [[-1,-1],[1,-1],[1,1],[-1,1],[-1,-1]]
                ]
            }}]}"#,
        )
        .unwrap();
        assert!((area(&triangles) - 32.0).abs() < 1e-9);

        let clipped = clip_fills(&triangles, [-2.0, -2.0, 2.0, 2.0]);
        assert!((area(&clipped) - 12.0).abs() < 1e-9);
        assert!(clip_fills(&triangles, [-0.5, -0.5, 0.5, 0.5]).is_empty());
        assert!(clip_fills(&triangles, [4.0, 4.0, 5.0, 5.0]).is_empty());
    }

    #[test]
    fn triangulates_concave_islands() {
        let triangles = parse_fills(
            br#"{"type":"FeatureCollection","features":[{"geometry":{
                "type":"MultiPolygon","coordinates":[
                    [[[-3,-3],[3,-3],[3,-1],[-1,-1],[-1,3],[-3,3],[-3,-3]]],
                    [[[10,0],[11,0],[11,1],[10,1],[10,0]]]
                ]
            }}]}"#,
        )
        .unwrap();
        assert!((area(&triangles) - 21.0).abs() < 1e-9);
        assert!(clip_fills(&triangles, [0.0, 0.0, 2.0, 2.0]).is_empty());
        let interior = clip_fills(&triangles, [-2.0, -2.0, -1.5, -1.5]);
        assert!((area(&interior) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn rejects_invalid_rings() {
        for coordinates in [
            "[]",
            "[[[0,0],[1,0],[0,1]]]",
            "[[[0,0],[181,0],[0,1],[0,0]]]",
        ] {
            let json = format!(
                r#"{{"type":"FeatureCollection","features":[{{"geometry":{{"type":"Polygon","coordinates":{coordinates}}}}}]}}"#
            );
            assert!(matches!(
                parse_fills(json.as_bytes()),
                Err(DataError::InvalidPath)
            ));
        }
    }

    fn area(triangles: &[[[f64; 2]; 3]]) -> f64 {
        triangles
            .iter()
            .map(|[a, b, c]| {
                ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
            })
            .sum()
    }
}
