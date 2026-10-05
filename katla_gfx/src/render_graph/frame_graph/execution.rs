//! Backend execution and material preparation.

use super::*;

#[cfg(target_os = "macos")]
impl FrameGraph<crate::MetalRenderer> {
    /// Collect draw lists from the user closure without executing passes.
    ///
    /// Creates a Frame context, calls the closure to submit draw lists,
    /// and returns the pending draw data for MetalRenderer to execute.
    pub(crate) fn collect_draw_lists<F>(
        &mut self,
        renderer: &mut crate::MetalRenderer,
        f: F,
    ) -> Result<
        std::collections::HashMap<usize, crate::render_graph::frame::PassExecutionData>,
        RenderGraphError,
    >
    where
        F: FnOnce(&mut crate::render_graph::frame::Frame<'_, crate::MetalRenderer>),
    {
        self.prepare_external_image_producers(renderer);
        if !self.compiled {
            self.compile()?;
        }

        self.initialize_transient_textures(renderer)?;
        self.initialize_transient_buffers(renderer)?;
        self.prepare_external_buffer_producers(renderer);
        self.compile()?;

        let frame_idx = renderer.frame_index();
        let mut frame = crate::render_graph::frame::Frame::new(self, renderer, 0, frame_idx);
        f(&mut frame);
        frame.validate_submissions()?;

        let pending = std::mem::take(&mut frame.pending);

        for pass_index in self.execution_order() {
            let pass = &self.passes[pass_index];
            let format = pass
                .output_format
                .unwrap_or(crate::texture::ImageFormat::Auto);
            let explicit =
                pass.material
                    .iter()
                    .copied()
                    .chain(
                        pass.bindings
                            .pipelines
                            .iter()
                            .map(|pipeline| pipeline.material),
                    )
                    .chain(pass.bindings.phases.iter().flat_map(|phase| {
                        phase.pipelines.iter().map(|pipeline| pipeline.material)
                    }));
            let mut seen = HashSet::new();
            for material in explicit {
                if seen.insert(material) {
                    renderer.ensure_material_variant_impl(material, format)?;
                }
            }
            let uses_submitted_materials = pass.bindings.pipelines.is_empty()
                && (pass.bindings.phases.is_empty()
                    || pass.bindings.phases.iter().any(|phase| {
                        phase.pipelines.is_empty()
                            && matches!(
                                phase.draw,
                                crate::renderer::frame_bindings::PassDraw::Submissions
                                    | crate::renderer::frame_bindings::PassDraw::ObjectIndices(_)
                            )
                    }));
            if uses_submitted_materials && let Some(data) = pending.get(&pass_index) {
                for draw_list in &data.draw_lists {
                    for draw in &draw_list.draws {
                        if seen.insert(draw.material) {
                            renderer.ensure_material_variant_impl(draw.material, format)?;
                        }
                    }
                }
            }
        }

        Ok(pending)
    }
}

// --- Vulkan-specific methods ---
impl FrameGraph<crate::renderer::VulkanRenderer> {
    /// Resolve deferred materials - compile materials for their pass formats.
    fn resolve_materials(
        &mut self,
        renderer: &mut crate::renderer::VulkanRenderer,
    ) -> Result<(), RenderGraphError> {
        for pass_index in self.execution_order() {
            let pass = &self.passes[pass_index];
            let format = pass
                .output_format
                .unwrap_or(crate::texture::ImageFormat::Auto);
            let materials =
                pass.material
                    .iter()
                    .copied()
                    .chain(
                        pass.bindings
                            .pipelines
                            .iter()
                            .map(|pipeline| pipeline.material),
                    )
                    .chain(
                        pass.bindings.phases.iter().flat_map(|phase| {
                            phase.pipelines.iter().map(|pipeline| pipeline.material)
                        }),
                    )
                    .collect::<Vec<_>>();
            let mut seen = HashSet::new();
            for material in materials {
                if seen.insert(material) {
                    renderer.ensure_material_compiled(material, format)?;
                }
            }
        }

        Ok(())
    }

    /// Execute the graph with the given frame context.
    ///
    /// Called internally by `VulkanRenderer::render()`.
    pub(crate) fn execute(
        &mut self,
        renderer: &mut crate::renderer::VulkanRenderer,
        image_index: u32,
        f: impl FnOnce(&mut crate::render_graph::frame::Frame<'_, crate::renderer::VulkanRenderer>),
    ) -> Result<(), RenderGraphError> {
        self.prepare_external_image_producers(renderer);
        if !self.compiled {
            self.compile()?;
        }

        self.initialize_transient_textures(renderer)?;
        self.initialize_transient_buffers(renderer)?;
        self.prepare_external_buffer_producers(renderer);
        self.compile()?;

        let frame_idx = renderer.current_frame();

        log::trace!(
            "Frame graph execute: frame_idx={}, image_index={}",
            frame_idx,
            image_index
        );

        self.resolve_materials(renderer)?;

        let mut frame =
            crate::render_graph::frame::Frame::new(self, renderer, image_index, frame_idx);
        if self.trace_enabled {
            frame.enable_execution_trace();
        }
        f(&mut frame);
        frame.validate_submissions()?;
        frame.pre_compile_materials()?;
        frame.execute_passes()?;

        if self.trace_enabled {
            self.last_execution_trace = frame.execution_trace().clone();
        }
        self.record_buffer_execution(renderer);

        Ok(())
    }
}
