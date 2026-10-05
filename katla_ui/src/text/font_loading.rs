use super::*;
use skrifa::FontRef;
use std::sync::Arc;

impl super::FontSystem {
    /// Add a font from bytes (TTF/OTF data).
    ///
    /// Returns the font ID for use with text rendering.
    pub fn add_font(&mut self, bytes: &[u8]) -> Result<FontId, FontError> {
        let data = Arc::new(bytes.to_vec());

        FontRef::new(&data).map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        let id = FontId(self.next_font_id);
        self.next_font_id += 1;
        self.fonts.insert(id, data);

        self.register_font_cosmic(id, bytes);

        Ok(id)
    }

    /// Add a font from bytes with a specific ID.
    pub fn add_font_with_id(&mut self, bytes: &[u8], id: FontId) -> Result<(), FontError> {
        let data = Arc::new(bytes.to_vec());

        FontRef::new(&data).map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        self.fonts.insert(id, data);

        self.register_font_cosmic(id, bytes);

        Ok(())
    }

    /// Get a font by ID, returning a borrowed FontRef.
    pub fn get_font(&self, id: FontId) -> Option<FontRef<'_>> {
        self.fonts.get(&id).and_then(|data| FontRef::new(data).ok())
    }

    /// Register a font with the cosmic-text integration layer and store family name.
    fn register_font_cosmic(&mut self, font_id: FontId, bytes: &[u8]) {
        if let Err(e) = self.cosmic.add_font_with_id(bytes, font_id) {
            log::warn!(
                "Failed to register font {:?} with cosmic-text: {}",
                font_id,
                e
            );
            return;
        }

        if let Some(cosmic_id) = self.cosmic.get_cosmic_id(font_id) {
            let family_name = self
                .cosmic
                .font_system()
                .db()
                .face(cosmic_id)
                .and_then(|face| face.families.first().map(|(name, _)| name.clone()));

            if let Some(name) = family_name {
                self.font_families.insert(font_id, name);
            }
        }
    }
}
