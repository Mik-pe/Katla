//! Replacement packets validate against immutable graph access contracts.

use super::*;
use crate::backend::command::ShaderStages;
use crate::renderer::frame_bindings::{BufferBinding, PassBindings};

fn graph() -> (FrameGraph<crate::VulkanRenderer>, PassId, ResourceId) {
    let mut graph = FrameGraph::new();
    let resource = graph
        .import_buffer(
            "uniform",
            BufferHandle::from_raw(1, 0),
            BufferDesc::new(64, BufferUsages::UNIFORM, BufferMemoryPolicy::CpuVisible),
        )
        .unwrap();
    let pass = graph
        .add_pass(
            PassDesc::new("draw", PassType::Graphics, vec![], vec![]).with_buffer_accesses([
                super::super::BufferAccess::new(
                    resource,
                    ResourceAccessMode::Read,
                    BufferUsage::Uniform,
                    ResourceAccessStage::VertexShader,
                    BufferByteRange::new(0, 64),
                ),
            ]),
        )
        .unwrap();
    graph
        .set_pass_bindings(pass, packet(resource, 0, 16))
        .unwrap();
    graph.compile().unwrap();
    (graph, pass, resource)
}

fn packet(resource: ResourceId, offset: u64, size: u64) -> PassBindings {
    PassBindings {
        buffers: vec![BufferBinding {
            group: 0,
            binding: 0,
            resource,
            range: BufferByteRange::new(offset, size),
            stages: ShaderStages::VERTEX,
        }],
        ..Default::default()
    }
}

#[test]
fn test_valid_binding_replacement_preserves_the_compiled_plan() {
    let (mut graph, pass, resource) = graph();
    let plan = graph.execution_plan.as_ref().unwrap() as *const ExecutionPlan;
    for offset in [0, 16, 32, 48, 0] {
        graph
            .set_pass_bindings(pass, packet(resource, offset, 16))
            .unwrap();
        assert!(graph.compiled);
        assert!(std::ptr::eq(plan, graph.execution_plan.as_ref().unwrap()));
        assert_eq!(graph.passes[0].bindings.buffers[0].range.offset, offset);
    }
}

#[test]
fn test_invalid_binding_replacement_preserves_the_previous_packet_and_plan() {
    let (mut graph, pass, resource) = graph();
    let error = graph
        .set_pass_bindings(pass, packet(resource, 48, 32))
        .unwrap_err();
    assert!(matches!(
        error,
        RenderGraphError::Validation(GraphValidationError::InvalidPassBinding { .. })
    ));
    assert!(graph.compiled);
    assert_eq!(
        graph.passes[0].bindings.buffers[0].range,
        BufferByteRange::new(0, 16)
    );
    graph
        .set_pass_bindings(pass, packet(resource, 32, 16))
        .unwrap();
    assert!(graph.compiled);
}
