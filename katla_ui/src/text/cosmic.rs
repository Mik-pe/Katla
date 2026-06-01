use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::fontdb::{self, Database, Source};

use super::{FontError, FontId};

/// Integration layer wrapping cosmic-text's `FontSystem`, `SwashCache`,
/// and font management for use by the text pipeline and editor widget.
///
/// This is the shared infrastructure that both the text rendering module
/// and the code editor widget use. Create it once per application and share
/// via `Rc<RefCell<>>` or similar.
///
/// # Responsibilities
///
/// - Wraps `cosmic_text::FontSystem` for font discovery and management
/// - Wraps `cosmic_text::SwashCache` for glyph rasterization caching
/// - Maps between katla_ui `FontId` and cosmic-text `fontdb::ID`
/// - Loads TTF/OTF font files and assigns `FontId` handles
pub(crate) struct CosmicTextSystem {
    /// cosmic-text FontSystem (created once, manages fontdb internally).
    font_system: cosmic_text::FontSystem,
    /// Map from katla_ui FontId to the primary cosmic-text fontdb::ID.
    font_id_to_cosmic: HashMap<FontId, fontdb::ID>,
}

impl CosmicTextSystem {
    /// Create a new CosmicTextSystem with system fonts loaded.
    ///
    /// This creates a `cosmic_text::FontSystem` which discovers and loads
    /// system fonts.
    #[cfg(test)]
    pub fn new() -> Self {
        let font_system = cosmic_text::FontSystem::new();

        Self {
            font_system,
            font_id_to_cosmic: HashMap::new(),
        }
    }

    /// Create a CosmicTextSystem without loading system fonts.
    ///
    /// Useful for testing or when fonts are loaded explicitly.
    pub fn new_empty() -> Self {
        let db = Database::new();
        let locale = "en-US".to_string();
        let font_system = cosmic_text::FontSystem::new_with_locale_and_db(locale, db);

        Self {
            font_system,
            font_id_to_cosmic: HashMap::new(),
        }
    }

    /// Load a font from bytes (TTF/OTF data).
    ///
    /// The font data is added to cosmic-text's fontdb. Returns a `FontId`
    /// handle that can be used to reference this font in katla_ui.
    ///
    /// For font collections (TTC/OTC), only the first face's ID is mapped.
    #[cfg(test)]
    pub fn add_font(&mut self, bytes: &[u8]) -> Result<FontId, FontError> {
        use std::sync::atomic::{AtomicU32, Ordering};
        static NEXT_ID: AtomicU32 = AtomicU32::new(0);

        if bytes.len() < 4 {
            return Err(FontError::LoadFailed("Font data too short".to_string()));
        }

        let data = Arc::new(bytes.to_vec());
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(Source::Binary(data));

        if ids.is_empty() {
            return Err(FontError::LoadFailed(
                "No font faces found in data".to_string(),
            ));
        }

        let font_id = FontId(NEXT_ID.fetch_add(1, Ordering::Relaxed));
        let cosmic_id = ids[0];
        self.font_id_to_cosmic.insert(font_id, cosmic_id);

        Ok(font_id)
    }

    /// Load a font from bytes with a specific FontId.
    ///
    /// Use this when you need deterministic font IDs (e.g., FontId::DEFAULT,
    /// FontId::ICON).
    pub fn add_font_with_id(&mut self, bytes: &[u8], id: FontId) -> Result<(), FontError> {
        if bytes.len() < 4 {
            return Err(FontError::LoadFailed("Font data too short".to_string()));
        }

        let data = Arc::new(bytes.to_vec());
        let ids = self
            .font_system
            .db_mut()
            .load_font_source(Source::Binary(data));

        if ids.is_empty() {
            return Err(FontError::LoadFailed(
                "No font faces found in data".to_string(),
            ));
        }

        let cosmic_id = ids[0];
        self.font_id_to_cosmic.insert(id, cosmic_id);

        Ok(())
    }

    /// Get the cosmic-text fontdb::ID for a katla_ui FontId.
    pub fn get_cosmic_id(&self, font_id: FontId) -> Option<fontdb::ID> {
        self.font_id_to_cosmic.get(&font_id).copied()
    }

