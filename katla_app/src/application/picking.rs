#[cfg(feature = "editor")]
use crate::application::Application;
#[cfg(feature = "editor")]
use katla_gfx::GpuRenderer;

#[cfg(feature = "editor")]
impl Application {
    pub(crate) fn capture_picking_entities(&mut self) {
        let Some(source) = self
            .frame_graph_bindings
            .resources
            .object_id
            .as_deref()
            .and_then(|name| self.frame_graph.resource_id(name))
            .and_then(|resource| self.renderer.graph_texture_source(resource))
        else {
            return;
        };
        self.editor_features.committed_pick = Some(super::features::CommittedPick {
            source,
            size: if self.panel_rt_size.width > 0 && self.panel_rt_size.height > 0 {
                self.panel_rt_size
            } else {
                self.renderer.swapchain_extent()
            },
            entity_map: self.editor.entity_instance_map.clone(),
        });
    }

    pub(crate) fn process_picking(&mut self) {
        let selection = poll_pick_requests(
            &mut self.editor_features.pick_requests,
            self.editor_features.latest_pick_sequence,
            |ticket| self.renderer.poll_texture_readback(ticket),
        );
        if let Some(entity) = selection {
            self.editor.editor_ui.selected_entity =
                entity.filter(|entity| self.world.entity_exists(*entity));
        }
    }

    pub(crate) fn queue_pick(&mut self, relative: [f32; 2], viewport_size: [f32; 2]) {
        if !self.frame_graph_runtime.uses_katla_scene() || self.pass_ids.picking.is_none() {
            return;
        }
        let Some(committed) = self.editor_features.committed_pick.as_ref() else {
            return;
        };
        let Some([x, y]) = pick_pixel(
            relative,
            viewport_size,
            committed.size,
            self.renderer.capabilities().clip_y_down,
        ) else {
            return;
        };
        let Some(sequence) = self.editor_features.latest_pick_sequence.checked_add(1) else {
            log::warn!("Picking request sequence exhausted");
            return;
        };
        self.editor_features.latest_pick_sequence = sequence;
        match self.renderer.queue_texture_readback(
            committed.source,
            katla_gfx::TextureReadbackRegion::pixel(x, y),
        ) {
            Ok(ticket) => {
                self.editor_features
                    .pick_requests
                    .push(super::features::PickRequest {
                        sequence,
                        ticket,
                        entity_map: committed.entity_map.clone(),
                    });
            }
            Err(error) => log::warn!("Failed to queue picking readback: {error}"),
        }
    }
}

#[cfg(feature = "editor")]
pub(crate) fn pick_pixel(
    relative: [f32; 2],
    viewport_size: [f32; 2],
    extent: katla_gfx::Size2D,
    clip_y_down: bool,
) -> Option<[u32; 2]> {
    if relative
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
        || viewport_size
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        || relative[0] >= viewport_size[0]
        || relative[1] >= viewport_size[1]
        || extent.width == 0
        || extent.height == 0
    {
        return None;
    }
    let x = (relative[0] / viewport_size[0] * extent.width as f32) as u32;
    let y = (relative[1] / viewport_size[1] * extent.height as f32) as u32;
    if x >= extent.width || y >= extent.height {
        return None;
    }
    Some([
        x,
        if clip_y_down {
            y
        } else {
            extent.height - 1 - y
        },
    ])
}

#[cfg(feature = "editor")]
fn poll_pick_requests(
    requests: &mut Vec<super::features::PickRequest>,
    latest_sequence: u64,
    mut poll: impl FnMut(
        katla_gfx::TextureReadbackTicket,
    )
        -> Result<Option<katla_gfx::TextureReadbackData>, katla_gfx::RendererError>,
) -> Option<Option<katla_ecs::EntityId>> {
    let mut selection = None;
    requests.retain(|request| match poll(request.ticket) {
        Ok(Some(data)) => {
            if request.sequence == latest_sequence
                && let Some(index) = data.single_u32()
            {
                selection = Some(
                    index
                        .checked_sub(1)
                        .and_then(|index| request.entity_map.get(&index).copied()),
                );
            }
            false
        }
        Ok(None) => true,
        Err(error) => {
            log::warn!("Picking readback failed: {error}");
            false
        }
    });
    selection
}

