use super::*;

use ab_glyph::FontArc;

impl super::FontSystem {
    /// Add a font from bytes (TTF/OTF data).
    ///
    /// Returns the font ID for use with text rendering.
    pub fn add_font(&mut self, bytes: &[u8]) -> Result<FontId, FontError> {
        let font = FontArc::try_from_vec(bytes.to_vec())
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        let id = FontId(self.next_font_id);
        self.next_font_id += 1;
        self.fonts.insert(id, font);

        Ok(id)
    }

    /// Add a font from bytes with a specific ID.
    pub fn add_font_with_id(&mut self, bytes: &[u8], id: FontId) -> Result<(), FontError> {
        let font = FontArc::try_from_vec(bytes.to_vec())
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        self.fonts.insert(id, font);
        Ok(())
    }

    /// Get a font by ID.
    pub fn get_font(&self, id: FontId) -> Option<&FontArc> {
        self.fonts.get(&id)
    }
}
