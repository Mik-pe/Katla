use cosmic_text::{Attrs, Buffer, Family, Metrics, Shaping, Weight};

use super::*;

/// Result of shaping text with cosmic-text.
pub(crate) struct ShapedText {
    /// The cosmic-text Buffer containing shaped layout data.
    pub buffer: Buffer,
}

impl ShapedText {
    /// Get the total width and height of the shaped text.
    pub fn dimensions(&self) -> (f32, f32) {
        let runs: Vec<_> = self.buffer.layout_runs().collect();
        let width = runs.iter().map(|run| run.line_w).fold(0.0f32, f32::max);

        if runs.is_empty() {
            return (0.0, 0.0);
        }

        let last = runs.last().expect("runs is non-empty");
        let height = last.line_top + last.line_height;

        (width, height)
    }
}

impl super::FontSystem {
    /// Shape text using cosmic-text for proper layout.
    ///
    /// This creates a cosmic-text Buffer, applies shaping (harfrust), and returns
    /// the shaped result for measurement or rendering.
    ///
    /// Handles kerning, ligatures, BiDi, CJK line breaking, word wrapping,
    /// and font fallback automatically.
    pub(crate) fn shape_text(
        &mut self,
        font_id: FontId,
        text: &str,
        size: f32,
        _scale_factor: f32,
        max_width: Option<f32>,
    ) -> Option<ShapedText> {
        if text.is_empty() {
            let metrics = Metrics::new(size, size * 1.2);
            let buffer = Buffer::new(self.cosmic.font_system_mut(), metrics);
            return Some(ShapedText { buffer });
        }

        let family_name = self.font_families.get(&font_id)?;
        let metrics = Metrics::new(size, size * 1.2);
        let attrs = Attrs::new()
            .family(Family::Name(family_name))
            .weight(Weight::NORMAL);

        let mut buffer = Buffer::new(self.cosmic.font_system_mut(), metrics);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);

        if let Some(width) = max_width {
            buffer.set_size(Some(width), None);
        }

        // Shape the buffer to populate layout runs
        buffer.shape_until_scroll(self.cosmic.font_system_mut(), false);

        Some(ShapedText { buffer })
    }

    /// Measure text using cosmic-text Buffer for proper shaping.
    ///
    /// Returns (width, height) in logical pixels, accounting for kerning,
    /// ligatures, BiDi, and other shaping features.
    pub(crate) fn measure_text_shaped(
        &mut self,
        font_id: FontId,
        text: &str,
        size: f32,
        scale_factor: f32,
    ) -> Vec2 {
        if text.is_empty() {
            // shape_text builds every buffer with Metrics(size, size * 1.2),
            // so a shaped line reports that as its line box height. Report the
            // same for empty text (placeholders) or centring maths diverge.
            return Vec2::new(0.0, size * 1.2);
        }

        let shaped = self.shape_text(font_id, text, size, scale_factor, None);
        match shaped {
            Some(s) => {
                let (w, h) = s.dimensions();
                Vec2::new(w, h)
            }
            None => Vec2::new(0.0, 0.0),
        }
    }

    /// Get font metrics for a given size using cosmic-text.
    ///
    /// Returns (ascent, descent, line_gap) in logical pixels.
    pub fn get_font_metrics_cosmic(&self, font_id: FontId, size: f32) -> Option<(f32, f32, f32)> {
        let _cosmic_id = self.cosmic.get_cosmic_id(font_id)?;

        let font_data = self.fonts.get(&font_id)?;
        let swash_font = swash::FontDataRef::new(font_data).and_then(|fd| fd.get(0))?;

        let metrics = swash_font.metrics(&[]);
        let scale = size / metrics.units_per_em as f32;
        let ascent = metrics.ascent * scale;
        let descent = metrics.descent * scale;
        let line_gap = metrics.leading * scale;
        Some((ascent, -descent, line_gap))
    }
}
