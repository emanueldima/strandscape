use {
    crate::{
        composer::{self, Target as Subject},
        config,
        events::{self, Dataset},
        geography,
        render::{RenderConfig, RenderError, Renderer},
        text::{FontConfig, FontError, FontId, FontSystem},
    },
    glam::{Vec2, Vec3},
    thiserror::Error,
};

pub(crate) use crate::render::{Target, Viewport};

pub(crate) struct App {
    settings: config::Config,
    dataset: Dataset,
    geography: geography::Dataset,
    output: composer::Output,
    renderer: Renderer,
    viewport: Vec2,
    selected: Option<String>,
}

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error(transparent)]
    Events(#[from] events::DataError),
    #[error(transparent)]
    Geography(#[from] geography::DataError),
    #[error(transparent)]
    Font(#[from] FontError),
    #[error(transparent)]
    Compose(#[from] composer::ComposeError),
    #[error(transparent)]
    Render(#[from] RenderError),
}

impl App {
    pub(crate) async fn create(
        target: Target,
        font_bytes: Vec<u8>,
        viewport: Viewport,
        preference: &str,
    ) -> Result<(App, String), AppError> {
        let settings = config::load(preference.parse().map_err(config::ConfigError::from)?)?;
        let page_css = settings.theme.page_css();
        let clear_color = settings.theme.colors.background.linear.map(f64::from);
        let fonts = FontSystem::new(FontConfig {
            fonts: vec![(FONT, font_bytes)],
        })?;

        let dataset = Dataset::parse(include_bytes!("../data/einstein.json"))?;
        let geography = geography::Dataset::parse(geography::Sources {
            coastlines: include_bytes!("../data/coastlines.geojson"),
            land: include_bytes!("../data/land.geojson"),
            lakes: include_bytes!("../data/lakes.geojson"),
            rivers: include_bytes!("../data/rivers.geojson"),
            terrain: include_bytes!("../data/terrain.bin"),
        })?;
        let size = (Vec2::new(viewport.width as f32, viewport.height as f32)
            / viewport.scale_factor as f32)
            .max(Vec2::ONE);
        let output = composer::build(
            &dataset,
            &composer::Config {
                settings: &settings,
                selected: None,
                geography: &geography,
                font: FONT,
                viewport: size,
                timeline_offset: 0.0,
            },
        )?;
        let renderer = Renderer::new(
            target,
            fonts,
            RenderConfig {
                viewport,
                clear_color,
            },
        )
        .await?;
        Ok((
            Self {
                settings,
                dataset,
                geography,
                output,
                renderer,
                viewport: size,
                selected: None,
            },
            page_css,
        ))
    }

    pub(crate) fn set_theme(&mut self, preference: &str) -> Result<String, AppError> {
        self.settings = config::load(preference.parse().map_err(config::ConfigError::from)?)?;
        self.rebuild(self.output.timeline.offset)?;
        self.renderer
            .set_background(self.settings.theme.colors.background.linear.map(f64::from));
        Ok(self.settings.theme.page_css())
    }

    pub(crate) fn draw(&mut self) -> Result<(), AppError> {
        self.renderer.draw(&mut self.output.scene)?;
        Ok(())
    }

    pub(crate) fn resize(&mut self, viewport: Viewport) -> Result<(), AppError> {
        self.viewport = (Vec2::new(viewport.width as f32, viewport.height as f32)
            / viewport.scale_factor as f32)
            .max(Vec2::ONE);
        self.renderer.resize(viewport);
        self.rebuild(self.output.timeline.offset)
    }

    pub(crate) fn orbit(&mut self, x: f32, y: f32) {
        self.output.scene.camera.orbit(Vec2::new(x, y));
    }

    pub(crate) fn pan(&mut self, x: f32, y: f32) {
        self.output.scene.camera.pan(Vec2::new(x, y));
    }

    pub(crate) fn focus(&mut self, x: f32, y: f32) -> Result<Option<String>, AppError> {
        let Some(hit) = self
            .output
            .scene
            .pick(Vec2::new(x, y), self.viewport, |index| {
                self.output.bindings.contains_key(&index)
            })
        else {
            return Ok(None);
        };
        match self.output.bindings.get(&hit.item) {
            Some(Subject::Entry(id)) => {
                // Event selection updates emphasis while preserving the user's map view.
                let title = self
                    .dataset
                    .events
                    .iter()
                    .find(|event| &event.id == id)
                    .map(|event| event.title.clone());
                self.selected = Some(id.clone());
                self.rebuild(self.output.timeline.offset)?;
                Ok(title)
            }
            Some(Subject::Place(id, point)) => {
                self.output.scene.camera.focus(*point);
                Ok(self
                    .dataset
                    .events
                    .iter()
                    .filter_map(|event| event.place.as_ref())
                    .find(|place| &place.id == id)
                    .map(|place| place.name.clone()))
            }
            None => Ok(None),
        }
    }

    pub(crate) fn zoom(&mut self, scroll: f32) {
        self.output.scene.camera.zoom(scroll);
    }

    pub(crate) fn scroll_timeline(&mut self, x: f32, y: f32, delta: f32) -> Result<bool, AppError> {
        if !Vec3::new(x, y, delta).is_finite()
            || !(-1.0..=1.0).contains(&x)
            || !(-1.0..=1.0).contains(&y)
            || (1.0 - y) * self.viewport.y * 0.5 < self.output.timeline.top
        {
            return Ok(false);
        }
        let offset =
            (self.output.timeline.offset + delta).clamp(0.0, self.output.timeline.max_offset);
        if offset != self.output.timeline.offset {
            self.rebuild(offset)?;
        }
        Ok(true)
    }
}

impl App {
    fn rebuild(&mut self, timeline_offset: f32) -> Result<(), AppError> {
        let output = composer::build(
            &self.dataset,
            &composer::Config {
                settings: &self.settings,
                selected: self.selected.as_deref(),
                geography: &self.geography,
                font: FONT,
                viewport: self.viewport,
                timeline_offset,
            },
        )?;
        // Resize, selection, and timeline scrolling preserve the user's camera.
        self.output.scene.items = output.scene.items;
        self.output.bindings = output.bindings;
        self.output.timeline = output.timeline;
        Ok(())
    }
}

const FONT: FontId = FontId(0);
