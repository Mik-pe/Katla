use super::*;
use skrifa::FontRef;
use std::sync::Arc;

impl super::FontSystem {
    /// Add a font from bytes (TTF/OTF data).
    ///
    /// Returns the font ID for use with text rendering.
    pub fn add_font(&mut self, bytes: &[u8]) -> Result<FontId, FontError> {
        let data = Arc::new(bytes.to_vec());

        // Validate the font can be parsed
        FontRef::new(&data)
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        let id = FontId(self.next_font_id);
        self.next_font_id += 1;
        self.fonts.insert(id, data);

        Ok(id)
    }

    /// Add a font from bytes with a specific ID.
    pub fn add_font_with_id(&mut self, bytes: &[u8], id: FontId) -> Result<(), FontError> {
        let data = Arc::new(bytes.to_vec());

        FontRef::new(&data)
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        self.fonts.insert(id, data);
        Ok(())
    }

    /// Get a font by ID, returning a borrowed FontRef.
    pub fn get_font(&self, id: FontId) -> Option<FontRef<'_>> {
        self.fonts.get(&id).and_then(|data| FontRef::new(data).ok())
    }
}
