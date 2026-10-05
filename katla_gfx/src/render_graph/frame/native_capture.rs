use super::Frame;
use crate::render_graph::capture::{CapturedEncoder, CapturedEncoderKind};
use crate::render_graph::{PassDesc, ResourceId};
use crate::renderer::VulkanRenderer;
use ash::vk;

impl Frame<'_, VulkanRenderer> {
    pub(super) fn capture_render_encoder(
        &mut self,
        pass: &PassDesc,
        color: &[vk::RenderingAttachmentInfo<'_>],
        depth: Option<&vk::RenderingAttachmentInfo<'_>>,
        stencil: Option<&vk::RenderingAttachmentInfo<'_>>,
    ) {
        if !self.trace_enabled {
            return;
        }
        let mut resources = Vec::new();
        for attachment in color.iter().chain(depth).chain(stencil) {
            for (index, _) in self.graph.resources.iter().enumerate() {
                let resource = ResourceId(index as u32);
                if self
                    .color_target_view(resource)
                    .is_ok_and(|view| view == attachment.image_view)
                    && !resources.contains(&resource.0)
                {
                    resources.push(resource.0);
                }
            }
        }
        self.capture_encoder(pass, CapturedEncoderKind::Render, resources);
    }

    pub(super) fn capture_encoder(
        &mut self,
        pass: &PassDesc,
        kind: CapturedEncoderKind,
        mut resources: Vec<u32>,
    ) {
        if !self.trace_enabled {
            return;
        }
        resources.sort_unstable();
        resources.dedup();
        let pass_index = self
            .graph
            .passes
            .iter()
            .position(|candidate| std::ptr::eq(candidate, pass));
        let ordinal = self.execution_trace.backend.encoders.len();
        self.execution_trace.backend.encoders.push(CapturedEncoder {
            ordinal,
            pass_index,
            label: pass.name.clone(),
            kind,
            resources,
        });
    }

    pub(super) fn capture_bound_resource(&mut self, resource: ResourceId) {
        if self.trace_enabled
            && let Some(encoder) = self.execution_trace.backend.encoders.last_mut()
            && !encoder.resources.contains(&resource.0)
        {
            encoder.resources.push(resource.0);
            encoder.resources.sort_unstable();
        }
    }
}

impl Frame<'_, VulkanRenderer> {
    pub(super) fn capture_memory_barrier(
        &mut self,
        barrier: &vk::MemoryBarrier2<'_>,
        required: &vk::MemoryBarrier2<'_>,
        boundary: &str,
    ) {
        if !self.trace_enabled {
            return;
        }
        use crate::render_graph::capture::{
            CapturedNativeSyncRange, CapturedNativeSyncScope, CapturedSyncOperation,
        };
        let scope = |barrier: &vk::MemoryBarrier2<'_>| CapturedNativeSyncScope {
            range: CapturedNativeSyncRange::Global,
            source_stages: barrier.src_stage_mask.as_raw(),
            destination_stages: barrier.dst_stage_mask.as_raw(),
            source_access: barrier.src_access_mask.as_raw(),
            destination_access: barrier.dst_access_mask.as_raw(),
            old_layout: None,
            new_layout: None,
            visibility: 0,
        };
        self.execution_trace
            .backend
            .synchronization
            .push(CapturedSyncOperation {
                resource: u32::MAX,
                origin: "backend".into(),
                resource_kind: "global".into(),
                producer: None,
                consumer: None,
                version: "backend".into(),
                range: "global".into(),
                source: format!("{:?}/{:?}", barrier.src_stage_mask, barrier.src_access_mask),
                destination: format!("{:?}/{:?}", barrier.dst_stage_mask, barrier.dst_access_mask),
                reason: boundary.into(),
                boundary: boundary.into(),
                native_scope: vec![scope(barrier)],
                required_native_scope: vec![scope(required)],
                emitted: true,
                omission_reason: None,
            });
    }
    pub(super) fn capture_binding_set(&mut self, layout_identity: String) {
        if !self.trace_enabled {
            return;
        }
        let identity = self.execution_trace.backend.bindings.len();
        self.execution_trace.backend.bindings.push(
            crate::render_graph::capture::CapturedBindingSet {
                identity,
                layout_identity,
                residency_members: Vec::new(),
                snapshot_generation: None,
            },
        );
    }
}
