//! ForkAwesome icon codepoints for UI rendering.
//!
//! This crate provides icon constants for use with icon fonts like ForkAwesome.
//! Icons are represented as Unicode characters in the Private Use Area (PUA).
//!
//! # Usage
//!
//! ```ignore
//! use katla_icons::ForkAwesome;
//!
//! let icon = ForkAwesome::CUBE;
//! ```

/// ForkAwesome icon codepoints.
///
/// ForkAwesome is an icon font with 796+ icons. Each icon is mapped to a
/// Unicode codepoint in the Private Use Area (U+F000-U+F2E0).
///
/// See https://forkaweso.me/Fork-Awesome/icons/ for visual reference.
pub struct ForkAwesome;

impl ForkAwesome {
    // =========================================================================
    // Common UI Icons
    // =========================================================================

    /// Home icon
    pub const HOME: char = '\u{F015}';
    /// Search/Magnifier icon
    pub const SEARCH: char = '\u{F002}';
    /// User/Person icon
    pub const USER: char = '\u{F007}';
    /// Users/People icon
    pub const USERS: char = '\u{F0C0}';
    /// Cog/Settings icon
    pub const COG: char = '\u{F013}';
    /// Cogs/Settings icon
    pub const COGS: char = '\u{F085}';

    // =========================================================================
    // Media Controls
    // =========================================================================

    /// Play icon
    pub const PLAY: char = '\u{F04B}';
    /// Pause icon
    pub const PAUSE: char = '\u{F04C}';
    /// Stop icon
    pub const STOP: char = '\u{F04D}';
    /// Step Forward icon
    pub const STEP_FORWARD: char = '\u{F051}';
    /// Step Backward icon
    pub const STEP_BACKWARD: char = '\u{F048}';
    /// Fast Forward icon
    pub const FAST_FORWARD: char = '\u{F050}';
    /// Fast Backward icon
    pub const FAST_BACKWARD: char = '\u{F049}';
    /// Eject icon
    pub const EJECT: char = '\u{F052}';

    // =========================================================================
    // Arrows & Navigation
    // =========================================================================

    /// Chevron Up icon
    pub const CHEVRON_UP: char = '\u{F077}';
    /// Chevron Down icon
    pub const CHEVRON_DOWN: char = '\u{F078}';
    /// Chevron Left icon
    pub const CHEVRON_LEFT: char = '\u{F053}';
    /// Chevron Right icon
    pub const CHEVRON_RIGHT: char = '\u{F054}';
    /// Arrow Up icon
    pub const ARROW_UP: char = '\u{F062}';
    /// Arrow Down icon
    pub const ARROW_DOWN: char = '\u{F063}';
    /// Arrow Left icon
    pub const ARROW_LEFT: char = '\u{F060}';
    /// Arrow Right icon
    pub const ARROW_RIGHT: char = '\u{F061}';
    /// Caret Up icon
    pub const CARET_UP: char = '\u{F0D8}';
    /// Caret Down icon
    pub const CARET_DOWN: char = '\u{F0D7}';
    /// Caret Left icon
    pub const CARET_LEFT: char = '\u{F0D9}';
    /// Caret Right icon
    pub const CARET_RIGHT: char = '\u{F0DA}';
    /// Angle Right icon - for submenu indicators
    pub const ANGLE_RIGHT: char = '\u{F105}';
    pub const ANGLE_LEFT: char = '\u{F104}';
    pub const ANGLE_UP: char = '\u{F106}';
    pub const ANGLE_DOWN: char = '\u{F107}';
    /// Expand/Arrows Alt icon
    pub const EXPAND: char = '\u{F0B2}';
    /// Compress icon
    pub const COMPRESS: char = '\u{F066}';

    // =========================================================================
    // Editing & Content
    // =========================================================================

