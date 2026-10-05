//! Canonical descriptor-to-Metal binding ABI and reflected table layouts.

use naga::back::msl;

/// Shader compilation profile selecting the appropriate binding map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ShaderProfile {
    /// Standard graphics pipeline (bindless textures at buffer 9).
    Graphics,
}

fn binding_map(
    buffers: &[(u32, u32, u8)],
    textures: &[(u32, u32, u8)],
    samplers: &[(u32, u32, u8)],
    sizes_buffer: Option<u8>,
) -> msl::EntryPointResources {
    let mut resources = msl::EntryPointResources {
        sizes_buffer,
        ..Default::default()
    };
    for &(group, binding, index) in buffers {
        resources.resources.insert(
            naga::ResourceBinding { group, binding },
            msl::BindTarget {
                buffer: Some(index),
                ..Default::default()
            },
        );
    }
    for &(group, binding, index) in textures {
        resources.resources.insert(
            naga::ResourceBinding { group, binding },
            msl::BindTarget {
                texture: Some(index),
                ..Default::default()
            },
        );
    }
    for &(group, binding, index) in samplers {
        resources.resources.insert(
            naga::ResourceBinding { group, binding },
            msl::BindTarget {
                sampler: Some(msl::BindSamplerTarget::Resource(index)),
                ..Default::default()
            },
        );
    }
    resources
}

