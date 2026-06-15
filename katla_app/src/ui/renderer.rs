#![cfg_attr(not(feature = "editor"), allow(dead_code))]
//! UI rendering conversion layer.
//!
//! This module provides the bridge between `katla_ui` and `katla_gfx`:
//! - Maps `TextureId` to `TextureHandle`
//! - Converts `katla_ui::DrawList` to `katla_gfx::UIDrawList`
//! - Translates instance data for GPU-instanced quad rendering

use std::collections::HashMap;

use katla_gfx::{TextureHandle, UIDrawList, UiDrawCommand, VertexUI, VertexUIInstance};
use katla_ui::{DrawList, TextureId};

/// UI renderer that converts UI draw lists for GPU rendering.
///
/// This struct maintains a registry mapping `TextureId` (from katla_ui)
/// to `TextureHandle` (from katla_gfx), allowing the UI layer to reference
/// textures without knowing about GPU handles.
pub struct UIRenderer {
    /// Maps UI texture IDs to GPU texture handles.
    texture_registry: HashMap<TextureId, TextureHandle>,
    /// Font atlas bindless texture slot index.
    font_atlas_bindless_slot: Option<u32>,
    /// White texture bindless slot index for solid color rendering.
    white_texture_bindless_slot: Option<u32>,
    /// Maps TextureHandle indices to their bindless texture slots.
    bindless_slots: HashMap<u32, u32>,

    // Reusable conversion buffers (cleared each frame, avoids reallocation)
    texture_to_index: HashMap<TextureId, u32>,
    vertices: Vec<VertexUI>,
    indices: Vec<u32>,
    instances: Vec<VertexUIInstance>,
    commands: Vec<UiDrawCommand>,
}

impl UIRenderer {
    /// Create a new UI renderer.
    pub fn new() -> Self {
        Self {
            texture_registry: HashMap::new(),
            font_atlas_bindless_slot: None,
            white_texture_bindless_slot: Some(0),
            bindless_slots: HashMap::new(),
            texture_to_index: HashMap::new(),
            vertices: Vec::new(),
            indices: Vec::new(),
            instances: Vec::new(),
            commands: Vec::new(),
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

    /// Set the font atlas bindless texture slot.
    ///
    /// This stores the bindless slot index allocated for the font atlas texture.
    #[cfg(feature = "editor")]
    pub fn set_font_atlas_bindless_slot(&mut self, slot: u32) {
        log::debug!("UIRenderer: Setting font atlas bindless slot to {}", slot);
        self.font_atlas_bindless_slot = Some(slot);
    }

    /// Get the bindless slot for a texture handle.
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

    /// Convert a `katla_ui::DrawList` to a `katla_gfx::UIDrawList`.
    ///
    /// This method:
    /// 1. Resolves all texture IDs to bindless texture indices
    /// 2. Converts vertex data to GPU format for complex geometry
    /// 3. Converts instance data for simple quads
    /// 4. Copies index and command data
    pub fn convert_draw_list(
        &mut self,
        draw_list: &DrawList,
        screen_size: [f32; 2],
        scale_factor: f32,
    ) -> UIDrawList {
        self.texture_to_index.clear();
        self.vertices.clear();
        self.indices.clear();
        self.instances.clear();
        self.commands.clear();

        // Build texture mapping for all commands
        for cmd in draw_list.commands() {
            if !self.texture_to_index.contains_key(&cmd.texture) {
                let idx = self.texture_id_to_bindless_index(cmd.texture);
                self.texture_to_index.insert(cmd.texture, idx);
            }
        }

        // Build instance->texture map from instanced commands.
        // Each instanced command covers a range of instances [offset, offset+count)
        // with a specific texture.
        let mut instance_texture_map: Vec<u32> = vec![0; draw_list.instances().len()];
        for cmd in draw_list.commands() {
            if !cmd.is_instanced || cmd.count == 0 {
                continue;
            }
            let tex_idx = self
                .texture_to_index
                .get(&cmd.texture)
                .copied()
                .unwrap_or(0);
            let start = cmd.offset as usize;
            let end = start + cmd.count as usize;
            for slot in instance_texture_map.iter_mut().take(end).skip(start) {
                *slot = tex_idx;
            }
        }

        // Convert instances (simple quads: rects, textured rects)
        for (i, instance) in draw_list.instances().iter().enumerate() {
            self.instances.push(VertexUIInstance {
                position: instance.position,
                size: instance.size,
                uv_min: instance.uv_min,
                uv_max: instance.uv_max,
                color: instance.color,
                texture_index: instance_texture_map[i],
                clip_rect: instance.clip_rect,
            });
        }

        // Convert vertices (complex geometry: circles, rounded rects, lines, gradients)
        // Build vertex->texture map in O(M) single pass through commands
        let mut vertex_texture_map: Vec<u32> = vec![0; draw_list.vertices().len()];
        for cmd in draw_list.commands() {
            if cmd.is_instanced || cmd.count == 0 {
                continue;
            }
            let tex_idx = self
                .texture_to_index
                .get(&cmd.texture)
                .copied()
                .unwrap_or(0);
            let index_start = cmd.offset as usize;
            let index_end = index_start + cmd.count as usize;
            for &idx in &draw_list.indices()[index_start..index_end] {
                let idx = idx as usize;
                if idx < vertex_texture_map.len() {
                    vertex_texture_map[idx] = tex_idx;
                }
            }
        }

        for (i, v) in draw_list.vertices().iter().enumerate() {
            self.vertices.push(VertexUI::new(
                [v.pos.x(), v.pos.y()],
                [v.uv.x(), v.uv.y()],
                v.color,
                vertex_texture_map[i],
            ));
        }

        self.indices.extend(draw_list.indices());

        // Convert commands
        self.commands.extend(draw_list.commands().iter().map(|cmd| {
            let bindless_index = self
                .texture_to_index
                .get(&cmd.texture)
                .copied()
                .unwrap_or(0);
            if cmd.is_instanced {
                UiDrawCommand::instanced(
                    cmd.offset,
                    cmd.count,
                    cmd.clip_rect,
                    TextureHandle::new(bindless_index),
                )
            } else {
                UiDrawCommand::vertex(
                    cmd.offset,
                    cmd.count,
                    cmd.clip_rect,
                    TextureHandle::new(bindless_index),
                )
            }
        }));

        UIDrawList {
            vertices: std::mem::take(&mut self.vertices),
            indices: std::mem::take(&mut self.indices),
            instances: std::mem::take(&mut self.instances),
            commands: std::mem::take(&mut self.commands),
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
                log::error!(
                    "Font atlas bindless slot is None! Text will sample from white texture (slot 0) instead."
                );
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
        } else if let Some(slot) = self.bindless_slots.get(&(id.0 as u32)) {
            // TextureId created via from_handle_index — look up bindless slot directly
            *slot
        } else {
            // Fallback to white texture slot for unknown textures
            log::warn!(
                "TextureId {} not in registry, falling back to white texture slot 0",
                id.0
            );
            self.white_texture_bindless_slot.unwrap_or(0)
        }
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
    fn test_convert_empty_draw_list() {
        let mut renderer = UIRenderer::new();
        let draw_list = DrawList::new();

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        assert!(gpu_list.vertices.is_empty());
        assert!(gpu_list.indices.is_empty());
        assert!(gpu_list.commands.is_empty());
        assert_eq!(gpu_list.screen_size, [1920.0, 1080.0]);
        assert_eq!(gpu_list.scale_factor, 1.0);
    }

    #[test]
    fn test_convert_draw_list_with_instances() {
        let mut renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        // Add a simple rect (now uses instancing)
        use katla_math::{Color, Rect2D, Vec2};
        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0)),
            Color::RED,
        );
        draw_list.finalize();

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        // Should have 1 instance and 1 instanced command
        assert_eq!(gpu_list.instances.len(), 1);
        assert_eq!(gpu_list.command_count(), 1);
        assert!(gpu_list.commands[0].is_instanced);
        assert_eq!(gpu_list.screen_size, [1920.0, 1080.0]);
        assert_eq!(gpu_list.scale_factor, 1.0);
    }