    /// Plus/Add icon
    pub const PLUS: char = '\u{F067}';
    /// Minus/Subtract icon
    pub const MINUS: char = '\u{F068}';
    /// Times/Close/X icon
    pub const TIMES: char = '\u{F00D}';
    /// Check/Tick icon
    pub const CHECK: char = '\u{F00C}';
    /// Pencil/Edit icon
    pub const PENCIL: char = '\u{F040}';
    /// Edit (pen on square) icon
    pub const EDIT: char = '\u{F044}';
    /// Eraser icon
    pub const ERASER: char = '\u{F12D}';
    /// Copy icon
    pub const COPY: char = '\u{F0C5}';
    /// Paste/Clipboard icon
    pub const PASTE: char = '\u{F0EA}';
    /// Cut/Scissors icon
    pub const CUT: char = '\u{F0C4}';
    /// Undo icon
    pub const UNDO: char = '\u{F0E2}';
    /// Redo icon
    pub const REDO: char = '\u{F01E}';
    /// Refresh/Repeat icon
    pub const REFRESH: char = '\u{F021}';

    // =========================================================================
    // Files & Folders
    // =========================================================================

    /// File icon
    pub const FILE: char = '\u{F15B}';
    /// File Text icon
    pub const FILE_TEXT: char = '\u{F15C}';
    /// File Code icon
    pub const FILE_CODE: char = '\u{F1C9}';
    /// Folder icon
    pub const FOLDER: char = '\u{F07B}';
    /// Folder Open icon
    pub const FOLDER_OPEN: char = '\u{F07C}';
    /// Save/Floppy icon
    pub const SAVE: char = '\u{F0C7}';
    /// Download icon
    pub const DOWNLOAD: char = '\u{F019}';
    /// Upload icon
    pub const UPLOAD: char = '\u{F093}';
    /// External Link icon
    pub const EXTERNAL_LINK: char = '\u{F08E}';

    // =========================================================================
    // 3D & Scene Editor
    // =========================================================================

    /// Cube icon
    pub const CUBE: char = '\u{F1B2}';
    /// Cubes icon
    pub const CUBES: char = '\u{F1B3}';
    /// Camera icon
    pub const CAMERA: char = '\u{F030}';
    /// Video Camera icon
    pub const VIDEO_CAMERA: char = '\u{F03D}';
    /// Image/Picture icon
    pub const IMAGE: char = '\u{F03E}';
    /// Map icon
    pub const MAP: char = '\u{F279}';
    /// Compass icon
    pub const COMPASS: char = '\u{F14E}';
    /// Crosshairs icon
    pub const CROSSHAIRS: char = '\u{F05B}';
    /// Location Arrow icon
    pub const LOCATION_ARROW: char = '\u{F124}';
    /// Street View icon
    pub const STREET_VIEW: char = '\u{F21D}';

    // =========================================================================
    // Visibility
    // =========================================================================

    /// Eye/View icon
    pub const EYE: char = '\u{F06E}';
    /// Eye Slash/Hide icon
    pub const EYE_SLASH: char = '\u{F070}';
    /// Low Vision icon
    pub const LOW_VISION: char = '\u{F2A8}';
    /// Lightbulb icon
    pub const LIGHTBULB: char = '\u{F0EB}';
    /// Sun icon
    pub const SUN: char = '\u{F185}';
    /// Fire icon
    pub const FIRE: char = '\u{F06D}';

    // =========================================================================
    // Status & Feedback
    // =========================================================================

    /// Info Circle icon
    pub const INFO_CIRCLE: char = '\u{F05A}';
    /// Info (lowercase i) icon
    pub const INFO: char = '\u{F129}';
    /// Question Circle icon
    pub const QUESTION_CIRCLE: char = '\u{F059}';
    /// Exclamation Circle icon
    pub const EXCLAMATION_CIRCLE: char = '\u{F06A}';
    /// Exclamation Triangle/Warning icon
    pub const EXCLAMATION_TRIANGLE: char = '\u{F071}';
    /// Check Circle icon
    pub const CHECK_CIRCLE: char = '\u{F058}';
    /// Times Circle icon
    pub const TIMES_CIRCLE: char = '\u{F057}';
    /// Plus Circle icon
    pub const PLUS_CIRCLE: char = '\u{F055}';
    /// Minus Circle icon
    pub const MINUS_CIRCLE: char = '\u{F056}';
    /// Ban/Circle X icon
    pub const BAN: char = '\u{F05E}';

