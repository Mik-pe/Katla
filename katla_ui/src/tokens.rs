//! Central editor design tokens.
//!
//! Single source of truth for the desktop density scale: chrome heights,
//! control sizes, spacing steps, radii and icon sizes. Widgets and editor
//! panels read from here instead of scattering magic numbers; [`UiStyle`]
//! seeds its dimension fields from these constants so the two can't drift.
//!
//! Base grid: 4 px. `SPACING_2` exists for exceptional micro-spacing only.

/// Spacing step: exceptional micro-spacing (2 px).
pub const SPACING_2: f32 = 2.0;
/// Spacing step: icon/text micro-gap (4 px).
pub const SPACING_4: f32 = 4.0;
/// Spacing step: normal internal gap (8 px).
pub const SPACING_8: f32 = 8.0;
/// Spacing step: compact container padding (12 px).
pub const SPACING_12: f32 = 12.0;
/// Spacing step: normal panel padding (16 px).
pub const SPACING_16: f32 = 16.0;
/// Spacing step: major section separation (24 px).
pub const SPACING_24: f32 = 24.0;

/// Application bar (menu/title/play) height (38 px).
pub const APP_BAR_HEIGHT: f32 = 38.0;
/// Dock tab bar / panel header height (30 px).
pub const TAB_BAR_HEIGHT: f32 = 30.0;
/// Status bar height (24 px).
pub const STATUS_BAR_HEIGHT: f32 = 24.0;

/// Standard control height: buttons, inputs, icon buttons (28 px).
pub const CONTROL_HEIGHT: f32 = 28.0;
/// Tree/list row height (26 px).
pub const TREE_ROW_HEIGHT: f32 = 26.0;
/// Compact inline control height (24 px) — property fields, small toggles.
pub const COMPACT_CONTROL_HEIGHT: f32 = 24.0;
/// Viewport overlay toolbar height (32 px).
pub const VIEWPORT_TOOLBAR_HEIGHT: f32 = 32.0;

/// Standard icon size (14 px) — inline with text.
pub const ICON_SIZE: f32 = 14.0;
/// Emphasis icon size (16 px).
pub const ICON_SIZE_MEDIUM: f32 = 16.0;

/// Corner radius for ordinary controls (4 px).
pub const RADIUS_CONTROL: f32 = 4.0;
/// Corner radius for menus/popups (6 px).
pub const RADIUS_SURFACE: f32 = 6.0;
/// Corner radius for modal windows (10 px).
pub const RADIUS_WINDOW: f32 = 10.0;
/// Modal title bar height (40 px) — larger than dock tabs; modals float.
pub const MODAL_TITLE_HEIGHT: f32 = 40.0;
/// Modal close-button side (28 px) — a real hit target, not a tiny ×.
pub const MODAL_CLOSE_SIZE: f32 = 28.0;

/// Visual thickness of a divider (1 px).
pub const DIVIDER_THICKNESS: f32 = 1.0;
/// Hit-target width of a splitter handle (6 px); drawn as a thin line inside.
pub const SPLITTER_HIT_WIDTH: f32 = 6.0;
/// Visible line width of a splitter handle.
pub const SPLITTER_LINE_WIDTH: f32 = 1.0;
/// Left padding shared by tab labels and panel headers.
pub const TAB_LABEL_LEADING: f32 = 12.0;
/// Maximum width of a dock tab in a multi-tab strip (160 px). Tabs stack
/// from the left; the rest of the strip stays background.
pub const TAB_MAX_WIDTH: f32 = 160.0;
/// Safe margin for viewport overlays (10 px).
pub const VIEWPORT_OVERLAY_MARGIN: f32 = 10.0;
/// Per-level indentation increment in the hierarchy tree (16 px).
pub const TREE_INDENT: f32 = 16.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spacing_is_4px_grid() {
        // SPACING_2 is the sanctioned micro-spacing exception; everything
        // else must sit on the 4 px grid.
        assert_eq!(SPACING_2, 2.0);
        for v in [SPACING_4, SPACING_8, SPACING_12, SPACING_16, SPACING_24] {
            assert_eq!(v % 4.0, 0.0, "spacing {v} off the 4px grid");
        }
    }

    #[test]
    fn test_chrome_heights_match_density_targets() {
        assert!((36.0..=42.0).contains(&APP_BAR_HEIGHT));
        assert!((28.0..=32.0).contains(&TAB_BAR_HEIGHT));
        assert!((22.0..=24.0).contains(&STATUS_BAR_HEIGHT));
        assert!((26.0..=30.0).contains(&CONTROL_HEIGHT));
        assert!((24.0..=28.0).contains(&TREE_ROW_HEIGHT));
    }
}
