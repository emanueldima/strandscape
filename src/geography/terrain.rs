use super::DataError;

pub(super) struct Dataset {
    columns: usize,
    rows: usize,
    west: f64,
    south: f64,
    step: f64,
    elevations: Vec<i16>,
}

impl Dataset {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, DataError> {
        if bytes.len() < HEADER_LENGTH || &bytes[..8] != b"STRDEM01" {
            return Err(DataError::InvalidTerrain);
        }
        let columns = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let rows = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let west = f64::from_le_bytes(bytes[16..24].try_into().unwrap());
        let south = f64::from_le_bytes(bytes[24..32].try_into().unwrap());
        let step = f64::from_le_bytes(bytes[32..40].try_into().unwrap());
        let count = columns.checked_mul(rows);
        if columns < 2
            || rows < 2
            || count
                .and_then(|n| n.checked_mul(2))
                .and_then(|n| n.checked_add(HEADER_LENGTH))
                != Some(bytes.len())
            || !west.is_finite()
            || !south.is_finite()
            || !step.is_finite()
            || step <= 0.0
            || west < -180.0
            || south < -90.0
            || west + (columns - 1) as f64 * step > 180.0 + COORDINATE_EPSILON
            || south + (rows - 1) as f64 * step > 90.0 + COORDINATE_EPSILON
        {
            return Err(DataError::InvalidTerrain);
        }
        let elevations: Vec<_> = bytes[HEADER_LENGTH..]
            .chunks_exact(2)
            .map(|sample| i16::from_le_bytes(sample.try_into().unwrap()))
            .collect();
        if elevations.contains(&i16::MIN) {
            return Err(DataError::InvalidTerrain);
        }
        Ok(Self {
            columns,
            rows,
            west,
            south,
            step,
            elevations,
        })
    }

    pub(super) fn elevation(&self, [longitude, latitude]: [f64; 2]) -> f64 {
        let x = ((longitude - self.west) / self.step).clamp(0.0, (self.columns - 1) as f64);
        let y = ((latitude - self.south) / self.step).clamp(0.0, (self.rows - 1) as f64);
        let column = (x.floor() as usize).min(self.columns - 2);
        let row = (y.floor() as usize).min(self.rows - 2);
        let [dx, dy] = [x - column as f64, y - row as f64];
        let sw = self.point(column, row)[2];
        let ne = self.point(column + 1, row + 1)[2];

        // Use the mesh diagonal, rather than bilinear interpolation, for exact draping.
        if dx >= dy {
            sw * (1.0 - dx) + self.point(column + 1, row)[2] * (dx - dy) + ne * dy
        } else {
            sw * (1.0 - dy) + self.point(column, row + 1)[2] * (dy - dx) + ne * dx
        }
    }