    // =========================================================================
    // Actions
    // =========================================================================

    /// Trash/Delete icon
    pub const TRASH: char = '\u{F1F8}';
    /// Trash Alt icon (fa-trash-o)
    pub const TRASH_ALT: char = '\u{F014}';
    /// Lock icon
    pub const LOCK: char = '\u{F023}';
    /// Unlock icon
    pub const UNLOCK: char = '\u{F09C}';
    /// Unlock Alt icon
    pub const UNLOCK_ALT: char = '\u{F13E}';
    /// Key icon
    pub const KEY: char = '\u{F084}';
    /// Link/Chain icon
    pub const LINK: char = '\u{F0C1}';
    /// Unlink/Broken Chain icon
    pub const UNLINK: char = '\u{F127}';
    /// Hand Paper/Grab icon
    pub const HAND_PAPER: char = '\u{F256}';
    /// Hand Pointer icon
    pub const HAND_POINTER: char = '\u{F25A}';
    /// Hand Rock icon
    pub const HAND_ROCK: char = '\u{F255}';
    /// Hand Scissors icon
    pub const HAND_SCISSORS: char = '\u{F257}';
    /// Hand Spock icon
    pub const HAND_SPOCK: char = '\u{F259}';

    // =========================================================================
    // Objects & Tools
    // =========================================================================

    /// Wrench icon
    pub const WRENCH: char = '\u{F0AD}';
    /// Gavel/Hammer icon
    pub const HAMMER: char = '\u{F0E3}';
    /// Magic Wand icon
    pub const MAGIC: char = '\u{F0D0}';
    /// Paint Brush icon
    pub const PAINT_BRUSH: char = '\u{F1FC}';
    pub const PENCIL_SQUARE: char = '\u{F14B}';
    /// Ruler Combined icon (fa-sliders, no ruler in ForkAwesome)
    pub const RULER_COMBINED: char = '\u{F1DE}';
    /// Ruler Horizontal icon (fa-arrows-h, no ruler in ForkAwesome)
    pub const RULER_HORIZONTAL: char = '\u{F07E}';
    /// Ruler Vertical icon (fa-arrows-v, no ruler in ForkAwesome)
    pub const RULER_VERTICAL: char = '\u{F07D}';
    /// Sort icon
    pub const SORT: char = '\u{F0DC}';
    /// Sort Up icon
    pub const SORT_UP: char = '\u{F0DE}';
    /// Sort Down icon
    pub const SORT_DOWN: char = '\u{F0DD}';

    // =========================================================================
    // Layout & UI Elements
    // =========================================================================

    /// Bars/Hamburger Menu icon
    pub const BARS: char = '\u{F0C9}';
    /// Th/Grid icon
    pub const TH: char = '\u{F00A}';
    /// Th Large icon
    pub const TH_LARGE: char = '\u{F009}';
    /// Th List icon
    pub const TH_LIST: char = '\u{F00B}';
    /// List icon
    pub const LIST: char = '\u{F03A}';
    /// List Ol icon
    pub const LIST_OL: char = '\u{F0CB}';
    /// List Ul icon
    pub const LIST_UL: char = '\u{F0CA}';
    /// Columns icon
    pub const COLUMNS: char = '\u{F0DB}';
    /// Table icon
    pub const TABLE: char = '\u{F0CE}';
    /// Window Maximize icon
    pub const WINDOW_MAXIMIZE: char = '\u{F2D0}';
    /// Window Minimize icon
    pub const WINDOW_MINIMIZE: char = '\u{F2D1}';
    /// Window Restore icon
    pub const WINDOW_RESTORE: char = '\u{F2D2}';
    /// Window Close icon
    pub const WINDOW_CLOSE: char = '\u{F2D3}';

    // =========================================================================
    // Misc
    // =========================================================================

