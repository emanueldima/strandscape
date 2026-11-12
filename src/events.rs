use {serde::Deserialize, std::collections::HashSet, thiserror::Error};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Dataset {
    pub(crate) title: String,
    pub(crate) events: Vec<Event>,
    schema_version: String,
    sources: Vec<Source>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Event {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) date: Date,
    pub(crate) place: Option<Place>,
    date_precision: Precision,
    source_ids: Vec<String>,
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Place {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) latitude: f64,
    pub(crate) longitude: f64,
    coordinate_precision: String,
}

#[derive(Clone, Deserialize)]
#[serde(try_from = "String")]
pub(crate) struct Date {
    pub(crate) label: String,
    pub(crate) value: f64,
    precision: Precision,
}

#[derive(Debug, Error)]
pub(crate) enum DataError {
    #[error("Invalid event data: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Invalid event data: {0}")]
    Invalid(String),
}

impl Dataset {
    pub(crate) fn parse(json: &[u8]) -> Result<Self, DataError> {
        let dataset: Self = serde_json::from_slice(json)?;
        if dataset.schema_version != SCHEMA_VERSION || dataset.events.is_empty() {
            return Err(DataError::Invalid("expected schema 1.0 and events".into()));
        }

        let mut source_ids = HashSet::new();
        for source in &dataset.sources {
            if source.id.is_empty()
                || !source_ids.insert(&source.id)
                || source.title.is_empty()
                || source.url.is_empty()
            {
                return Err(DataError::Invalid("invalid or duplicate source".into()));
            }
        }

        let mut event_ids = HashSet::new();
        let mut places = std::collections::HashMap::new();
        for event in &dataset.events {
            if event.id.is_empty() || !event_ids.insert(&event.id) || event.title.is_empty() {
                return Err(DataError::Invalid("invalid or duplicate event".into()));
            }
            if event.date.precision != event.date_precision {
                return Err(DataError::Invalid(format!(
                    "date precision for {}",
                    event.id
                )));
            }
            if event.source_ids.iter().any(|id| !source_ids.contains(id)) {
                return Err(DataError::Invalid(format!(
                    "unknown source for {}",
                    event.id
                )));
            }

            let Some(place) = &event.place else { continue };
            if place.id.is_empty()
                || place.name.is_empty()
                || place.coordinate_precision != CITY_PRECISION
                || !(-90.0..=90.0).contains(&place.latitude)
                || !(-180.0..=180.0).contains(&place.longitude)
            {
                return Err(DataError::Invalid(format!(
                    "invalid place for {}",
                    event.id
                )));
            }
            if places
                .insert(&place.id, place)
                .is_some_and(|old| old != place)
            {
                return Err(DataError::Invalid(format!(
                    "conflicting place {}",
                    place.id
                )));
            }
        }
        Ok(dataset)
    }
}

impl TryFrom<String> for Date {
    type Error = String;

    fn try_from(label: String) -> Result<Self, Self::Error> {
        // Keep the original precision; the numeric value is only a layout anchor.
        let invalid = || format!("invalid Gregorian date: {label}");
        let parts: Vec<_> = label.split('-').collect();
        if !(1..=3).contains(&parts.len())
            || parts[0].len() != 4
            || parts.iter().skip(1).any(|part| part.len() != 2)
            || parts
                .iter()
                .any(|part| !part.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(invalid());
        }
        let year: u32 = parts[0].parse().map_err(|_| invalid())?;
        if year == 0 {
            return Err(invalid());
        }
        let month: usize = parts
            .get(1)
            .unwrap_or(&"01")
            .parse()
            .map_err(|_| invalid())?;
        let day: u32 = parts
            .get(2)
            .unwrap_or(&"01")
            .parse()
            .map_err(|_| invalid())?;
        let leap =
            year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
        let months = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        if !(1..=12).contains(&month) || day == 0 || day > months[month - 1] {
            return Err(invalid());
        }
        let ordinal = months[..month - 1].iter().sum::<u32>() + day - 1;
        let value = year as f64 + ordinal as f64 / if leap { 366.0 } else { 365.0 };
        let precision = match parts.len() {
            1 => Precision::Year,
            2 => Precision::Month,
            _ => Precision::Day,
        };
        Ok(Self {
            label,
            value,
            precision,
        })
    }
}

#[derive(Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Precision {
    Year,
    Month,
    Day,
}

#[derive(Deserialize)]
struct Source {
    id: String,
    title: String,
    url: String,
}

const SCHEMA_VERSION: &str = "1.0";
const CITY_PRECISION: &str = "city";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sample_dates() {
        let data = Dataset::parse(include_bytes!("../data/einstein.json")).unwrap();
        assert_eq!(data.events.len(), 18);
        assert_eq!(data.events[1].date.label, "1880-06");
        assert_eq!(data.events[4].date.label, "1896");
        assert!(data.events.iter().any(|event| event.place.is_none()));
    }

    #[test]
    fn validates_calendar_dates() {
        for date in [
            "1900-02-29",
            "2000-02-30",
            "1905-13",
            "0000",
            "1905-1",
            "1905-",
        ] {
            assert!(Date::try_from(date.to_owned()).is_err(), "{date}");
        }
        for date in ["2000-02-29", "1905", "1905-02", "1905-02-28"] {
            assert!(Date::try_from(date.to_owned()).is_ok(), "{date}");
        }
    }

    #[test]
    fn rejects_inconsistent_data() {
        let original: serde_json::Value =
            serde_json::from_slice(include_bytes!("../data/einstein.json")).unwrap();
        for (field, value) in [
            ("id", serde_json::json!("einstein-munich")),
            ("datePrecision", serde_json::json!("year")),
            ("sourceIds", serde_json::json!(["missing"])),
            (
                "place",
                serde_json::json!({"id":"ulm","name":"Ulm","latitude":91,"longitude":10,"coordinatePrecision":"city"}),
            ),
        ] {
            let mut data = original.clone();
            data["events"][0][field] = value;
            assert!(
                Dataset::parse(&serde_json::to_vec(&data).unwrap()).is_err(),
                "{field}"
            );
        }
    }
}
