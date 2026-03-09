//! UI rendering conversion layer.
//!
//! This module provides the bridge between `katla_ui` and `katla_gfx`:
//! - Maps `TextureId` to `TextureHandle`
//! - Converts `katla_ui::DrawList` to `katla_gfx::UIDrawList`

use std::collections::HashMap;

use katla_gfx::{TextureHandle, UIDrawList, UiDrawCommand, VertexUI};
use katla_ui::{DrawList, TextureId};

/// UI renderer that converts UI draw lists for GPU rendering.
///
/// This struct maintains a registry mapping `TextureId` (from katla_ui)
/// to `TextureHandle` (from katla_gfx), allowing the UI layer to reference
/// textures without knowing about GPU handles.
pub struct UIRenderer {
    /// Maps UI texture IDs to GPU texture handles.
    texture_registry: HashMap<TextureId, TextureHandle>,
    /// Font atlas texture handle.
    font_atlas: Option<TextureHandle>,
}

impl UIRenderer {
    /// Create a new UI renderer.
    pub fn new() -> Self {
        Self {
            texture_registry: HashMap::new(),
            font_atlas: None,
        }
    }

    /// Register a texture handle for a UI texture ID.
    ///
    /// Returns the previous handle if one was already registered.
    pub fn register_texture(
        &mut self,
        id: TextureId,
        handle: TextureHandle,
    ) -> Option<TextureHandle> {
        self.texture_registry.insert(id, handle)
    }

    /// Unregister a texture by ID.
    pub fn unregister_texture(&mut self, id: TextureId) -> Option<TextureHandle> {
        self.texture_registry.remove(&id)
    }

    /// Set the font atlas texture handle.
    ///
    /// This is a convenience method since the font atlas is frequently accessed.
    pub fn set_font_atlas(&mut self, handle: TextureHandle) {
        self.font_atlas = Some(handle);
    }

    /// Get the font atlas texture handle.
    pub fn font_atlas(&self) -> Option<TextureHandle> {
        self.font_atlas
    }

    /// Resolve a texture ID to a GPU handle.
    ///
    /// Falls back to `TextureHandle::NONE` if the texture is not registered.
    pub fn resolve_texture(&self, id: TextureId) -> TextureHandle {
        // Check font atlas first (most common case)
        if id == TextureId::FONT_ATLAS {
            return self.font_atlas.unwrap_or(TextureHandle::NONE);
        }

        // Check the registry
        self.texture_registry
            .get(&id)
            .copied()
            .unwrap_or(TextureHandle::NONE)
    }

    /// Convert a `katla_ui::DrawList` to a `katla_gfx::UIDrawList`.
    ///
    /// This method:
    /// 1. Resolves all texture IDs to GPU handles
    /// 2. Converts vertex data to GPU format
    /// 3. Copies index and command data
    ///
    /// # Arguments
    ///
    /// * `draw_list` - The UI draw list from katla_ui
    ///
    /// # Returns
    ///
    /// A `UIDrawList` ready for GPU submission.
    pub fn convert_draw_list(&self, draw_list: &DrawList) -> UIDrawList {
        // Convert vertices
        let vertices: Vec<VertexUI> = draw_list
            .vertices
            .iter()
            .map(|v| VertexUI::new([v.pos.x(), v.pos.y()], [v.uv.x(), v.uv.y()], v.color))
            .collect();

        // Copy indices directly
        let indices = draw_list.indices.clone();

        // Convert commands, resolving texture IDs
        let commands: Vec<UiDrawCommand> = draw_list
            .commands
            .iter()
            .map(|cmd| {
                UiDrawCommand::new(
                    cmd.index_offset,
                    cmd.index_count,
                    cmd.clip_rect,
                    self.resolve_texture(cmd.texture),
                )
            })
            .collect();

        UIDrawList {
            vertices,
            indices,
            commands,
        }
    }

    /// Clear all registered textures.
    pub fn clear(&mut self) {
        self.texture_registry.clear();
        self.font_atlas = None;
    }
}

impl Default for UIRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_renderer_new() {
        let renderer = UIRenderer::new();
        assert!(renderer.texture_registry.is_empty());
        assert!(renderer.font_atlas.is_none());
    }

    #[test]
    fn test_resolve_unregistered_texture() {
        let renderer = UIRenderer::new();
        let id = TextureId::new(999);

        // Should return NONE for unregistered texture
        assert_eq!(renderer.resolve_texture(id), TextureHandle::NONE);
    }

    #[test]
    fn test_resolve_font_atlas_when_not_set() {
        let renderer = UIRenderer::new();

        // FONT_ATLAS ID should return NONE when not set
        assert_eq!(
            renderer.resolve_texture(TextureId::FONT_ATLAS),
            TextureHandle::NONE
        );
    }

    #[test]
    fn test_convert_empty_draw_list() {
        let renderer = UIRenderer::new();
        let draw_list = DrawList::new();

        let gpu_list = renderer.convert_draw_list(&draw_list);

        assert!(gpu_list.vertices.is_empty());
        assert!(gpu_list.indices.is_empty());
        assert!(gpu_list.commands.is_empty());
    }

    #[test]
    fn test_convert_draw_list_with_vertices() {
        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Add a simple rect
        use katla_math::{Color, Rect2D, Vec2};
        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::RED,
        );
        draw_list.finalize();

        let gpu_list = renderer.convert_draw_list(&draw_list);

        // Should have 4 vertices and 6 indices (2 triangles)
        assert_eq!(gpu_list.vertex_count(), 4);
        assert_eq!(gpu_list.index_count(), 6);
        assert_eq!(gpu_list.command_count(), 1);
    }
}
