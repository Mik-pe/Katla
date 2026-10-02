use super::super::pass::PassType;
use super::*;
use crate::render_graph::backend::RenderGraphBackend;
use crate::render_graph::resource::ResourceState;

fn rid(n: u32) -> ResourceId {
    ResourceId(n)
}

/// A trivial mock backend for testing FrameGraph without a GPU.
#[derive(Clone)]
struct MockBackend {
    /// Member count of each `create_transient_slot` call, in call order.
    slot_member_counts: std::rc::Rc<std::cell::RefCell<Vec<usize>>>,
    policies: std::rc::Rc<std::cell::RefCell<Vec<super::super::backend::TransientSlotPolicy>>>,
    prepared_compute: Vec<String>,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            slot_member_counts: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            policies: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            prepared_compute: Vec::new(),
        }
    }
}

struct MockTexture {
    slot: std::cell::Cell<Option<u32>>,
}

struct MockBuffer {
    desc: BufferDesc,
}

#[derive(Clone)]
struct MockImageView;

unsafe impl Send for MockImageView {}
unsafe impl Sync for MockImageView {}

impl RenderGraphBackend for MockBackend {
    type TransientTexture = MockTexture;
    type ImageView = MockImageView;
    type TransientBuffer = MockBuffer;

    fn create_transient_slot(
        &self,
        members: &[super::super::resource::GraphResourceDesc],
        policy: super::super::backend::TransientSlotPolicy,
    ) -> Result<Vec<Self::TransientTexture>, RenderGraphError> {
        self.slot_member_counts.borrow_mut().push(members.len());
        self.policies.borrow_mut().push(policy);
        Ok(members
            .iter()
            .map(|_| MockTexture {
                slot: std::cell::Cell::new(None),
            })
            .collect())
    }

    fn create_transient_buffer(
        &self,
        desc: BufferDesc,
    ) -> Result<Self::TransientBuffer, RenderGraphError> {
        Ok(MockBuffer { desc })
    }

    fn destroy_transient_texture(_texture: Self::TransientTexture) {}

    fn destroy_transient_buffer(_buffer: Self::TransientBuffer) {}

    fn transient_buffer_size(buffer: &Self::TransientBuffer) -> u64 {
        buffer.desc.size
    }

    fn buffer_desc(buffer: &Self::TransientBuffer) -> BufferDesc {
        buffer.desc
    }

    fn buffer_by_handle(
        &self,
        _handle: crate::handle::BufferHandle,
    ) -> Option<&Self::TransientBuffer> {
        None
    }

    fn prepare_compute_pipeline(
        &mut self,
        descriptor: &super::super::ComputePipelineDesc,
    ) -> Result<(), RenderGraphError> {
        self.prepared_compute.push(descriptor.entry.clone());
        Ok(())
    }

    fn current_frame(&self) -> usize {
        0
    }

    fn transient_texture_frames() -> usize {
        2
    }

    fn register_bindless_texture(
        &mut self,
        _texture: &Self::TransientTexture,
    ) -> Result<u32, RenderGraphError> {
        Ok(0)
    }

    fn update_bindless_texture(
        &mut self,
        _slot: u32,
        _texture: &Self::TransientTexture,
    ) -> Result<(), RenderGraphError> {
        Ok(())
    }

    fn transient_texture_format(_texture: &Self::TransientTexture) -> crate::texture::ImageFormat {
        crate::texture::ImageFormat::R8G8B8A8Unorm
    }

    fn transient_texture_extent(_texture: &Self::TransientTexture) -> (u32, u32) {
        (1, 1)
    }

    fn transient_texture_is_depth(_texture: &Self::TransientTexture) -> bool {
        false
    }

    fn transient_texture_bindless_slot(texture: &Self::TransientTexture) -> Option<u32> {
        texture.slot.get()
    }

    fn set_transient_texture_bindless_slot(texture: &mut Self::TransientTexture, slot: u32) {
        texture.slot.set(Some(slot));
    }

    fn transient_texture_view(_texture: &Self::TransientTexture) -> Self::ImageView {
        MockImageView
    }

    fn swapchain_image_view(&self, _image_index: u32) -> Self::ImageView {
        MockImageView
    }
}

type TestGraph = FrameGraph<MockBackend>;

#[test]
fn test_compute_pipeline_warmup_excludes_culled_work() {
    let shader =
        "@compute @workgroup_size(1) fn live() {} @compute @workgroup_size(1) fn unused() {}";
    let mut graph = FrameGraphBuilder::new()
        .add_pass(super::super::ComputePass::new(
            "unused",
            super::super::ComputePipelineDesc {
                wgsl: shader.into(),
                entry: "unused".into(),
            },
        ))
        .add_side_effect_pass(super::super::ComputePass::new(
            "live",
            super::super::ComputePipelineDesc {
                wgsl: shader.into(),
                entry: "live".into(),
            },
        ))
        .build::<MockBackend>()
        .unwrap();
    let mut backend = MockBackend::new();
    graph.initialize_compute_pipelines(&mut backend).unwrap();
    assert_eq!(backend.prepared_compute, vec!["live"]);
}

#[test]
fn test_buffer_declarations_reject_incompatible_pipeline_stages() {
    use crate::render_graph::{ResourceAccessStage, SimplePass};

    for (usage, capability, stage) in [
        (
            BufferUsage::Vertex,
            BufferUsages::VERTEX,
            ResourceAccessStage::VertexShader,
        ),
        (
            BufferUsage::Index,
            BufferUsages::INDEX,
            ResourceAccessStage::FragmentShader,
        ),
        (
            BufferUsage::Indirect,
            BufferUsages::INDIRECT,
            ResourceAccessStage::ComputeShader,
        ),
        (
            BufferUsage::Uniform,
            BufferUsages::UNIFORM,
            ResourceAccessStage::Transfer,
        ),
        (
            BufferUsage::Storage,
            BufferUsages::STORAGE,
            ResourceAccessStage::DepthStencil,
        ),
        (
            BufferUsage::TransferSource,
            BufferUsages::TRANSFER_SOURCE,
            ResourceAccessStage::AllGraphics,
        ),
        (
            BufferUsage::Readback,
            BufferUsages::READBACK,
            ResourceAccessStage::Transfer,
        ),
    ] {
        let desc = BufferDesc::new(64, capability, BufferMemoryPolicy::Readback);
        let pass = SimplePass::new("consume", PassType::Graphics).buffer_access(
            "data",
            ResourceAccessMode::Read,
            usage,
            stage,
            BufferByteRange::WHOLE,
        );
        let result = FrameGraphBuilder::new()
            .create_buffer(GraphBufferDesc::new("data", desc))
            .add_pass(pass)
            .build::<MockBackend>();
        assert!(result.is_err(), "accepted {usage:?} at {stage:?}");
    }
}

