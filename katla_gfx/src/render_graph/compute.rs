//! Backend-neutral compute commands and reflected buffer interfaces.

use super::{
    BufferAccess, BufferByteRange, BufferUsage, PassDesc, ResourceAccessMode, ResourceAccessStage,
    ResourceId,
};

/// Renderer-owned buffer imported under a graph resource identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinBuffer {
    AnimationParams,
    AnimationClips,
    AnimationChannels,
    AnimationTimes,
    AnimationValues,
    AnimationJoints,
    AnimationWorld,
    AnimationOutput,
    Skeleton(crate::handle::SkeletonHandle),
    LightData,
    LightTiles,
    LightHeaders,
    LightFrame,
    ParticleData,
    ParticleDeadList,
    ParticleAliveRead,
    ParticleAliveWrite,
    ParticleCounters,
    ParticlePreviousCounters,
    ParticleIndirect,
    ParticleFrame,
    ParticleEmitters,
}

/// Built-in shader selected independently of its native pipeline representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinComputeKernel {
    AnimationPose,
    LightCulling,
    ParticleEmit,
    ParticleSimulate,
    ParticleDrawCommand,
}

/// Canonical shader buffer binding reflected from WGSL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeBindingLayout {
    pub minimum_buffer_bytes: u64,
    pub group: u32,
    pub binding: u32,
    pub usage: BufferUsage,
    pub mode: ResourceAccessMode,
}

/// Compute entry-point interface derived from the shader's declarations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeInterface {
    pub bindings: Vec<ComputeBindingLayout>,
    pub workgroup_size: [u32; 3],
}

impl ComputeInterface {
    /// Reflect and validate a WGSL compute entry point before graph construction.
    pub fn reflect(source: &str, entry: &str) -> Result<Self, String> {
        let module =
            naga::front::wgsl::parse_str(source).map_err(|error| error.emit_to_string(source))?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|error| error.to_string())?;
        let point = module
            .entry_points
            .iter()
            .find(|point| point.name == entry && point.stage == naga::ShaderStage::Compute)
            .ok_or_else(|| format!("Missing compute entry point '{entry}'"))?;
        let mut bindings = Vec::new();
        for (_, variable) in module.global_variables.iter() {
            let Some(binding) = variable.binding.as_ref() else {
                continue;
            };
            let (usage, mode) = match variable.space {
                naga::AddressSpace::Uniform => (BufferUsage::Uniform, ResourceAccessMode::Read),
                naga::AddressSpace::Storage { access } => (
                    BufferUsage::Storage,
                    match (
                        access.contains(naga::StorageAccess::LOAD),
                        access.contains(naga::StorageAccess::STORE),
                    ) {
                        (true, true) => ResourceAccessMode::ReadWrite,
                        (false, true) => ResourceAccessMode::Write,
                        _ => ResourceAccessMode::Read,
                    },
                ),
                _ => {
                    return Err(format!(
                        "Unsupported compute binding {}:{}",
                        binding.group, binding.binding
                    ));
                }
            };
            bindings.push(ComputeBindingLayout {
                minimum_buffer_bytes: u64::from(
                    module.types[variable.ty].inner.size(module.to_ctx()),
                ),
                group: binding.group,
                binding: binding.binding,
                usage,
                mode,
            });
        }
        bindings.sort_by_key(|binding| (binding.group, binding.binding));
        Ok(Self {
            bindings,
            workgroup_size: point.workgroup_size,
        })
    }
}

impl BuiltinComputeKernel {
    /// The canonical interface is reflected from the same source compiled by both adapters.
    pub fn descriptor(self) -> ComputePipelineDesc {
        self.descriptor_ref().clone()
    }

