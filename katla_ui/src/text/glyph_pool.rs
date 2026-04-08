use vello_cpu::{Pixmap, RenderContext};

/// Pool for reusing `vello_cpu::RenderContext` and `Pixmap` across glyph
/// rasterization calls.  Grows to the largest glyph dimensions encountered
/// so that repeated allocations are avoided.
pub(crate) struct GlyphRenderPool {
    ctx: Option<(RenderContext, Pixmap)>,
}

impl GlyphRenderPool {
    pub fn new() -> Self {
        Self { ctx: None }
    }

    /// Return a `(RenderContext, Pixmap)` pair that is at least `width` ×
    /// `height` pixels.
    ///
    /// If the cached pair is large enough it is reused (the context is reset
    /// and the pixmap is cleared via memset).  Otherwise a new pair is
    /// allocated that exactly matches the requested dimensions.
    pub fn acquire(&mut self, width: u16, height: u16) -> (RenderContext, Pixmap) {
        let needs_realloc = match &self.ctx {
            None => true,
            Some((ctx, _pix)) => ctx.width() < width || ctx.height() < height,
        };

        if needs_realloc {
            let ctx = RenderContext::new(width, height);
            let pix = Pixmap::new(width, height);
            self.ctx = Some((ctx, pix));
        }

        let (mut ctx, mut pix) = self.ctx.take().unwrap();

        // Clear all accumulated rendering state so the context is fresh.
        ctx.reset();

        // Clear via memset so no leftover data from a previous (larger)
        // glyph pollutes the result.
        pix.data_as_u8_slice_mut().fill(0);

        (ctx, pix)
    }
}
