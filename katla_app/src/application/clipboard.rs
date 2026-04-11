use katla_ui::ClipboardProvider;

/// OS clipboard provider using the `arboard` crate.
pub struct OsClipboard {
    inner: arboard::Clipboard,
}

impl OsClipboard {
    /// Create a new OS clipboard provider.
    pub fn new() -> Result<Self, arboard::Error> {
        arboard::Clipboard::new().map(|inner| Self { inner })
    }
}

impl ClipboardProvider for OsClipboard {
    fn get(&mut self) -> Option<String> {
        self.inner.get_text().ok()
    }

    fn set(&mut self, text: &str) {
        let _ = self.inner.set_text(text);
    }
}