#[test]
fn test_buffer_stage_validation_survives_graph_mutation() {
    use crate::render_graph::{BufferAccess, ResourceAccessStage, SimplePass};

    let mut graph = FrameGraphBuilder::new()
        .import_buffer(
            "data",
            crate::BufferHandle::from_raw(1, 0),
            BufferDesc::new(64, BufferUsages::UNIFORM, BufferMemoryPolicy::CpuVisible),
        )
        .add_pass(
            SimplePass::new("consume", PassType::Graphics).buffer_access(
                "data",
                ResourceAccessMode::Read,
                BufferUsage::Uniform,
                ResourceAccessStage::VertexShader,
                BufferByteRange::WHOLE,
            ),
        )
        .build::<MockBackend>()
        .unwrap();
    let data = graph.resource_id("data").unwrap();
    graph.add_pass(
        PassDesc::new("invalid", PassType::Graphics, vec![], vec![]).with_buffer_accesses([
            BufferAccess::uniform_read(data).with_stage(ResourceAccessStage::Transfer),
        ]),
    );
    assert!(matches!(
        graph.compile(),
        Err(RenderGraphError::Validation(
            GraphValidationError::InvalidBufferAccessStage {
                stage: ResourceAccessStage::Transfer,
                ..
            }
        ))
    ));
}

#[test]
fn test_buffer_helpers_build_valid_graphs_for_all_consumers() {
    use crate::render_graph::{BufferAccess, SimplePass};

    let data = ResourceId(0);
    for (access, usages, memory) in [
        (
            BufferAccess::uniform_read(data),
            BufferUsages::UNIFORM,
            BufferMemoryPolicy::CpuVisible,
        ),
        (
            BufferAccess::storage_read_write(data),
            BufferUsages::STORAGE,
            BufferMemoryPolicy::DeviceLocal,
        ),
        (
            BufferAccess::vertex_read(data),
            BufferUsages::VERTEX,
            BufferMemoryPolicy::DeviceLocal,
        ),
        (
            BufferAccess::index_read(data),
            BufferUsages::INDEX,
            BufferMemoryPolicy::DeviceLocal,
        ),
        (
            BufferAccess::indirect_read(data),
            BufferUsages::INDIRECT,
            BufferMemoryPolicy::DeviceLocal,
        ),
        (
            BufferAccess::transfer_read(data),
            BufferUsages::TRANSFER_SOURCE,
            BufferMemoryPolicy::DeviceLocal,
        ),
        (
            BufferAccess::transfer_write(data),
            BufferUsages::TRANSFER_DESTINATION,
            BufferMemoryPolicy::DeviceLocal,
        ),
        (
            BufferAccess::readback_read(data),
            BufferUsages::READBACK,
            BufferMemoryPolicy::Readback,
        ),
    ] {
        FrameGraphBuilder::new()
            .create_buffer(GraphBufferDesc::new(
                "data",
                BufferDesc::new(64, usages, memory),
            ))
            .add_pass(
                SimplePass::new("consume", PassType::Graphics).buffer_access(
                    "data",
                    access.mode,
                    access.usage,
                    access.stage,
                    access.range,
                ),
            )
            .build::<MockBackend>()
            .unwrap();
    }
}

#[test]
fn test_buffer_diagnostics_describe_transient_and_imported_allocations() {
    use crate::render_graph::{ResourceAccessStage, SimplePass};

    let graph = FrameGraphBuilder::new()
        .create_buffer(GraphBufferDesc::new(
            "scratch",
            BufferDesc::new(
                1024,
                BufferUsages::STORAGE | BufferUsages::TRANSFER_SOURCE,
                BufferMemoryPolicy::DeviceLocal,
            ),
        ))
        .import_buffer(
            "readback",
            crate::BufferHandle::from_raw(9, 2),
            BufferDesc::new(
                512,
                BufferUsages::READBACK | BufferUsages::TRANSFER_DESTINATION,
                BufferMemoryPolicy::Readback,
            ),
        )
        .add_side_effect_pass(
            SimplePass::new("compute", PassType::Graphics).buffer_access(
                "scratch",
                ResourceAccessMode::Write,
                BufferUsage::Storage,
                ResourceAccessStage::ComputeShader,
                BufferByteRange::new(128, 256),
            ),
        )
        .add_side_effect_pass(
            SimplePass::new("copy", PassType::Graphics)
                .buffer_access(
                    "scratch",
                    ResourceAccessMode::Read,
                    BufferUsage::TransferSource,
                    ResourceAccessStage::Transfer,
                    BufferByteRange::new(128, 256),
                )
                .buffer_access(
                    "readback",
                    ResourceAccessMode::Write,
                    BufferUsage::TransferDestination,
                    ResourceAccessStage::Transfer,
                    BufferByteRange::new(0, 256),
                ),
        )
        .build::<MockBackend>()
        .unwrap();
    let diagnostics = graph.diagnostics().unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&diagnostics.to_json_pretty().unwrap()).unwrap();
    assert_eq!(json["resources"][1]["kind"], "buffer");
    assert_eq!(json["resources"][1]["origin"], "transient");
    assert_eq!(
        json["resources"][1]["buffer"],
        serde_json::json!({
            "size": 1024, "usages": ["storage", "transfer_source"], "memory": "device_local",
        })
    );
    assert_eq!(json["resources"][2]["origin"], "imported");
    assert_eq!(
        json["resources"][2]["buffer"],
        serde_json::json!({
            "size": 512, "usages": ["transfer_destination", "readback"], "memory": "readback",
        })
    );
    assert_eq!(json["resources"][1]["width"], serde_json::Value::Null);
    assert_eq!(
        json["resources"][1]["physical_allocation_id"],
        serde_json::Value::Null
    );
    assert_eq!(json["resources"][1]["lifetime"]["last_pass"], 1);
    let text = diagnostics.to_string();
    assert!(text.contains(
        "r1 (scratch) transient buffer, 1024 bytes, DeviceLocal, usages [Storage, TransferSource]"
    ));
    let dot = diagnostics.to_dot();
    assert!(dot.contains("buffer 1024 bytes, DeviceLocal, usages [Storage, TransferSource]"));
    for _ in 0..8 {
        assert_eq!(
            diagnostics.to_json_pretty().unwrap(),
            graph.diagnostics().unwrap().to_json_pretty().unwrap()
        );
    }
}

