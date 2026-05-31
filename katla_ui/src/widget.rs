//! Clipboard provider trait.

/// Trait for clipboard access.
///
/// Implement this to provide OS clipboard integration to text input widgets.
/// Set via [`UiContext::set_clipboard_provider`].
pub trait ClipboardProvider {
    /// Get the current clipboard contents as a string.
    fn get(&mut self) -> Option<String>;
    /// Set the clipboard contents to a string.
    fn set(&mut self, text: &str);
}
