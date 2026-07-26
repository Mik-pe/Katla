//! Asset Browser panel for browsing Resources folder.
//!
//! Provides a scrollable view of assets with:
//! - Grid layout with type icons
//! - Folder navigation
//! - PNG image thumbnail support (loaded in background)
//! - Auto-refresh on file changes

mod state;
mod types;

pub use state::AssetBrowserState;
pub use types::{AssetAction, AssetType, ThumbnailState};
pub(crate) use types::AssetEntry;