    pub(crate) fn descriptor_ref(self) -> &'static ComputePipelineDesc {
        use std::sync::OnceLock;
        static ANIMATION: OnceLock<ComputePipelineDesc> = OnceLock::new();
        static LIGHTS: OnceLock<ComputePipelineDesc> = OnceLock::new();
        static EMIT: OnceLock<ComputePipelineDesc> = OnceLock::new();
        static SIMULATE: OnceLock<ComputePipelineDesc> = OnceLock::new();
        static DRAW: OnceLock<ComputePipelineDesc> = OnceLock::new();
        let cache = match self {
            Self::AnimationPose => &ANIMATION,
            Self::LightCulling => &LIGHTS,
            Self::ParticleEmit => &EMIT,
            Self::ParticleSimulate => &SIMULATE,
            Self::ParticleDrawCommand => &DRAW,
        };
        cache.get_or_init(|| self.build_descriptor())
    }

    fn build_descriptor(self) -> ComputePipelineDesc {
        let source = match self {
            Self::AnimationPose => {
                include_str!("../../../resources/shaders/compute/animation/pose_eval.wgsl")
            }
            Self::LightCulling => {
                include_str!("../../../resources/shaders/lighting/light_cull.wgsl")
            }
            Self::ParticleEmit => {
                include_str!("../../../resources/shaders/particles/particle_emit.wgsl")
            }
            Self::ParticleSimulate => {
                include_str!("../../../resources/shaders/particles/particle_simulate.wgsl")
            }
            Self::ParticleDrawCommand => {
                include_str!("../../../resources/shaders/particles/particle_draw_command.wgsl")
            }
        };
        let source = source
            .replace(
                "#include \"../common/lighting_types.wgsl\"",
                include_str!("../../../resources/shaders/common/lighting_types.wgsl"),
            )
            .replace(
                "#include \"common.wgsl\"",
                include_str!("../../../resources/shaders/particles/common.wgsl"),
            );
        ComputePipelineDesc {
            wgsl: source,
            entry: "cs_main".into(),
        }
    }
}

/// Portable compute pipeline identity; the interface is reflected from this exact source.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComputePipelineDesc {
    pub wgsl: String,
    pub entry: String,
}

impl ComputePipelineDesc {
    pub fn interface(&self) -> Result<ComputeInterface, String> {
        ComputeInterface::reflect(&self.wgsl, &self.entry)
    }
}

/// Compute pipeline and optional built-in frame-workload policy.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ComputeKernel {
    Builtin(BuiltinComputeKernel),
    Shader(ComputePipelineDesc),
}

impl ComputeKernel {
    pub fn descriptor(&self) -> ComputePipelineDesc {
        self.descriptor_ref().clone()
    }
    pub(crate) fn descriptor_ref(&self) -> &ComputePipelineDesc {
        match self {
            Self::Builtin(kernel) => kernel.descriptor_ref(),
            Self::Shader(descriptor) => descriptor,
        }
    }
    pub fn interface(&self) -> Result<ComputeInterface, String> {
        self.descriptor_ref().interface()
    }
}

/// A graph buffer bound at a canonical shader group and binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeBinding {
    pub group: u32,
    pub binding: u32,
    pub resource: ResourceId,
    pub range: BufferByteRange,
}

/// Number of workgroups, or a graph-declared indirect command location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeDispatchSize {
    Direct([u32; 3]),
    Indirect {
        resource: ResourceId,
        offset: u64,
    },
    /// Uses frame parameters supplied by the owning application.
    Frame,
}

/// One compute dispatch with a complete declared resource binding interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComputeDispatch {
    pub kernel: ComputeKernel,
    pub bindings: Vec<ComputeBinding>,
    /// Inline uniform bytes, interpreted through the kernel's reflected uniform binding.
    pub constants: Vec<u8>,
    pub size: ComputeDispatchSize,
}

/// Commands accepted by the shared graph execution contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComputeCommand {
    Dispatch(ComputeDispatch),
    FillBuffer {
        resource: ResourceId,
        range: BufferByteRange,
        value: u32,
    },
    CopyBuffer {
        source: ResourceId,
        destination: ResourceId,
        source_offset: u64,
        destination_offset: u64,
        size: u64,
    },
}

fn declared(
    pass: &PassDesc,
    resource: ResourceId,
    range: BufferByteRange,
    usage: BufferUsage,
    mode: ResourceAccessMode,
) -> bool {
    pass.buffer_accesses.iter().any(|access| {
        access.resource == resource
            && access.usage == usage
            && (!mode.reads() || access.mode.reads())
            && (!mode.writes() || access.mode.writes())
            && access.range.intersection(range) == Some(range)
    })
}

pub(crate) fn indirect_range_fits(offset: u64, capacity: u64) -> bool {
    offset.is_multiple_of(4) && offset.checked_add(12).is_some_and(|end| end <= capacity)
}