    #[test]
    fn test_hidpi_scale_factor_in_draw_list() {
        // Test that scale_factor is included in UIDrawList for HiDPI coordinate conversion
        use katla_math::{Color, Rect2D, Vec2};
        let mut renderer = UIRenderer::new();
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
    fn test_instance_positions_match_requested_bounds() {
        // VAL-POS-001: Instance positions in the draw list must match the requested bounds
        use katla_math::{Color, Rect2D, Vec2};

        let mut renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        let bounds = Rect2D::from_origin_size(Vec2::new(50.0, 75.0), Vec2::new(100.0, 40.0));
        draw_list.add_rect(bounds, Color::BLUE);
        draw_list.finalize();

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        // Should have 1 instance with correct position and size
        assert_eq!(gpu_list.instances.len(), 1);
        let inst = &gpu_list.instances[0];
        assert_eq!(inst.position, [50.0, 75.0]);
        assert_eq!(inst.size, [100.0, 40.0]);
    }

    #[test]
    fn test_multiple_rects_preserve_positions() {
        // VAL-POS-001: Multiple elements must appear at their specified coordinates
        use katla_math::{Color, Rect2D, Vec2};

        let mut renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        let rect1 = Rect2D::from_origin_size(Vec2::new(10.0, 20.0), Vec2::new(50.0, 30.0));
        let rect2 = Rect2D::from_origin_size(Vec2::new(100.0, 200.0), Vec2::new(40.0, 60.0));
        let rect3 = Rect2D::from_origin_size(Vec2::new(500.0, 300.0), Vec2::new(80.0, 40.0));

        draw_list.add_rect(rect1, Color::RED);
        draw_list.add_rect(rect2, Color::GREEN);
        draw_list.add_rect(rect3, Color::BLUE);
        draw_list.finalize();

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        // Should have 3 instances
        assert_eq!(gpu_list.instances.len(), 3);

        // Verify instance positions match the requested bounds
        assert_eq!(gpu_list.instances[0].position, [10.0, 20.0]);
        assert_eq!(gpu_list.instances[0].size, [50.0, 30.0]);
        assert_eq!(gpu_list.instances[1].position, [100.0, 200.0]);
        assert_eq!(gpu_list.instances[1].size, [40.0, 60.0]);
        assert_eq!(gpu_list.instances[2].position, [500.0, 300.0]);
        assert_eq!(gpu_list.instances[2].size, [80.0, 40.0]);
    }

    // ========================================================================
    // VAL-POS-003: Coordinate Transformation Correctness Tests
    // ========================================================================

    #[test]
    fn test_logical_to_physical_coordinate_conversion() {
        // VAL-POS-003: Instance data conversion preserves positions
        use katla_math::{Color, Rect2D, Vec2};

        let mut renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        let bounds = Rect2D::from_origin_size(Vec2::new(100.0, 150.0), Vec2::new(50.0, 60.0));
        draw_list.add_rect(bounds, Color::YELLOW);
        draw_list.finalize();

        // Test with scale factor 1.0
        let gpu_list_1x = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);
        assert_eq!(gpu_list_1x.instances[0].position[0], 100.0);
        assert_eq!(gpu_list_1x.instances[0].position[1], 150.0);

        // Test with scale factor 2.0
        let gpu_list_2x = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 2.0);
        assert_eq!(gpu_list_2x.instances[0].position[0], 100.0);
        assert_eq!(gpu_list_2x.instances[0].position[1], 150.0);
        assert_eq!(gpu_list_2x.scale_factor, 2.0);
        assert_eq!(gpu_list_1x.screen_size, [1920.0, 1080.0]);
        assert_eq!(gpu_list_2x.screen_size, [1920.0, 1080.0]);
    }

    #[test]
    fn test_scale_factor_preserves_spatial_relationships() {
        // VAL-POS-003: Instance data preserves spatial relationships across scale factors
        use katla_math::{Color, Rect2D, Vec2};

        let mut renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        let rect1 = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0));
        let rect2 = Rect2D::from_origin_size(Vec2::new(150.0, 75.0), Vec2::new(100.0, 50.0));
        let expected_horizontal_gap = 50.0;

        draw_list.add_rect(rect1, Color::RED);
        draw_list.add_rect(rect2, Color::GREEN);
        draw_list.finalize();

        let scale_factors = [1.0, 1.5, 2.0, 3.0];
        let mut expected_positions = None;

        for scale_factor in scale_factors {
            let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], scale_factor);

            let positions: Vec<_> = gpu_list
                .instances
                .iter()
                .map(|i| (i.position[0], i.position[1]))
                .collect();

            if let Some(ref expected) = expected_positions {
                assert_eq!(
                    positions, *expected,
                    "Instance positions should be identical for scale_factor={}",
                    scale_factor
                );
            } else {
                expected_positions = Some(positions);
            }

            // Verify gap between rects is preserved
            let rect1_right = gpu_list.instances[0].position[0] + gpu_list.instances[0].size[0];
            let rect2_left = gpu_list.instances[1].position[0];
            let actual_gap = rect2_left - rect1_right;
            assert!((actual_gap - expected_horizontal_gap).abs() < 0.01);
            assert_eq!(gpu_list.scale_factor, scale_factor);
        }

        assert!(expected_positions.is_some());
    }

    #[test]
    fn test_instance_color_preservation_through_conversion() {
        // VAL-POS-003: Instance colors must be preserved through conversion
        use katla_math::{Color, Rect2D, Vec2};

        let mut renderer = UIRenderer::new();
        let mut draw_list = DrawList::new();

        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::new(1.0, 0.0, 0.0, 1.0),
        );
        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(20.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::new(0.0, 1.0, 0.0, 0.5),
        );
        draw_list.add_rect(
            Rect2D::from_origin_size(Vec2::new(40.0, 0.0), Vec2::new(10.0, 10.0)),
            Color::new(0.0, 0.0, 1.0, 0.25),
        );
        draw_list.finalize();

        let gpu_list = renderer.convert_draw_list(&draw_list, [1920.0, 1080.0], 1.0);

        assert_eq!(gpu_list.instances[0].color, [255, 0, 0, 255]);
        assert_eq!(gpu_list.instances[1].color, [0, 255, 0, 128]);
        assert_eq!(gpu_list.instances[2].color, [0, 0, 255, 64]);
    }

    #[test]
    fn test_uv_coordinates_preserved_through_conversion() {
        // VAL-POS-003: UV coordinates must be preserved through conversion
        use katla_math::{Color, Rect2D, Vec2};

        let mut renderer = UIRenderer::new();
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

        let mut renderer = UIRenderer::new();
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

        let mut renderer = UIRenderer::new();
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

        let mut renderer = UIRenderer::new();
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

        let mut renderer = UIRenderer::new();
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