    /// Get the katla_ui FontId for a cosmic-text fontdb::ID.
    #[cfg(test)]
    pub fn get_font_id(&self, cosmic_id: fontdb::ID) -> Option<FontId> {
        self.font_id_to_cosmic
            .iter()
            .find(|(_, id)| **id == cosmic_id)
            .map(|(font_id, _)| *font_id)
    }

    /// Access the cosmic-text FontSystem mutably.
    ///
    /// Required for creating cosmic-text `Buffer` instances and
    /// performing text shaping/layout operations.
    pub fn font_system_mut(&mut self) -> &mut cosmic_text::FontSystem {
        &mut self.font_system
    }

    /// Access the cosmic-text FontSystem immutably.
    pub fn font_system(&self) -> &cosmic_text::FontSystem {
        &self.font_system
    }

    /// Get the number of loaded katla_ui fonts.
    #[cfg(test)]
    pub fn font_count(&self) -> usize {
        self.font_id_to_cosmic.len()
    }

    /// Check if a font with the given FontId is loaded.
    #[cfg(test)]
    pub fn has_font(&self, id: FontId) -> bool {
        self.font_id_to_cosmic.contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::SwashCache;

    /// Load the bundled Roboto font for testing. Panics if not found.
    fn load_roboto() -> Vec<u8> {
        let candidates = [
            "resources/fonts/roboto-regular.ttf",
            "../resources/fonts/roboto-regular.ttf",
            "../../resources/fonts/roboto-regular.ttf",
        ];
        for path in &candidates {
            if let Ok(data) = std::fs::read(path) {
                return data;
            }
        }
        panic!("Could not find roboto-regular.ttf from any candidate path");
    }

    #[test]
    fn test_cosmic_text_system_creation() {
        let sys = CosmicTextSystem::new();
        assert_eq!(sys.font_count(), 0, "No custom fonts loaded initially");
    }

    #[test]
    fn test_cosmic_text_system_creation_empty() {
        let sys = CosmicTextSystem::new_empty();
        assert_eq!(sys.font_count(), 0);
    }

    #[test]
    fn test_add_font_returns_unique_ids() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();

        let id1 = sys.add_font(&roboto).expect("First font should load");
        let id2 = sys.add_font(&roboto).expect("Second font should load");

        assert_ne!(id1, id2, "Each add_font call should return a unique FontId");
        assert_eq!(sys.font_count(), 2);
    }

    #[test]
    fn test_add_font_with_specific_id() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();

        let custom_id = FontId(42);
        sys.add_font_with_id(&roboto, custom_id)
            .expect("Font should load with specific ID");

        assert!(sys.has_font(custom_id));
        assert_eq!(sys.font_count(), 1);

        let cosmic_id = sys.get_cosmic_id(custom_id);
        assert!(cosmic_id.is_some(), "Should map to a cosmic-text ID");

