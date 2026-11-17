use {serde::Deserialize, std::str::FromStr, thiserror::Error};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Kind {
    Dark,
    Light,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Theme {
    pub(crate) kind: Kind,
    pub(crate) colors: Colors,
    pub(crate) opacity: Opacity,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Colors {
    pub(crate) background: Color,
    page: Color,
    status: Color,
    pub(crate) panel: Color,
    pub(crate) water: Color,
    pub(crate) ink: Color,
    pub(crate) muted: Color,
    pub(crate) accent: Color,
    pub(crate) event: Color,
    pub(crate) selected_event: Color,
    pub(crate) stem: Color,
    pub(crate) coastline: Color,
    pub(crate) land: Color,
    pub(crate) river: Color,
    pub(crate) contour: Color,
}

#[derive(Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct Color {
    pub(crate) linear: [f32; 4],
    css: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Opacity {
    pub(crate) tubes: f32,
    pub(crate) labels: f32,
    pub(crate) places: f32,
}

#[derive(Debug, Error)]
pub(crate) enum ThemeError {
    #[error("Invalid theme: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unknown theme preference: {0}; expected dark or light")]
    Kind(String),
    #[error("Unknown theme name: {0}")]
    Missing(String),
    #[error("Theme {0} does not match the requested dark/light kind")]
    WrongKind(String),
    #[error("Invalid theme color: {0}; expected #RRGGBB or #RRGGBBAA")]
    Color(String),
    #[error("Invalid theme opacity: {0}; expected a value from 0 to 1")]
    Opacity(&'static str),
}

pub(super) fn load(name: &str, kind: Kind) -> Result<Theme, ThemeError> {
    let (_, json) = THEMES
        .iter()
        .find(|(filename, _)| *filename == name)
        .ok_or_else(|| ThemeError::Missing(name.into()))?;
    let theme: Theme = serde_json::from_str(json)?;
    if theme.kind != kind {
        return Err(ThemeError::WrongKind(name.into()));
    }
    for (name, value) in [
        ("tubes", theme.opacity.tubes),
        ("labels", theme.opacity.labels),
        ("places", theme.opacity.places),
    ] {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(ThemeError::Opacity(name));
        }
    }
    Ok(theme)
}

impl Theme {
    pub(crate) fn page_css(&self) -> String {
        let scheme = match self.kind {
            Kind::Dark => "dark",
            Kind::Light => "light",
        };
        format!(
            "color-scheme:{scheme};--page-background:{};--status-color:{}",
            self.colors.page.css, self.colors.status.css,
        )
    }
}

impl FromStr for Kind {
    type Err = ThemeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "dark" => Ok(Self::Dark),
            "light" => Ok(Self::Light),
            _ => Err(ThemeError::Kind(value.into())),
        }
    }
}

impl TryFrom<String> for Color {
    type Error = ThemeError;

    fn try_from(css: String) -> Result<Self, Self::Error> {
        let Some(hex) = css.strip_prefix('#') else {
            return Err(ThemeError::Color(css));
        };
        if ![RGB_HEX_LENGTH, RGBA_HEX_LENGTH].contains(&hex.len())
            || !hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(ThemeError::Color(css));
        }

        // CSS hex values are sRGB; rendering needs linear RGB, with alpha unchanged.
        let mut linear = [1.0; 4];
        for (index, channel) in linear.iter_mut().enumerate().take(hex.len() / 2) {
            let value = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16)
                .map_err(|_| ThemeError::Color(css.clone()))?;
            let value = f32::from(value) / f32::from(u8::MAX);
            *channel = if index == 3 {
                value
            } else if value <= SRGB_THRESHOLD {
                value / SRGB_LINEAR_SCALE
            } else {
                ((value + SRGB_OFFSET) / SRGB_SCALE).powf(SRGB_EXPONENT)
            };
        }
        Ok(Self { linear, css })
    }
}

const RGB_HEX_LENGTH: usize = 6;
const RGBA_HEX_LENGTH: usize = 8;
const SRGB_THRESHOLD: f32 = 0.04045;
const SRGB_LINEAR_SCALE: f32 = 12.92;
const SRGB_OFFSET: f32 = 0.055;
const SRGB_SCALE: f32 = 1.055;
const SRGB_EXPONENT: f32 = 2.4;

const THEMES: &[(&str, &str)] = &[
    ("dark", include_str!("../../config/dark.json")),
    ("light", include_str!("../../config/light.json")),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_hex_to_linear() {
        let color = Color::try_from("#80808080".to_owned()).unwrap();
        assert!((color.linear[0] - 0.21586).abs() < 0.00001);
        assert_eq!(color.linear[0], color.linear[1]);
        assert!((color.linear[3] - 128.0 / 255.0).abs() < 0.00001);
        assert_eq!(
            Color::try_from("#ffffff".to_owned()).unwrap().linear,
            [1.0; 4]
        );
        assert_eq!(
            Color::try_from("#000000".to_owned()).unwrap().linear,
            [0.0, 0.0, 0.0, 1.0]
        );
        for invalid in ["red", "#fff", "#gg0000", "#１２３"] {
            assert!(Color::try_from(invalid.to_owned()).is_err());
        }
    }
}