#[test]
fn test_frame_graph_add_and_index_passes() {
    let mut graph = TestGraph::new();
    let p1 = PassDesc::new("a", PassType::Graphics, vec![], vec![rid(1)]);
    let p2 = PassDesc::new("b", PassType::Graphics, vec![rid(1)], vec![rid(2)]);

    graph.add_pass(p1);
    graph.add_pass(p2);

    assert_eq!(graph.pass_count(), 2);
    assert_eq!(graph.pass_index("a"), Some(0));
    assert_eq!(graph.pass_index("b"), Some(1));
    assert_eq!(graph.pass_index("nonexistent"), None);
}

#[test]
fn test_frame_graph_insert_pass_reindexes() {
    let mut graph = TestGraph::new();
    graph.add_pass(PassDesc::new("a", PassType::Graphics, vec![], vec![]));
    graph.add_pass(PassDesc::new("b", PassType::Graphics, vec![], vec![]));

    graph.insert_pass(
        1,
        PassDesc::new("inserted", PassType::Graphics, vec![], vec![]),
    );

    assert_eq!(graph.pass_count(), 3);
    assert_eq!(graph.pass_index("a"), Some(0));
    assert_eq!(graph.pass_index("inserted"), Some(1));
    assert_eq!(graph.pass_index("b"), Some(2));
}

#[test]
fn test_frame_graph_add_pass_resets_compiled() {
    let mut graph = TestGraph::new();
    graph.add_pass(PassDesc::new("a", PassType::Graphics, vec![], vec![]));
    graph.compile().unwrap();
    assert!(graph.compiled);

    graph.add_pass(PassDesc::new("b", PassType::Graphics, vec![], vec![]));
    assert!(!graph.compiled);
    assert!(graph.execution_plan.is_none());
}

#[test]
fn test_frame_graph_builder_with_resources() {
    let builder = FrameGraphBuilder::new().import_resource(
        "ext",
        TextureHandle::from_raw(42, 0),
        ImportedImageContract::undefined(),
    );

    let graph = builder.build::<MockBackend>().unwrap();
    assert!(graph.resource_id("ext").is_some());
}

#[test]
fn test_resource_id_lookup() {
    let mut graph = TestGraph::new();
    let id = graph.create_resource_id("hdr_color");
    assert_eq!(graph.resource_id("hdr_color"), Some(id));
    assert_eq!(graph.resource_name(id), Some("hdr_color"));
    assert_eq!(graph.resource_id("nonexistent"), None);
}

fn validation_depth_resource(name: &str) -> GraphResourceDesc {
    GraphResourceDesc {
        name: name.into(),
        resource_type: super::super::resource::GraphResourceType::DepthAttachment {
            clear_value: 0.0,
            sampled: false,
        },
        format: crate::texture::ImageFormat::D32Sfloat,
        width: 64,
        height: 64,
        tracks_swapchain_size: false,
    }
}

fn validation_resource(name: &str, width: u32, height: u32) -> GraphResourceDesc {
    GraphResourceDesc {
        name: name.to_string(),
        resource_type: super::super::resource::GraphResourceType::ColorAttachment {
            clear_value: None,
        },
        format: crate::texture::ImageFormat::R8G8B8A8Unorm,
        width,
        height,
        tracks_swapchain_size: true,
    }
}

fn validation_error(builder: FrameGraphBuilder) -> GraphValidationError {
    match builder.build::<MockBackend>() {
        Err(RenderGraphError::Validation(error)) => error,
        Err(error) => panic!("expected graph validation error, got {error}"),
        Ok(_) => panic!("expected graph validation to fail"),
    }
}

#[test]
fn builder_rejects_duplicate_resource_names() {
    let error = validation_error(
        FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 1, 1))
            .import_resource(
                "color",
                TextureHandle::from_raw(7, 0),
                ImportedImageContract::undefined(),
            ),
    );
    assert_eq!(
        error,
        GraphValidationError::DuplicateResourceName("color".to_string())
    );
}

#[test]
fn builder_rejects_repeated_imports() {
    let error = validation_error(
        FrameGraphBuilder::new()
            .import_resource(
                "external",
                TextureHandle::from_raw(1, 0),
                ImportedImageContract::undefined(),
            )
            .import_resource(
                "external",
                TextureHandle::from_raw(2, 0),
                ImportedImageContract::undefined(),
            ),
    );
    assert_eq!(
        error,
        GraphValidationError::DuplicateResourceName("external".to_string())
    );
}

#[test]
fn builder_rejects_duplicate_pass_names() {
    let error = validation_error(
        FrameGraphBuilder::new()
            .add_pass(super::super::builder::SimplePass::new(
                "same",
                PassType::Graphics,
            ))
            .add_pass(super::super::builder::SimplePass::new(
                "same",
                PassType::Graphics,
            )),
    );
    assert_eq!(
        error,
        GraphValidationError::DuplicatePassName("same".to_string())
    );
}

#[test]
fn builder_rejects_undeclared_pass_resources() {
    let error = validation_error(FrameGraphBuilder::new().add_pass(
        super::super::builder::SimplePass::new("geometry", PassType::Graphics).write("typo_color"),
    ));
    assert_eq!(
        error,
        GraphValidationError::UndeclaredResource {
            pass: "geometry".to_string(),
            resource: "typo_color".to_string(),
        }
    );
}

#[test]
fn builder_rejects_invalid_resource_descriptors_and_imports() {
    assert_eq!(
        validation_error(
            FrameGraphBuilder::new().create_resource(validation_resource("color", 0, 64))
        ),
        GraphValidationError::InvalidResourceExtent {
            resource: "color".to_string(),
            width: 0,
            height: 64,
        }
    );
    assert_eq!(
        validation_error(FrameGraphBuilder::new().import_resource(
            "external",
            TextureHandle::NONE,
            ImportedImageContract::undefined(),
        )),
        GraphValidationError::InvalidImportedResource("external".to_string())
    );
}

// --- Attachment operation validation (#95) ---

use crate::render_pass::{AttachmentOps, ClearValue};

fn missing_ops_error(builder: FrameGraphBuilder) -> GraphValidationError {
    match builder.build::<MockBackend>() {
        Err(RenderGraphError::Validation(error)) => error,
        Err(error) => panic!("expected graph validation error, got {error}"),
        Ok(_) => panic!("expected graph validation error, graph compiled"),
    }
}

#[test]
fn writing_an_attachment_without_declared_ops_fails_validation() {
    let error = missing_ops_error(
        FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .export_resource("color")
            .add_pass(
                super::super::builder::SimplePass::new("paint", PassType::Graphics).write("color"),
            ),
    );
    assert!(matches!(
        error,
        GraphValidationError::MissingAttachmentOps { pass, resource }
            if pass == "paint" && resource == "color"
    ));
}

