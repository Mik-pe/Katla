//! Reflected compute-pass construction using named graph buffers.
use super::super::access::NamedBufferAccess;
use super::super::builder::{InternalPassBuilder, PassBuilder, SimplePass};
use super::super::{
    BufferByteRange, BufferUsage, ComputeBinding, ComputeCommand, ComputeDispatch,
    ComputeDispatchSize, ComputePipelineDesc, PassType, RenderGraphError, ResourceAccessStage,
};

/// A reflected compute entry point with named buffer bindings.
pub struct ComputePass {
    name: String,
    pipeline: ComputePipelineDesc,
    bindings: Vec<(u32, u32, String, BufferByteRange)>,
    size: ComputeDispatchSize,
    constants: Vec<u8>,
    indirect: Option<(String, u64)>,
}

impl ComputePass {
    pub fn new(name: impl Into<String>, descriptor: ComputePipelineDesc) -> Self {
        Self {
            name: name.into(),
            pipeline: descriptor,
            bindings: Vec::new(),
            size: ComputeDispatchSize::Direct([1, 1, 1]),
            constants: Vec::new(),
            indirect: None,
        }
    }
    pub fn bind_buffer(
        mut self,
        group: u32,
        binding: u32,
        name: impl Into<String>,
        range: BufferByteRange,
    ) -> Self {
        self.bindings.push((group, binding, name.into(), range));
        self
    }
    pub fn dispatch(mut self, groups: [u32; 3]) -> Self {
        self.size = ComputeDispatchSize::Direct(groups);
        self.indirect = None;
        self
    }
    /// Read three dispatch dimensions from a declared graph buffer.
    pub fn dispatch_indirect(mut self, name: impl Into<String>, offset: u64) -> Self {
        self.indirect = Some((name.into(), offset));
        self
    }
    pub fn constants(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.constants = data.into();
        self
    }
}

impl PassBuilder for ComputePass {
    fn as_builder(self) -> InternalPassBuilder {
        let pass_name = self.name.clone();
        let mut builder = SimplePass::new(self.name, PassType::Compute).as_builder();
        builder.uses_depth = false;
        let interface = self.pipeline.interface();
        if let Ok(ref reflected) = interface {
            for (group, binding, name, range) in &self.bindings {
                if let Some(slot) = reflected
                    .bindings
                    .iter()
                    .find(|slot| slot.group == *group && slot.binding == *binding)
                {
                    builder.buffer_accesses.push(NamedBufferAccess {
                        resource: name.clone(),
                        mode: slot.mode,
                        usage: slot.usage,
                        stage: ResourceAccessStage::ComputeShader,
                        range: *range,
                    });
                    if slot.mode.reads() {
                        builder.reads.push(name.clone());
                    }
                    if slot.mode.writes() {
                        builder.writes.push(name.clone());
                    }
                    if !self.constants.is_empty() && slot.usage == BufferUsage::Uniform {
                        builder.buffer_accesses.push(NamedBufferAccess {
                            resource: name.clone(),
                            mode: super::super::ResourceAccessMode::Write,
                            usage: BufferUsage::TransferDestination,
                            stage: ResourceAccessStage::Transfer,
                            range: *range,
                        });
                        builder.writes.push(name.clone());
                    }
                }
            }
        }
        if let Some((name, offset)) = &self.indirect {
            builder.reads.push(name.clone());
            builder.buffer_accesses.push(NamedBufferAccess {
                resource: name.clone(),
                mode: super::super::ResourceAccessMode::Read,
                usage: BufferUsage::Indirect,
                stage: ResourceAccessStage::DrawIndirect,
                range: BufferByteRange::new(*offset, 12),
            });
        }
        builder.build_fn = Box::new(move |resources| {
            interface.map_err(|reason| {
                RenderGraphError::Validation(
                    super::super::GraphValidationError::InvalidComputeCommand {
                        pass: pass_name,
                        reason,
                    },
                )
            })?;
            let bindings = self
                .bindings
                .into_iter()
                .map(|(group, binding, name, range)| {
                    let resource = resources
                        .get(&name)
                        .ok_or(RenderGraphError::ResourceNotFound(name))?;
                    Ok(ComputeBinding {
                        group,
                        binding,
                        resource: super::super::ResourceId(resource.index()),
                        range,
                    })
                })
                .collect::<Result<Vec<_>, RenderGraphError>>()?;
            let size = if let Some((name, offset)) = self.indirect {
                ComputeDispatchSize::Indirect {
                    resource: super::super::ResourceId(
                        resources
                            .get(&name)
                            .ok_or(RenderGraphError::ResourceNotFound(name))?
                            .index(),
                    ),
                    offset,
                }
            } else {
                self.size
            };
            Ok(Box::new(vec![ComputeCommand::Dispatch(ComputeDispatch {
                pipeline: self.pipeline,
                bindings,
                constants: self.constants,
                size,
            })]))
        });
        builder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::{GraphResourceHandle, ResourceId};
    use std::collections::HashMap;

    #[test]
    fn test_compute_builder_resolves_indirect_buffer_and_offset() {
        let builder = ComputePass::new(
            "indirect",
            ComputePipelineDesc {
                wgsl: "@compute @workgroup_size(1) fn entry() {}".into(),
                entry: "entry".into(),
            },
        )
        .dispatch_indirect("arguments", 16)
        .as_builder();
        let resources = HashMap::from([("arguments".into(), GraphResourceHandle::new(7))]);
        let commands = (builder.build_fn)(&resources)
            .unwrap()
            .downcast::<Vec<ComputeCommand>>()
            .unwrap();
        let ComputeCommand::Dispatch(dispatch) = &commands[0] else {
            panic!("dispatch command")
        };
        assert_eq!(
            dispatch.size,
            ComputeDispatchSize::Indirect {
                resource: ResourceId(7),
                offset: 16
            }
        );
        assert_eq!(
            builder.buffer_accesses[0].range,
            BufferByteRange::new(16, 12)
        );
    }
}
