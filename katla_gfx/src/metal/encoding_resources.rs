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
        })
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
