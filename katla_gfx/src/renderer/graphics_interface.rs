//! Reflected graphics resource contracts shared by native encoders.

use std::collections::BTreeMap;

use crate::backend::command::ShaderStages;
use crate::render_graph::{BufferUsage, ResourceAccessMode, ResourceId};

use super::frame_bindings::PassBindings;
use super::pipeline_descriptor::PipelineStages;

/// Resource kind required by a reflected WGSL binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsBindingKind {
    /// A uniform or storage buffer and its permitted access.
    Buffer {
        usage: BufferUsage,
        mode: ResourceAccessMode,
        minimum_bytes: u64,
    },
    /// A sampled or storage image.
    Image { storage: bool },
    /// A sampler, optionally performing depth comparison.
    Sampler { comparison: bool },
}

/// One binding actually used by a selected graphics entry point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphicsBindingLayout {
    pub group: u32,
    pub binding: u32,
    pub stages: ShaderStages,
    pub kind: GraphicsBindingKind,
    /// Whether WGSL declares an array of bound resources at this slot.
    pub array: bool,
}

/// The sorted, merged resource interface of a graphics pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphicsInterface {
    pub bindings: Vec<GraphicsBindingLayout>,
    /// Fragment color locations, in increasing order. A void/depth-only entry has none.
    pub color_outputs: Vec<u32>,
}