    pub(super) fn build(&self, fills: &[[[f64; 2]; 3]], bounds: [f64; 4]) -> Vec<[[f64; 3]; 3]> {
        let mut output = Vec::new();
        for boundary in super::clip_fills(fills, bounds) {
            let sign = area(boundary).signum();
            let min_x = boundary.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
            let max_x = boundary
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = boundary.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
            let max_y = boundary
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max);
            let columns = [min_x, max_x].map(|x| {
                (((x - self.west) / self.step).floor().max(0.0) as usize).min(self.columns - 2)
            });
            let rows = [min_y, max_y].map(|y| {
                (((y - self.south) / self.step).floor().max(0.0) as usize).min(self.rows - 2)
            });

            // Intersect grid faces with land polygons so relief preserves coasts and holes.
            for row in rows[0]..=rows[1] {
                for column in columns[0]..=columns[1] {
                    let sw = self.point(column, row);
                    let se = self.point(column + 1, row);
                    let ne = self.point(column + 1, row + 1);
                    let nw = self.point(column, row + 1);
                    for triangle in [[sw, se, ne], [sw, ne, nw]] {
                        let mut polygon = triangle.to_vec();
                        for edge in 0..3 {
                            let a = boundary[edge];
                            let b = boundary[(edge + 1) % 3];
                            polygon = clip(&polygon, |p| {
                                sign * ((b[0] - a[0]) * (p[1] - a[1])
                                    - (b[1] - a[1]) * (p[0] - a[0]))
                            });
                        }
                        for pair in polygon.windows(2).skip(1) {
                            let face = [polygon[0], pair[0], pair[1]];
                            if area(face.map(|p| [p[0], p[1]])).abs() > COORDINATE_EPSILON {
                                output.push(face);
                            }
                        }
                    }
                }
            }
        }
        output
    }

    pub(super) fn lines(&self, segments: &[[[f64; 2]; 2]]) -> Vec<[[f64; 3]; 2]> {
        let mut output = Vec::new();
        for &[a, b] in segments {
            let mut cuts = vec![0.0, 1.0];
            let [ax, ay] = [
                (a[0] - self.west) / self.step,
                (a[1] - self.south) / self.step,
            ];
            let [bx, by] = [
                (b[0] - self.west) / self.step,
                (b[1] - self.south) / self.step,
            ];

            // Split at cell edges and diagonals so rivers cannot cut through terrain faces.
            for (start, end) in [(ax, bx), (ay, by), (ax - ay, bx - by)] {
                if start == end {
                    continue;
                }
                for crossing in start.min(end).floor() as i32 + 1..end.max(start).ceil() as i32 {
                    cuts.push((f64::from(crossing) - start) / (end - start));
                }
            }
            cuts.sort_by(f64::total_cmp);
            cuts.dedup_by(|a, b| (*a - *b).abs() < COORDINATE_EPSILON);
            for pair in cuts.windows(2) {
                output.push([pair[0], pair[1]].map(|t| {
                    let point = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                    [point[0], point[1], self.elevation(point)]
                }));
            }
        }
        output
    }

    fn point(&self, column: usize, row: usize) -> [f64; 3] {
        [
            self.west + column as f64 * self.step,
            self.south + row as f64 * self.step,
            f64::from(self.elevations[row * self.columns + column].max(0)),
        ]
    }
}

pub(super) fn contours(triangles: &[[[f64; 3]; 3]], interval: f64) -> Vec<[[f64; 3]; 2]> {
    let mut segments = Vec::new();
    for &triangle in triangles {
        let minimum = triangle.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
        let maximum = triangle.iter().map(|p| p[2]).fold(0.0, f64::max);
        let mut level = ((minimum / interval).floor() + 1.0) * interval;
        while level <= maximum {
            let mut points = Vec::new();
            for edge in 0..3 {
                let a = triangle[edge];
                let b = triangle[(edge + 1) % 3];
                if (a[2] < level) == (b[2] < level) {
                    continue;
                }
                let t = (level - a[2]) / (b[2] - a[2]);
                points.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, level]);
            }
            if points.len() == 2 && points[0] != points[1] {
                segments.push([points[0], points[1]]);
            }
            level += interval;
        }
    }
    segments
}

const HEADER_LENGTH: usize = 40;
const COORDINATE_EPSILON: f64 = 1e-12;