#[test]
fn writing_the_backbuffer_without_declared_ops_fails_validation() {
    let error = missing_ops_error(
        FrameGraphBuilder::new().add_pass(
            super::super::builder::SimplePass::new("present", PassType::Graphics)
                .write(BACKBUFFER_NAME),
        ),
    );
    assert!(matches!(
        error,
        GraphValidationError::MissingAttachmentOps { resource, .. } if resource == BACKBUFFER_NAME
    ));
}

#[test]
fn ops_targeting_unwritten_resources_fail_validation() {
    let error = missing_ops_error(
        FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .create_resource(validation_resource("other", 64, 64))
            .export_resource("color")
            .add_pass(
                super::super::builder::SimplePass::new("paint", PassType::Graphics)
                    .write("color")
                    .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                    .attachment("other", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
            ),
    );
    assert!(matches!(
        error,
        GraphValidationError::StrayAttachmentOps { pass, resource }
            if pass == "paint" && resource == "other"
    ));
}

#[test]
fn clear_op_rejects_depth_clear_value_on_a_color_target() {
    let error = missing_ops_error(
        FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .export_resource("color")
            .add_pass(
                super::super::builder::SimplePass::new("paint", PassType::Graphics)
                    .write("color")
                    .attachment("color", AttachmentOps::clear(ClearValue::DEFAULT_DEPTH)),
            ),
    );
    assert!(matches!(
        error,
        GraphValidationError::AttachmentClearValueAspect {
            expected: "a color clear value",
            ..
        }
    ));
}

#[test]
fn loading_an_unproduced_transient_fails_validation() {
    let error = missing_ops_error(
        FrameGraphBuilder::new()
            .create_resource(validation_resource("history", 64, 64))
            .export_resource("history")
            .add_pass(
                super::super::builder::SimplePass::new("first", PassType::Graphics)
                    .write("history")
                    .attachment("history", AttachmentOps::load()),
            ),
    );
    assert!(matches!(
        error,
        GraphValidationError::LoadingUndefinedAttachment { pass, resource }
            if pass == "first" && resource == "history"
    ));
}

#[test]
fn two_pass_accumulation_compiles() {
    let graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("color", 64, 64))
        .export_resource("color")
        .add_pass(
            super::super::builder::SimplePass::new("paint", PassType::Graphics)
                .write("color")
                .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
        )
        .add_pass(
            super::super::builder::SimplePass::new("extend", PassType::Graphics)
                .read("color")
                .write("color")
                .attachment("color", AttachmentOps::load()),
        )
        .build::<MockBackend>()
        .unwrap();
    assert_eq!(graph.execution_order().len(), 2);
}

#[test]
fn imported_backbuffer_may_load_without_an_in_graph_producer() {
    let graph = FrameGraphBuilder::new()
        .add_pass(
            super::super::builder::SimplePass::new("overlay", PassType::Graphics)
                .read(BACKBUFFER_NAME)
                .write(BACKBUFFER_NAME)
                .attachment(BACKBUFFER_NAME, AttachmentOps::load()),
        )
        .build::<MockBackend>()
        .unwrap();
    assert_eq!(graph.execution_order().len(), 1);
}

#[test]
fn compute_passes_reject_attachment_ops() {
    let error = missing_ops_error(
        FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .export_resource("color")
            .add_pass(
                super::super::builder::SimplePass::new("dispatch", PassType::Compute)
                    .write("color")
                    .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK)),
            ),
    );
    assert!(matches!(
        error,
        GraphValidationError::AttachmentOpsOnComputePass(ref pass) if pass == "dispatch"
    ));
}

#[test]
fn depth_ops_without_depth_use_fail_validation() {
    let mut builder = super::super::builder::SimplePass::new("flat", PassType::Graphics)
        .write(BACKBUFFER_NAME)
        .attachment(BACKBUFFER_NAME, AttachmentOps::load())
        .as_builder();
    builder.uses_depth = false;
    builder.depth_attachment =
        Some(crate::render_pass::DepthStencilAttachmentOps::reverse_z_default());
    let error = match FrameGraphBuilder::new()
        .add_pass(AnonPass(builder))
        .build::<MockBackend>()
    {
        Err(RenderGraphError::Validation(error)) => error,
        Err(other) => panic!("expected validation error, got {other}"),
        Ok(_) => panic!("expected validation error, graph compiled"),
    };
    assert!(matches!(
        error,
        GraphValidationError::DepthOpsWithoutDepthUse(ref pass) if pass == "flat"
    ));
}

struct AnonPass(super::super::builder::InternalPassBuilder);
impl super::super::builder::PassBuilder for AnonPass {
    fn as_builder(self) -> super::super::builder::InternalPassBuilder {
        self.0
    }
}

#[test]
fn test_depth_pass_requires_an_explicit_graph_target() {
    let error = missing_ops_error(
        FrameGraphBuilder::new()
            .add_side_effect_pass(super::super::passes::DepthPrepass::new("depth_prepass")),
    );
    assert!(
        matches!(error, GraphValidationError::MissingDepthTarget { pass } if pass == "depth_prepass")
    );
}

#[test]
fn test_depth_target_rejects_a_color_attachment() {
    let error = missing_ops_error(
        FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .export_resource("color")
            .add_pass(
                super::super::passes::GeometryPass::new("geometry")
                    .write_color("color", crate::texture::ImageFormat::R8G8B8A8Unorm)
                    .depth_target("color"),
            ),
    );
    assert!(
        matches!(error, GraphValidationError::InvalidDepthTarget { pass, resource } if pass == "geometry" && resource == "color")
    );
}

#[test]
fn test_imported_buffer_rebinding_keeps_the_compiled_contract() {
    let desc = BufferDesc::new(64, BufferUsages::UNIFORM, BufferMemoryPolicy::CpuVisible);
    let mut graph = TestGraph::new();
    let id = graph
        .import_buffer("uniform", BufferHandle::from_raw(1, 0), desc)
        .unwrap();
    graph.compile().unwrap();
    graph
        .rebind_imported_buffer(id, BufferHandle::from_raw(2, 0))
        .unwrap();
    assert!(graph.compiled);
    assert_eq!(graph.buffer_desc("uniform"), Some(desc));
    assert!(
        graph
            .rebind_imported_buffer(id, BufferHandle::NONE)
            .is_err()
    );
    let resized = BufferDesc::new(128, BufferUsages::UNIFORM, BufferMemoryPolicy::CpuVisible);
    graph
        .redefine_imported_buffer(id, BufferHandle::from_raw(3, 0), resized)
        .unwrap();
    assert!(!graph.compiled);
    assert_eq!(graph.buffer_desc("uniform"), Some(resized));
}