pub(crate) const BINDING_ABI_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TableBindingKind {
    Buffer,
    Texture,
    Sampler,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct TableBinding {
    pub(crate) group: u32,
    pub(crate) binding: u32,
    pub(crate) kind: TableBindingKind,
    pub(crate) index: usize,
    pub(crate) name: String,
    pub(crate) minimum_buffer_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct RuntimeArrayBinding {
    pub(crate) global_index: usize,
    pub(crate) size_index: usize,
    pub(crate) buffer_index: usize,
}

fn needs_runtime_size(ty: naga::Handle<naga::Type>, types: &naga::UniqueArena<naga::Type>) -> bool {
    match &types[ty].inner {
        naga::TypeInner::Array {
            size: naga::ArraySize::Dynamic,
            ..
        } => true,
        naga::TypeInner::Struct { members, .. } => members
            .last()
            .is_some_and(|member| needs_runtime_size(member.ty, types)),
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct ArgumentTableLayout {
    pub(crate) entry_point: String,
    pub(crate) stage: String,
    pub(crate) bindings: Vec<TableBinding>,
    pub(crate) buffer_count: usize,
    pub(crate) texture_count: usize,
    pub(crate) sampler_count: usize,
    pub(crate) sizes_buffer: Option<usize>,
    pub(crate) sizes_word_count: usize,
    pub(crate) runtime_array_bindings: Vec<RuntimeArrayBinding>,
}

impl ShaderProfile {
    pub(crate) fn resources(self) -> msl::EntryPointResources {
        match self {
            Self::Graphics => binding_map(
                &[(0, 0, 0), (0, 1, 1), (1, 0, 9)],
                &[],
                &[(1, 1, 0)],
                Some(8),
            ),
        }
    }
}

/// Supply the same canonical map to every declared entry point, including
/// compute entries in otherwise graphics-oriented modules.
pub(crate) fn options_for_module(profile: ShaderProfile, module: &naga::Module) -> msl::Options {
    let mut options = msl::Options {
        lang_version: (3, 0),
        fake_missing_bindings: false,
        ..Default::default()
    };
    let compute = module
        .entry_points
        .iter()
        .all(|entry| entry.stage == naga::ShaderStage::Compute);
    let mut resources = if compute && profile == ShaderProfile::Graphics {
        msl::EntryPointResources::default()
    } else {
        profile.resources()
    };
    if module
        .global_variables
        .iter()
        .any(|(_, global)| needs_runtime_size(global.ty, &module.types))
    {
        resources.sizes_buffer = Some(8);
    }
    let mut globals: Vec<_> = module
        .global_variables
        .iter()
        .filter_map(|(_, global)| global.binding.map(|binding| (binding, global)))
        .collect();
    globals.sort_by_key(|(binding, _)| (binding.group, binding.binding));
    resources
        .resources
        .retain(|binding, _| globals.iter().any(|(declared, _)| declared == binding));
    let resource_kind = |global: &naga::GlobalVariable| match module.types[global.ty].inner {
        naga::TypeInner::Image { .. } => TableBindingKind::Texture,
        naga::TypeInner::Sampler { .. } => TableBindingKind::Sampler,
        _ => TableBindingKind::Buffer,
    };
    for (binding, global) in &globals {
        let kind = resource_kind(global);
        let valid = resources
            .resources
            .get(binding)
            .is_some_and(|target| match kind {
                TableBindingKind::Buffer => target.buffer.is_some(),
                TableBindingKind::Texture => target.texture.is_some(),
                TableBindingKind::Sampler => target.sampler.is_some(),
            });
        if !valid {
            resources.resources.remove(binding);
        }
    }
    for (binding, global) in globals {
        if resources.resources.contains_key(&binding) {
            continue;
        }
        let kind = resource_kind(global);
        let mut index = 0u8;
        let limit = match kind {
            TableBindingKind::Buffer => 31,
            TableBindingKind::Texture => 128,
            TableBindingKind::Sampler => 16,
        };
        while index < limit && ((kind == TableBindingKind::Buffer && (resources.sizes_buffer == Some(index) || (!compute && index as usize == VERTEX_ATTRIBUTE_BUFFER_INDEX))) || resources.resources.values().any(|target| match kind {
            TableBindingKind::Buffer => target.buffer == Some(index),
            TableBindingKind::Texture => target.texture == Some(index),
            TableBindingKind::Sampler => matches!(target.sampler, Some(msl::BindSamplerTarget::Resource(existing)) if existing == index),
        })) { index += 1; }
        let target = match kind {
            TableBindingKind::Buffer => msl::BindTarget {
                buffer: Some(index),
                ..Default::default()
            },
            TableBindingKind::Texture => msl::BindTarget {
                texture: Some(index),
                ..Default::default()
            },
            TableBindingKind::Sampler => msl::BindTarget {
                sampler: Some(msl::BindSamplerTarget::Resource(index)),
                ..Default::default()
            },
        };
        resources.resources.insert(binding, target);
    }
    for entry in &module.entry_points {
        options
            .per_entry_point_map
            .insert(entry.name.clone(), resources.clone());
    }
    options
}

/// Reflect only entry-point-live resources. The identical binding map supplies
/// MSL translation and Metal 4 argument-table allocation/population.
pub(crate) fn reflect_table_layout(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    options: &msl::Options,
) -> Result<Vec<ArgumentTableLayout>, crate::error::RendererError> {
    use crate::error::RendererError;
    let mut layouts = Vec::new();
    let runtime_globals: Vec<_> = module
        .global_variables
        .iter()
        .filter(|(_, global)| needs_runtime_size(global.ty, &module.types))
        .map(|(handle, _)| handle)
        .collect();
    for (entry_index, entry) in module.entry_points.iter().enumerate() {
        let resources = options
            .per_entry_point_map
            .get(&entry.name)
            .ok_or_else(|| {
                RendererError::InvalidOperation(format!(
                    "Missing canonical Metal bindings for '{}'",
                    entry.name
                ))
            })?;
        let mut bindings = Vec::new();
        for (handle, global) in module.global_variables.iter() {
            if info.get_entry_point(entry_index)[handle].is_empty() {
                continue;
            }
            let Some(binding) = global.binding else {
                continue;
            };
            let target = resources.resources.get(&binding).ok_or_else(|| {
                RendererError::InvalidOperation(format!(
                    "Shader '{}' has no canonical binding for {}:{}",
                    entry.name, binding.group, binding.binding
                ))
            })?;
            let (kind, index) = if let Some(index) = target.buffer {
                (TableBindingKind::Buffer, index as usize)
            } else if let Some(index) = target.texture {
                (TableBindingKind::Texture, index as usize)
            } else if let Some(msl::BindSamplerTarget::Resource(index)) = target.sampler {
                (TableBindingKind::Sampler, index as usize)
            } else {
                return Err(RendererError::InvalidOperation(format!(
                    "Unsupported canonical Metal binding {}:{}",
                    binding.group, binding.binding
                )));
            };
            if bindings
                .iter()
                .any(|existing: &TableBinding| existing.kind == kind && existing.index == index)
            {
                return Err(RendererError::InvalidOperation(format!(
                    "Shader '{}' aliases two live {:?} bindings at slot {}",
                    entry.name, kind, index
                )));
            }
            bindings.push(TableBinding {
                group: binding.group,
                binding: binding.binding,
                kind,
                index,
                name: global
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{}_{}", binding.group, binding.binding)),
                minimum_buffer_bytes: if kind == TableBindingKind::Buffer {
                    match module.types[global.ty].inner {
                        naga::TypeInner::BindingArray {
                            size: naga::ArraySize::Constant(count),
                            ..
                        } => {
                            u64::from(count.get())
                                * std::mem::size_of::<objc2_metal::MTLResourceID>() as u64
                        }
                        _ => u64::from(module.types[global.ty].inner.size(module.to_ctx())),
                    }
                } else {
                    0
                },
            });
        }
        bindings
            .sort_by_key(|binding| (binding.kind, binding.index, binding.group, binding.binding));
        let count = |kind| {
            bindings
                .iter()
                .filter(|binding| binding.kind == kind)
                .map(|binding| binding.index + 1)
                .max()
                .unwrap_or(0)
        };
        let runtime_array_bindings: Vec<_> = runtime_globals
            .iter()
            .enumerate()
            .filter_map(|(size_index, &handle)| {
                if info.get_entry_point(entry_index)[handle].is_empty() {
                    return None;
                }
                let global = &module.global_variables[handle];
                let binding = global.binding?;
                let buffer_index = resources.resources.get(&binding)?.buffer? as usize;
                Some(RuntimeArrayBinding {
                    global_index: handle.index(),
                    size_index,
                    buffer_index,
                })
            })
            .collect();
        let sizes_buffer = if runtime_array_bindings.is_empty() {
            None
        } else {
            resources.sizes_buffer.map(usize::from)
        };
        let buffer_count =
            count(TableBindingKind::Buffer).max(sizes_buffer.map(|index| index + 1).unwrap_or(0));
        let texture_count = count(TableBindingKind::Texture);
        let sampler_count = count(TableBindingKind::Sampler);
        if buffer_count > 31 || texture_count > 128 || sampler_count > 16 {
            return Err(RendererError::InvalidOperation(format!(
                "Shader '{}' exceeds Metal 4 argument-table limits: {buffer_count} buffers, {texture_count} textures, {sampler_count} samplers",
                entry.name
            )));
        }
        layouts.push(ArgumentTableLayout {
            entry_point: entry.name.clone(),
            stage: format!("{:?}", entry.stage).to_ascii_lowercase(),
            buffer_count,
            texture_count,
            sampler_count,
            bindings,
            sizes_buffer,
            sizes_word_count: if runtime_array_bindings.is_empty() {
                0
            } else {
                runtime_globals.len()
            },
            runtime_array_bindings,
        });
    }
    Ok(layouts)
}

/// Native vertex attributes occupy their own buffer slot outside descriptor sets.
pub(crate) const VERTEX_ATTRIBUTE_BUFFER_INDEX: usize = 10;

#[cfg(test)]
mod layout_tests {
    use super::*;
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    fn reflect(source: &str, profile: ShaderProfile) -> Vec<ArgumentTableLayout> {
        let module = naga::front::wgsl::parse_str(source).unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap();
        reflect_table_layout(&module, &info, &options_for_module(profile, &module)).unwrap()
    }

    #[test]
    fn test_layout_is_deterministic_and_stage_live() {
        let source = "@group(0) @binding(0) var<storage,read> frame:array<vec4f>; @group(0) @binding(1) var<storage,read> objects:array<vec4f>; @vertex fn vs_main()->@builtin(position) vec4f {return frame[0];} @fragment fn fs_main()->@location(0) vec4f {return objects[0];}";
        let first = reflect(source, ShaderProfile::Graphics);
        assert_eq!(first, reflect(source, ShaderProfile::Graphics));
        assert_eq!(first[0].bindings.len(), 1);
        assert_eq!(first[0].bindings[0].index, 0);
        assert_eq!(first[1].bindings[0].index, 1);
    }

    #[test]
    fn test_bindless_indirection_reflects_native_resource_id_byte_capacity() {
        let layouts = reflect(
            "enable wgpu_binding_array;
             @group(1) @binding(0) var images: binding_array<texture_2d<f32>, 16>;
             @group(1) @binding(1) var filtering: sampler;
             @fragment fn fs_main() -> @location(0) vec4f { return textureSampleLevel(images[3], filtering, vec2f(0.5), 0.0); }",
            ShaderProfile::Graphics,
        );
        let image_ids = layouts[0]
            .bindings
            .iter()
            .find(|binding| binding.group == 1 && binding.binding == 0)
            .unwrap();
        assert_eq!(image_ids.kind, TableBindingKind::Buffer);
        assert_eq!(image_ids.index, 9);
        assert_eq!(image_ids.minimum_buffer_bytes, 128);
    }

    #[test]
    fn test_runtime_metadata_slot_is_reserved_before_descriptor_assignment() {
        let mut source = String::new();
        for index in 0..9 {
            source.push_str(&format!(
                "@group(0) @binding({index}) var<storage, read_write> data{index}: array<u32>;\n"
            ));
        }
        source.push_str("@compute @workgroup_size(1) fn cs_main() {\n");
        for index in 0..9 {
            source.push_str(&format!("data{index}[0] = {index}u;\n"));
        }
        source.push('}');
        let layout = reflect(&source, ShaderProfile::Graphics).remove(0);
        assert_eq!(layout.sizes_buffer, Some(8));
        assert_eq!(layout.sizes_word_count, 9);
        assert!(!layout.bindings.iter().any(|binding| binding.index == 8));
        assert_eq!(
            layout
                .bindings
                .iter()
                .find(|binding| binding.binding == 8)
                .unwrap()
                .index,
            9
        );
        for (index, binding) in layout.runtime_array_bindings.iter().enumerate() {
            assert_eq!(binding.size_index, index);
            assert_ne!(binding.buffer_index, 8);
        }
    }

    #[test]
    fn test_particle_compute_uses_canonical_frame_binding() {
        let source = include_str!("../../../resources/shaders/particles/particle_emit.wgsl")
            .replace(
                "#include \"common.wgsl\"",
                include_str!("../../../resources/shaders/particles/common.wgsl"),
            );
        let layouts = reflect(&source, ShaderProfile::Graphics);
        let frame = layouts[0]
            .bindings
            .iter()
            .find(|binding| binding.group == 1 && binding.binding == 0)
            .unwrap();
        assert_eq!(frame.index, 5);
        let emitters = layouts[0]
            .bindings
            .iter()
            .find(|binding| binding.group == 1 && binding.binding == 1)
            .unwrap();
        assert_eq!(emitters.index, 6);
        assert_eq!(layouts[0].sizes_buffer, Some(8));
        assert!(layouts[0].sizes_word_count > 0);
        assert!(layouts[0].bindings.iter().all(|binding| binding.index != 8));
    }

    #[test]
    fn test_reflected_compute_layout_generates_msl_without_fake_bindings() {
        let module = naga::front::wgsl::parse_str("@group(0) @binding(0) var<storage,read_write> output:array<f32>; @compute @workgroup_size(1) fn cs_main(){output[0]=1.0;}").unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap();
        let options = options_for_module(ShaderProfile::Graphics, &module);
        let layout = reflect_table_layout(&module, &info, &options).unwrap();
        assert_eq!(layout[0].buffer_count, 9);
        assert_eq!(layout[0].sizes_buffer, Some(8));
        assert_eq!(layout[0].runtime_array_bindings[0].buffer_index, 0);
        let (source, translation) =
            msl::write_string(&module, &info, &options, &msl::PipelineOptions::default()).unwrap();
        assert!(
            translation.entry_point_names.iter().all(Result::is_ok),
            "{:?}",
            translation.entry_point_names
        );
        assert!(source.contains("[[buffer(0)]]"));
    }

    #[test]
    fn test_builtin_compute_sources_translate_with_reflected_byte_sizes() {
        for source in [
            include_str!("../../../resources/shaders/compute/animation/pose_eval.wgsl").to_string(),
            include_str!("../../../resources/shaders/lighting/light_cull.wgsl").replace(
                "#include \"../common/lighting_types.wgsl\"",
                include_str!("../../../resources/shaders/common/lighting_types.wgsl"),
            ),
        ] {
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            let info = Validator::new(ValidationFlags::all(), Capabilities::all())
                .validate(&module)
                .unwrap();
            let options = options_for_module(ShaderProfile::Graphics, &module);
            let layouts = reflect_table_layout(&module, &info, &options).unwrap();
            assert!(layouts[0].sizes_word_count > 0);
            assert!(
                layouts[0]
                    .runtime_array_bindings
                    .iter()
                    .all(|binding| binding.size_index < layouts[0].sizes_word_count)
            );
            let (_, translation) =
                msl::write_string(&module, &info, &options, &msl::PipelineOptions::default())
                    .unwrap();
            assert!(
                translation.entry_point_names.iter().all(Result::is_ok),
                "{:?}",
                translation.entry_point_names
            );
        }
    }

    #[test]
    fn test_compute_declaration_order_preserves_descriptor_slot_mapping() {
        let first = "@group(0) @binding(0) var<storage,read_write> a:array<f32>; @group(0) @binding(1) var<storage,read_write> b:array<f32>; @compute @workgroup_size(1) fn cs_main(){a[0]=b[0];}";
        let second = "@group(0) @binding(1) var<storage,read_write> b:array<f32>; @group(0) @binding(0) var<storage,read_write> a:array<f32>; @compute @workgroup_size(1) fn cs_main(){a[0]=b[0];}";
        assert_eq!(
            reflect(first, ShaderProfile::Graphics)[0].bindings,
            reflect(second, ShaderProfile::Graphics)[0].bindings
        );
    }

    #[test]
    fn test_runtime_size_words_pack_dynamic_globals_in_module_order() {
        let source = "@group(0) @binding(0) var<uniform> frame:vec4f; @group(0) @binding(1) var<storage,read_write> a:array<f32>; @group(0) @binding(2) var<storage,read_write> b:array<f32>; @compute @workgroup_size(1) fn cs_main(){a[0]=frame.x;}";
        let layout = &reflect(source, ShaderProfile::Graphics)[0];
        assert_eq!(layout.sizes_word_count, 2);
        assert_eq!(layout.runtime_array_bindings.len(), 1);
        assert_eq!(layout.runtime_array_bindings[0].global_index, 1);
        assert_eq!(layout.runtime_array_bindings[0].size_index, 0);
        assert_eq!(layout.runtime_array_bindings[0].buffer_index, 1);
    }

    #[test]
    fn test_excess_native_binding_count_returns_validation_error() {
        let declarations = (0..32)
            .map(|index| {
                format!("@group(0) @binding({index}) var<storage,read_write> value{index}:f32;")
            })
            .collect::<String>();
        let writes = (0..32)
            .map(|index| format!("value{index}=1.0;"))
            .collect::<String>();
        let module = naga::front::wgsl::parse_str(&format!(
            "{declarations} @compute @workgroup_size(1) fn cs_main(){{{writes}}}"
        ))
        .unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap();
        assert!(
            reflect_table_layout(
                &module,
                &info,
                &options_for_module(ShaderProfile::Graphics, &module)
            )
            .is_err()
        );
    }

    #[test]
    fn test_missing_binding_is_rejected_before_msl_translation() {
        let module = naga::front::wgsl::parse_str("@group(19) @binding(0) var<storage,read_write> output:array<f32>; @compute @workgroup_size(1) fn cs_main(){output[0]=1.0;}").unwrap();
        let info = Validator::new(ValidationFlags::all(), Capabilities::all())
            .validate(&module)
            .unwrap();
        let mut options = options_for_module(ShaderProfile::Graphics, &module);
        options
            .per_entry_point_map
            .get_mut("cs_main")
            .unwrap()
            .resources
            .clear();
        assert!(reflect_table_layout(&module, &info, &options).is_err());
    }
}
