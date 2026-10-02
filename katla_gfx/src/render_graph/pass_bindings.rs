//! Validate explicit graphics inputs against the canonical graph accesses.

use super::{BufferUsage, GraphValidationError, RenderGraphError, ResourceAccessStage};
use std::collections::BTreeSet;

pub(crate) fn validate(
    pass: &super::PassDesc,
    bindings: &crate::renderer::frame_bindings::PassBindings,
) -> Result<(), RenderGraphError> {
    use super::ResourceAccessUsage;
    use crate::backend::command::ShaderStages;
    let stage_matches = |stage, stages: ShaderStages| match stage {
        ResourceAccessStage::VertexShader => stages.vertex,
        ResourceAccessStage::FragmentShader => stages.fragment,
        ResourceAccessStage::ComputeShader => stages.compute,
        ResourceAccessStage::AllGraphics => stages.vertex || stages.fragment,
        _ => false,
    };
    {
        let invalid = |reason: String| GraphValidationError::InvalidPassBinding {
            pass: pass.name.clone(),
            reason,
        };
        let mut slots = BTreeSet::new();
        for binding in &bindings.buffers {
            if binding.stages.is_empty() || binding.range.is_empty() {
                return Err(
                    invalid("Buffer binding has no shader stages or byte range".into()).into(),
                );
            }
            for (stage, active) in [
                (ResourceAccessStage::VertexShader, binding.stages.vertex),
                (ResourceAccessStage::FragmentShader, binding.stages.fragment),
                (ResourceAccessStage::ComputeShader, binding.stages.compute),
            ] {
                if !active {
                    continue;
                }
                if !slots.insert((stage as u8, binding.group, binding.binding)) {
                    return Err(invalid(format!(
                        "Duplicate shader slot {}:{}",
                        binding.group, binding.binding
                    ))
                    .into());
                }
                if !pass.buffer_accesses.iter().any(|access| {
                    access.resource == binding.resource
                        && matches!(access.usage, BufferUsage::Uniform | BufferUsage::Storage)
                        && stage_matches(
                            access.stage,
                            ShaderStages {
                                vertex: stage == ResourceAccessStage::VertexShader,
                                fragment: stage == ResourceAccessStage::FragmentShader,
                                compute: stage == ResourceAccessStage::ComputeShader,
                            },
                        )
                        && access.range.intersection(binding.range) == Some(binding.range)
                }) {
                    return Err(invalid(format!(
                        "Buffer r{} at {}:{} exceeds its declared range or shader stage",
                        binding.resource.0, binding.group, binding.binding
                    ))
                    .into());
                }
            }
        }
        for binding in &bindings.images {
            if binding.stages.is_empty() || binding.range.is_empty() {
                return Err(invalid(
                    "Image binding has no shader stages or subresource range".into(),
                )
                .into());
            }
            for (stage, active) in [
                (ResourceAccessStage::VertexShader, binding.stages.vertex),
                (ResourceAccessStage::FragmentShader, binding.stages.fragment),
                (ResourceAccessStage::ComputeShader, binding.stages.compute),
            ] {
                if !active {
                    continue;
                }
                if !slots.insert((stage as u8, binding.group, binding.binding)) {
                    return Err(invalid(format!(
                        "Duplicate shader slot {}:{}",
                        binding.group, binding.binding
                    ))
                    .into());
                }
                if !pass.image_accesses.iter().any(|access| {
                    access.resource == binding.resource
                        && access.mode.reads()
                        && access.usage == ResourceAccessUsage::Sampled
                        && stage_matches(
                            access.stage,
                            ShaderStages {
                                vertex: stage == ResourceAccessStage::VertexShader,
                                fragment: stage == ResourceAccessStage::FragmentShader,
                                compute: stage == ResourceAccessStage::ComputeShader,
                            },
                        )
                        && access.range.intersection(binding.range) == Some(binding.range)
                }) {
                    return Err(invalid(format!(
                        "Image r{} at {}:{} exceeds its declared sampled access",
                        binding.resource.0, binding.group, binding.binding
                    ))
                    .into());
                }
            }
        }
        for phase in &bindings.phases {
            if let Some(viewport) = phase.viewport
                && (!viewport
                    .min
                    .into_iter()
                    .chain(viewport.max)
                    .all(f32::is_finite)
                    || !viewport.width().is_finite()
                    || !viewport.height().is_finite()
                    || viewport.width() <= 0.0
                    || viewport.height() <= 0.0)
            {
                return Err(invalid(
                    "Viewport must have finite coordinates and positive extent".into(),
                )
                .into());
            }
            if let crate::renderer::frame_bindings::PassDraw::Indirect { resource, offset } =
                phase.draw
            {
                let range = super::BufferByteRange::new(offset, 16);
                if !offset.is_multiple_of(4)
                    || offset.checked_add(16).is_none()
                    || !pass.buffer_accesses.iter().any(|access| {
                        access.resource == resource
                            && access.usage == BufferUsage::Indirect
                            && access.mode == super::ResourceAccessMode::Read
                            && access.stage == ResourceAccessStage::DrawIndirect
                            && access.range.intersection(range) == Some(range)
                    })
                {
                    return Err(invalid(format!(
                        "Indirect draw r{} at byte {} lacks an aligned declared command range",
                        resource.0, offset
                    ))
                    .into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::command::ShaderStages;
    use crate::render_graph::{
        BufferAccess, BufferByteRange, ImageAccess, ImageAspects, ImageSubresourceRange, PassDesc,
        PassType, ResourceAccessMode, ResourceAccessUsage, ResourceId,
    };
    use crate::renderer::frame_bindings::{
        BufferBinding, ImageBinding, PassBindings, PassDraw, PassDrawPhase,
    };

    fn bound_buffer() -> PassDesc {
        PassDesc::new("custom", PassType::Graphics, vec![], vec![])
            .with_buffer_accesses([BufferAccess::new(
                ResourceId(1),
                ResourceAccessMode::Read,
                BufferUsage::Uniform,
                ResourceAccessStage::VertexShader,
                BufferByteRange::new(16, 32),
            )])
            .with_bindings(PassBindings {
                buffers: vec![BufferBinding {
                    group: 0,
                    binding: 0,
                    resource: ResourceId(1),
                    range: BufferByteRange::new(16, 32),
                    stages: ShaderStages::VERTEX,
                }],
                ..Default::default()
            })
    }

    #[test]
    fn test_graph_binding_rejects_missing_access_and_larger_byte_range() {
        let mut pass = bound_buffer();
        validate(&pass, &pass.bindings).unwrap();
        pass.bindings.buffers[0].range.size = 36;
        assert!(validate(&pass, &pass.bindings).is_err());
        pass.bindings.buffers[0].range.size = 32;
        pass.set_buffer_accesses(Vec::new());
        assert!(validate(&pass, &pass.bindings).is_err());
    }

    #[test]
    fn test_graph_binding_requires_every_selected_shader_stage() {
        let mut pass = bound_buffer();
        pass.bindings.buffers[0].stages = ShaderStages::VERTEX_FRAGMENT;
        assert!(validate(&pass, &pass.bindings).is_err());
        pass.buffer_accesses[0].stage = ResourceAccessStage::AllGraphics;
        validate(&pass, &pass.bindings).unwrap();
    }

    #[test]
    fn test_graph_image_binding_respects_mip_range_and_sampled_usage() {
        let range = ImageSubresourceRange::new(ImageAspects::COLOR, 1, 1, 0, 1);
        let mut pass = PassDesc::new("sample", PassType::Graphics, vec![], vec![])
            .with_image_accesses([ImageAccess::new(
                ResourceId(2),
                ResourceAccessMode::Read,
                ResourceAccessUsage::Sampled,
                ResourceAccessStage::FragmentShader,
                range,
            )])
            .with_bindings(PassBindings {
                images: vec![ImageBinding {
                    group: 0,
                    binding: 2,
                    resource: ResourceId(2),
                    range,
                    stages: ShaderStages::FRAGMENT,
                }],
                ..Default::default()
            });
        validate(&pass, &pass.bindings).unwrap();
        pass.bindings.images[0].range.base_mip_level = 0;
        assert!(validate(&pass, &pass.bindings).is_err());
        pass.bindings.images[0].range = range;
        pass.image_accesses[0].usage = ResourceAccessUsage::Storage;
        assert!(validate(&pass, &pass.bindings).is_err());
    }

    #[test]
    fn test_graph_indirect_draw_requires_complete_aligned_range() {
        let mut pass = PassDesc::new("indirect", PassType::Graphics, vec![], vec![])
            .with_buffer_accesses([BufferAccess::new(
                ResourceId(3),
                ResourceAccessMode::Read,
                BufferUsage::Indirect,
                ResourceAccessStage::DrawIndirect,
                BufferByteRange::new(4, 16),
            )])
            .with_bindings(PassBindings {
                phases: vec![PassDrawPhase {
                    pipelines: vec![],
                    constants: vec![],
                    draw: PassDraw::Indirect {
                        resource: ResourceId(3),
                        offset: 4,
                    },
                    viewport: None,
                }],
                ..Default::default()
            });
        validate(&pass, &pass.bindings).unwrap();
        pass.buffer_accesses[0].range.size = 12;
        assert!(validate(&pass, &pass.bindings).is_err());
    }
    #[test]
    fn test_graph_draw_phase_rejects_nonfinite_and_empty_viewports() {
        let phase = |viewport| PassDrawPhase {
            pipelines: vec![],
            constants: vec![],
            draw: PassDraw::Vertices {
                count: 3,
                instances: 1,
            },
            viewport: Some(viewport),
        };
        let mut pass = PassDesc::new("viewport", PassType::Graphics, vec![], vec![]).with_bindings(
            PassBindings {
                phases: vec![phase(crate::Rect::new([0.0, 0.0], [64.0, 64.0]))],
                ..Default::default()
            },
        );
        validate(&pass, &pass.bindings).unwrap();
        for viewport in [
            crate::Rect::new([0.0, 0.0], [0.0, 64.0]),
            crate::Rect::new([f32::NAN, 0.0], [64.0, 64.0]),
            crate::Rect::new([0.0, 0.0], [f32::INFINITY, 64.0]),
        ] {
            pass.bindings.phases[0] = phase(viewport);
            assert!(validate(&pass, &pass.bindings).is_err());
        }
    }
}
