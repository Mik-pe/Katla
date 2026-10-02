//! Native objects retained by one command-buffer submission.

use objc2::Message;
use std::cell::RefCell;
use std::rc::Rc;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTL4ArgumentTable, MTL4ArgumentTableDescriptor, MTLBuffer, MTLComputePipelineState,
    MTLDepthStencilState, MTLDevice, MTLRenderPipelineState, MTLResourceOptions, MTLSamplerState,
};

use super::residency::MetalResidency;

pub(crate) struct EncodingResources {
    pub(crate) device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub(crate) residency: Rc<MetalResidency>,
    failure: RefCell<Option<String>>,
    tables: RefCell<Vec<Retained<ProtocolObject<dyn MTL4ArgumentTable>>>>,
    render_pipelines: RefCell<Vec<Retained<ProtocolObject<dyn MTLRenderPipelineState>>>>,
    compute_pipelines: RefCell<Vec<Retained<ProtocolObject<dyn MTLComputePipelineState>>>>,
    depth_states: RefCell<Vec<Retained<ProtocolObject<dyn MTLDepthStencilState>>>>,
    bindless: RefCell<Vec<Rc<super::argument_buffer::BindlessSnapshot>>>,
    samplers: RefCell<Vec<Retained<ProtocolObject<dyn MTLSamplerState>>>>,
    persistent: RefCell<Vec<Rc<MetalResidency>>>,
    identity: RefCell<Option<SubmissionIdentity>>,
    layouts: RefCell<Vec<super::binding_schema::ArgumentTableLayout>>,
    capture: RefCell<Option<crate::render_graph::capture::BackendExecutionTrace>>,
    capture_context: RefCell<(Option<usize>, String, Vec<u32>)>,
    #[cfg(test)]
    native_encoder_count: std::cell::Cell<usize>,
    #[cfg(test)]
    native_barrier_count: std::cell::Cell<usize>,
}

impl EncodingResources {
    pub(crate) fn new(device: &ProtocolObject<dyn MTLDevice>, label: &str) -> Rc<Self> {
        Rc::new(Self {
            device: device.retain(),
            residency: Rc::new(
                MetalResidency::new(device, label).expect("Metal residency allocation"),
            ),
            failure: RefCell::new(None),
            tables: RefCell::new(Vec::new()),
            render_pipelines: RefCell::new(Vec::new()),
            compute_pipelines: RefCell::new(Vec::new()),
            depth_states: RefCell::new(Vec::new()),
            bindless: RefCell::new(Vec::new()),
            samplers: RefCell::new(Vec::new()),
            persistent: RefCell::new(Vec::new()),
            identity: RefCell::new(None),
            layouts: RefCell::new(Vec::new()),
            capture: RefCell::new(None),
            capture_context: RefCell::new((None, String::new(), Vec::new())),
            #[cfg(test)]
            native_encoder_count: std::cell::Cell::new(0),
            #[cfg(test)]
            native_barrier_count: std::cell::Cell::new(0),
        })
    }

    pub(crate) fn enable_capture(&self) {
        *self.capture.borrow_mut() = Some(crate::render_graph::capture::BackendExecutionTrace {
            backend: "Metal4".into(),
            ..Default::default()
        });
    }

    pub(crate) fn capture_context(&self, pass: Option<usize>, label: &str, resources: Vec<u32>) {
        if self.capture.borrow().is_some() {
            *self.capture_context.borrow_mut() = (pass, label.into(), resources);
        }
    }

    pub(crate) fn record_encoder(&self, kind: crate::render_graph::capture::CapturedEncoderKind) {
        #[cfg(test)]
        self.native_encoder_count
            .set(self.native_encoder_count.get() + 1);
        if let Some(capture) = self.capture.borrow_mut().as_mut() {
            let (pass, label, resources) = &*self.capture_context.borrow();
            capture
                .encoders
                .push(crate::render_graph::capture::CapturedEncoder {
                    ordinal: capture.encoders.len(),
                    pass_index: *pass,
                    label: label.clone(),
                    kind,
                    resources: resources.clone(),
                });
        }
    }

    pub(crate) fn observe_resource(&self, resource: u32) {
        if let Some(capture) = self.capture.borrow_mut().as_mut()
            && let Some(encoder) = capture.encoders.last_mut()
            && !encoder.resources.contains(&resource)
        {
            encoder.resources.push(resource);
        }
    }

    pub(crate) fn record_sync(
        &self,
        operation: crate::render_graph::capture::CapturedSyncOperation,
    ) {
        if let Some(capture) = self.capture.borrow_mut().as_mut() {
            capture.synchronization.push(operation);
        }
    }

