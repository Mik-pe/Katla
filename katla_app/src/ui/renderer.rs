//! UI rendering conversion layer.
//!
//! This module provides the bridge between `katla_ui` and `katla_gfx`:
//! - Maps `TextureId` to `TextureHandle`
//! - Converts `katla_ui::DrawList` to `katla_gfx::UIDrawList`

use ash::vk;
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
    /// Font atlas bindless texture slot index.
    /// This is the slot allocated by the bindless system for the font atlas.
    font_atlas_bindless_slot: Option<u32>,
    /// White texture bindless slot index for solid color rendering.
    /// This is the default white texture from the bindless system (slot 0).
    white_texture_bindless_slot: Option<u32>,
    /// Maps bindless indices to transient texture resources (for viewport rendering).
    /// Stores (image_view, sampler) tuples for textures not in the texture manager.
    transient_textures: HashMap<u32, (vk::ImageView, vk::Sampler)>,
    /// Maps TextureHandle indices to their bindless texture slots.
    /// This allows us to look up the bindless index for thumbnails and other textures.
    bindless_slots: HashMap<u32, u32>,
}

impl UIRenderer {
    /// Create a new UI renderer.
    pub fn new() -> Self {
        Self {
            texture_registry: HashMap::new(),
            font_atlas: None,
            font_atlas_bindless_slot: None,
            white_texture_bindless_slot: Some(0), // Default white texture is always at slot 0
            transient_textures: HashMap::new(),
            bindless_slots: HashMap::new(),
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

    /// Set the font atlas bindless texture slot.
    ///
    /// This stores the bindless slot index allocated for the font atlas texture.
    pub fn set_font_atlas_bindless_slot(&mut self, slot: u32) {
        log::debug!("UIRenderer: Setting font atlas bindless slot to {}", slot);
        self.font_atlas_bindless_slot = Some(slot);
    }

    /// Get the font atlas bindless texture slot.
    ///
    /// Returns None if the font atlas has not been registered with the bindless system.
    pub fn font_atlas_bindless_slot(&self) -> Option<u32> {
        self.font_atlas_bindless_slot
    }

    /// Set the white texture bindless slot for solid color rendering.
    ///
    /// The default white texture is used for rendering solid color rectangles
    /// instead of sampling from the font atlas.
    pub fn set_white_texture_bindless_slot(&mut self, slot: u32) {
        self.white_texture_bindless_slot = Some(slot);
    }

    /// Get the white texture bindless slot.
    pub fn white_texture_bindless_slot(&self) -> Option<u32> {
        self.white_texture_bindless_slot
    }

    /// Get the font atlas texture handle.
    pub fn font_atlas(&self) -> Option<TextureHandle> {
        self.font_atlas
    }

    /// Register a bindless slot for a texture handle.
    ///
    /// This tracks which bindless slot a texture was registered to,
    /// allowing lookup by handle index later.
    pub fn register_bindless_slot(&mut self, handle: TextureHandle, slot: u32) {
        self.bindless_slots.insert(handle.index(), slot);
    }

    /// Get the bindless slot for a texture handle.
    ///
    /// Returns None if the texture hasn't been registered with bindless.
    pub fn get_bindless_slot(&self, handle: TextureHandle) -> Option<u32> {
        self.bindless_slots.get(&handle.index()).copied()
    }

    /// Resolve a texture ID to a GPU handle.
    ///
    /// Falls back to `TextureHandle::NONE` if the texture is not registered.
    /// Supports bindless texture IDs (encoded with high bit set).
    pub fn resolve_texture(&self, id: TextureId) -> TextureHandle {
        const BINDLESS_FLAG: u64 = 1 << 63;
        const BINDLESS_OFFSET: u32 = 1000; // Bindless indices start at 1000

        // Check if this is a bindless texture (high bit set)
        if id.0 & BINDLESS_FLAG != 0 {
            // Extract the bindless index and encode it with an offset
            // This distinguishes bindless textures from regular texture handles
            let bindless_index = (id.0 & !BINDLESS_FLAG) as u32;
            return TextureHandle::new(BINDLESS_OFFSET + bindless_index);
        }

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
    /// 1. Resolves all texture IDs to bindless texture indices
    /// 2. Converts vertex data to GPU format with texture indices
    /// 3. Copies index and command data
    ///
    /// # Arguments
    ///
    /// * `draw_list` - The UI draw list from katla_ui
    /// * `screen_size` - Screen size for coordinate transformation (logical pixels)
    /// * `scale_factor` - DPI scale factor (physical pixels per logical pixel)
    ///
    /// # Returns
    ///
    /// A `UIDrawList` ready for GPU submission.
    pub fn convert_draw_list(
        &self,
        draw_list: &DrawList,
        screen_size: [f32; 2],
        scale_factor: f32,
    ) -> UIDrawList {
        // Build a map from TextureId to bindless index for this frame
        let mut texture_to_index: HashMap<TextureId, u32> = HashMap::new();

        // First pass: build texture mapping
        for cmd in draw_list.commands() {
            texture_to_index
                .entry(cmd.texture)
                .or_insert_with(|| self.texture_id_to_bindless_index(cmd.texture));
        }

        // Create a mapping from vertex index to texture index
        let mut vertex_texture_indices: Vec<u32> = vec![0; draw_list.vertices().len()];

        // Assign texture indices to vertices based on which commands use them
        for cmd in draw_list.commands() {
            let bindless_index = texture_to_index.get(&cmd.texture).copied().unwrap_or(0);
            let index_start = cmd.index_offset as usize;
            let index_end = index_start + cmd.index_count as usize;

            // Mark all vertices referenced by this command with its texture index
            for i in index_start..index_end {
                if i < draw_list.indices().len() {
                    let vertex_idx = draw_list.indices()[i] as usize;
                    if vertex_idx < vertex_texture_indices.len() {
                        vertex_texture_indices[vertex_idx] = bindless_index;
                    }
                }
            }
        }

        // Now convert vertices with their texture indices
        let vertices: Vec<VertexUI> = draw_list
            .vertices()
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let tex_index = vertex_texture_indices.get(i).copied().unwrap_or(0);
                VertexUI::new(
                    [v.pos.x(), v.pos.y()],
                    [v.uv.x(), v.uv.y()],
                    v.color,
                    tex_index,
                )
            })
            .collect();

        // Copy indices directly
        let indices = draw_list.indices().to_vec();

        // Convert commands, resolving texture IDs to bindless indices (for validation)
        let commands: Vec<UiDrawCommand> = draw_list
            .commands()
            .iter()
            .map(|cmd| {
                let bindless_index = texture_to_index.get(&cmd.texture).copied().unwrap_or(0);
                UiDrawCommand::new(
                    cmd.index_offset,
                    cmd.index_count,
                    cmd.clip_rect,
                    TextureHandle::new(bindless_index),
                )
            })
            .collect();

        UIDrawList {
            vertices,
            indices,
            commands,
            screen_size,
            scale_factor,
        }
    }

    /// Convert a TextureId to a bindless texture index.
    ///
    /// This maps the texture ID to the corresponding bindless slot index.
    fn texture_id_to_bindless_index(&self, id: TextureId) -> u32 {
        const BINDLESS_FLAG: u64 = 1 << 63;

        // Check if this is already a bindless texture (high bit set)
        if id.0 & BINDLESS_FLAG != 0 {
            // Extract the bindless index
            let bindless_index = (id.0 & !BINDLESS_FLAG) as u32;
            log::trace!(
                "TextureId {} is bindless texture at slot {}",
                id.0,
                bindless_index
            );
            return bindless_index;
        }

        // Check font atlas - use the registered bindless slot
        if id == TextureId::FONT_ATLAS {
            let slot = self.font_atlas_bindless_slot.unwrap_or(0);
            if self.font_atlas_bindless_slot.is_none() {
                log::error!("Font atlas bindless slot is None! Text will sample from white texture (slot 0) instead.");
            }
            return slot;
        }

        // For TextureId::NONE, use white texture slot for solid color rendering
        if id == TextureId::NONE {
            return self.white_texture_bindless_slot.unwrap_or(0);
        }

        // Look up in registry - get the handle and then look up its bindless slot
        if let Some(handle) = self.texture_registry.get(&id) {
            // Try to get the bindless slot from our tracking map
            if let Some(slot) = self.get_bindless_slot(*handle) {
                return slot;
            }
            // Fallback to white texture slot if not found
            log::warn!(
                "TextureId {} found in registry but no bindless slot tracked, falling back to white texture",
                id.0
            );
            self.white_texture_bindless_slot.unwrap_or(0)
        } else {
            // Fallback to white texture slot for unknown textures
            log::warn!(
                "TextureId {} not in registry, falling back to white texture slot 0",
                id.0
            );
            self.white_texture_bindless_slot.unwrap_or(0)
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

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        assert!(gpu_list.vertices.is_empty());
        assert!(gpu_list.indices.is_empty());
        assert!(gpu_list.commands.is_empty());
        assert_eq!(gpu_list.screen_size, [1920.0, 1080.0]);
        assert_eq!(gpu_list.scale_factor, 1.0);
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

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        // Should have 4 vertices and 6 indices (2 triangles)
        assert_eq!(gpu_list.vertex_count(), 4);
        assert_eq!(gpu_list.index_count(), 6);
        assert_eq!(gpu_list.command_count(), 1);
        assert_eq!(gpu_list.screen_size, [1920.0, 1080.0]);
        assert_eq!(gpu_list.scale_factor, 1.0);
    }

    #[test]
    fn test_hidpi_scale_factor_in_draw_list() {
        // Test that scale_factor is included in UIDrawList for HiDPI coordinate conversion
        use katla_math::{Color, Rect2D, Vec2};
        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Add a rect with clipping
        draw_list.set_clip(Rect2D::from_origin_size(
            Vec2::new(10.0, 20.0),
            Vec2::new(100.0, 50.0),
        ));
        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::RED,
        );
        draw_list.finalize();

        // Convert with a 2.0 scale factor (HiDPI display)
        let scale_factor = 2.0;
        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], scale_factor);

