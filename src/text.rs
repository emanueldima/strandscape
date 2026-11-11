use {
    cosmic_text::{
        Attrs, Buffer, CacheKey, Family, Metrics, Shaping, SwashCache, SwashContent, Wrap, fontdb,
    },
    std::{
        collections::{HashMap, hash_map::Entry},
        sync::Arc,
    },
    thiserror::Error,
};

pub(crate) struct FontSystem {
    inner: cosmic_text::FontSystem,
    rasterizer: SwashCache,
    families: HashMap<FontId, String>,
    layouts: HashMap<LayoutKey, TextLayout>,
    images: HashMap<GlyphKey, GlyphImage>,
}

pub(crate) struct FontConfig {
    pub(crate) fonts: Vec<(FontId, FontBytes)>,
}

pub(crate) type FontBytes = Vec<u8>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FontId(pub(crate) u32);

#[derive(Clone)]
pub(crate) struct TextSpec {
    pub(crate) content: String,
    pub(crate) font: FontId,
    pub(crate) size: f32,
    pub(crate) max_width: Option<f32>,
}

#[derive(Clone)]
pub(crate) struct TextLayout {
    pub(crate) glyphs: Arc<[PositionedGlyph]>,
    pub(crate) bounds: [f32; 2],
}

pub(crate) struct PositionedGlyph {
    pub(crate) key: GlyphKey,
    pub(crate) position: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey(CacheKey);

#[derive(Clone)]
pub(crate) struct GlyphImage {
    pub(crate) pixels: Arc<[u8]>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) offset: [i32; 2],
}

#[derive(Debug, Error)]
pub(crate) enum FontError {
    #[error("Supply at least one font")]
    Empty,
    #[error("Invalid font data for {0:?}")]
    Invalid(FontId),
    #[error("Duplicate font ID {0:?}")]
    Duplicate(FontId),
    #[error("Unknown font {0:?}")]
    Unknown(FontId),
    #[error("Font size and wrapping width must be finite and positive")]
    InvalidSize,
    #[error("Unable to rasterize glyph")]
    Rasterization,
}

impl FontSystem {
    pub(crate) fn new(config: FontConfig) -> Result<Self, FontError> {
        if config.fonts.is_empty() {
            return Err(FontError::Empty);
        }

        // Build only from supplied bytes; never discover installed fonts.
        let mut database = fontdb::Database::new();
        let mut families = HashMap::new();
        for (id, bytes) in config.fonts {
            if families.contains_key(&id) {
                return Err(FontError::Duplicate(id));
            }

            let faces = database.load_font_source(fontdb::Source::Binary(Arc::new(bytes)));
            let family = faces
                .first()
                .and_then(|face| database.face(*face))
                .and_then(|face| face.families.first())
                .map(|(name, _)| name.clone())
                .ok_or(FontError::Invalid(id))?;
            families.insert(id, family);
        }

        Ok(Self {
            inner: cosmic_text::FontSystem::new_with_locale_and_db("en-US".into(), database),
            rasterizer: SwashCache::new(),
            families,
            layouts: HashMap::new(),
            images: HashMap::new(),
        })
    }

    pub(crate) fn layout(&mut self, spec: &TextSpec) -> Result<TextLayout, FontError> {
        if !spec.size.is_finite()
            || spec.size <= 0.0
            || spec
                .max_width
                .is_some_and(|width| !width.is_finite() || width <= 0.0)
        {
            return Err(FontError::InvalidSize);
        }

        let family = self
            .families
            .get(&spec.font)
            .ok_or(FontError::Unknown(spec.font))?;
        let key = LayoutKey {
            content: spec.content.clone(),
            font: spec.font,
            size: spec.size.to_bits(),
            max_width: spec.max_width.map(f32::to_bits),
        };
        if let Some(layout) = self.layouts.get(&key) {
            return Ok(layout.clone());
        }

        let mut buffer = Buffer::new(
            &mut self.inner,
            Metrics::new(spec.size, spec.size * LINE_HEIGHT),
        );
        buffer.set_size(spec.max_width, None);
        buffer.set_wrap(Wrap::WordOrGlyph);
        buffer.set_text(
            &spec.content,
            &Attrs::new().family(Family::Name(family)),
            Shaping::Advanced,
            None,
        );
        buffer.shape_until_scroll(&mut self.inner, false);

        let mut glyphs = Vec::new();
        let mut bounds = [0.0_f32; 2];
        for run in buffer.layout_runs() {
            bounds[0] = bounds[0].max(run.line_w);
            bounds[1] = bounds[1].max(run.line_top + run.line_height);
            for glyph in run.glyphs {
                let physical = glyph.physical((0.0, 0.0), 1.0);
                glyphs.push(PositionedGlyph {
                    key: GlyphKey(physical.cache_key),
                    position: [physical.x as f32, physical.y as f32 + run.line_y],
                });
            }
        }

        let layout = TextLayout {
            glyphs: glyphs.into(),
            bounds,
        };
        self.layouts.insert(key, layout.clone());
        Ok(layout)
    }

