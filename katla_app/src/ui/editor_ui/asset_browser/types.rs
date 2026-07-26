use std::path::PathBuf;

use katla_gfx::TextureHandle;
use katla_math::Color;

use crate::ui::ColorScheme;

/// Asset type classification for icons and filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetType {
    /// 3D model files (.glb, .gltf)
    Model,
    /// Material definitions (.toml)
    Material,
    /// Shader source (.wgsl)
    Shader,
    /// Script files (.luau, .lua)
    Script,
    /// Image files (.png, .jpg)
    Image,
    /// Font files (.ttf, .otf)
    Font,
    /// Audio files (.wav, .ogg, .mp3, .flac)
    Audio,
    /// Directory
    Folder,
    /// Unknown/other file type
    Unknown,
}

/// Thumbnail loading state for an asset.
#[derive(Debug, Clone, Default)]
pub enum ThumbnailState {
    /// Thumbnail not yet loaded or requested.
    #[default]
    Pending,
    /// Currently loading in background thread.
    Loading,
    /// Loaded and uploaded to GPU.
    Loaded { texture_handle: TextureHandle },
    /// Failed to load.
    Failed,
}

/// Single asset entry in the browser.
#[derive(Debug, Clone)]
pub struct AssetEntry {
    /// Display name (filename without path)
    pub name: String,
    /// Full filesystem path
    pub path: PathBuf,
    /// Asset type classification
    pub asset_type: AssetType,
    /// Thumbnail loading state (for images)
    pub thumbnail_state: ThumbnailState,
}

/// Action from the asset browser context menu.
#[derive(Debug, Clone)]
pub enum AssetAction {
    /// Request model preview (double-click on model file)
    ModelPreviewRequested(PathBuf),
    /// Copy path to clipboard
    CopyPath(PathBuf),
    /// Show in Explorer/Finder
    ShowInExplorer(PathBuf),
    /// Delete asset
    Delete(PathBuf),
    /// Create new folder
    CreateFolder(PathBuf),
    /// Toggle audio preview play/stop
    AudioPreviewToggle { path: PathBuf },
}

impl AssetType {
    /// Determine asset type from file extension.
    pub fn from_path(path: &std::path::Path) -> Self {
        if path.is_dir() {
            return Self::Folder;
        }

        match path.extension().and_then(|e| e.to_str()) {
            Some("glb") | Some("gltf") | Some("stl") => Self::Model,
            Some("toml") => Self::Material,
            Some("wgsl") => Self::Shader,
            Some("luau") | Some("lua") => Self::Script,
            Some("png") | Some("jpg") | Some("jpeg") => Self::Image,
            Some("ttf") | Some("otf") => Self::Font,
            Some("wav") | Some("ogg") | Some("mp3") | Some("flac") => Self::Audio,
            _ => Self::Unknown,
        }
    }

    /// Get the ForkAwesome icon for this asset type.
    pub fn icon(&self) -> char {
        use katla_ui::ForkAwesome;

        match self {
            Self::Model => ForkAwesome::CUBE,
            Self::Material => ForkAwesome::PAINT_BRUSH,
            Self::Shader => ForkAwesome::FILE_CODE,
            Self::Script => ForkAwesome::COG,
            Self::Image => ForkAwesome::IMAGE,
            Self::Font => ForkAwesome::FILE_TEXT,
            Self::Audio => ForkAwesome::MUSIC,
            Self::Folder => ForkAwesome::FOLDER,
            Self::Unknown => ForkAwesome::FILE,
        }
    }

    /// Get icon color for this asset type.
    pub fn color(&self, theme: &ColorScheme) -> Color {
        match self {
            Self::Model => theme.entity_mesh,
            Self::Material => theme.text_accent,
            Self::Shader => theme.info,
            Self::Script => theme.success,
            Self::Image => theme.warning,
            Self::Font => theme.text_secondary,
            Self::Audio => Color::rgb(0.4, 0.6, 0.9),
            Self::Folder => theme.text_secondary,
            Self::Unknown => theme.text_muted,
        }
    }
}