impl GraphicsInterface {
    /// Reflect exactly the selected vertex and optional fragment entry points.
    pub fn reflect(source: &str, stages: &PipelineStages) -> Result<Self, String> {
        let PipelineStages::Graphics {
            vertex_entry,
            fragment_entry,
        } = stages
        else {
            return Err("A graphics interface requires graphics entry points".into());
        };
        let module =
            naga::front::wgsl::parse_str(source).map_err(|error| error.emit_to_string(source))?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|error| error.to_string())?;
        let selected = std::iter::once((vertex_entry.as_str(), naga::ShaderStage::Vertex)).chain(
            fragment_entry
                .as_deref()
                .map(|entry| (entry, naga::ShaderStage::Fragment)),
        );
        let mut bindings: BTreeMap<(u32, u32), GraphicsBindingLayout> = BTreeMap::new();
        let mut color_outputs = Vec::new();
        for (name, stage) in selected {
            let entry_index = module
                .entry_points
                .iter()
                .position(|entry| entry.name == name && entry.stage == stage)
                .ok_or_else(|| format!("Missing {stage:?} entry point '{name}'"))?;
            let shader_stages = match stage {
                naga::ShaderStage::Vertex => ShaderStages::VERTEX,
                naga::ShaderStage::Fragment => ShaderStages::FRAGMENT,
                _ => return Err("Unexpected graphics shader stage".into()),
            };
            if stage == naga::ShaderStage::Fragment
                && let Some(result) = &module.entry_points[entry_index].function.result
            {
                match &module.types[result.ty].inner {
                    naga::TypeInner::Struct { members, .. } => {
                        for member in members {
                            if let Some(naga::Binding::Location { location, .. }) = member.binding {
                                color_outputs.push(location);
                            }
                        }
                    }
                    _ => {
                        if let Some(naga::Binding::Location { location, .. }) = result.binding {
                            color_outputs.push(location);
                        }
                    }
                }
            }
            for (handle, global) in module.global_variables.iter() {
                if info.get_entry_point(entry_index)[handle].is_empty() {
                    continue;
                }
                let Some(slot) = global.binding else {
                    continue;
                };
                let (ty, array) = match module.types[global.ty].inner {
                    naga::TypeInner::BindingArray { base, .. } => (base, true),
                    _ => (global.ty, false),
                };
                let kind = match global.space {
                    naga::AddressSpace::Uniform => GraphicsBindingKind::Buffer {
                        usage: BufferUsage::Uniform,
                        mode: ResourceAccessMode::Read,
                        minimum_bytes: u64::from(module.types[ty].inner.size(module.to_ctx())),
                    },
                    naga::AddressSpace::Storage { access } => GraphicsBindingKind::Buffer {
                        usage: BufferUsage::Storage,
                        mode: storage_mode(access),
                        minimum_bytes: u64::from(module.types[ty].inner.size(module.to_ctx())),
                    },
                    naga::AddressSpace::Handle => match module.types[ty].inner {
                        naga::TypeInner::Image { class, .. } => GraphicsBindingKind::Image {
                            storage: matches!(class, naga::ImageClass::Storage { .. }),
                        },
                        naga::TypeInner::Sampler { comparison } => {
                            GraphicsBindingKind::Sampler { comparison }
                        }
                        _ => {
                            return Err(format!(
                                "Unsupported handle binding {}:{}",
                                slot.group, slot.binding
                            ));
                        }
                    },
                    _ => {
                        return Err(format!(
                            "Unsupported graphics binding {}:{}",
                            slot.group, slot.binding
                        ));
                    }
                };
                let binding =
                    bindings
                        .entry((slot.group, slot.binding))
                        .or_insert(GraphicsBindingLayout {
                            group: slot.group,
                            binding: slot.binding,
                            stages: ShaderStages::NONE,
                            kind,
                            array,
                        });
                binding.stages.vertex |= shader_stages.vertex;
                binding.stages.fragment |= shader_stages.fragment;
            }
        }
        color_outputs.sort_unstable();
        color_outputs.dedup();
        Ok(Self {
            bindings: bindings.into_values().collect(),
            color_outputs,
        })
    }

    /// Native attachment-array span needed for the selected fragment outputs.
    pub fn color_attachment_count(&self) -> usize {
        self.color_outputs
            .last()
            .map_or(0, |location| *location as usize + 1)
    }

    /// Require graph accesses to cover each explicit buffer's reflected use.
    pub fn validate_buffer_accesses(
        &self,
        packet: &PassBindings,
        accesses: &[crate::render_graph::BufferAccess],
    ) -> Result<(), String> {
        use crate::render_graph::ResourceAccessStage;
        for binding in &packet.buffers {
            let Some(required) = self.bindings.iter().find(|required| {
                required.group == binding.group && required.binding == binding.binding
            }) else {
                continue;
            };
            let GraphicsBindingKind::Buffer { usage, mode, .. } = required.kind else {
                continue;
            };
            for (stage, used) in [
                (ResourceAccessStage::VertexShader, required.stages.vertex),
                (
                    ResourceAccessStage::FragmentShader,
                    required.stages.fragment,
                ),
            ] {
                if used
                    && !accesses.iter().any(|access| {
                        access.resource == binding.resource
                            && access.usage == usage
                            && (!mode.reads() || access.mode.reads())
                            && (!mode.writes() || access.mode.writes())
                            && (access.stage == stage
                                || access.stage == ResourceAccessStage::AllGraphics)
                            && access.range.offset <= binding.range.offset
                            && access.range.end() >= binding.range.end()
                    })
                {
                    return Err(format!(
                        "Graphics buffer {}:{} (resource {}) lacks its reflected {stage:?} {usage:?} {mode:?} graph access",
                        binding.group, binding.binding, binding.resource.0
                    ));
                }
            }
        }
        Ok(())
    }

    /// Validate supplied buffer, image and inline bindings for this interface.
    ///
    /// `provided` describes resources supplied by the native core, such as draw
    /// storage or a bindless table. Every other used slot must be explicit in
    /// the packet. Buffer extents come from resolved graph resources.
    pub fn validate_bindings(
        &self,
        packet: &PassBindings,
        provided: &[GraphicsBindingLayout],
        buffer_size: impl Fn(ResourceId) -> Option<u64>,
    ) -> Result<(), String> {
        let mut supplied = BTreeMap::new();
        for layout in provided {
            if !packet.contains_binding(layout.group, layout.binding) {
                insert_binding(&mut supplied, *layout)?;
            }
        }
        for binding in &packet.buffers {
            let size = buffer_size(binding.resource)
                .ok_or_else(|| format!("Unknown graph buffer {}", binding.resource.0))?;
            let bytes = if binding.range.size == u64::MAX {
                size.checked_sub(binding.range.offset)
            } else {
                binding
                    .range
                    .offset
                    .checked_add(binding.range.size)
                    .filter(|end| *end <= size)
                    .map(|_| binding.range.size)
            }
            .filter(|bytes| *bytes > 0)
            .ok_or_else(|| {
                format!(
                    "Out-of-bounds graphics buffer {}:{}",
                    binding.group, binding.binding
                )
            })?;
            let expected = self
                .bindings
                .iter()
                .find(|slot| slot.group == binding.group && slot.binding == binding.binding);
            let (usage, mode) = match expected.map(|slot| slot.kind) {
                Some(GraphicsBindingKind::Buffer { usage, mode, .. }) => (usage, mode),
                _ => (BufferUsage::Storage, ResourceAccessMode::Read),
            };
            insert_binding(
                &mut supplied,
                GraphicsBindingLayout {
                    group: binding.group,
                    binding: binding.binding,
                    stages: binding.stages,
                    kind: GraphicsBindingKind::Buffer {
                        usage,
                        mode,
                        minimum_bytes: bytes,
                    },
                    array: false,
                },
            )?;
        }
        for binding in &packet.images {
            insert_binding(
                &mut supplied,
                GraphicsBindingLayout {
                    group: binding.group,
                    binding: binding.binding,
                    stages: binding.stages,
                    kind: GraphicsBindingKind::Image { storage: false },
                    array: false,
                },
            )?;
        }
        for binding in &packet.samplers {
            insert_binding(
                &mut supplied,
                GraphicsBindingLayout {
                    group: binding.group,
                    binding: binding.binding,
                    stages: binding.stages,
                    kind: GraphicsBindingKind::Sampler {
                        comparison: binding.sampling
                            == super::frame_bindings::SamplingMode::DepthComparison,
                    },
                    array: false,
                },
            )?;
        }
        for binding in &packet.constants {
            let expected = self
                .bindings
                .iter()
                .find(|slot| slot.group == binding.group && slot.binding == binding.binding);
            let (usage, mode) = match expected.map(|slot| slot.kind) {
                Some(GraphicsBindingKind::Buffer { usage, mode, .. }) => (usage, mode),
                _ => (BufferUsage::Uniform, ResourceAccessMode::Read),
            };
            insert_binding(
                &mut supplied,
                GraphicsBindingLayout {
                    group: binding.group,
                    binding: binding.binding,
                    stages: binding.stages,
                    kind: GraphicsBindingKind::Buffer {
                        usage,
                        mode,
                        minimum_bytes: binding.bytes.len() as u64,
                    },
                    array: false,
                },
            )?;
        }
        for required in &self.bindings {
            let actual = supplied
                .get(&(required.group, required.binding))
                .ok_or_else(|| {
                    format!(
                        "Missing graphics binding {}:{}",
                        required.group, required.binding
                    )
                })?;
            if !contains_stages(actual.stages, required.stages) {
                return Err(format!(
                    "Graphics binding {}:{} does not cover its shader stages",
                    required.group, required.binding
                ));
            }
            let compatible = match (required.kind, actual.kind) {
                (
                    GraphicsBindingKind::Buffer {
                        usage,
                        mode,
                        minimum_bytes,
                    },
                    GraphicsBindingKind::Buffer {
                        usage: actual_usage,
                        mode: actual_mode,
                        minimum_bytes: bytes,
                    },
                ) => {
                    usage == actual_usage
                        && bytes >= minimum_bytes
                        && (!mode.reads() || actual_mode.reads())
                        && (!mode.writes() || actual_mode.writes())
                }
                (
                    GraphicsBindingKind::Image { storage },
                    GraphicsBindingKind::Image {
                        storage: actual_storage,
                    },
                ) => storage == actual_storage,
                (
                    GraphicsBindingKind::Sampler { comparison },
                    GraphicsBindingKind::Sampler {
                        comparison: actual_comparison,
                    },
                ) => comparison == actual_comparison,
                _ => false,
            };
            if !compatible || required.array != actual.array {
                return Err(format!(
                    "Incompatible graphics binding {}:{}",
                    required.group, required.binding
                ));
            }
        }
        Ok(())
    }
}