fn area([a, b, c]: [[f64; 2]; 3]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn clip(polygon: &[[f64; 3]], distance: impl Fn([f64; 3]) -> f64) -> Vec<[f64; 3]> {
    let Some(&last) = polygon.last() else {
        return Vec::new();
    };
    let mut previous = last;
    let mut output = Vec::new();
    for &point in polygon {
        let before = distance(previous);
        let after = distance(point);
        if (before < 0.0) != (after < 0.0) {
            let fraction = before / (before - after);
            output.push(std::array::from_fn(|axis| {
                previous[axis] + fraction * (point[axis] - previous[axis])
            }));
        }
        if after >= 0.0 {
            output.push(point);
        }
        previous = point;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_grid_and_rejects_bad_data() {
        let original = grid_bytes(&[0, 1000, 2000, 3000]);
        let dataset = Dataset::parse(&original).unwrap();
        assert_eq!(dataset.elevation([0.0, 0.0]), 0.0);
        assert_eq!(dataset.elevation([1.0, 1.0]), 3000.0);
        assert_eq!(dataset.elevation([0.5, 0.5]), 1500.0);
        for end in [0, HEADER_LENGTH - 1, original.len() - 1] {
            assert!(Dataset::parse(&original[..end]).is_err());
        }
        for (range, replacement) in [
            (0..8, b"INVALID!".to_vec()),
            (8..12, u32::MAX.to_le_bytes().to_vec()),
            (12..16, 1_u32.to_le_bytes().to_vec()),
            (32..40, f64::NAN.to_le_bytes().to_vec()),
            (32..40, 0_f64.to_le_bytes().to_vec()),
            (40..42, i16::MIN.to_le_bytes().to_vec()),
        ] {
            let mut bytes = original.clone();
            bytes[range].copy_from_slice(&replacement);
            assert!(Dataset::parse(&bytes).is_err());
        }
    }

    #[test]
    fn mesh_and_lines_share_heights() {
        let dataset = Dataset::parse(&grid_bytes(&[-500, 1000, 2000, 0])).unwrap();
        assert_eq!(dataset.elevation([0.75, 0.25]), 500.0);
        assert_eq!(dataset.elevation([0.25, 0.75]), 1000.0);

        let fills = [
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        ];
        let faces = dataset.build(&fills, [0.2, 0.1, 0.8, 0.9]);
        let total_area: f64 = faces
            .iter()
            .map(|face| area(face.map(|p| [p[0], p[1]])).abs() * 0.5)
            .sum();
        assert!((total_area - 0.48).abs() < 1e-9);
        for &[x, y, height] in faces.iter().flatten() {
            assert!((0.2 - 1e-9..=0.8 + 1e-9).contains(&x));
            assert!((0.1 - 1e-9..=0.9 + 1e-9).contains(&y));
            assert!((height - dataset.elevation([x, y])).abs() < 1e-9);
        }

        let lines = dataset.lines(&[[[0.0, 0.5], [1.0, 0.5]]]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0][1], lines[1][0]);
        for [a, b] in lines {
            let midpoint = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            assert!(((a[2] + b[2]) * 0.5 - dataset.elevation(midpoint)).abs() < 1e-9);
        }
    }

    #[test]
    fn contours_interpolate_and_connect() {
        let dataset = Dataset::parse(&grid_bytes(&[0, 2000, 0, 2000])).unwrap();
        let faces = dataset.build(
            &[
                [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
                [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            ],
            [0.0, 0.0, 1.0, 1.0],
        );
        let lines = contours(&faces, 1000.0);
        let middle: Vec<_> = lines.iter().filter(|line| line[0][2] == 1000.0).collect();
        assert_eq!(middle.len(), 2);
        let shared = [0.5, 0.5, 1000.0];
        assert!(middle.iter().all(|line| line.contains(&shared)));
        for &[a, b] in &lines {
            assert_ne!(a, b);
            assert_eq!(a[2], b[2]);
            assert_eq!(a[2] % 1000.0, 0.0);
            for [x, y, height] in [a, b] {
                assert!((height - dataset.elevation([x, y])).abs() < 1e-9);
            }
        }
        assert!(
            contours(
                &[[[0.0, 0.0, 1000.0], [1.0, 0.0, 1000.0], [0.0, 1.0, 1000.0]]],
                1000.0
            )
            .is_empty()
        );
    }

    #[test]
    fn preserves_terrain_holes() {
        let dataset = Dataset::parse(&grid_bytes(&[0, 1000, 2000, 3000])).unwrap();
        let holed = super::super::parse_fills(br#"{"type":"FeatureCollection","features":[{"geometry":{"type":"Polygon","coordinates":[[[0,0],[1,0],[1,1],[0,1],[0,0]],[[0.25,0.25],[0.25,0.75],[0.75,0.75],[0.75,0.25],[0.25,0.25]]]}}]}"#).unwrap();
        let faces = dataset.build(&holed, [0.0, 0.0, 1.0, 1.0]);
        let total_area: f64 = faces
            .iter()
            .map(|face| area(face.map(|p| [p[0], p[1]])).abs() * 0.5)
            .sum();
        assert!((total_area - 0.75).abs() < 1e-9);
    }

    fn grid_bytes(samples: &[i16]) -> Vec<u8> {
        let mut bytes = b"STRDEM01".to_vec();
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        for value in [0_f64, 0.0, 1.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
        bytes
    }
}