        let roundtrip = sys.get_font_id(cosmic_id.unwrap());
        assert_eq!(roundtrip, Some(custom_id), "Roundtrip mapping should work");
    }

    #[test]
    fn test_font_id_mapping_roundtrip() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();

        let font_id = sys.add_font(&roboto).expect("Font should load");
        let cosmic_id = sys
            .get_cosmic_id(font_id)
            .expect("Should have cosmic-text ID mapping");

        let recovered = sys
            .get_font_id(cosmic_id)
            .expect("Should have reverse mapping");
        assert_eq!(
            recovered, font_id,
            "Roundtrip should recover original FontId"
        );
    }

    #[test]
    fn test_has_font() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();

        let font_id = sys.add_font(&roboto).expect("Font should load");

        assert!(sys.has_font(font_id), "Loaded font should be present");
        assert!(
            !sys.has_font(FontId(999)),
            "Unknown ID should not be present"
        );
    }

    #[test]
    fn test_invalid_font_data() {
        let mut sys = CosmicTextSystem::new_empty();

        let result = sys.add_font(&[0x00, 0x01, 0x02]);
        assert!(result.is_err(), "Too-short data should fail");

        let result = sys.add_font(&[0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(result.is_err(), "Invalid font data should fail");
    }

    #[test]
    fn test_font_system_access() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();
        sys.add_font(&roboto).expect("Font should load");

        let _locale = sys.font_system().locale();
        let db = sys.font_system().db();
        assert!(!db.is_empty(), "fontdb should contain loaded font");
    }

    #[test]
    fn test_font_system_mut_access() {
        let mut sys = CosmicTextSystem::new_empty();

        let fs = sys.font_system_mut();
        let _db = fs.db_mut();
    }

    #[test]
    fn test_swash_cache_access() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();
        sys.add_font(&roboto).expect("Font should load");

        let mut cache = SwashCache::new();
        let _ = &mut cache;
    }

    #[test]
    fn test_swash_cache_glyph_rasterization() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();
        let font_id = sys.add_font(&roboto).expect("Font should load");
        let cosmic_id = sys.get_cosmic_id(font_id).expect("Should have cosmic ID");

        let font = sys
            .font_system_mut()
            .get_font(cosmic_id, cosmic_text::Weight::NORMAL)
            .expect("Should get font from cosmic-text");

        let swash_font = font.as_swash();
        let glyph_id = swash_font.charmap().map('A');
        assert_ne!(glyph_id, 0, "'A' should map to a valid glyph ID");

        let (cache_key, _x_offset, _y_offset) = cosmic_text::CacheKey::new(
            cosmic_id,
            glyph_id,
            16.0,
            (0.0, 0.0),
            cosmic_text::Weight::NORMAL,
            cosmic_text::CacheKeyFlags::empty(),
        );

        let mut swash_cache = SwashCache::new();
        let image = swash_cache.get_image_uncached(&mut sys.font_system, cache_key);

        assert!(image.is_some(), "SwashCache should rasterize glyph 'A'");
        let img = image.unwrap();
        assert!(img.placement.width > 0, "Glyph should have non-zero width");
        assert!(
            img.placement.height > 0,
            "Glyph should have non-zero height"
        );
    }

    #[test]
    fn test_multiple_fonts_independent() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();

        let id1 = sys.add_font(&roboto).expect("First font should load");
        let id2 = sys
            .add_font_with_id(&roboto, FontId::ICON)
            .map(|()| FontId::ICON)
            .expect("Second font should load");

        assert_ne!(id1, id2);
        assert!(sys.has_font(id1));
        assert!(sys.has_font(id2));

        let cosmic1 = sys
            .get_cosmic_id(id1)
            .expect("Font 1 should have cosmic ID");
        let cosmic2 = sys
            .get_cosmic_id(id2)
            .expect("Font 2 should have cosmic ID");

        assert_ne!(
            cosmic1, cosmic2,
            "Different FontIds should map to different cosmic IDs"
        );
    }

    #[test]
    fn test_default_font_id_constants() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();

        sys.add_font_with_id(&roboto, FontId::DEFAULT)
            .expect("Should load with DEFAULT ID");
        sys.add_font_with_id(&roboto, FontId::ICON)
            .expect("Should load with ICON ID");

        assert!(sys.has_font(FontId::DEFAULT));
        assert!(sys.has_font(FontId::ICON));

        let cosmic_default = sys
            .get_cosmic_id(FontId::DEFAULT)
            .expect("DEFAULT should map");
        let cosmic_icon = sys.get_cosmic_id(FontId::ICON).expect("ICON should map");

        assert_ne!(
            cosmic_default, cosmic_icon,
            "DEFAULT and ICON should map to different cosmic IDs"
        );
    }

    #[test]
    fn test_overwrite_font_id() {
        let mut sys = CosmicTextSystem::new_empty();
        let roboto = load_roboto();

        sys.add_font_with_id(&roboto, FontId::DEFAULT)
            .expect("First load should succeed");

        let first_cosmic = sys
            .get_cosmic_id(FontId::DEFAULT)
            .expect("Should have mapping");

        sys.add_font_with_id(&roboto, FontId::DEFAULT)
            .expect("Second load should succeed (overwrite)");

        let second_cosmic = sys
            .get_cosmic_id(FontId::DEFAULT)
            .expect("Should have mapping after overwrite");

        assert_ne!(
            first_cosmic, second_cosmic,
            "Overwriting should create a new cosmic ID"
        );
    }
}