pub(crate) fn validate_commands(pass: &PassDesc) -> Result<(), String> {
    if !pass.commands.is_empty() && pass.pass_type == super::PassType::Graphics {
        return Err("Graphics passes cannot record compute or transfer commands".into());
    }
    for command in &pass.commands {
        match command {
            ComputeCommand::Dispatch(dispatch) => {
                if pass.pass_type != super::PassType::Compute {
                    return Err("Dispatch commands require a compute pass".into());
                }
                let interface = dispatch.kernel.interface()?;
                if dispatch.bindings.len() != interface.bindings.len() {
                    return Err("Compute binding count differs from shader reflection".into());
                }
                let mut slots = std::collections::BTreeSet::new();
                for binding in &dispatch.bindings {
                    if !slots.insert((binding.group, binding.binding)) {
                        return Err("Duplicate compute binding".into());
                    }
                    let expected = interface
                        .bindings
                        .iter()
                        .find(|slot| slot.group == binding.group && slot.binding == binding.binding)
                        .ok_or_else(|| {
                            "Compute binding absent from shader reflection".to_string()
                        })?;
                    if binding.range.size < expected.minimum_buffer_bytes {
                        return Err(
                            "Compute binding is smaller than the reflected shader type".into()
                        );
                    }
                    if !declared(
                        pass,
                        binding.resource,
                        binding.range,
                        expected.usage,
                        expected.mode,
                    ) {
                        return Err("Compute binding exceeds the pass access declaration".into());
                    }
                }
                if !dispatch.constants.is_empty()
                    && !interface
                        .bindings
                        .iter()
                        .any(|binding| binding.usage == BufferUsage::Uniform)
                {
                    return Err("Inline constants require a reflected uniform binding".into());
                }
                if dispatch.size == ComputeDispatchSize::Frame
                    && matches!(dispatch.kernel, ComputeKernel::Shader(_))
                {
                    return Err(
                        "Custom compute requires explicit direct or indirect dimensions".into(),
                    );
                }
                if !dispatch.constants.is_empty() {
                    if dispatch.constants.len() % 4 != 0 || dispatch.constants.len() > 65536 {
                        return Err(
                            "Inline constants require aligned bytes up to 65536 bytes".into()
                        );
                    }
                    let uniform = interface
                        .bindings
                        .iter()
                        .find(|slot| slot.usage == BufferUsage::Uniform)
                        .ok_or_else(|| "Inline constants require uniform reflection".to_string())?;
                    let binding = dispatch
                        .bindings
                        .iter()
                        .find(|binding| {
                            binding.group == uniform.group && binding.binding == uniform.binding
                        })
                        .ok_or_else(|| "Missing uniform binding".to_string())?;
                    if binding.range.offset % 4 != 0
                        || dispatch.constants.len() as u64 > binding.range.size
                    {
                        return Err(
                            "Inline constants exceed the aligned uniform binding range".into()
                        );
                    }
                    if !declared(
                        pass,
                        binding.resource,
                        BufferByteRange::new(binding.range.offset, dispatch.constants.len() as u64),
                        BufferUsage::TransferDestination,
                        ResourceAccessMode::Write,
                    ) {
                        return Err(
                            "Inline constants require a declared uniform transfer destination"
                                .into(),
                        );
                    }
                }
                if let ComputeDispatchSize::Indirect { resource, offset } = dispatch.size
                    && (offset % 4 != 0
                        || !declared(
                            pass,
                            resource,
                            BufferByteRange::new(offset, 12),
                            BufferUsage::Indirect,
                            ResourceAccessMode::Read,
                        ))
                {
                    return Err(
                        "Indirect dispatch requires an aligned declared indirect buffer range"
                            .into(),
                    );
                }
            }
            ComputeCommand::FillBuffer {
                resource, range, ..
            } => {
                if pass.pass_type != super::PassType::Transfer {
                    return Err("Fill commands require a transfer pass".into());
                }
                if range.offset % 4 != 0
                    || (range.size != u64::MAX && range.size % 4 != 0)
                    || !declared(
                        pass,
                        *resource,
                        *range,
                        BufferUsage::TransferDestination,
                        ResourceAccessMode::Write,
                    )
                {
                    return Err(
                        "Buffer fill requires an aligned declared transfer destination".into(),
                    );
                }
            }
            ComputeCommand::CopyBuffer {
                source,
                destination,
                source_offset,
                destination_offset,
                size,
            } => {
                if pass.pass_type != super::PassType::Transfer {
                    return Err("Copy commands require a transfer pass".into());
                }
                if *size == 0
                    || *source_offset % 4 != 0
                    || *destination_offset % 4 != 0
                    || *size % 4 != 0
                {
                    return Err("Buffer copies require nonempty aligned ranges".into());
                }
                if source == destination
                    && BufferByteRange::new(*source_offset, *size)
                        .intersection(BufferByteRange::new(*destination_offset, *size))
                        .is_some()
                {
                    return Err("A buffer copy cannot overlap its own source range".into());
                }
                if !declared(
                    pass,
                    *source,
                    BufferByteRange::new(*source_offset, *size),
                    BufferUsage::TransferSource,
                    ResourceAccessMode::Read,
                ) || !declared(
                    pass,
                    *destination,
                    BufferByteRange::new(*destination_offset, *size),
                    BufferUsage::TransferDestination,
                    ResourceAccessMode::Write,
                ) {
                    return Err("Buffer copy exceeds the pass transfer declarations".into());
                }
            }
        }
    }
    Ok(())
}