#[test]
fn test_imported_buffer_retirement_rejects_live_references() {
    let desc = BufferDesc::new(64, BufferUsages::STORAGE, BufferMemoryPolicy::CpuVisible);
    let mut graph = TestGraph::new();
    let id = graph
        .import_buffer("animated", BufferHandle::from_raw(1, 0), desc)
        .unwrap();
    let next = graph
        .import_buffer("other", BufferHandle::from_raw(2, 0), desc)
        .unwrap();
    let pass = graph.add_pass(
        PassDesc::new("consume", PassType::Compute, vec![], vec![]).with_buffer_accesses([
            super::super::BufferAccess::new(
                id,
                ResourceAccessMode::Read,
                BufferUsage::Storage,
                ResourceAccessStage::ComputeShader,
                BufferByteRange::WHOLE,
            ),
        ]),
    );
    assert!(matches!(
        graph.remove_imported_buffer(id),
        Err(RenderGraphError::Validation(
            GraphValidationError::ImportedBufferStillInUse { .. }
        ))
    ));
    graph.set_pass_commands(pass, vec![], vec![]).unwrap();
    graph.remove_imported_buffer(id).unwrap();
    assert!(graph.imported_buffer_handle(id).is_none());
    assert_eq!(graph.resource_id("other"), Some(next));
    graph.compile().unwrap();
}

#[test]
fn depth_load_without_a_depth_producer_fails_validation() {
    let error = missing_ops_error(
        FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .create_resource(validation_depth_resource("depth"))
            .export_resource("color")
            .add_pass(
                super::super::builder::SimplePass::new("lone", PassType::Graphics)
                    .write("color")
                    .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                    .depth_ops(
                        AttachmentOps::clear(ClearValue::DEFAULT_DEPTH)
                            .with_load(crate::render_pass::LoadOp::Load),
                        AttachmentOps::dont_care(),
                    )
                    .depth_target("depth"),
            ),
    );
    assert!(matches!(
        error,
        GraphValidationError::LoadingUndefinedAttachment { pass, .. } if pass == "lone"
    ));
}

#[test]
fn graphics_depth_defaults_are_normalized_to_the_reverse_z_contract() {
    let graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("color", 64, 64))
        .create_resource(validation_depth_resource("depth"))
        .export_resource("color")
        .add_pass(
            super::super::builder::SimplePass::new("paint", PassType::Graphics)
                .write("color")
                .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                .depth_target("depth"),
        )
        .build::<MockBackend>()
        .unwrap();
    let ops = graph
        .pass(graph.pass_index("paint").unwrap())
        .unwrap()
        .depth_attachment
        .expect("depth contract normalized");
    assert_eq!(ops.depth.load, crate::render_pass::LoadOp::Clear);
    assert_eq!(ops.depth.store, crate::render_pass::StoreOp::Store);
    assert_eq!(
        ops.depth.clear_value,
        ClearValue::DepthStencil {
            depth: 0.0,
            stencil: 0
        }
    );
    assert_eq!(ops.stencil.load, crate::render_pass::LoadOp::Clear);
    assert_eq!(ops.stencil.store, crate::render_pass::StoreOp::DontCare);
}

#[test]
fn distinct_same_format_targets_keep_separate_declarations() {
    let graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("albedo", 64, 64))
        .create_resource(validation_resource("normals", 64, 64))
        .export_resource("albedo")
        .export_resource("normals")
        .add_pass(
            super::super::builder::SimplePass::new("mrt", PassType::Graphics)
                .write("albedo")
                .write("normals")
                .attachment("albedo", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                .attachment(
                    "normals",
                    AttachmentOps::clear(ClearValue::TRANSPARENT_BLACK),
                ),
        )
        .build::<MockBackend>()
        .unwrap();
    let pass = graph.pass(graph.pass_index("mrt").unwrap()).unwrap();
    assert_eq!(pass.color_attachments.len(), 2);
    assert_ne!(pass.color_attachments[0].0, pass.color_attachments[1].0);
    assert_ne!(pass.color_attachments[0].1, pass.color_attachments[1].1);
}

#[test]
fn out_of_range_depth_clear_values_fail_validation() {
    let error = missing_ops_error(
        FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .create_resource(validation_depth_resource("depth"))
            .export_resource("color")
            .add_pass(AnonPass({
                let mut builder =
                    super::super::builder::SimplePass::new("bad_depth", PassType::Graphics)
                        .write("color")
                        .attachment("color", AttachmentOps::clear(ClearValue::OPAQUE_BLACK))
                        .as_builder();
                builder.uses_depth = true;
                builder.depth_target = Some("depth".into());
                builder.depth_attachment =
                    Some(crate::render_pass::DepthStencilAttachmentOps::clear(
                        ClearValue::DepthStencil {
                            depth: 1.5,
                            stencil: 0,
                        },
                    ));
                builder
            })),
    );
    assert!(matches!(
        error,
        GraphValidationError::InvalidDepthClearValue { pass, depth }
            if pass == "bad_depth" && depth == 1.5
    ));
}

#[test]
fn builder_rejects_empty_names() {
    assert_eq!(
        validation_error(FrameGraphBuilder::new().create_resource(validation_resource("", 1, 1))),
        GraphValidationError::EmptyResourceName
    );
    assert_eq!(
        validation_error(FrameGraphBuilder::new().add_pass(
            super::super::builder::SimplePass::new("", PassType::Graphics)
        )),
        GraphValidationError::EmptyPassName
    );
}

#[test]
fn builder_accepts_declared_resources_and_builtin_backbuffer() {
    let result = FrameGraphBuilder::new()
        .create_resource(validation_resource("color", 64, 64))
        .add_pass(
            super::super::builder::SimplePass::new("geometry", PassType::Graphics)
                .write("color")
                .attachment(
                    "color",
                    crate::render_pass::AttachmentOps::clear(
                        crate::render_pass::ClearValue::OPAQUE_BLACK,
                    ),
                ),
        )
        .add_pass(
            super::super::builder::SimplePass::new("present", PassType::Graphics)
                .read("color")
                .write(BACKBUFFER_NAME)
                .attachment(BACKBUFFER_NAME, crate::render_pass::AttachmentOps::load()),
        )
        .build::<MockBackend>();
    assert!(result.is_ok());
}

#[test]
fn builder_culls_unobserved_branches_but_keeps_the_backbuffer_chain() {
    let graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("dead", 64, 64))
        .add_pass(
            super::super::builder::SimplePass::new("dead_branch", PassType::Graphics).write("dead"),
        )
        .add_pass(
            super::super::builder::SimplePass::new("present", PassType::Graphics)
                .write(BACKBUFFER_NAME)
                .attachment(BACKBUFFER_NAME, crate::render_pass::AttachmentOps::load()),
        )
        .build::<MockBackend>()
        .unwrap();

    let dead = graph.pass_id("dead_branch").unwrap();
    let present = graph.pass_id("present").unwrap();
    assert_eq!(graph.is_pass_live(dead), Some(false));
    assert_eq!(graph.is_pass_live(present), Some(true));
    assert_eq!(graph.execution_order(), vec![present.0 as usize]);
}