        // Verify scale_factor is stored
        assert_eq!(gpu_list.scale_factor, scale_factor);

        // Verify clip_rect is in logical pixels (not scaled yet)
        // The actual scaling happens in execute_ui_draw_list when setting Vulkan scissor
        let clip_rect = gpu_list.commands[0].clip_rect;
        assert_eq!(clip_rect, Some([10.0, 20.0, 100.0, 50.0]));
    }

    // ========================================================================
    // VAL-POS-001: Element Positioning Accuracy Tests
    // ========================================================================

    #[test]
    fn test_vertex_positions_match_requested_bounds() {
        // VAL-POS-001: Vertex positions in the draw list must match the requested bounds
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Draw a rect at specific logical coordinates
        let bounds = Rect2D::from_origin_size(Vec2::new(50.0, 75.0), Vec2::new(100.0, 40.0));
        draw_list.add_rect(bounds, Color::BLUE);
        draw_list.finalize();

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        // Verify that vertex positions match the requested bounds
        // Rect has 4 vertices for the corners
        assert_eq!(gpu_list.vertices.len(), 4);

        // Check that all vertex positions are within or on the bounds
        for vertex in &gpu_list.vertices {
            let x = vertex.position[0];
            let y = vertex.position[1];
            assert!(
                x >= bounds.min.x() && x <= bounds.max.x(),
                "Vertex x={} is outside bounds x=[{}, {}]",
                x,
                bounds.min.x(),
                bounds.max.x()
            );
            assert!(
                y >= bounds.min.y() && y <= bounds.max.y(),
                "Vertex y={} is outside bounds y=[{}, {}]",
                y,
                bounds.min.y(),
                bounds.max.y()
            );
        }

        // Verify that we have vertices at the exact corners
        let positions: Vec<_> = gpu_list
            .vertices
            .iter()
            .map(|v| (v.position[0], v.position[1]))
            .collect();
        assert!(
            positions.contains(&(50.0, 75.0)), // Top-left
            "Missing top-left corner vertex"
        );
        assert!(
            positions.contains(&(150.0, 75.0)), // Top-right (50 + 100)
            "Missing top-right corner vertex"
        );
        assert!(
            positions.contains(&(50.0, 115.0)), // Bottom-left (75 + 40)
            "Missing bottom-left corner vertex"
        );
        assert!(
            positions.contains(&(150.0, 115.0)), // Bottom-right
            "Missing bottom-right corner vertex"
        );
    }

    #[test]
    fn test_multiple_rects_preserve_positions() {
        // VAL-POS-001: Multiple elements must appear at their specified coordinates
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Draw multiple rects at different positions
        let rect1 = Rect2D::from_origin_size(Vec2::new(10.0, 20.0), Vec2::new(50.0, 30.0));
        let rect2 = Rect2D::from_origin_size(Vec2::new(100.0, 200.0), Vec2::new(40.0, 60.0));
        let rect3 = Rect2D::from_origin_size(Vec2::new(500.0, 300.0), Vec2::new(80.0, 40.0));

        draw_list.add_rect(rect1, Color::RED);
        draw_list.add_rect(rect2, Color::GREEN);
        draw_list.add_rect(rect3, Color::BLUE);
        draw_list.finalize();

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        // Should have 12 vertices (4 per rect)
        assert_eq!(gpu_list.vertices.len(), 12);

        // Verify vertices are in correct locations
        let mut rect1_count = 0;
        let mut rect2_count = 0;
        let mut rect3_count = 0;

        for vertex in &gpu_list.vertices {
            let x = vertex.position[0];
            let y = vertex.position[1];

            if rect1.contains(Vec2::new(x, y)) {
                rect1_count += 1;
            } else if rect2.contains(Vec2::new(x, y)) {
                rect2_count += 1;
            } else if rect3.contains(Vec2::new(x, y)) {
                rect3_count += 1;
            } else {
                panic!(
                    "Vertex at ({}, {}) is not in any of the expected rects",
                    x, y
                );
            }
        }

        assert_eq!(rect1_count, 4, "Rect 1 should have 4 vertices");
        assert_eq!(rect2_count, 4, "Rect 2 should have 4 vertices");
        assert_eq!(rect3_count, 4, "Rect 3 should have 4 vertices");
    }

    // ========================================================================
    // VAL-POS-003: Coordinate Transformation Correctness Tests
    // ========================================================================

    #[test]
    fn test_logical_to_physical_coordinate_conversion() {
        // VAL-POS-003: VertexUI conversion preserves positions
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Create vertices at known logical coordinates
        let bounds = Rect2D::from_origin_size(Vec2::new(100.0, 150.0), Vec2::new(50.0, 60.0));
        draw_list.add_rect(bounds, Color::YELLOW);
        draw_list.finalize();

        // Test with scale factor 1.0 (logical == physical)
        let gpu_list_1x = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        // Vertex positions should be preserved exactly
        assert_eq!(gpu_list_1x.vertices[0].position[0], 100.0);
        assert_eq!(gpu_list_1x.vertices[0].position[1], 150.0);

        // Test with scale factor 2.0 (HiDPI)
        let gpu_list_2x = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 2.0);

        // Logical positions should STILL be the same in VertexUI
        // The scaling to physical happens in the shader: physical = logical * scale_factor
        assert_eq!(gpu_list_2x.vertices[0].position[0], 100.0);
        assert_eq!(gpu_list_2x.vertices[0].position[1], 150.0);

        // But the scale_factor field should be set correctly
        assert_eq!(gpu_list_2x.scale_factor, 2.0);

        // screen_size should also be in logical pixels
        assert_eq!(gpu_list_1x.screen_size, [1920.0, 1080.0]);
        assert_eq!(gpu_list_2x.screen_size, [1920.0, 1080.0]);
    }

    #[test]
    fn test_scale_factor_application_preserves_spatial_relationships() {
        // VAL-POS-003: Coordinate transformation must preserve spatial relationships
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Create two rects with a specific spatial relationship
        let rect1 = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0));
        let rect2 = Rect2D::from_origin_size(Vec2::new(150.0, 75.0), Vec2::new(100.0, 50.0));

        // The horizontal distance between rects is 150 - 100 = 50 pixels
        // The vertical distance is 75 - 50 = 25 pixels
        let expected_horizontal_gap = 50.0;
        let expected_vertical_gap = 25.0;

        draw_list.add_rect(rect1, Color::RED);
        draw_list.add_rect(rect2, Color::GREEN);
        draw_list.finalize();

        // Test with different scale factors
        for scale_factor in [1.0, 1.5, 2.0, 2.5] {
            let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], scale_factor);

            // Calculate the actual gap from vertex positions
            // Find the rightmost vertex of rect1 and leftmost of rect2
            let mut rect1_max_x: f32 = 0.0;
            let mut rect2_min_x: f32 = f32::MAX;

            for vertex in &gpu_list.vertices {
                let x = vertex.position[0];

                // Classify vertex by position
                if rect1.contains(Vec2::new(x, vertex.position[1])) {
                    rect1_max_x = rect1_max_x.max(x);
                } else if rect2.contains(Vec2::new(x, vertex.position[1])) {
                    rect2_min_x = rect2_min_x.min(x);
                }
            }

            let actual_horizontal_gap = rect2_min_x - rect1_max_x;

            // The gap in logical pixels should be preserved regardless of scale_factor
            assert!(
                (actual_horizontal_gap - expected_horizontal_gap).abs() < 0.01,
                "Scale factor {}: Expected horizontal gap {}, got {}",
                scale_factor,
                expected_horizontal_gap,
                actual_horizontal_gap
            );
        }
    }

    #[test]
    fn test_vertex_color_preservation_through_conversion() {
        // VAL-POS-003: Vertex colors must be preserved through conversion
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Draw rects with different colors
        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::new(1.0, 0.0, 0.0, 1.0), // Red
        );
        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(20.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::new(0.0, 1.0, 0.0, 0.5), // Green with 50% alpha
        );
        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(40.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::new(0.0, 0.0, 1.0, 0.25), // Blue with 25% alpha
        );
        draw_list.finalize();

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        // Verify colors are preserved
        // Color is stored as [u8; 4], so we need to check the byte values
        // Color::new(1.0, 0.0, 0.0, 1.0) should become [255, 0, 0, 255]
        let red_vertex = &gpu_list.vertices[0];
        assert_eq!(red_vertex.color, [255, 0, 0, 255]);

        // Green with 50% alpha: [0, 255, 0, 128]
        let green_vertex = &gpu_list.vertices[4];
        assert_eq!(green_vertex.color, [0, 255, 0, 128]);

        // Blue with 25% alpha: [0, 0, 255, 64]
        let blue_vertex = &gpu_list.vertices[8];
        assert_eq!(blue_vertex.color, [0, 0, 255, 64]);
    }

    #[test]
    fn test_uv_coordinates_preserved_through_conversion() {
        // VAL-POS-003: UV coordinates must be preserved through conversion
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // add_rect creates vertices with UV=(0, 0) for solid color rendering
        let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0));
        draw_list.add_rect(bounds, Color::WHITE);
        draw_list.finalize();

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        // All vertices should have UV=(0, 0) for solid color rendering
        for vertex in &gpu_list.vertices {
            assert_eq!(vertex.uv, [0.0, 0.0], "UV should be (0, 0) for solid color");
        }
    }

    // ========================================================================
    // FLOW-001: HiDPI Text Rendering Pipeline Tests
    // ========================================================================

    #[test]
    fn test_hidpi_pipeline_scale_factor_storage() {
        // FLOW-001: scale_factor flows through the rendering pipeline
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::WHITE,
        );
        draw_list.finalize();

        // Simulate different DPI scenarios
        let test_cases = [
            (1.0, "Standard DPI (96 DPI)"),
            (1.25, "125% DPI scaling"),
            (1.5, "150% DPI scaling (common Mac)"),
            (2.0, "200% DPI scaling (Retina)"),
            (2.5, "250% DPI scaling (high-DPI Windows)"),
            (3.0, "300% DPI scaling (4K)"),
        ];

        for (scale_factor, description) in test_cases {
            let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], scale_factor);

            assert_eq!(
                gpu_list.scale_factor, scale_factor,
                "{}: scale_factor should be preserved",
                description
            );

            // Vertex positions should still be in logical pixels
            // The shader will apply the scale: physical = logical * scale_factor
            for vertex in &gpu_list.vertices {
                assert!(
                    vertex.position[0] <= 1920.0,
                    "{}: Vertex x should be in logical pixels",
                    description
                );
                assert!(
                    vertex.position[1] <= 1080.0,
                    "{}: Vertex y should be in logical pixels",
                    description
                );
            }
        }
    }

    #[test]
    fn test_clip_rect_logical_to_physical_scaling() {
        // FLOW-001: clip_rect is in logical pixels, scaled to physical for Vulkan scissor
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Set a clip rect in logical pixels
        let clip_rect = Rect2D::from_origin_size(Vec2::new(50.0, 100.0), Vec2::new(200.0, 150.0));
        draw_list.set_clip(clip_rect);

        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(300.0, 300.0)),
            Color::WHITE,
        );
        draw_list.finalize();

        // Convert with scale factor 2.0
        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 2.0);

        // clip_rect should be in logical pixels (not scaled in convert_draw_list)
        // The actual scaling to physical happens in execute_ui_draw_list when setting Vulkan scissor
        let cmd = &gpu_list.commands[0];
        assert_eq!(
            cmd.clip_rect,
            Some([50.0, 100.0, 200.0, 150.0]),
            "clip_rect should remain in logical pixels"
        );

        // But scale_factor should be stored so execute_ui_draw_list can scale it
        assert_eq!(gpu_list.scale_factor, 2.0);

        // Verify the expected physical scissor would be:
        // x: 50.0 * 2.0 = 100
        // y: 100.0 * 2.0 = 200
        // width: 200.0 * 2.0 = 400
        // height: 150.0 * 2.0 = 300
        let expected_physical = [100.0, 200.0, 400.0, 300.0];

        // This scaling happens in execute_ui_draw_list in katla_gfx
        // We're documenting the expected behavior here
        if let Some([x, y, w, h]) = cmd.clip_rect {
            let scaled_x = x * gpu_list.scale_factor;
            let scaled_y = y * gpu_list.scale_factor;
            let scaled_w = w * gpu_list.scale_factor;
            let scaled_h = h * gpu_list.scale_factor;

            assert_eq!(
                [scaled_x, scaled_y, scaled_w, scaled_h],
                expected_physical,
                "Physical scissor should be logical * scale_factor"
            );
        }
    }

    #[test]
    fn test_coordinate_system_consistency_across_scales() {
        // FLOW-001: Metrics are consistent (logical units throughout)
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Draw a 100x50 rect at position (50, 75)
        let bounds = Rect2D::from_origin_size(Vec2::new(50.0, 75.0), Vec2::new(100.0, 50.0));
        draw_list.add_rect(bounds, Color::WHITE);
        draw_list.finalize();

        // Test that the logical coordinate system is consistent across different scale factors
        let scale_factors = [1.0, 1.5, 2.0, 3.0];
        let mut expected_vertices = None;

        for scale_factor in scale_factors {
            let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], scale_factor);

            // Extract vertex positions
            let vertices: Vec<_> = gpu_list
                .vertices
                .iter()
                .map(|v| (v.position[0], v.position[1]))
                .collect();

            // First iteration: store expected positions
            if let Some(ref expected) = expected_vertices {
                // Subsequent iterations: verify positions match
                assert_eq!(
                    vertices, *expected,
                    "Vertex positions should be identical for scale_factor={}",
                    scale_factor
                );
            } else {
                // Store the first set as expected
                expected_vertices = Some(vertices);
            }

            // Verify scale_factor is stored correctly
            assert_eq!(gpu_list.scale_factor, scale_factor);
        }

        // Verify we actually tested something
        assert!(expected_vertices.is_some());
    }

    #[test]
    fn test_screen_size_in_logical_pixels() {
        // FLOW-001: UIDrawList.screen_size must be in logical pixels
        use katla_math::{Color, Rect2D, Vec2};

        let renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::WHITE,
        );
        draw_list.finalize();

        // Simulate a 1920x1080 physical display at 1.5x scaling
        // Logical size would be 1280x720 (physical / scale_factor)
        let physical_width = 1920.0;
        let physical_height = 1080.0;
        let scale_factor = 1.5;
        let logical_width = physical_width / scale_factor; // 1280
        let logical_height = physical_height / scale_factor; // 720

        let gpu_list =
            renderer.convert_draw_list(&draw_list, [logical_width, logical_height], scale_factor);

        // screen_size should be in logical pixels
        assert_eq!(
            gpu_list.screen_size,
            [logical_width, logical_height],
            "screen_size should be in logical pixels"
        );

        // scale_factor should be stored
        assert_eq!(gpu_list.scale_factor, scale_factor);

        // Verify screen_size makes sense for the shader transformation
        // The shader converts: physical = logical * scale_factor
        // So screen_size * scale_factor should give us the physical resolution
        let expected_physical_width = logical_width * scale_factor;
        let expected_physical_height = logical_height * scale_factor;

        assert_eq!(
            expected_physical_width, physical_width,
            "Logical width * scale should equal physical width"
        );
        assert_eq!(
            expected_physical_height, physical_height,
            "Logical height * scale should equal physical height"
        );
    }

    #[test]
    fn test_bindless_texture_id_decoding() {
        // Test that bindless texture IDs (with high bit set) are correctly decoded
        let renderer = UIRenderer::new();

        const BINDLESS_FLAG: u64 = 1 << 63;

        // Create a bindless texture ID with index 42
        let bindless_index = 42u32;
        let texture_id = TextureId::new(BINDLESS_FLAG | (bindless_index as u64));

        // Decode the bindless index
        let decoded_index = renderer.texture_id_to_bindless_index(texture_id);

        assert_eq!(
            decoded_index, bindless_index,
            "Should extract the bindless index from TextureId with high bit set"
        );
    }

    #[test]
    fn test_bindless_texture_id_preserves_index() {
        // Test that different bindless indices are preserved correctly
        let renderer = UIRenderer::new();

        const BINDLESS_FLAG: u64 = 1 << 63;

        // Test various bindless indices
        let test_indices = [0u32, 1, 7, 8, 42, 100, 1000];

        for index in test_indices {
            let texture_id = TextureId::new(BINDLESS_FLAG | (index as u64));
            let decoded = renderer.texture_id_to_bindless_index(texture_id);

            assert_eq!(
                decoded, index,
                "Bindless index {} should be preserved correctly",
                index
            );
        }
    }

    #[test]
    fn test_font_atlas_bindless_slot() {
        // Test that font atlas returns the registered bindless slot
        let mut renderer = UIRenderer::new();

        // Set font atlas bindless slot to 11
        renderer.font_atlas_bindless_slot = Some(11);

        // FONT_ATLAS texture ID should return slot 11
        let font_atlas_index = renderer.texture_id_to_bindless_index(TextureId::FONT_ATLAS);

        assert_eq!(
            font_atlas_index, 11,
            "FONT_ATLAS should return the registered bindless slot"
        );
    }

    #[test]
    fn test_texture_id_none_returns_white_texture_slot() {
        // Test that TextureId::NONE uses white texture slot for solid color rendering
        let renderer = UIRenderer::new();

        // White texture is at slot 0 by default
        let none_index = renderer.texture_id_to_bindless_index(TextureId::NONE);

        assert_eq!(
            none_index, 0,
            "TextureId::NONE should return white texture bindless slot for solid color rendering"
        );
    }

    #[test]
    fn test_texture_id_none_custom_white_slot() {
        // Test that TextureId::NONE respects custom white texture slot
        let mut renderer = UIRenderer::new();

        // Set white texture to a different slot
        renderer.white_texture_bindless_slot = Some(7);

        let none_index = renderer.texture_id_to_bindless_index(TextureId::NONE);

        assert_eq!(
            none_index, 7,
            "TextureId::NONE should return the configured white texture slot"
        );
    }

    #[test]
    fn test_viewport_bindless_texture_mapping() {
        // Test viewport bindless texture IDs are correctly mapped
        let renderer = UIRenderer::new();

        const BINDLESS_FLAG: u64 = 1 << 63;

        // Simulate LDR texture at bindless index 8 (as seen in logs)
        let ldr_bindless_index = 8u32;
        let viewport_texture_id = TextureId::new(BINDLESS_FLAG | (ldr_bindless_index as u64));

        // Verify the texture ID is correctly decoded
        let decoded_index = renderer.texture_id_to_bindless_index(viewport_texture_id);

        assert_eq!(
            decoded_index, ldr_bindless_index,
            "Viewport texture ID should decode to correct bindless index"
        );
    }

    #[test]
    fn test_multi_viewport_bindless_indices() {
        // Test that multiple viewports can have different bindless indices
        let renderer = UIRenderer::new();

        const BINDLESS_FLAG: u64 = 1 << 63;

        // Simulate multiple viewports with different bindless indices
        let viewport_indices = [8u32, 9, 10, 11];

        for (i, &index) in viewport_indices.iter().enumerate() {
            let texture_id = TextureId::new(BINDLESS_FLAG | (index as u64));
            let decoded = renderer.texture_id_to_bindless_index(texture_id);

            assert_eq!(
                decoded, index,
                "Viewport {} should decode to bindless index {}",
                i, index
            );
        }
    }

    #[test]
    fn test_bindless_flag_detection() {
        // Test that the high bit correctly identifies bindless textures
        let renderer = UIRenderer::new();

        const BINDLESS_FLAG: u64 = 1 << 63;

        // Bindless texture ID (high bit set)
        let bindless_id = TextureId::new(BINDLESS_FLAG | 42);
        assert_eq!(renderer.texture_id_to_bindless_index(bindless_id), 42);

        // Regular texture ID (high bit not set) should not be treated as bindless
        // This will fall through to the font atlas or registry lookup
        let regular_id = TextureId::new(42);
        // Without high bit set, it's not a bindless texture
        // The function should handle this gracefully (returns 0 or font atlas slot)
        let regular_decoded = renderer.texture_id_to_bindless_index(regular_id);
        // Should not equal 42 since high bit is not set
        assert_ne!(
            regular_decoded, 42,
            "Regular texture ID without high bit should not decode to bindless index"
        );
    }
}