fn insert_binding(
    bindings: &mut BTreeMap<(u32, u32), GraphicsBindingLayout>,
    binding: GraphicsBindingLayout,
) -> Result<(), String> {
    if bindings
        .insert((binding.group, binding.binding), binding)
        .is_some()
    {
        return Err(format!(
            "Duplicate graphics binding {}:{}",
            binding.group, binding.binding
        ));
    }
    Ok(())
}

fn contains_stages(actual: ShaderStages, required: ShaderStages) -> bool {
    (!required.vertex || actual.vertex)
        && (!required.fragment || actual.fragment)
        && (!required.compute || actual.compute)
}

fn storage_mode(access: naga::StorageAccess) -> ResourceAccessMode {
    match (
        access.contains(naga::StorageAccess::LOAD),
        access.contains(naga::StorageAccess::STORE),
    ) {
        (true, true) => ResourceAccessMode::ReadWrite,
        (false, true) => ResourceAccessMode::Write,
        _ => ResourceAccessMode::Read,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_graph::BufferByteRange;
    use crate::renderer::frame_bindings::{BufferBinding, ConstantBinding};

    const SHADER: &str = "
        struct Frame { color: vec4f }
        @group(0) @binding(0) var<uniform> frame: Frame;
        @group(2) @binding(3) var<storage, read> positions: array<vec4f>;
        @group(4) @binding(0) var<uniform> unused: Frame;
        @vertex fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
            return positions[index];
        }
        @fragment fn fs_main() -> @location(0) vec4f { return frame.color; }
    ";

    fn packet() -> PassBindings {
        PassBindings {
            buffers: vec![BufferBinding {
                group: 2,
                binding: 3,
                resource: ResourceId(7),
                range: BufferByteRange::new(16, 16),
                stages: ShaderStages::VERTEX,
            }],
            constants: vec![ConstantBinding {
                group: 0,
                binding: 0,
                stages: ShaderStages::FRAGMENT,
                bytes: vec![0; 16],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn test_graphics_reflection_keeps_only_selected_entry_resources() {
        let interface = GraphicsInterface::reflect(SHADER, &PipelineStages::graphics()).unwrap();
        assert_eq!(interface.bindings.len(), 2);
        assert_eq!(interface.color_attachment_count(), 1);
        assert_eq!(interface.bindings[0].stages, ShaderStages::FRAGMENT);
        assert_eq!(interface.bindings[1].stages, ShaderStages::VERTEX);
        let depth = GraphicsInterface::reflect(
            SHADER,
            &PipelineStages::Graphics {
                vertex_entry: "vs_main".into(),
                fragment_entry: None,
            },
        )
        .unwrap();
        assert_eq!(depth.bindings.len(), 1);
        assert_eq!(depth.bindings[0].group, 2);
        assert_eq!(depth.color_attachment_count(), 0);
    }

    #[test]
    fn test_void_fragment_preserves_alpha_test_without_color_outputs() {
        let source = "@vertex fn vs_main() -> @builtin(position) vec4f { return vec4f(0.0); } @fragment fn fs_main() { discard; }";
        let interface = GraphicsInterface::reflect(source, &PipelineStages::graphics()).unwrap();
        assert_eq!(interface.color_attachment_count(), 0);
    }

    #[test]
    fn test_graphics_reflection_rejects_hidden_storage_writes_and_wrong_usage() {
        use crate::render_graph::{BufferAccess, ResourceAccessStage};
        let source = "@group(0) @binding(0) var<storage, read_write> values: array<vec4f>; @vertex fn vs_main() -> @builtin(position) vec4f { return vec4f(0.0); } @fragment fn fs_main() -> @location(0) vec4f { values[0] = vec4f(1.0); return values[0]; }";
        let interface = GraphicsInterface::reflect(source, &PipelineStages::graphics()).unwrap();
        let packet = PassBindings {
            buffers: vec![BufferBinding {
                group: 0,
                binding: 0,
                resource: ResourceId(7),
                range: BufferByteRange::new(16, 16),
                stages: ShaderStages::FRAGMENT,
            }],
            ..Default::default()
        };
        let mut access = BufferAccess::new(
            ResourceId(7),
            ResourceAccessMode::Read,
            BufferUsage::Storage,
            ResourceAccessStage::FragmentShader,
            BufferByteRange::new(0, 32),
        );
        assert!(
            interface
                .validate_buffer_accesses(&packet, &[access])
                .is_err()
        );
        access.mode = ResourceAccessMode::ReadWrite;
        interface
            .validate_buffer_accesses(&packet, &[access])
            .unwrap();
        access.usage = BufferUsage::Uniform;
        assert!(
            interface
                .validate_buffer_accesses(&packet, &[access])
                .is_err()
        );
        access.usage = BufferUsage::Storage;
        access.stage = ResourceAccessStage::VertexShader;
        assert!(
            interface
                .validate_buffer_accesses(&packet, &[access])
                .is_err()
        );
        access.stage = ResourceAccessStage::FragmentShader;
        access.range = BufferByteRange::new(0, 16);
        assert!(
            interface
                .validate_buffer_accesses(&packet, &[access])
                .is_err()
        );
    }

    #[test]
    fn test_graphics_bindings_reject_stage_span_and_duplicate_divergence() {
        let interface = GraphicsInterface::reflect(SHADER, &PipelineStages::graphics()).unwrap();
        interface
            .validate_bindings(&packet(), &[], |_| Some(32))
            .unwrap();
        let mut wrong_stage = packet();
        wrong_stage.buffers[0].stages = ShaderStages::FRAGMENT;
        assert!(
            interface
                .validate_bindings(&wrong_stage, &[], |_| Some(32))
                .unwrap_err()
                .contains("shader stages")
        );
        let mut short = packet();
        short.constants[0].bytes.truncate(8);
        assert!(
            interface
                .validate_bindings(&short, &[], |_| Some(32))
                .unwrap_err()
                .contains("Incompatible")
        );
        let mut duplicate = packet();
        duplicate.constants.push(duplicate.constants[0].clone());
        assert!(
            interface
                .validate_bindings(&duplicate, &[], |_| Some(32))
                .unwrap_err()
                .contains("Duplicate")
        );
        assert!(
            interface
                .validate_bindings(&packet(), &[], |_| Some(24))
                .unwrap_err()
                .contains("Out-of-bounds")
        );
    }

    #[test]
    fn test_graphics_bindings_reject_missing_and_wrong_resource_kind() {
        let interface = GraphicsInterface::reflect(SHADER, &PipelineStages::graphics()).unwrap();
        assert!(
            interface
                .validate_bindings(&PassBindings::default(), &[], |_| Some(32))
                .unwrap_err()
                .contains("Missing")
        );
        let mut provided = interface.bindings.clone();
        provided[0].kind = GraphicsBindingKind::Sampler { comparison: false };
        assert!(
            interface
                .validate_bindings(&PassBindings::default(), &provided, |_| None)
                .unwrap_err()
                .contains("Incompatible")
        );
    }
    #[test]
    fn test_explicit_constants_replace_implicit_draw_bindings() {
        let source = "struct Cascades { matrix: mat4x4f }; @group(2) @binding(0) var<uniform> cascades: Cascades; @vertex fn vs_main() -> @builtin(position) vec4f { return cascades.matrix * vec4f(0.0, 0.0, 0.0, 1.0); }";
        let interface = GraphicsInterface::reflect(
            source,
            &PipelineStages::Graphics {
                vertex_entry: "vs_main".into(),
                fragment_entry: None,
            },
        )
        .unwrap();
        let provided = [GraphicsBindingLayout {
            group: 2,
            binding: 0,
            stages: ShaderStages::VERTEX,
            kind: GraphicsBindingKind::Buffer {
                usage: BufferUsage::Storage,
                mode: ResourceAccessMode::Read,
                minimum_bytes: 64,
            },
            array: false,
        }];
        let mut packet = PassBindings::default();
        packet.constants.push(ConstantBinding {
            group: 2,
            binding: 0,
            stages: ShaderStages::VERTEX,
            bytes: vec![0; 64],
        });
        interface
            .validate_bindings(&packet, &provided, |_| None)
            .unwrap();
        packet.constants.push(packet.constants[0].clone());
        assert!(
            interface
                .validate_bindings(&packet, &provided, |_| None)
                .unwrap_err()
                .contains("Duplicate")
        );
    }
}