impl ComputeDispatch {
    /// Declare accesses directly from the canonical shader interface.
    pub fn accesses(&self) -> Result<Vec<BufferAccess>, String> {
        let interface = self.kernel.interface()?;
        self.bindings
            .iter()
            .map(|binding| {
                let expected = interface
                    .bindings
                    .iter()
                    .find(|slot| slot.group == binding.group && slot.binding == binding.binding)
                    .ok_or_else(|| "Unknown shader binding".to_string())?;
                Ok(BufferAccess::new(
                    binding.resource,
                    expected.mode,
                    expected.usage,
                    ResourceAccessStage::ComputeShader,
                    binding.range,
                ))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "@group(2) @binding(4) var<storage, read_write> output: array<u32>; @compute @workgroup_size(8) fn main(@builtin(global_invocation_id) id:vec3u) { output[id.x]=id.x+1u; }";
    fn dispatch() -> ComputeDispatch {
        ComputeDispatch {
            kernel: ComputeKernel::Shader(ComputePipelineDesc {
                wgsl: SOURCE.into(),
                entry: "main".into(),
            }),
            bindings: vec![ComputeBinding {
                group: 2,
                binding: 4,
                resource: ResourceId(1),
                range: BufferByteRange::new(0, 32),
            }],
            constants: vec![],
            size: ComputeDispatchSize::Direct([1, 1, 1]),
        }
    }
    #[test]
    fn test_reflection_preserves_nondefault_entry_point_and_bindings() {
        let interface = dispatch().kernel.interface().unwrap();
        assert_eq!(interface.workgroup_size, [8, 1, 1]);
        assert_eq!(interface.bindings[0].group, 2);
        assert_eq!(interface.bindings[0].binding, 4);
        assert_eq!(interface.bindings[0].mode, ResourceAccessMode::ReadWrite);
    }
    #[test]
    fn test_builtin_interfaces_are_reflected_from_valid_canonical_sources() {
        for kernel in [
            BuiltinComputeKernel::AnimationPose,
            BuiltinComputeKernel::LightCulling,
            BuiltinComputeKernel::ParticleEmit,
            BuiltinComputeKernel::ParticleSimulate,
            BuiltinComputeKernel::ParticleDrawCommand,
        ] {
            assert!(!kernel.descriptor().interface().unwrap().bindings.is_empty());
        }
    }
    #[test]
    fn test_compute_commands_reject_undeclared_resources_and_ranges() {
        let command = dispatch();
        let mut pass = PassDesc::new("compute", super::super::PassType::Compute, vec![], vec![])
            .with_commands([ComputeCommand::Dispatch(command.clone())]);
        assert!(validate_commands(&pass).is_err());
        pass.set_buffer_accesses(command.accesses().unwrap());
        assert!(validate_commands(&pass).is_ok());
        if let ComputeCommand::Dispatch(dispatch) = &mut pass.commands[0] {
            dispatch.bindings[0].range.size = 36;
        }
        assert!(validate_commands(&pass).is_err());
    }
    #[test]
    fn test_compute_commands_reject_duplicate_slots_and_wrong_shader_mode() {
        let command = dispatch();
        let mut pass = PassDesc::new("compute", super::super::PassType::Compute, vec![], vec![])
            .with_buffer_accesses(command.accesses().unwrap())
            .with_commands([ComputeCommand::Dispatch(command.clone())]);
        pass.buffer_accesses[0].mode = ResourceAccessMode::Read;
        assert!(validate_commands(&pass).is_err());
        pass.set_buffer_accesses(command.accesses().unwrap());
        if let ComputeCommand::Dispatch(dispatch) = &mut pass.commands[0] {
            dispatch.bindings.push(dispatch.bindings[0].clone());
        }
        assert!(validate_commands(&pass).is_err());
    }
    #[test]
    fn test_indirect_dispatch_requires_aligned_declared_indirect_range() {
        let mut command = dispatch();
        command.size = ComputeDispatchSize::Indirect {
            resource: ResourceId(2),
            offset: 4,
        };
        let accesses = command.accesses().unwrap();
        let mut pass = PassDesc::new("compute", super::super::PassType::Compute, vec![], vec![])
            .with_buffer_accesses(accesses)
            .with_commands([ComputeCommand::Dispatch(command)]);
        assert!(validate_commands(&pass).is_err());
        pass.buffer_accesses.push(BufferAccess::new(
            ResourceId(2),
            ResourceAccessMode::Read,
            BufferUsage::Indirect,
            ResourceAccessStage::DrawIndirect,
            BufferByteRange::new(4, 12),
        ));
        assert!(validate_commands(&pass).is_ok());
    }
    #[test]
    fn test_inline_constants_require_transfer_write_and_fit_uniform_range() {
        let command = ComputeDispatch {
            kernel: ComputeKernel::Shader(ComputePipelineDesc {
                wgsl: "@group(0) @binding(0) var<uniform> params: vec4u; @compute @workgroup_size(1) fn main() { let value = params.x; }".into(),
                entry: "main".into(),
            }),
            bindings: vec![ComputeBinding { group: 0, binding: 0, resource: ResourceId(3), range: BufferByteRange::new(16, 16) }],
            constants: vec![0; 16],
            size: ComputeDispatchSize::Direct([1, 1, 1]),
        };
        let mut pass = PassDesc::new("constants", super::super::PassType::Compute, vec![], vec![])
            .with_buffer_accesses(command.accesses().unwrap())
            .with_commands([ComputeCommand::Dispatch(command)]);
        assert!(validate_commands(&pass).is_err());
        pass.buffer_accesses.push(
            BufferAccess::transfer_write(ResourceId(3)).with_range(BufferByteRange::new(16, 32)),
        );
        assert!(validate_commands(&pass).is_ok());
        let ComputeCommand::Dispatch(dispatch) = &mut pass.commands[0] else {
            panic!("dispatch")
        };
        dispatch.constants.resize(20, 0);
        assert!(validate_commands(&pass).is_err());
    }
    #[test]
    fn test_indirect_native_range_rejects_short_overflow_and_unaligned_views() {
        assert!(indirect_range_fits(16, 28));
        assert!(!indirect_range_fits(16, 27));
        assert!(!indirect_range_fits(u64::MAX - 3, u64::MAX));
        assert!(!indirect_range_fits(2, 64));
    }
    #[test]
    fn test_particle_shader_interfaces_use_runtime_pool_capacity() {
        for source in [
            include_str!("../../../resources/shaders/particles/particle_emit.wgsl"),
            include_str!("../../../resources/shaders/particles/particle_simulate.wgsl"),
            include_str!("../../../resources/shaders/particles/particle_render.wgsl"),
            include_str!("../../../resources/shaders/particles/particle_validate.wgsl"),
        ] {
            let source = source
                .replace(
                    "#include \"common.wgsl\"",
                    include_str!("../../../resources/shaders/particles/common.wgsl"),
                )
                .replace(
                    "#include \"../common/frame_uniforms.wgsl\"",
                    include_str!("../../../resources/shaders/common/frame_uniforms.wgsl"),
                );
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
            for (_, variable) in module.global_variables.iter() {
                if variable.name.as_deref().is_some_and(|name| {
                    [
                        "particles",
                        "dead_list",
                        "alive_list",
                        "alive_list_next",
                        "emitters",
                    ]
                    .contains(&name)
                }) {
                    assert!(matches!(
                        module.types[variable.ty].inner,
                        naga::TypeInner::Array {
                            size: naga::ArraySize::Dynamic,
                            ..
                        }
                    ));
                }
            }
        }
    }
    #[test]
    fn test_fixed_shader_buffer_span_constrains_binding_ranges() {
        let mut command = ComputeDispatch {
            kernel: ComputeKernel::Shader(ComputePipelineDesc { wgsl: "@group(0) @binding(0) var<uniform> params: vec4u; @compute @workgroup_size(1) fn main() { let value = params.x; }".into(), entry: "main".into() }),
            bindings: vec![ComputeBinding { group: 0, binding: 0, resource: ResourceId(3), range: BufferByteRange::new(0, 4) }],
            constants: vec![], size: ComputeDispatchSize::Direct([1, 1, 1]),
        };
        let interface = command.kernel.interface().unwrap();
        assert_eq!(interface.bindings[0].minimum_buffer_bytes, 16);
        let pass = PassDesc::new("span", super::super::PassType::Compute, vec![], vec![])
            .with_buffer_accesses(command.accesses().unwrap())
            .with_commands([ComputeCommand::Dispatch(command.clone())]);
        assert!(validate_commands(&pass).is_err());
        command.bindings[0].range.size = 16;
        let pass = PassDesc::new("span", super::super::PassType::Compute, vec![], vec![])
            .with_buffer_accesses(command.accesses().unwrap())
            .with_commands([ComputeCommand::Dispatch(command)]);
        assert!(validate_commands(&pass).is_ok());
    }
}
