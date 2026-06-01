use swash::scale::ScaleContext;

/// Pool for reusing `swash::scale::ScaleContext` across glyph
/// rasterization calls. The context manages internal LRU caches and
/// scratch buffers for efficient glyph rendering.
pub(crate) struct GlyphRenderPool {
    context: Option<ScaleContext>,
}

impl GlyphRenderPool {
    pub fn new() -> Self {
        Self { context: None }
    }

    /// Execute `f` with a borrowed `ScaleContext`.
    ///
    /// Creates a context on first use and reuses it across calls.
    /// The closure-based approach ensures the pool retains ownership.
    pub fn acquire<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut ScaleContext) -> R,
    {
        if self.context.is_none() {
            self.context = Some(ScaleContext::new());
        }
        let ctx = self
            .context
            .as_mut()
            .expect("glyph pool must be initialized before use");

        f(ctx)
    }
}