    pub(crate) fn rasterize(&mut self, key: GlyphKey) -> Result<GlyphImage, FontError> {
        if let Entry::Occupied(image) = self.images.entry(key) {
            return Ok(image.get().clone());
        }

        let image = self
            .rasterizer
            .get_image_uncached(&mut self.inner, key.0)
            .ok_or(FontError::Rasterization)?;
        let pixels = coverage(image.content, image.data);
        let image = GlyphImage {
            pixels: pixels.into(),
            width: image.placement.width,
            height: image.placement.height,
            offset: [image.placement.left, -image.placement.top],
        };
        self.images.insert(key, image.clone());
        Ok(image)
    }
}

const LINE_HEIGHT: f32 = 1.35;

#[derive(PartialEq, Eq, Hash)]
struct LayoutKey {
    content: String,
    font: FontId,
    size: u32,
    max_width: Option<u32>,
}

fn coverage(content: SwashContent, data: Vec<u8>) -> Vec<u8> {
    // The atlas stores one coverage value per pixel, including color glyph alpha.
    match content {
        SwashContent::Mask => data,
        SwashContent::Color => data.chunks_exact(4).map(|pixel| pixel[3]).collect(),
        SwashContent::SubpixelMask => data
            .chunks_exact(4)
            .map(|pixel| *pixel[..3].iter().max().unwrap())
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_rgba_to_coverage() {
        assert_eq!(
            coverage(SwashContent::Color, vec![10, 20, 30, 80, 40, 50, 60, 90]),
            vec![80, 90]
        );
        assert_eq!(
            coverage(
                SwashContent::SubpixelMask,
                vec![10, 20, 30, 255, 40, 50, 60, 255]
            ),
            vec![30, 60]
        );
    }

    #[test]
    fn rejects_missing_or_invalid_fonts() {
        assert!(matches!(
            FontSystem::new(FontConfig { fonts: vec![] }),
            Err(FontError::Empty)
        ));
        assert!(matches!(
            FontSystem::new(FontConfig {
                fonts: vec![(FontId(0), b"not a font".to_vec())],
            }),
            Err(FontError::Invalid(FontId(0)))
        ));
    }

    #[test]
    #[ignore = "requires STRANDSCAPE_TEST_FONT pointing to a downloaded TTF/OTF"]
    fn shapes_wraps_and_rasterizes() {
        let bytes = std::fs::read(
            std::env::var("STRANDSCAPE_TEST_FONT").expect("set STRANDSCAPE_TEST_FONT"),
        )
        .unwrap();
        let mut fonts = FontSystem::new(FontConfig {
            fonts: vec![(FontId(0), bytes)],
        })
        .unwrap();
        let mut spec = TextSpec {
            content: "People, ideas, time and place.".into(),
            font: FontId(0),
            size: 32.0,
            max_width: None,
        };
        let wide = fonts.layout(&spec).unwrap();
        assert!(!wide.glyphs.is_empty());
        assert!(wide.bounds[0] > 100.0);

        spec.max_width = Some(160.0);
        let wrapped = fonts.layout(&spec).unwrap();
        assert!(wrapped.bounds[0] <= 160.0);
        assert!(wrapped.bounds[1] > wide.bounds[1]);
        let cached = fonts.layout(&spec).unwrap();
        assert!(Arc::ptr_eq(&wrapped.glyphs, &cached.glyphs));

        for glyph in wrapped.glyphs.iter() {
            fonts.rasterize(glyph.key).unwrap();
        }

        let image = fonts.rasterize(wide.glyphs[0].key).unwrap();
        assert_eq!(image.pixels.len(), (image.width * image.height) as usize);
        assert!(image.pixels.iter().any(|coverage| *coverage > 0));
        let cached = fonts.rasterize(wide.glyphs[0].key).unwrap();
        assert!(Arc::ptr_eq(&image.pixels, &cached.pixels));

        spec.size = f32::NAN;
        assert!(matches!(fonts.layout(&spec), Err(FontError::InvalidSize)));
    }
}
