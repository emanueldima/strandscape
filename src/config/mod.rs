mod theme;

use {serde::Deserialize, thiserror::Error};

pub(crate) use theme::Kind;

pub(crate) struct Config {
    pub(crate) layout: Layout,
    pub(crate) theme: theme::Theme,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Layout {
    pub(crate) map: Map,
    pub(crate) events: Events,
    pub(crate) text: Text,
    pub(crate) timeline: Timeline,
    pub(crate) line_width: f32,
    pub(crate) selected_scale: f32,
    pub(crate) field_of_view: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Map {
    pub(crate) width: f64,
    pub(crate) depth: f64,
    pub(crate) margin: f64,
    pub(crate) min_margin: f64,
    pub(crate) marker_size: f32,
    pub(crate) coastline_width: f32,
    pub(crate) river_width: f32,
    pub(crate) terrain: Terrain,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Terrain {
    pub(crate) elevation_scale: f32,
    pub(crate) shading: f32,
    pub(crate) contour_interval: f64,
    pub(crate) contour_width: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Events {
    pub(crate) height_base: f32,
    pub(crate) height_span: f32,
    pub(crate) radius: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Text {
    pub(crate) size: f32,
    pub(crate) scale: f32,
    pub(crate) label_gap: f32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Timeline {
    pub(crate) margin: f32,
    pub(crate) line_height: f32,
    pub(crate) dot_radius: f32,
    pub(crate) min_gap: f32,
    pub(crate) height: f32,
    pub(crate) label_size: f32,
    pub(crate) label_limit: usize,
    pub(crate) label_room: f32,
    pub(crate) line_width: f32,
    pub(crate) label_angle: f32,
}

#[derive(Debug, Error)]
pub(crate) enum ConfigError {
    #[error("Invalid configuration: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Theme(#[from] theme::ThemeError),
    #[error("Invalid configuration: {0}")]
    Layout(&'static str),
}

pub(crate) fn load(kind: Kind) -> Result<Config, ConfigError> {
    let json = include_str!("../../config/config.json");
    parse(json, kind)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    default_themes: Defaults,
    layout: Layout,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Defaults {
    dark: String,
    light: String,
}

fn parse(json: &str, kind: Kind) -> Result<Config, ConfigError> {
    let file: File = serde_json::from_str(json)?;
    let layout = &file.layout;
    // Reject values that would make projection, spacing, or hit areas invalid.
    for (name, value) in [
        ("map.width", layout.map.width),
        ("map.depth", layout.map.depth),
        ("map.min_margin", layout.map.min_margin),
        ("map.marker_size", f64::from(layout.map.marker_size)),
        ("map.coastline_width", f64::from(layout.map.coastline_width)),
        ("map.river_width", f64::from(layout.map.river_width)),
        (
            "map.terrain.elevation_scale",
            f64::from(layout.map.terrain.elevation_scale),
        ),
        (
            "map.terrain.contour_interval",
            layout.map.terrain.contour_interval,
        ),
        (
            "map.terrain.contour_width",
            f64::from(layout.map.terrain.contour_width),
        ),
        ("events.radius", f64::from(layout.events.radius)),
        ("text.size", f64::from(layout.text.size)),
        ("text.scale", f64::from(layout.text.scale)),
        (
            "timeline.line_height",
            f64::from(layout.timeline.line_height),
        ),
        ("timeline.dot_radius", f64::from(layout.timeline.dot_radius)),
        ("timeline.min_gap", f64::from(layout.timeline.min_gap)),
        ("timeline.height", f64::from(layout.timeline.height)),
        ("timeline.label_size", f64::from(layout.timeline.label_size)),
        ("timeline.line_width", f64::from(layout.timeline.line_width)),
        ("line_width", f64::from(layout.line_width)),
        ("selected_scale", f64::from(layout.selected_scale)),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(ConfigError::Layout(name));
        }
    }
    for (name, value) in [
        ("map.margin", layout.map.margin),
        ("events.height_base", f64::from(layout.events.height_base)),
        ("events.height_span", f64::from(layout.events.height_span)),
        ("text.label_gap", f64::from(layout.text.label_gap)),
        ("timeline.margin", f64::from(layout.timeline.margin)),
        ("timeline.label_room", f64::from(layout.timeline.label_room)),
    ] {
        if !value.is_finite() || value < 0.0 {
            return Err(ConfigError::Layout(name));
        }
    }
    if layout.timeline.label_limit == 0 {
        return Err(ConfigError::Layout("timeline.label_limit"));
    }
    if !layout.map.terrain.shading.is_finite() || !(0.0..=1.0).contains(&layout.map.terrain.shading)
    {
        return Err(ConfigError::Layout("map.terrain.shading"));
    }
    if !layout.field_of_view.is_finite()
        || layout.field_of_view <= 0.0
        || layout.field_of_view >= 180.0
    {
        return Err(ConfigError::Layout("field_of_view"));
    }
    if !layout.timeline.label_angle.is_finite()
        || !(0.0..=90.0).contains(&layout.timeline.label_angle)
    {
        return Err(ConfigError::Layout("timeline.label_angle"));
    }

    let name = match kind {
        Kind::Dark => &file.default_themes.dark,
        Kind::Light => &file.default_themes.light,
    };
    Ok(Config {
        theme: theme::load(name, kind)?,
        layout: file.layout,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_both_default_themes() {
        for kind in [Kind::Dark, Kind::Light] {
            let settings = load(kind).unwrap();
            assert_eq!(settings.theme.kind, kind);
            assert!(
                settings
                    .theme
                    .colors
                    .background
                    .linear
                    .iter()
                    .all(|channel| (0.0..=1.0).contains(channel))
            );
            assert!(settings.theme.page_css().contains(match kind {
                Kind::Dark => "color-scheme:dark",
                Kind::Light => "color-scheme:light",
            }));
        }
    }

    #[test]
    fn validates_layout_and_names() {
        let json = include_str!("../../config/config.json");
        let original: serde_json::Value = serde_json::from_str(json).unwrap();
        for (field, value) in [
            ("line_width", serde_json::json!(0)),
            ("field_of_view", serde_json::json!(180)),
        ] {
            let mut file = original.clone();
            file["layout"][field] = value;
            assert!(matches!(
                parse(&file.to_string(), Kind::Dark),
                Err(ConfigError::Layout(_))
            ));
        }
        for (field, value) in [
            ("elevation_scale", serde_json::json!(0)),
            ("contour_interval", serde_json::json!(0)),
            ("contour_width", serde_json::json!(-1)),
            ("shading", serde_json::json!(1.1)),
        ] {
            let mut file = original.clone();
            file["layout"]["map"]["terrain"][field] = value;
            assert!(matches!(
                parse(&file.to_string(), Kind::Dark),
                Err(ConfigError::Layout(_))
            ));
        }
        let mut file = original.clone();
        file["default_themes"]["dark"] = serde_json::json!("missing");
        assert!(matches!(
            parse(&file.to_string(), Kind::Dark),
            Err(ConfigError::Theme(theme::ThemeError::Missing(_)))
        ));
        file["default_themes"]["dark"] = serde_json::json!("light");
        assert!(matches!(
            parse(&file.to_string(), Kind::Dark),
            Err(ConfigError::Theme(theme::ThemeError::WrongKind(_)))
        ));
    }
}