#[cfg(all(test, feature = "editor"))]
mod tests {
    use super::*;
    use crate::application::features::PickRequest;
    use katla_ecs::EntityId;
    use katla_gfx::{
        GraphTextureSource, ImageFormat, Size2D, TextureReadbackData, TextureReadbackTicket,
    };

    fn request(sequence: u64, source_id: u64, entity: EntityId) -> PickRequest {
        PickRequest {
            sequence,
            ticket: TextureReadbackTicket {
                id: sequence,
                source: GraphTextureSource {
                    id: source_id,
                    resource: katla_gfx::render_graph::ResourceId(7),
                    frame_slot: sequence as usize % 3,
                    generation: source_id,
                    submission: source_id,
                },
            },
            entity_map: [(0, entity)].into_iter().collect(),
        }
    }

    fn pixel(index: u32) -> TextureReadbackData {
        TextureReadbackData {
            format: ImageFormat::R32Uint,
            size: Size2D::new(1, 1),
            bytes: index.to_ne_bytes().to_vec(),
        }
    }

    #[test]
    fn test_latest_click_survives_an_older_delayed_readback() {
        let entity_a = EntityId::from_raw(11);
        let entity_b = EntityId::from_raw(22);
        let mut requests = vec![request(1, 100, entity_a)];
        assert_eq!(poll_pick_requests(&mut requests, 1, |_| Ok(None)), None);
        requests.push(request(2, 101, entity_b));
        let result = poll_pick_requests(&mut requests, 2, |ticket| {
            Ok((ticket.id == 1).then(|| pixel(1)))
        });
        assert_eq!(result, None);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].sequence, 2);
        assert_eq!(
            poll_pick_requests(&mut requests, 2, |_| Ok(Some(pixel(1)))),
            Some(Some(entity_b))
        );
        assert!(requests.is_empty());
    }

    #[test]
    fn test_old_click_is_consumed_without_overwriting_new_selection() {
        let mut requests = vec![
            request(1, 100, EntityId::from_raw(11)),
            request(2, 101, EntityId::from_raw(22)),
        ];
        assert_eq!(
            poll_pick_requests(&mut requests, 2, |ticket| Ok(
                (ticket.id == 2).then(|| pixel(1))
            )),
            Some(Some(EntityId::from_raw(22)))
        );
        assert_eq!(requests.len(), 1);
        assert_eq!(
            poll_pick_requests(&mut requests, 2, |_| Ok(Some(pixel(1)))),
            None
        );
        assert!(requests.is_empty());
    }

    #[test]
    fn test_pending_pick_retains_its_source_and_entity_snapshot_after_resize_or_abort() {
        let entity = EntityId::from_raw(11);
        let mut committed_map = [(0, entity)]
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();
        let mut queued = request(1, 100, entity);
        queued.entity_map = committed_map.clone();
        let source = queued.ticket.source;
        let mut requests = vec![queued];
        committed_map.insert(0, EntityId::from_raw(22));
        let replacement_source = GraphTextureSource {
            id: 101,
            generation: 101,
            submission: 101,
            ..source
        };
        assert_ne!(source, replacement_source);
        let result = poll_pick_requests(&mut requests, 1, |ticket| {
            assert_eq!(ticket.source, source);
            Ok(Some(pixel(1)))
        });
        assert_eq!(result, Some(Some(entity)));
        assert!(requests.is_empty());
    }

    #[test]
    fn test_pick_coordinates_use_clicked_viewport_and_committed_extent() {
        let committed_extent = Size2D::new(800, 600);
        assert_eq!(
            pick_pixel([100.0, 150.0], [400.0, 300.0], committed_extent, true),
            Some([200, 300])
        );
        assert_eq!(
            pick_pixel([100.0, 150.0], [400.0, 300.0], committed_extent, false),
            Some([200, 299])
        );
        assert_eq!(
            pick_pixel([-1.0, 0.0], [400.0, 300.0], committed_extent, true),
            None
        );
        assert_eq!(
            pick_pixel([400.0, 0.0], [400.0, 300.0], committed_extent, true),
            None
        );
        assert_eq!(
            pick_pixel([f32::NAN, 0.0], [400.0, 300.0], committed_extent, true),
            None
        );
    }
}