    pub(crate) fn capture_binding(
        &self,
        table: &ProtocolObject<dyn MTL4ArgumentTable>,
        layout: &super::binding_schema::ArgumentTableLayout,
    ) {
        let Some(identity) = self
            .tables
            .borrow()
            .iter()
            .position(|retained| std::ptr::eq(&**retained, table))
        else {
            return;
        };
        if let Some(capture) = self.capture.borrow_mut().as_mut() {
            let layout_identity = serde_json::to_string(layout).unwrap_or_default();
            if !capture.bindings.iter().any(|binding| {
                binding.identity == identity && binding.layout_identity == layout_identity
            }) {
                capture
                    .bindings
                    .push(crate::render_graph::capture::CapturedBindingSet {
                        identity,
                        layout_identity,
                        residency_members: Vec::new(),
                        snapshot_generation: None,
                    });
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn native_counts(&self) -> (usize, usize) {
        (
            self.native_encoder_count.get(),
            self.native_barrier_count.get(),
        )
    }
    #[cfg(test)]
    pub(crate) fn native_barriers(&self, count: usize) {
        self.native_barrier_count
            .set(self.native_barrier_count.get() + count);
    }
    pub(crate) fn captured_boundary(&self, boundary: &str) -> bool {
        self.capture.borrow().as_ref().is_some_and(|capture| {
            capture
                .synchronization
                .iter()
                .any(|operation| operation.boundary == boundary)
        })
    }

    pub(crate) fn capture_enabled(&self) -> bool {
        self.capture.borrow().is_some()
    }

    pub(crate) fn capture(&self) -> crate::render_graph::capture::BackendExecutionTrace {
        let Some(mut capture) = self.capture.borrow().clone() else {
            return Default::default();
        };
        let diagnostics = self.diagnostics();
        let mut memberships = vec![diagnostics.submission];
        memberships.extend(diagnostics.persistent);
        memberships.extend(
            diagnostics
                .bindless
                .iter()
                .map(|snapshot| snapshot.residency.clone()),
        );
        let members = memberships
            .iter()
            .enumerate()
            .flat_map(|(set_identity, residency)| {
                residency.members.iter().map(move |member| {
                    crate::render_graph::capture::CapturedResidentResource {
                        set_identity,
                        kind: member.kind.into(),
                        ordinal: member.id,
                        estimated_bytes: member.estimated_bytes,
                    }
                })
            })
            .collect::<Vec<_>>();
        for binding in &mut capture.bindings {
            binding.residency_members = members.clone();
        }
        for snapshot in diagnostics.bindless {
            capture
                .bindings
                .push(crate::render_graph::capture::CapturedBindingSet {
                    identity: self.tables.borrow().len() + capture.bindings.len(),
                    layout_identity: "bindless_resource_ids".into(),
                    residency_members: snapshot
                        .residency
                        .members
                        .iter()
                        .map(
                            |member| crate::render_graph::capture::CapturedResidentResource {
                                set_identity: capture.bindings.len(),
                                kind: member.kind.into(),
                                ordinal: member.id,
                                estimated_bytes: member.estimated_bytes,
                            },
                        )
                        .collect(),
                    snapshot_generation: Some(snapshot.generation),
                });
        }
        capture
    }

    pub(crate) fn fail(&self, error: String) {
        self.failure.borrow_mut().get_or_insert(error);
    }
    pub(crate) fn check(&self) -> Result<(), crate::error::RendererError> {
        if let Some(error) = &*self.failure.borrow() {
            Err(crate::error::RendererError::InvalidOperation(error.clone()))
        } else {
            if log::log_enabled!(log::Level::Debug) {
                log::debug!("Metal submission residency: {:?}", self.diagnostics());
            }
            Ok(())
        }
    }

    pub(crate) fn diagnostics(&self) -> EncodingResourceDiagnostics {
        EncodingResourceDiagnostics {
            identity: *self.identity.borrow(),
            submission: self.residency.diagnostics(),
            argument_table_count: self.tables.borrow().len(),
            argument_table_layouts: self.layouts.borrow().clone(),
            bindless: self
                .bindless
                .borrow()
                .iter()
                .map(|snapshot| BindlessResidencyDiagnostics {
                    generation: snapshot.generation,
                    residency: snapshot.residency.diagnostics(),
                })
                .collect(),
            persistent: self
                .persistent
                .borrow()
                .iter()
                .map(|residency| residency.diagnostics())
                .collect(),
        }
    }

    pub(crate) fn set_submission_identity(&self, slot: usize, generation: u64) {
        *self.identity.borrow_mut() = Some(SubmissionIdentity { slot, generation });
    }

    pub(crate) fn retain_table_layout(&self, layout: &super::binding_schema::ArgumentTableLayout) {
        let mut layouts = self.layouts.borrow_mut();
        if !layouts.contains(layout) {
            layouts.push(layout.clone());
        }
    }

    pub(crate) fn retain_persistent_residency(&self, snapshot: Rc<MetalResidency>) {
        self.persistent.borrow_mut().push(snapshot);
    }

    pub(crate) fn argument_table(
        &self,
        label: &str,
    ) -> Retained<ProtocolObject<dyn MTL4ArgumentTable>> {
        let descriptor = MTL4ArgumentTableDescriptor::new();
        descriptor.setMaxBufferBindCount(31);
        descriptor.setMaxTextureBindCount(128);
        descriptor.setMaxSamplerStateBindCount(16);
        descriptor.setInitializeBindings(true);
        descriptor.setLabel(Some(&objc2_foundation::NSString::from_str(label)));
        let table = self
            .device
            .newArgumentTableWithDescriptor_error(&descriptor)
            .expect("Metal argument table allocation");
        self.tables.borrow_mut().push(table.clone());
        table
    }

    pub(crate) fn inline_bytes(&self, data: &[u8]) -> Retained<ProtocolObject<dyn MTLBuffer>> {
        let buffer = self
            .device
            .newBufferWithLength_options(data.len().max(16), MTLResourceOptions::StorageModeShared)
            .expect("Metal inline constant allocation");
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                buffer.contents().as_ptr().cast(),
                data.len(),
            );
        }
        self.residency
            .add_buffer(&buffer)
            .expect("inline buffer residency");
        buffer
    }

    pub(crate) fn retain_graphics_pipeline(
        &self,
        pipeline: &super::pipeline::MetalGraphicsPipeline,
    ) {
        self.retain_table_layout(&pipeline.vertex_layout);
        if let Some(layout) = &pipeline.fragment_layout {
            self.retain_table_layout(layout);
        }
        self.render_pipelines
            .borrow_mut()
            .push(pipeline.pipeline_state.clone());
        if let Some(state) = &pipeline.depth_stencil_state {
            self.depth_states.borrow_mut().push(state.clone());
        }
    }
    pub(crate) fn retain_compute_pipeline(
        &self,
        pipeline: &Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    ) {
        self.compute_pipelines.borrow_mut().push(pipeline.clone());
    }
    pub(crate) fn retain_bindless(&self, snapshot: Rc<super::argument_buffer::BindlessSnapshot>) {
        if let Err(error) = snapshot.residency.validate_buffer(&snapshot.buffer) {
            self.fail(error.to_string());
            return;
        }
        let mut snapshots = self.bindless.borrow_mut();
        if !snapshots
            .iter()
            .any(|retained| Rc::ptr_eq(retained, &snapshot))
        {
            snapshots.push(snapshot);
        }
    }
    pub(crate) fn retain_sampler(&self, sampler: &ProtocolObject<dyn MTLSamplerState>) {
        let retained = sampler.retain();
        self.samplers.borrow_mut().push(retained);
    }
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct BindlessResidencyDiagnostics {
    pub(crate) generation: u64,
    pub(crate) residency: super::residency::ResidencyDiagnostics,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct EncodingResourceDiagnostics {
    pub(crate) identity: Option<SubmissionIdentity>,
    pub(crate) submission: super::residency::ResidencyDiagnostics,
    pub(crate) argument_table_count: usize,
    pub(crate) argument_table_layouts: Vec<super::binding_schema::ArgumentTableLayout>,
    pub(crate) bindless: Vec<BindlessResidencyDiagnostics>,
    pub(crate) persistent: Vec<super::residency::ResidencyDiagnostics>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) struct SubmissionIdentity {
    pub(crate) slot: usize,
    pub(crate) generation: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_submission_diagnostics_identify_actual_membership_and_layout() {
        let device = objc2_metal::MTLCreateSystemDefaultDevice().expect("Metal device");
        let resources = EncodingResources::new(&device, "frame_slot.2.frame.41");
        resources.set_submission_identity(2, 41);
        let buffer = device
            .newBufferWithLength_options(64, MTLResourceOptions::StorageModeShared)
            .unwrap();
        resources.residency.add_buffer(&buffer).unwrap();
        resources.residency.validate_buffer(&buffer).unwrap();
        let _table = resources.argument_table("test.compute");
        let module = naga::front::wgsl::parse_str(
            "@group(0) @binding(3) var<storage, read_write> values: array<u32, 16>;
             @compute @workgroup_size(1) fn cs_main() { values[0] = 4u; }",
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        let options = super::super::binding_schema::options_for_module(
            super::super::binding_schema::ShaderProfile::Graphics,
            &module,
        );
        let layout = super::super::binding_schema::reflect_table_layout(&module, &info, &options)
            .unwrap()
            .remove(0);
        resources.retain_table_layout(&layout);
        resources.retain_table_layout(&layout);
        let diagnostics = resources.diagnostics();
        assert_eq!(
            diagnostics.identity,
            Some(SubmissionIdentity {
                slot: 2,
                generation: 41
            })
        );
        assert_eq!(diagnostics.argument_table_count, 1);
        assert_eq!(diagnostics.argument_table_layouts, vec![layout]);
        assert_eq!(diagnostics.submission.label, "frame_slot.2.frame.41");
        assert_eq!(diagnostics.submission.allocation_count, 1);
        assert_eq!(diagnostics.submission.estimated_resident_bytes, 64);
        assert_eq!(diagnostics.submission.members[0].kind, "buffer");
        let json = serde_json::to_value(diagnostics).unwrap();
        assert_eq!(
            json["argument_table_layouts"][0]["bindings"][0]["binding"],
            3
        );
    }
}