#[test]
fn submissions_to_culled_passes_fail_with_a_structured_error() {
    let graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("dead", 64, 64))
        .add_pass(
            super::super::builder::SimplePass::new("dead_branch", PassType::Graphics).write("dead"),
        )
        .build::<MockBackend>()
        .unwrap();
    let dead = graph.pass_id("dead_branch").unwrap();
    let mut backend = MockBackend::new();
    let mut frame = super::super::frame::Frame::new(&graph, &mut backend, 0, 0);
    frame.submit(
        dead,
        std::rc::Rc::new(crate::renderer::types::DrawList::new()),
    );

    assert!(matches!(
        frame.validate_submissions(),
        Err(RenderGraphError::SubmissionToCulledPass(name)) if name == "dead_branch"
    ));
}

#[test]
fn explicit_offscreen_export_keeps_its_producer_chain() {
    let graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("intermediate", 64, 64))
        .create_resource(validation_resource("readback", 64, 64))
        .export_resource("readback")
        .add_pass(
            super::super::builder::SimplePass::new("produce", PassType::Graphics)
                .write("intermediate")
                .attachment(
                    "intermediate",
                    crate::render_pass::AttachmentOps::clear(
                        crate::render_pass::ClearValue::OPAQUE_BLACK,
                    ),
                ),
        )
        .add_pass(
            super::super::builder::SimplePass::new("copy_for_readback", PassType::Graphics)
                .read("intermediate")
                .write("readback")
                .attachment(
                    "readback",
                    crate::render_pass::AttachmentOps::clear(
                        crate::render_pass::ClearValue::OPAQUE_BLACK,
                    ),
                ),
        )
        .build::<MockBackend>()
        .unwrap();

    assert_eq!(graph.execution_order(), vec![0, 1]);
    assert_eq!(
        graph.is_pass_live(graph.pass_id("produce").unwrap()),
        Some(true)
    );
    assert_eq!(
        graph.is_pass_live(graph.pass_id("copy_for_readback").unwrap()),
        Some(true)
    );
}

#[test]
fn explicit_side_effect_keeps_data_producers_without_fake_outputs() {
    let graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("query_input", 64, 64))
        .add_pass(
            super::super::builder::SimplePass::new("produce_query_data", PassType::Graphics)
                .write("query_input")
                .attachment(
                    "query_input",
                    crate::render_pass::AttachmentOps::clear(
                        crate::render_pass::ClearValue::OPAQUE_BLACK,
                    ),
                ),
        )
        .add_side_effect_pass(
            super::super::builder::SimplePass::new("timestamp_readback", PassType::Graphics)
                .read("query_input"),
        )
        .build::<MockBackend>()
        .unwrap();

    assert_eq!(graph.execution_order(), vec![0, 1]);
    assert!(
        graph
            .pass(graph.pass_index("timestamp_readback").unwrap())
            .unwrap()
            .side_effect
    );
}

#[test]
fn builder_rejects_undeclared_exports() {
    assert_eq!(
        validation_error(FrameGraphBuilder::new().export_resource("typo_output")),
        GraphValidationError::UndeclaredExportedResource("typo_output".to_string())
    );
}

#[test]
fn transient_initialization_rejects_a_missing_namespace_entry() {
    let mut graph = TestGraph::new();
    graph
        .transient_resources
        .push(validation_resource("orphan", 1, 1));

    let error = graph
        .initialize_transient_textures(&MockBackend::new())
        .unwrap_err();
    assert!(matches!(
        error,
        RenderGraphError::Validation(
            GraphValidationError::MissingResourceNamespaceEntry(resource)
        ) if resource == "orphan"
    ));
}

#[test]
fn test_culled_textures_have_no_native_allocation_in_either_debug_mode() {
    use super::super::builder::SimplePass;
    let mut graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("live", 16, 16))
        .create_resource(validation_resource("dead", 16, 16))
        .add_side_effect_pass(
            SimplePass::new("live pass", PassType::Graphics)
                .without_depth()
                .write("live")
                .attachment("live", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
        )
        .add_pass(
            SimplePass::new("dead pass", PassType::Graphics)
                .without_depth()
                .write("dead")
                .attachment("dead", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
        )
        .build::<MockBackend>()
        .unwrap();
    for optimize in [true, false] {
        graph.cleanup();
        graph.set_transient_aliasing(optimize).unwrap();
        let backend = MockBackend::new();
        graph.initialize_transient_textures(&backend).unwrap();
        assert_eq!(*backend.slot_member_counts.borrow(), vec![1, 1]);
        assert!(graph.transient_texture("dead", 0).is_none());
    }
}

#[test]
fn test_alias_handoffs_are_cached_for_first_use_and_debug_mode_rejects_live_change() {
    use super::super::builder::SimplePass;
    let mut graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("early", 16, 16))
        .create_resource(validation_resource("late", 16, 16))
        .add_side_effect_pass(
            SimplePass::new("early pass", PassType::Graphics)
                .without_depth()
                .write("early")
                .attachment("early", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
        )
        .add_side_effect_pass(
            SimplePass::new("late pass", PassType::Graphics)
                .without_depth()
                .write("late")
                .attachment("late", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
        )
        .build::<MockBackend>()
        .unwrap();
    assert!(graph.texture_alias_handoff_before(0));
    assert!(graph.texture_alias_handoff_before(1));
    graph
        .initialize_transient_textures(&MockBackend::new())
        .unwrap();
    assert!(graph.set_transient_aliasing(false).is_err());
    assert!(graph.texture_alias_handoff_before(0));
    graph.cleanup();
    graph.set_transient_aliasing(false).unwrap();
    graph.compile().unwrap();
    assert!(!graph.texture_alias_handoff_before(0));
    let diagnostics = graph.diagnostics().unwrap();
    assert_eq!(diagnostics.summary.physical_transient_allocations, 2);
    assert_eq!(diagnostics.summary.transient_alias_savings_bytes, 0);
}

#[test]
fn test_memoryless_policy_requires_tile_local_discard_and_owns_frame_slot() {
    use super::super::builder::SimplePass;
    use crate::render_pass::StoreOp;
    let mut graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("tile", 16, 16))
        .add_side_effect_pass(
            SimplePass::new("tile pass", PassType::Graphics)
                .without_depth()
                .write("tile")
                .attachment(
                    "tile",
                    AttachmentOps::clear(ClearValue::Color([0.0; 4])).with_store(StoreOp::DontCare),
                ),
        )
        .build::<MockBackend>()
        .unwrap();
    let backend = MockBackend::new();
    graph.initialize_transient_textures(&backend).unwrap();
    let policies = backend.policies.borrow();
    assert!(policies.iter().all(|policy| policy.memoryless));
    assert_eq!(
        policies
            .iter()
            .map(|policy| policy.frame_slot)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    drop(policies);
    graph.cleanup();
    graph.set_transient_aliasing(false).unwrap();
    let backend = MockBackend::new();
    graph.initialize_transient_textures(&backend).unwrap();
    assert!(
        backend
            .policies
            .borrow()
            .iter()
            .all(|policy| !policy.memoryless && !policy.optimize)
    );
}

#[test]
fn test_store_action_prevents_memoryless_even_for_one_pass() {
    use super::super::builder::SimplePass;
    let mut graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("stored", 16, 16))
        .add_side_effect_pass(
            SimplePass::new("store pass", PassType::Graphics)
                .without_depth()
                .write("stored")
                .attachment("stored", AttachmentOps::clear(ClearValue::Color([0.0; 4]))),
        )
        .build::<MockBackend>()
        .unwrap();
    let backend = MockBackend::new();
    graph.initialize_transient_textures(&backend).unwrap();
    assert!(
        backend
            .policies
            .borrow()
            .iter()
            .all(|policy| !policy.memoryless)
    );
}

#[test]
fn transient_aliasing_groups_non_overlapping_compatible_transients() {
    let mut graph = TestGraph::new();
    graph.create_resource_id("early");
    graph.create_resource_id("late");
    graph.transient_resources = vec![
        validation_resource("early", 64, 64),
        validation_resource("late", 64, 64),
    ];
    graph.add_pass(PassDesc::new(
        "first",
        PassType::Graphics,
        vec![],
        vec![ResourceId(0)],
    ));
    graph.add_pass(PassDesc::new(
        "second",
        PassType::Graphics,
        vec![],
        vec![ResourceId(1)],
    ));

    let backend = MockBackend::new();
    graph.initialize_transient_textures(&backend).unwrap();

    // One two-member slot per frame in flight.
    assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2]);
}

