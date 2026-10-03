//! Explicit resource and pipeline inputs for a graph pass.

use crate::backend::command::ShaderStages;
use crate::handle::MaterialHandle;
use crate::render_graph::{BufferByteRange, ImageSubresourceRange, ResourceId};
use crate::vertex::VertexLayout;
use crate::{Rect, SamplerDescriptor};

/// A graphics pipeline selected by the submitted mesh's declared layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassPipeline {
    pub vertex_layout: VertexLayout,
    pub material: MaterialHandle,
}

/// A graph buffer bound to a reflected shader slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferBinding {
    pub group: u32,
    pub binding: u32,
    pub resource: ResourceId,
    pub range: BufferByteRange,
    pub stages: ShaderStages,
}

/// A graph image bound to a reflected shader slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageBinding {
    pub group: u32,
    pub binding: u32,
    pub resource: ResourceId,
    pub range: ImageSubresourceRange,
    pub stages: ShaderStages,
}

/// Application-authored inputs; native encoders retain resolved resources for submission.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassBindings {
    pub pipelines: Vec<PassPipeline>,
    pub buffers: Vec<BufferBinding>,
    pub images: Vec<ImageBinding>,
    pub samplers: Vec<SamplerBinding>,
    /// Immutable bytes for explicitly identified reflected buffer slots.
    pub constants: Vec<ConstantBinding>,
    /// Explicit rendering phases sharing this packet's resource bindings.
    pub phases: Vec<PassDrawPhase>,
}

impl PassBindings {
    pub(crate) fn samplers_for_phase<'a>(
        &'a self,
        overrides: &'a [SamplerBinding],
    ) -> impl Iterator<Item = &'a SamplerBinding> {
        self.samplers
            .iter()
            .filter(move |base| {
                !overrides
                    .iter()
                    .any(|binding| (binding.group, binding.binding) == (base.group, base.binding))
            })
            .chain(overrides)
    }

    pub(crate) fn constants_for_phase<'a>(
        &'a self,
        overrides: &'a [ConstantBinding],
    ) -> impl Iterator<Item = &'a ConstantBinding> {
        self.constants
            .iter()
            .filter(move |base| {
                !overrides
                    .iter()
                    .any(|binding| (binding.group, binding.binding) == (base.group, base.binding))
            })
            .chain(overrides)
    }

    pub(crate) fn override_phase(&mut self, phase: &PassDrawPhase) {
        for sampler in &phase.samplers {
            self.samplers
                .retain(|base| (base.group, base.binding) != (sampler.group, sampler.binding));
            self.samplers.push(sampler.clone());
        }
        for constant in &phase.constants {
            self.constants
                .retain(|base| (base.group, base.binding) != (constant.group, constant.binding));
            self.constants.push(constant.clone());
        }
    }

    pub(crate) fn contains_binding(&self, group: u32, binding: u32) -> bool {
        self.buffers
            .iter()
            .any(|value| value.group == group && value.binding == binding)
            || self
                .images
                .iter()
                .any(|value| value.group == group && value.binding == binding)
            || self
                .samplers
                .iter()
                .any(|value| value.group == group && value.binding == binding)
            || self
                .constants
                .iter()
                .any(|value| value.group == group && value.binding == binding)
    }
}

/// Geometry source used by one phase of a graphics pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassDraw {
    /// Every submitted draw, preserving its stable object-storage index.
    Submissions,
    /// Submitted objects selected by their stable object-storage indices.
    ObjectIndices(Vec<u32>),
    /// Shader-generated geometry without a mesh.
    Vertices { count: u32, instances: u32 },
    /// A native indirect command declared as a graph buffer read.
    Indirect { resource: ResourceId, offset: u64 },
}

/// One ordinary drawing phase, independent of editor or scene semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct PassDrawPhase {
    /// Replacements or additions to this pass's sampler slots for this phase.
    pub samplers: Vec<SamplerBinding>,
    pub pipelines: Vec<PassPipeline>,
    /// Replacements or additions to this pass's constant slots for this phase.
    pub constants: Vec<ConstantBinding>,
    pub draw: PassDraw,
    /// Target-local viewport; absence uses the declared attachment extent.
    pub viewport: Option<Rect>,
}

/// Immutable inline contents for a reflected uniform or storage buffer binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantBinding {
    pub group: u32,
    pub binding: u32,
    pub stages: ShaderStages,
    pub bytes: Vec<u8>,
}

/// An independent sampler bound to a reflected shader slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamplerBinding {
    pub group: u32,
    pub binding: u32,
    pub stages: ShaderStages,
    pub sampling: SamplerDescriptor,
}
