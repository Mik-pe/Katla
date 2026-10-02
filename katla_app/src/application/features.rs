//! Application-owned editor GPU resources.

use katla_gfx::{GpuRenderer, TextureDescriptor, TextureHandle};

use crate::{AppResult, Renderer};

#[derive(Default)]
pub(crate) struct EditorFeatures {
    #[cfg(feature = "editor")]
    pub(crate) pick_requests: Vec<PickRequest>,
    #[cfg(feature = "editor")]
    pub(crate) latest_pick_sequence: u64,
    #[cfg(feature = "editor")]
    pub(crate) committed_pick: Option<CommittedPick>,
    font_atlas: Option<TextureHandle>,
    font_atlas_slot: Option<u32>,
}

impl EditorFeatures {
    #[cfg(test)]
    pub(crate) fn has_font_atlas(&self) -> bool {
        self.font_atlas.is_some() || self.font_atlas_slot.is_some()
    }

    #[cfg(feature = "editor")]
    pub(crate) fn font_atlas_slot(&self) -> Option<u32> {
        self.font_atlas_slot
    }

    pub(crate) fn upload_font_atlas(
        &mut self,
        renderer: &mut Renderer,
        ui: &mut katla_ui::UiContext,
    ) -> AppResult<()> {
        let (width, height, data, resized) = {
            let fonts = ui.fonts();
            if self.font_atlas.is_some() && !fonts.atlas_needs_update() {
                return Ok(());
            }
            let (width, height) = fonts.atlas_size();
            (
                width,
                height,
                fonts.atlas_data_rgba(),
                fonts.atlas_was_resized(),
            )
        };
        if let Some(handle) = self.font_atlas
            && !resized
        {
            renderer.update_texture(handle, &data)?;
        } else {
            let descriptor =
                TextureDescriptor::rgba8_unorm(width, height).with_label("editor font atlas");
            let replacement = renderer.create_texture(&descriptor, &data)?;
            let slot = renderer.get_bindless_slot(replacement).ok_or_else(|| {
                crate::AppError::RendererInitFailed {
                    reason: "Font atlas has no sampled texture binding".into(),
                }
            })?;
            if let Some(previous) = self.font_atlas.replace(replacement) {
                renderer.destroy_texture(previous);
            }
            self.font_atlas_slot = Some(slot);
            ui.fonts_mut().clear_atlas_resized();
        }
        ui.fonts_mut().mark_atlas_updated();
        Ok(())
    }
}

#[cfg(feature = "editor")]
pub(crate) struct PickRequest {
    pub(crate) sequence: u64,
    pub(crate) ticket: katla_gfx::renderer::texture_readback::TextureReadbackTicket,
    pub(crate) entity_map: std::collections::HashMap<u32, katla_ecs::EntityId>,
}

#[cfg(feature = "editor")]
pub(crate) struct CommittedPick {
    pub(crate) source: katla_gfx::GraphTextureSource,
    pub(crate) size: katla_gfx::Size2D,
    pub(crate) entity_map: std::collections::HashMap<u32, katla_ecs::EntityId>,
}