#[test]
fn test_allocation_contract_rejects_new_overlap_until_cleanup() {
    let mut graph = TestGraph::new();
    let early = graph.create_resource_id("early");
    let late = graph.create_resource_id("late");
    graph.transient_resources = vec![
        validation_resource("early", 64, 64),
        validation_resource("late", 64, 64),
    ];
    graph.add_pass(PassDesc::new(
        "early write",
        PassType::Graphics,
        vec![],
        vec![early],
    ));
    graph.add_pass(PassDesc::new(
        "late write",
        PassType::Graphics,
        vec![],
        vec![late],
    ));
    let backend = MockBackend::new();
    graph.initialize_transient_textures(&backend).unwrap();
    assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2]);
    graph.add_pass(PassDesc::new(
        "later early read",
        PassType::Graphics,
        vec![early],
        vec![],
    ));
    assert!(matches!(
        graph.initialize_transient_textures(&backend),
        Err(RenderGraphError::AllocationContractChanged)
    ));
    assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2]);
    assert!(graph.transient_texture("early", 0).is_some());
    graph.cleanup();
    graph.initialize_transient_textures(&backend).unwrap();
    assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2, 1, 1, 1, 1]);
}

#[test]
fn test_allocation_contract_rejects_sampling_old_memoryless_storage() {
    use super::super::builder::SimplePass;
    use crate::render_pass::StoreOp;
    let mut graph = FrameGraphBuilder::new()
        .create_resource(validation_resource("tile", 16, 16))
        .add_side_effect_pass(
            SimplePass::new("tile pass", PassType::Graphics)
                .without_depth()
                .write("tile")
                .attachment(
                    "tile",
                    AttachmentOps {
                        load: LoadOp::Clear,
                        store: StoreOp::DontCare,
                        clear_value: ClearValue::Color([0.0; 4]),
                    },
                ),
        )
        .build::<MockBackend>()
        .unwrap();
    let backend = MockBackend::new();
    graph.initialize_transient_textures(&backend).unwrap();
    assert!(
        backend
            .policies
            .borrow()
            .iter()
            .all(|policy| policy.memoryless)
    );
    let tile = graph.resource_id("tile").unwrap();
    let mut sample = PassDesc::new("sample tile", PassType::Compute, vec![tile], vec![]);
    sample.side_effect = true;
    graph.add_pass(sample);
    assert!(matches!(
        graph.initialize_transient_textures(&backend),
        Err(RenderGraphError::AllocationContractChanged)
    ));
    graph.cleanup();
    graph.initialize_transient_textures(&backend).unwrap();
    assert!(
        backend
            .policies
            .borrow()
            .iter()
            .skip(2)
            .all(|policy| !policy.memoryless)
    );
}

#[test]
fn test_allocation_contract_accepts_changed_disjoint_intervals() {
    let mut graph = TestGraph::new();
    let early = graph.create_resource_id("early");
    let late = graph.create_resource_id("late");
    graph.transient_resources = vec![
        validation_resource("early", 64, 64),
        validation_resource("late", 64, 64),
    ];
    graph.add_pass(PassDesc::new(
        "early write",
        PassType::Graphics,
        vec![],
        vec![early],
    ));
    graph.add_pass(PassDesc::new(
        "late write",
        PassType::Graphics,
        vec![],
        vec![late],
    ));
    let backend = MockBackend::new();
    graph.initialize_transient_textures(&backend).unwrap();
    graph.insert_pass(
        1,
        PassDesc::new("early read", PassType::Graphics, vec![early], vec![]),
    );
    graph.initialize_transient_textures(&backend).unwrap();
    assert_eq!(*backend.slot_member_counts.borrow(), vec![2, 2]);
}

#[test]
fn transient_aliasing_keeps_overlapping_transients_separate() {
    let mut graph = TestGraph::new();
    graph.create_resource_id("a");
    graph.create_resource_id("b");
    graph.transient_resources = vec![
        validation_resource("a", 64, 64),
        validation_resource("b", 64, 64),
    ];
    graph.add_pass(PassDesc::new(
        "write_a",
        PassType::Graphics,
        vec![],
        vec![ResourceId(0)],
    ));
    graph.add_pass(PassDesc::new(
        "write_b",
        PassType::Graphics,
        vec![],
        vec![ResourceId(1)],
    ));
    graph.add_pass(PassDesc::new(
        "read_a",
        PassType::Graphics,
        vec![ResourceId(0)],
        vec![],
    ));

    let backend = MockBackend::new();
    graph.initialize_transient_textures(&backend).unwrap();

    // `a` is live across `b`'s write, so the two never share a slot.
    assert_eq!(*backend.slot_member_counts.borrow(), vec![1, 1, 1, 1]);
}