    /// Star icon
    pub const STAR: char = '\u{F005}';
    /// Star Outline icon
    pub const STAR_OUTLINE: char = '\u{F006}';
    /// Heart icon
    pub const HEART: char = '\u{F004}';
    /// Bookmark icon
    pub const BOOKMARK: char = '\u{F02E}';
    /// Tag icon
    pub const TAG: char = '\u{F02B}';
    /// Tags icon
    pub const TAGS: char = '\u{F02C}';
    /// Flag icon
    pub const FLAG: char = '\u{F024}';
    /// Circle icon
    pub const CIRCLE: char = '\u{F111}';
    /// Circle Outline icon
    pub const CIRCLE_OUTLINE: char = '\u{F10C}';
    /// Square icon
    pub const SQUARE: char = '\u{F0C8}';
    /// Square Outline icon
    pub const SQUARE_OUTLINE: char = '\u{F096}';

    /// Returns a list of commonly used icons for precaching.
    ///
    /// Use this to precache the most frequently used icons in your UI:
    /// ```ignore
    /// ui.fonts.precache_icons(FontId::ICON, 16.0, scale_factor, ForkAwesome::common_icons());
    /// ```
    pub fn common_icons() -> &'static [char] {
        &[
            // Navigation
            Self::CHEVRON_UP,
            Self::CHEVRON_DOWN,
            Self::CHEVRON_LEFT,
            Self::CHEVRON_RIGHT,
            Self::ARROW_UP,
            Self::ARROW_DOWN,
            Self::ARROW_LEFT,
            Self::ARROW_RIGHT,
            // Actions
            Self::PLUS,
            Self::MINUS,
            Self::TIMES,
            Self::CHECK,
            Self::PENCIL,
            Self::EDIT,
            Self::TRASH,
            Self::REFRESH,
            // 3D/Scene
            Self::CUBE,
            Self::CUBES,
            Self::CAMERA,
            // Visibility
            Self::EYE,
            Self::EYE_SLASH,
            // Status
            Self::INFO_CIRCLE,
            Self::EXCLAMATION_TRIANGLE,
            // UI
            Self::BARS,
            Self::TH,
            Self::LIST,
            Self::SEARCH,
            // Files
            Self::FOLDER,
            Self::FOLDER_OPEN,
            Self::FILE,
            Self::SAVE,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::ForkAwesome;

    fn all_icon_constants() -> Vec<(&'static str, char)> {
        vec![
            ("HOME", ForkAwesome::HOME),
            ("SEARCH", ForkAwesome::SEARCH),
            ("USER", ForkAwesome::USER),
            ("USERS", ForkAwesome::USERS),
            ("COG", ForkAwesome::COG),
            ("COGS", ForkAwesome::COGS),
            ("PLAY", ForkAwesome::PLAY),
            ("PAUSE", ForkAwesome::PAUSE),
            ("STOP", ForkAwesome::STOP),
            ("STEP_FORWARD", ForkAwesome::STEP_FORWARD),
            ("STEP_BACKWARD", ForkAwesome::STEP_BACKWARD),
            ("FAST_FORWARD", ForkAwesome::FAST_FORWARD),
            ("FAST_BACKWARD", ForkAwesome::FAST_BACKWARD),
            ("EJECT", ForkAwesome::EJECT),
            ("CHEVRON_UP", ForkAwesome::CHEVRON_UP),
            ("CHEVRON_DOWN", ForkAwesome::CHEVRON_DOWN),
            ("CHEVRON_LEFT", ForkAwesome::CHEVRON_LEFT),
            ("CHEVRON_RIGHT", ForkAwesome::CHEVRON_RIGHT),
            ("ARROW_UP", ForkAwesome::ARROW_UP),
            ("ARROW_DOWN", ForkAwesome::ARROW_DOWN),
            ("ARROW_LEFT", ForkAwesome::ARROW_LEFT),
            ("ARROW_RIGHT", ForkAwesome::ARROW_RIGHT),
            ("CARET_UP", ForkAwesome::CARET_UP),
            ("CARET_DOWN", ForkAwesome::CARET_DOWN),
            ("CARET_LEFT", ForkAwesome::CARET_LEFT),
            ("CARET_RIGHT", ForkAwesome::CARET_RIGHT),
            ("ANGLE_RIGHT", ForkAwesome::ANGLE_RIGHT),
            ("ANGLE_LEFT", ForkAwesome::ANGLE_LEFT),
            ("ANGLE_UP", ForkAwesome::ANGLE_UP),
            ("ANGLE_DOWN", ForkAwesome::ANGLE_DOWN),
            ("EXPAND", ForkAwesome::EXPAND),
            ("COMPRESS", ForkAwesome::COMPRESS),
            ("PLUS", ForkAwesome::PLUS),
            ("MINUS", ForkAwesome::MINUS),
            ("TIMES", ForkAwesome::TIMES),
            ("CHECK", ForkAwesome::CHECK),
            ("PENCIL", ForkAwesome::PENCIL),
            ("EDIT", ForkAwesome::EDIT),
            ("ERASER", ForkAwesome::ERASER),
            ("COPY", ForkAwesome::COPY),
            ("PASTE", ForkAwesome::PASTE),
            ("CUT", ForkAwesome::CUT),
            ("UNDO", ForkAwesome::UNDO),
            ("REDO", ForkAwesome::REDO),
            ("REFRESH", ForkAwesome::REFRESH),
            ("FILE", ForkAwesome::FILE),
            ("FILE_TEXT", ForkAwesome::FILE_TEXT),
            ("FILE_CODE", ForkAwesome::FILE_CODE),
            ("FOLDER", ForkAwesome::FOLDER),
            ("FOLDER_OPEN", ForkAwesome::FOLDER_OPEN),
            ("SAVE", ForkAwesome::SAVE),
            ("DOWNLOAD", ForkAwesome::DOWNLOAD),
            ("UPLOAD", ForkAwesome::UPLOAD),
            ("EXTERNAL_LINK", ForkAwesome::EXTERNAL_LINK),
            ("CUBE", ForkAwesome::CUBE),
            ("CUBES", ForkAwesome::CUBES),
            ("CAMERA", ForkAwesome::CAMERA),
            ("VIDEO_CAMERA", ForkAwesome::VIDEO_CAMERA),
            ("IMAGE", ForkAwesome::IMAGE),
            ("MAP", ForkAwesome::MAP),
            ("COMPASS", ForkAwesome::COMPASS),
            ("CROSSHAIRS", ForkAwesome::CROSSHAIRS),
            ("LOCATION_ARROW", ForkAwesome::LOCATION_ARROW),
            ("STREET_VIEW", ForkAwesome::STREET_VIEW),
            ("EYE", ForkAwesome::EYE),
            ("EYE_SLASH", ForkAwesome::EYE_SLASH),
            ("LOW_VISION", ForkAwesome::LOW_VISION),
            ("LIGHTBULB", ForkAwesome::LIGHTBULB),
            ("SUN", ForkAwesome::SUN),
            ("FIRE", ForkAwesome::FIRE),
            ("INFO_CIRCLE", ForkAwesome::INFO_CIRCLE),
            ("INFO", ForkAwesome::INFO),
            ("QUESTION_CIRCLE", ForkAwesome::QUESTION_CIRCLE),
            ("EXCLAMATION_CIRCLE", ForkAwesome::EXCLAMATION_CIRCLE),
            ("EXCLAMATION_TRIANGLE", ForkAwesome::EXCLAMATION_TRIANGLE),
            ("CHECK_CIRCLE", ForkAwesome::CHECK_CIRCLE),
            ("TIMES_CIRCLE", ForkAwesome::TIMES_CIRCLE),
            ("PLUS_CIRCLE", ForkAwesome::PLUS_CIRCLE),
            ("MINUS_CIRCLE", ForkAwesome::MINUS_CIRCLE),
            ("BAN", ForkAwesome::BAN),
            ("TRASH", ForkAwesome::TRASH),
            ("TRASH_ALT", ForkAwesome::TRASH_ALT),
            ("LOCK", ForkAwesome::LOCK),
            ("UNLOCK", ForkAwesome::UNLOCK),
            ("UNLOCK_ALT", ForkAwesome::UNLOCK_ALT),
            ("KEY", ForkAwesome::KEY),
            ("LINK", ForkAwesome::LINK),
            ("UNLINK", ForkAwesome::UNLINK),
            ("HAND_PAPER", ForkAwesome::HAND_PAPER),
            ("HAND_POINTER", ForkAwesome::HAND_POINTER),
            ("HAND_ROCK", ForkAwesome::HAND_ROCK),
            ("HAND_SCISSORS", ForkAwesome::HAND_SCISSORS),
            ("HAND_SPOCK", ForkAwesome::HAND_SPOCK),
            ("WRENCH", ForkAwesome::WRENCH),
            ("HAMMER", ForkAwesome::HAMMER),
            ("MAGIC", ForkAwesome::MAGIC),
            ("PAINT_BRUSH", ForkAwesome::PAINT_BRUSH),
            ("PENCIL_SQUARE", ForkAwesome::PENCIL_SQUARE),
            ("RULER_COMBINED", ForkAwesome::RULER_COMBINED),
            ("RULER_HORIZONTAL", ForkAwesome::RULER_HORIZONTAL),
            ("RULER_VERTICAL", ForkAwesome::RULER_VERTICAL),
            ("SORT", ForkAwesome::SORT),
            ("SORT_UP", ForkAwesome::SORT_UP),
            ("SORT_DOWN", ForkAwesome::SORT_DOWN),
            ("BARS", ForkAwesome::BARS),
            ("TH", ForkAwesome::TH),
            ("TH_LARGE", ForkAwesome::TH_LARGE),
            ("TH_LIST", ForkAwesome::TH_LIST),
            ("LIST", ForkAwesome::LIST),
            ("LIST_OL", ForkAwesome::LIST_OL),
            ("LIST_UL", ForkAwesome::LIST_UL),
            ("COLUMNS", ForkAwesome::COLUMNS),
            ("TABLE", ForkAwesome::TABLE),
            ("WINDOW_MAXIMIZE", ForkAwesome::WINDOW_MAXIMIZE),
            ("WINDOW_MINIMIZE", ForkAwesome::WINDOW_MINIMIZE),
            ("WINDOW_RESTORE", ForkAwesome::WINDOW_RESTORE),
            ("WINDOW_CLOSE", ForkAwesome::WINDOW_CLOSE),
            ("STAR", ForkAwesome::STAR),
            ("STAR_OUTLINE", ForkAwesome::STAR_OUTLINE),
            ("HEART", ForkAwesome::HEART),
            ("BOOKMARK", ForkAwesome::BOOKMARK),
            ("TAG", ForkAwesome::TAG),
            ("TAGS", ForkAwesome::TAGS),
            ("FLAG", ForkAwesome::FLAG),
            ("CIRCLE", ForkAwesome::CIRCLE),
            ("CIRCLE_OUTLINE", ForkAwesome::CIRCLE_OUTLINE),
            ("SQUARE", ForkAwesome::SQUARE),
            ("SQUARE_OUTLINE", ForkAwesome::SQUARE_OUTLINE),
        ]
    }

    #[test]
    fn test_all_codepoints_in_valid_range() {
        for (name, cp) in all_icon_constants() {
            let code = cp as u32;
            assert!(
                (0xF000..=0xF8FF).contains(&code),
                "{name} = 0x{code:04X} is outside ForkAwesome PUA range F000-F8FF"
            );
        }
    }

    #[test]
    fn test_no_duplicate_codepoints() {
        let icons = all_icon_constants();
        for i in 0..icons.len() {
            for j in (i + 1)..icons.len() {
                assert_ne!(
                    icons[i].1, icons[j].1,
                    "{} and {} share the same codepoint 0x{:04X}",
                    icons[i].0, icons[j].0, icons[i].1 as u32
                );
            }
        }
    }

    #[test]
    fn test_common_icons_non_empty() {
        assert!(!ForkAwesome::common_icons().is_empty());
    }

    #[test]
    fn test_common_icons_are_valid() {
        for &cp in ForkAwesome::common_icons() {
            assert!(
                cp.len_utf8() > 0,
                "Codepoint 0x{:04X} is not a valid char",
                cp as u32
            );
        }
    }
}
