use super::*;

use ab_glyph::FontRef;

impl super::FontSystem {
    /// Add a font from bytes (TTF/OTF data).
    ///
    /// Returns the font ID for use with text rendering.
    ///
    /// Note: Font data is leaked with `Box::leak` to satisfy `'static` lifetime.
    /// This is intentional - fonts are typically loaded once and live for the
    /// application lifetime, so the leak is acceptable.
    pub fn add_font(&mut self, bytes: &[u8]) -> Result<FontId, FontError> {
        let bytes: &'static [u8] = Box::leak(bytes.to_vec().into_boxed_slice());

        let font = FontRef::try_from_slice(bytes)
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        let id = FontId(self.next_font_id);
        self.next_font_id += 1;
        self.fonts.insert(id, font);

        Ok(id)
    }

    /// Add a font from bytes with a specific ID.
    ///
    /// See [`add_font`](Self::add_font) for lifetime notes.
    pub fn add_font_with_id(&mut self, bytes: &[u8], id: FontId) -> Result<(), FontError> {
        let bytes: &'static [u8] = Box::leak(bytes.to_vec().into_boxed_slice());

        let font = FontRef::try_from_slice(bytes)
            .map_err(|e| FontError::LoadFailed(format!("{:?}", e)))?;

        self.fonts.insert(id, font);
        Ok(())
    }

    /// Get a font by ID.
    pub fn get_font(&self, id: FontId) -> Option<&FontRef<'static>> {
        self.fonts.get(&id)
    }
}