#[test]
fn transient_aliasing_disabled_creates_standalone_textures() {
    let mut graph = TestGraph::new();
    graph.create_resource_id("early");
    graph.create_resource_id("late");
    graph.transient_resources = vec![
        validation_resource("early", 64, 64),
        validation_resource("late", 64, 64),
    ];
    graph.add_pass(PassDesc::new(
        "first",
        PassType::Graphics,
        vec![],
        vec![ResourceId(0)],
    ));
    graph.add_pass(PassDesc::new(
        "second",
        PassType::Graphics,
        vec![],
        vec![ResourceId(1)],
    ));
    graph.set_transient_aliasing(false).unwrap();

    let backend = MockBackend::new();
    graph.initialize_transient_textures(&backend).unwrap();

    assert_eq!(*backend.slot_member_counts.borrow(), vec![1, 1, 1, 1]);
}

// --- Typed template declarations keep cross-aspect hazards ordered (#30) ---

#[test]
fn sampling_a_depth_atlas_orders_after_the_shadow_pass_and_keeps_it_live() {
    use super::super::passes::{FullscreenPass, GeometryPass, ShadowPass};
    use crate::GraphResourceType;
    use crate::ImageFormat;

    // Editor-graph shape: sky produces hdr_color, shadow clears the depth
    // atlas, geometry loads hdr_color and samples the atlas, tonemap
    // presents through the exported backbuffer. The sampled read covers
    // every aspect of the atlas (the graph cannot narrow sampling to the
    // depth aspect without the image format), so the shadow write and the
    // geometry read must stay RAW-ordered and liveness must keep the
    // shadow pass.
    let graph = FrameGraphBuilder::new()
        .create_resource(GraphResourceDesc {
            name: "hdr_color".to_string(),
            resource_type: GraphResourceType::ColorAttachment {
                clear_value: Some([0.0; 4]),
            },
            format: ImageFormat::R16G16B16A16Sfloat,
            width: 64,
            height: 64,
            tracks_swapchain_size: false,
        })
        .create_resource(GraphResourceDesc {
            name: "shadow_atlas".to_string(),
            resource_type: GraphResourceType::DepthAttachment {
                clear_value: 1.0,
                sampled: true,
            },
            format: ImageFormat::D32Sfloat,
            width: 64,
            height: 64,
            tracks_swapchain_size: false,
        })
        .add_pass(FullscreenPass::new("sky").write("hdr_color", ImageFormat::R16G16B16A16Sfloat))
        .add_pass(ShadowPass::new("shadow").write_depth("shadow_atlas", ImageFormat::D32Sfloat))
        .add_pass(
            GeometryPass::new("geometry")
                .write_color_ops(
                    "hdr_color",
                    ImageFormat::R16G16B16A16Sfloat,
                    AttachmentOps::load(),
                )
                .read("shadow_atlas")
                .without_depth(),
        )
        .add_pass(
            FullscreenPass::new("tonemap")
                .read("hdr_color")
                .write_backbuffer(),
        )
        .build::<MockBackend>()
        .unwrap();

    let plan = graph.build_execution_plan().unwrap();
    assert!(
        plan.sorted_passes.contains(&1),
        "shadow pass must stay live: geometry samples its atlas"
    );
    assert!(
        plan.dag[2].predecessors.contains(&1),
        "geometry must depend on the shadow pass it samples from"
    );
}

// --- Imported-image state contracts (#30) ---

#[test]
fn test_unused_imported_image_compiles_its_final_transition() {
    let graph = FrameGraphBuilder::new()
        .import_resource(
            "external",
            TextureHandle::from_raw(7, 0),
            ImportedImageContract::arrives_in(ResourceState::ShaderRead)
                .must_end_in(ResourceState::TransferSrc),
        )
        .build::<MockBackend>()
        .unwrap();
    let ops = graph.final_image_sync_ops();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].before_pass, None);
    assert_eq!(ops[0].reason, super::super::SyncReason::ImportedFinal);
}

#[test]
fn imported_final_state_is_reachable_when_a_live_pass_accesses_the_image() {
    FrameGraphBuilder::new()
        .import_resource(
            "external",
            TextureHandle::from_raw(7, 0),
            ImportedImageContract::arrives_in(ResourceState::ShaderRead)
                .must_end_in(ResourceState::TransferSrc),
        )
        .add_side_effect_pass(
            super::super::builder::SimplePass::new("readback", PassType::Compute).read("external"),
        )
        .build::<MockBackend>()
        .unwrap();
}

#[test]
fn backbuffer_loads_rely_on_the_default_contract_and_undefining_it_fails() {
    // The default backbuffer contract declares observable contents, so a
    // UI-only graph may load the backbuffer without an in-graph producer.
    FrameGraphBuilder::new()
        .add_pass(
            super::super::builder::SimplePass::new("overlay", PassType::Graphics)
                .write(BACKBUFFER_NAME)
                .attachment(BACKBUFFER_NAME, AttachmentOps::load()),
        )
        .build::<MockBackend>()
        .unwrap();

    // Overriding the contract to Undefined removes that guarantee.
    let error = validation_error(
        FrameGraphBuilder::new()
            .backbuffer_contract(ImportedImageContract::undefined())
            .add_pass(
                super::super::builder::SimplePass::new("overlay", PassType::Graphics)
                    .write(BACKBUFFER_NAME)
                    .attachment(BACKBUFFER_NAME, AttachmentOps::load()),
            ),
    );
    assert!(matches!(
        error,
        GraphValidationError::LoadingUninitializedImport { pass, resource: 0, aspects }
            if pass == "overlay" && aspects == super::super::ImageAspects::COLOR
    ));
}
#[test]
fn test_duplicate_native_import_identities_are_rejected_before_scheduling() {
    let texture = TextureHandle::from_raw(8, 2);
    let error = validation_error(
        FrameGraphBuilder::new()
            .import_resource("first", texture, ImportedImageContract::undefined())
            .import_resource("second", texture, ImportedImageContract::undefined()),
    );
    assert!(matches!(
        error,
        GraphValidationError::DuplicateImportedIdentity { kind: "image", .. }
    ));
    let buffer = BufferHandle::from_raw(8, 2);
    let desc = BufferDesc::new(64, BufferUsages::STORAGE, BufferMemoryPolicy::DeviceLocal);
    let error = validation_error(
        FrameGraphBuilder::new()
            .import_buffer("first", buffer, desc)
            .import_buffer("second", buffer, desc),
    );
    assert!(matches!(
        error,
        GraphValidationError::DuplicateImportedIdentity { kind: "buffer", .. }
    ));
}
