//! Offline Naga compiler tool, independent of the engine runtime.

use naga::{
    back::{msl, spv},
    valid::{Capabilities, ValidationFlags, Validator},
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, panic::AssertUnwindSafe};

const ABI: u32 = 1;
const MAX_REQUEST: usize = 8 * 1024 * 1024;
const SIZES_BUFFER: u8 = 8;
const VERTEX_BUFFER: u8 = 10;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
enum Stage {
    Vertex,
    Fragment,
    Compute,
}
impl Stage {
    fn naga(self) -> naga::ShaderStage {
        match self {
            Self::Vertex => naga::ShaderStage::Vertex,
            Self::Fragment => naga::ShaderStage::Fragment,
            Self::Compute => naga::ShaderStage::Compute,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    name: String,
    stage: Stage,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    abi: u32,
    source: String,
    selections: Vec<Selection>,
    #[serde(default)]
    constants: BTreeMap<String, f64>,
}
#[derive(Serialize)]
struct Reply {
    abi: u32,
    compiler: &'static str,
    error: &'static str,
    message: String,
    entries: Vec<Entry>,
}
#[derive(Debug, Serialize)]
struct Entry {
    name: String,
    stage: Stage,
    metal_name: String,
    metal_source: String,
    spirv: Vec<u32>,
    workgroup_size: [u32; 3],
    bindings: Vec<Binding>,
    inputs: Vec<Io>,
    outputs: Vec<Io>,
    sizes_buffer: i32,
    sizes_word_count: u32,
}
#[derive(Debug, Serialize)]
struct Binding {
    group: u32,
    binding: u32,
    name: String,
    kind: &'static str,
    access: &'static str,
    declared_access: &'static str,
    uniform: bool,
    minimum_size: u64,
    alignment: u32,
    runtime_array: bool,
    runtime_array_offset: u32,
    runtime_array_stride: u32,
    array_count: u32,
    dimension: &'static str,
    arrayed: bool,
    multisampled: bool,
    depth: bool,
    comparison: bool,
    sample_type: &'static str,
    storage_format: String,
    metal_kind: &'static str,
    metal_index: u32,
    metal_minimum_size: u64,
    size_index: i32,
}
#[derive(Debug, Serialize)]
struct Io {
    name: String,
    location: i32,
    builtin: String,
    scalar: &'static str,
    width: u8,
    components: u8,
    interpolation: String,
    sampling: String,
    blend_source: i32,
    per_primitive: bool,
}
struct Failure(&'static str, String);
type Result<T> = std::result::Result<T, Failure>;
fn fail<T>(kind: &'static str, message: impl Into<String>) -> Result<T> {
    Err(Failure(kind, message.into()))
}
fn scalar(kind: naga::ScalarKind) -> &'static str {
    match kind {
        naga::ScalarKind::Sint => "Sint",
        naga::ScalarKind::Uint => "Uint",
        naga::ScalarKind::Float => "Float",
        naga::ScalarKind::Bool => "Bool",
        _ => "Abstract",
    }
}
fn access(value: naga::StorageAccess) -> &'static str {
    match (
        value.contains(naga::StorageAccess::LOAD),
        value.contains(naga::StorageAccess::STORE),
    ) {
        (true, true) => "Read_Write",
        (false, true) => "Write",
        _ => "Read",
    }
}
fn runtime_array(module: &naga::Module, ty: naga::Handle<naga::Type>) -> Option<(u32, u32)> {
    match &module.types[ty].inner {
        naga::TypeInner::Array {
            size: naga::ArraySize::Dynamic,
            stride,
            ..
        } => Some((0, *stride)),
        naga::TypeInner::Struct { members, .. } => members.last().and_then(|m| {
            runtime_array(module, m.ty).map(|(offset, stride)| (m.offset + offset, stride))
        }),
        _ => None,
    }
}
fn binding(
    module: &naga::Module,
    global: &naga::GlobalVariable,
    layout: &naga::proc::Layouter,
) -> Result<Binding> {
    let Some(slot) = global.binding else {
        return fail("Reflection", "A reflected resource has no binding");
    };
    let (ty, count) = match module.types[global.ty].inner {
        naga::TypeInner::BindingArray {
            base,
            size: naga::ArraySize::Constant(count),
        } => (base, count.get()),
        naga::TypeInner::BindingArray { .. } => {
            return fail(
                "Unsupported",
                "Runtime or override-sized resource binding arrays require an explicit backend limit",
            );
        }
        _ => (global.ty, 1),
    };
    let mut result = Binding {
        group: slot.group,
        binding: slot.binding,
        name: global.name.clone().unwrap_or_default(),
        kind: "Buffer",
        access: "Read",
        declared_access: "Read",
        uniform: false,
        minimum_size: 0,
        alignment: 1,
        runtime_array: false,
        runtime_array_offset: 0,
        runtime_array_stride: 0,
        array_count: count,
        dimension: "None",
        arrayed: false,
        multisampled: false,
        depth: false,
        comparison: false,
        sample_type: "None",
        storage_format: String::new(),
        metal_kind: "Buffer",
        metal_index: 0,
        metal_minimum_size: 0,
        size_index: -1,
    };
    match global.space {
        naga::AddressSpace::Uniform | naga::AddressSpace::Storage { .. } => {
            result.uniform = global.space == naga::AddressSpace::Uniform;
            result.minimum_size = u64::from(layout[ty].size);
            result.alignment = layout[ty].alignment * 1;
            if let naga::AddressSpace::Storage { access: flags } = global.space {
                result.access = access(flags);
            }
            if let Some((offset, stride)) = runtime_array(module, ty) {
                result.runtime_array = true;
                result.runtime_array_offset = offset;
                result.runtime_array_stride = stride;
                result.minimum_size = u64::from(offset) + u64::from(stride);
            }
        }
        naga::AddressSpace::Handle => match module.types[ty].inner {
            naga::TypeInner::Image {
                dim,
                arrayed,
                class,
            } => {
                result.kind = "Texture";
                result.dimension = match dim {
                    naga::ImageDimension::D1 => "D1",
                    naga::ImageDimension::D2 => "D2",
                    naga::ImageDimension::D3 => "D3",
                    naga::ImageDimension::Cube => "Cube",
                };
                result.arrayed = arrayed;
                match class {
                    naga::ImageClass::Sampled { kind, multi } => {
                        result.sample_type = scalar(kind);
                        result.multisampled = multi;
                    }
                    naga::ImageClass::Depth { multi } => {
                        result.sample_type = "Float";
                        result.depth = true;
                        result.multisampled = multi;
                    }
                    naga::ImageClass::Storage {
                        format,
                        access: flags,
                    } => {
                        result.storage_format = format!("{format:?}");
                        result.sample_type = scalar(naga::Scalar::from(format).kind);
                        result.access = access(flags);
                    }
                    naga::ImageClass::External => {
                        return fail(
                            "Unsupported",
                            "External textures require a multi-plane binding contract",
                        );
                    }
                }
            }
            naga::TypeInner::Sampler { comparison } => {
                result.kind = "Sampler";
                result.comparison = comparison;
            }
            _ => {
                return fail(
                    "Unsupported",
                    format!("Unsupported resource {}:{}", slot.group, slot.binding),
                );
            }
        },
        _ => {
            return fail(
                "Unsupported",
                format!(
                    "Unsupported address space at {}:{}",
                    slot.group, slot.binding
                ),
            );
        }
    }
    let array = matches!(
        module.types[global.ty].inner,
        naga::TypeInner::BindingArray { .. }
    );
    result.metal_kind = if array { "Buffer" } else { result.kind };
    result.metal_minimum_size = if array {
        u64::from(count) * 8
    } else {
        result.minimum_size
    };
    Ok(result)
}
fn io(
    module: &naga::Module,
    ty: naga::Handle<naga::Type>,
    bound: &Option<naga::Binding>,
    name: &str,
    result: &mut Vec<Io>,
) -> Result<()> {
    if let naga::TypeInner::Struct { ref members, .. } = module.types[ty].inner {
        for member in members {
            io(
                module,
                member.ty,
                &member.binding,
                member.name.as_deref().unwrap_or(""),
                result,
            )?;
        }
        return Ok(());
    }
    let Some(bound) = bound else {
        return fail("Reflection", "Entry-point IO has no location or built-in");
    };
    let (kind, width, components) = match module.types[ty].inner {
        naga::TypeInner::Scalar(s) => (scalar(s.kind), s.width, 1),
        naga::TypeInner::Vector { size, scalar: s } => (scalar(s.kind), s.width, size as u8),
        _ => return fail("Unsupported", "Entry-point IO must be scalar or vector"),
    };
    let mut value = Io {
        name: name.into(),
        location: -1,
        builtin: String::new(),
        scalar: kind,
        width,
        components,
        interpolation: String::new(),
        sampling: String::new(),
        blend_source: -1,
        per_primitive: false,
    };
    match bound {
        naga::Binding::BuiltIn(b) => value.builtin = format!("{b:?}"),
        naga::Binding::Location {
            location,
            interpolation,
            sampling,
            blend_src,
            per_primitive,
        } => {
            value.per_primitive = *per_primitive;
            value.location = i32::try_from(*location).map_err(|_| {
                Failure("Unsupported", "Location exceeds compiler ABI range".into())
            })?;
            value.interpolation = interpolation.map(|x| format!("{x:?}")).unwrap_or_default();
            value.sampling = sampling.map(|x| format!("{x:?}")).unwrap_or_default();
            value.blend_source = blend_src.map(|x| x as i32).unwrap_or(-1);
        }
    }
    result.push(value);
    Ok(())
}
fn compile_entry(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    selection: &Selection,
    constants: &naga::back::PipelineConstants,
) -> Result<Entry> {
    let stage = selection.stage.naga();
    if !module
        .entry_points
        .iter()
        .any(|e| e.name == selection.name && e.stage == stage)
    {
        return fail(
            "Missing_Entry",
            format!("Missing {:?} entry '{}'", selection.stage, selection.name),
        );
    }
    let (module, info) = naga::back::pipeline_constants::process_overrides(
        module,
        info,
        Some((stage, &selection.name)),
        constants,
    )
    .map_err(|e| Failure("Constants", e.to_string()))?;
    let Some(entry_index) = module
        .entry_points
        .iter()
        .position(|e| e.name == selection.name && e.stage == stage)
    else {
        return fail(
            "Missing_Entry",
            "Selected entry disappeared while resolving overrides",
        );
    };
    let entry = &module.entry_points[entry_index];
    let mut layouter = naga::proc::Layouter::default();
    layouter
        .update(module.to_ctx())
        .map_err(|e| Failure("Reflection", e.to_string()))?;
    let runtime_globals: Vec<_> = module
        .global_variables
        .iter()
        .filter(|(_, g)| runtime_array(&module, g.ty).is_some())
        .map(|(h, _)| h)
        .collect();
    let mut resources = msl::EntryPointResources::default();
    let mut bindings = Vec::new();
    let mut sorted: Vec<_> = module
        .global_variables
        .iter()
        .filter(|(handle, global)| {
            global.binding.is_some() && !info.get_entry_point(entry_index)[*handle].is_empty()
        })
        .collect();
    sorted.sort_by_key(|(_, global)| global.binding.map(|x| (x.group, x.binding)));
    let mut used_buffers = Vec::new();
    let mut used_textures = Vec::new();
    let mut used_samplers = Vec::new();
    let has_runtime = sorted
        .iter()
        .any(|(_, g)| runtime_array(&module, g.ty).is_some());
    if has_runtime {
        resources.sizes_buffer = Some(SIZES_BUFFER);
        used_buffers.push(SIZES_BUFFER);
    }
    if selection.stage == Stage::Vertex {
        used_buffers.push(VERTEX_BUFFER);
    }
    for (_, global) in &sorted {
        let b = binding(&module, global, &layouter)?;
        match (b.group, b.binding, b.metal_kind) {
            (0, 0, "Buffer") => used_buffers.push(0),
            (0, 1, "Buffer") => used_buffers.push(1),
            (1, 0, "Buffer") => used_buffers.push(9),
            (1, 1, "Sampler") => used_samplers.push(0),
            _ => {}
        }
    }
    for (handle, global) in sorted {
        let mut b = binding(&module, global, &layouter)?;
        b.declared_access = b.access;
        let usage = info.get_entry_point(entry_index)[handle];
        let read = usage.contains(naga::valid::GlobalUse::READ)
            || usage.contains(naga::valid::GlobalUse::ATOMIC);
        let write = usage.contains(naga::valid::GlobalUse::WRITE)
            || usage.contains(naga::valid::GlobalUse::ATOMIC);
        b.access = match (read, write) {
            (true, true) => "Read_Write",
            (true, false) => "Read",
            (false, true) => "Write",
            (false, false) => "None",
        };
        let (used, limit) = match b.metal_kind {
            "Buffer" => (&mut used_buffers, 31),
            "Texture" => (&mut used_textures, 128),
            _ => (&mut used_samplers, 16),
        };
        let preferred = match (b.group, b.binding, b.metal_kind) {
            (0, 0, "Buffer") => Some(0),
            (0, 1, "Buffer") => Some(1),
            (1, 0, "Buffer") => Some(9),
            (1, 1, "Sampler") => Some(0),
            _ => None,
        };
        let slot = preferred
            .or_else(|| (0..limit).find(|s| !used.contains(s)))
            .ok_or_else(|| {
                Failure(
                    "Unsupported",
                    format!("Too many Metal {} arguments", b.metal_kind),
                )
            })?;
        if !used.contains(&slot) {
            used.push(slot);
        }
        b.metal_index = u32::from(slot);
        b.size_index = runtime_globals
            .iter()
            .position(|h| *h == handle)
            .map(|i| i as i32)
            .unwrap_or(-1);
        let target = match b.metal_kind {
            "Buffer" => msl::BindTarget {
                buffer: Some(slot),
                ..Default::default()
            },
            "Texture" => msl::BindTarget {
                texture: Some(slot),
                ..Default::default()
            },
            _ => msl::BindTarget {
                sampler: Some(msl::BindSamplerTarget::Resource(slot)),
                ..Default::default()
            },
        };
        let Some(resource_binding) = global.binding else {
            return fail("Reflection", "Missing resource binding");
        };
        resources.resources.insert(resource_binding, target);
        bindings.push(b);
    }
    let bounds = naga::proc::BoundsCheckPolicies {
        index: naga::proc::BoundsCheckPolicy::ReadZeroSkipWrite,
        buffer: naga::proc::BoundsCheckPolicy::ReadZeroSkipWrite,
        image_load: naga::proc::BoundsCheckPolicy::ReadZeroSkipWrite,
        binding_array: naga::proc::BoundsCheckPolicy::Restrict,
    };
    let mut options = msl::Options {
        lang_version: (3, 0),
        fake_missing_bindings: false,
        bounds_check_policies: bounds,
        ..Default::default()
    };
    options
        .per_entry_point_map
        .insert(entry.name.clone(), resources);
    let (metal_source, translation) = msl::write_string(
        &module,
        &info,
        &options,
        &msl::PipelineOptions {
            entry_point: Some((stage, entry.name.clone())),
            ..Default::default()
        },
    )
    .map_err(|e| Failure("Metal_Generation", e.to_string()))?;
    let metal_name = translation
        .entry_point_names
        .get(entry_index)
        .ok_or_else(|| Failure("Reflection", "Missing translated entry mapping".into()))?
        .as_ref()
        .map_err(|e| Failure("Metal_Generation", format!("{e:?}")))?
        .clone();
    let spirv = spv::write_vec(
        &module,
        &info,
        &spv::Options {
            flags: spv::WriterFlags::LABEL_VARYINGS | spv::WriterFlags::CLAMP_FRAG_DEPTH,
            bounds_check_policies: bounds,
            ..Default::default()
        },
        Some(&spv::PipelineOptions {
            shader_stage: stage,
            entry_point: entry.name.clone(),
        }),
    )
    .map_err(|e| Failure("Spirv_Generation", e.to_string()))?;
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    for argument in &entry.function.arguments {
        io(
            &module,
            argument.ty,
            &argument.binding,
            argument.name.as_deref().unwrap_or(""),
            &mut inputs,
        )?;
    }
    if let Some(result) = &entry.function.result {
        io(&module, result.ty, &result.binding, "", &mut outputs)?;
    }
    Ok(Entry {
        name: entry.name.clone(),
        stage: selection.stage,
        metal_name,
        metal_source,
        spirv,
        workgroup_size: entry.workgroup_size,
        bindings,
        inputs,
        outputs,
        sizes_buffer: if has_runtime {
            i32::from(SIZES_BUFFER)
        } else {
            -1
        },
        sizes_word_count: if has_runtime {
            runtime_globals.len() as u32
        } else {
            0
        },
    })
}
fn compile(request: Request) -> Result<Vec<Entry>> {
    if request.abi != ABI {
        return fail("ABI_Mismatch", "Unsupported compiler request ABI");
    }
    if request.source.is_empty()
        || request.selections.is_empty()
        || request.selections.len() > 16
        || request.constants.len() > 1024
    {
        return fail(
            "Invalid_Request",
            "Empty source, invalid selection count or too many overrides",
        );
    }
    for (index, selected) in request.selections.iter().enumerate() {
        if selected.name.is_empty()
            || request.selections[..index]
                .iter()
                .any(|s| s.name == selected.name && s.stage == selected.stage)
        {
            return fail("Invalid_Request", "Empty or duplicate selected entry");
        }
    }
    let module = naga::front::wgsl::parse_str(&request.source)
        .map_err(|e| Failure("Parse", e.emit_to_string(&request.source)))?;
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .subgroup_stages(naga::valid::ShaderStages::all())
        .subgroup_operations(naga::valid::SubgroupOperationSet::all())
        .validate(&module)
        .map_err(|e| Failure("Validation", e.emit_to_string(&request.source)))?;
    let constants = request.constants.into_iter().collect();
    request
        .selections
        .iter()
        .map(|selected| compile_entry(&module, &info, selected, &constants))
        .collect()
}
fn reply(bytes: &[u8]) -> Reply {
    let outcome = serde_json::from_slice::<Request>(bytes)
        .map_err(|e| Failure("Invalid_Request", e.to_string()))
        .and_then(compile);
    match outcome {
        Ok(entries) => Reply {
            abi: ABI,
            compiler: "naga-29.0.1;msl-3.0;binding-abi-1;bounds-readzero;binding-arrays-restrict",
            error: "None",
            message: String::new(),
            entries,
        },
        Err(Failure(error, message)) => Reply {
            abi: ABI,
            compiler: "naga-29.0.1;msl-3.0;binding-abi-1;bounds-readzero;binding-arrays-restrict",
            error,
            message,
            entries: Vec::new(),
        },
    }
}
/// Compiles one bounded offline JSON request into a validated owned artifact.
pub fn compile_json(bytes: &[u8]) -> std::result::Result<Vec<u8>, serde_json::Error> {
    let answer = if bytes.is_empty() || bytes.len() > MAX_REQUEST {
        Reply {
            abi: ABI,
            compiler: "naga-29.0.1;msl-3.0;binding-abi-1;bounds-readzero;binding-arrays-restrict",
            error: "Invalid_Request",
            message: "Compiler request exceeds its byte limit or is empty".into(),
            entries: Vec::new(),
        }
    } else {
        std::panic::catch_unwind(AssertUnwindSafe(|| reply(bytes))).unwrap_or_else(|_| Reply {
            abi: ABI,
            compiler: "naga-29.0.1;msl-3.0;binding-abi-1;bounds-readzero;binding-arrays-restrict",
            error: "Internal_Failure",
            message: "Compiler dependency panicked".into(),
            entries: Vec::new(),
        })
    };
    serde_json::to_vec(&answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SHADER: &str = r#"
@group(0) @binding(0) var<storage,read_write> output:array<u32>;
override WIDTH:u32=8;
@compute @workgroup_size(WIDTH) fn main(@builtin(global_invocation_id) id:vec3u) {output[id.x]=id.x*17u;}
@fragment fn fragment_main()->@location(3) vec4f {return vec4f(0.2,0.3,0.4,1);}
"#;
    fn request(source: &str, name: &str, stage: Stage) -> Request {
        Request {
            abi: ABI,
            source: source.into(),
            selections: vec![Selection {
                name: name.into(),
                stage,
            }],
            constants: BTreeMap::new(),
        }
    }
    #[test]
    fn test_arbitrary_compute_overrides_exact_runtime_abi() {
        let mut req = request(SHADER, "main", Stage::Compute);
        req.constants.insert("WIDTH".into(), 16.0);
        let entries = compile(req).unwrap_or_else(|Failure(_, message)| panic!("{message}"));
        assert_eq!(entries[0].workgroup_size, [16, 1, 1]);
        assert_eq!(entries[0].bindings.len(), 1);
        let binding = &entries[0].bindings[0];
        assert!(binding.runtime_array);
        assert_eq!(binding.runtime_array_stride, 4);
        assert_eq!(binding.minimum_size, 4);
        assert_eq!(binding.metal_index, 0);
        assert_eq!(binding.size_index, 0);
        assert!(entries[0].metal_source.contains("_mslBufferSizes"));
        assert_eq!(entries[0].sizes_buffer, 8);
        assert_eq!(entries[0].spirv[0], 0x07230203);
    }
    #[test]
    fn test_selected_fragment_drops_unrelated_compute_bindings() {
        let entries = compile(request(SHADER, "fragment_main", Stage::Fragment))
            .unwrap_or_else(|Failure(_, message)| panic!("{message}"));
        assert!(entries[0].bindings.is_empty());
        assert_eq!(entries[0].sizes_buffer, -1);
        assert_eq!(entries[0].outputs[0].location, 3);
    }
    #[test]
    fn test_parser_validator_entries_and_constants_fail_explicitly() {
        assert!(matches!(
            compile(request("invalid", "main", Stage::Compute)),
            Err(Failure("Parse", _))
        ));
        assert!(matches!(
            compile(request(SHADER, "absent", Stage::Compute)),
            Err(Failure("Missing_Entry", _))
        ));
        let mut req = request(SHADER, "main", Stage::Compute);
        req.constants.insert("NOT_PRESENT".into(), 1.0);
        assert!(matches!(compile(req), Err(Failure("Constants", _))));
        let mut req = request(SHADER, "main", Stage::Compute);
        req.constants.insert("WIDTH".into(), 0.0);
        assert!(compile(req).is_err());
    }
    #[test]
    fn test_depth_comparison_and_storage_images_are_typed() {
        let source = r#"
@group(2) @binding(0) var depth:texture_depth_2d;
@group(2) @binding(1) var sampler_depth:sampler_comparison;
@group(3) @binding(0) var output:texture_storage_2d<rgba8unorm,write>;
@compute @workgroup_size(1) fn main(){let d=textureSampleCompareLevel(depth,sampler_depth,vec2f(0.5),0.3);textureStore(output,vec2i(0),vec4f(d));}
"#;
        let entries = compile(request(source, "main", Stage::Compute))
            .unwrap_or_else(|Failure(_, message)| panic!("{message}"));
        assert_eq!(entries[0].bindings.len(), 3);
        assert!(entries[0].bindings[0].depth);
        assert!(entries[0].bindings[1].comparison);
        assert_eq!(entries[0].bindings[2].storage_format, "Rgba8Unorm");
        assert_eq!(entries[0].bindings[2].access, "Write");
    }
    #[test]
    fn test_binding_arrays_keep_logical_texture_and_native_argument_buffer() {
        let source = r#"
@group(1) @binding(0) var images:binding_array<texture_2d<f32>,4>;
@group(1) @binding(1) var image_sampler:sampler;
@fragment fn main(@builtin(position) p:vec4f)->@location(0) vec4f {return textureSample(images[u32(p.x)%4u],image_sampler,p.xy);}
"#;
        for count in [1, 4, 4096] {
            let source = source.replace(",4>", &format!(",{count}>"));
            let entries = compile(request(&source, "main", Stage::Fragment))
                .unwrap_or_else(|Failure(_, message)| panic!("{message}"));
            let image = &entries[0].bindings[0];
            assert_eq!(image.kind, "Texture");
            assert_eq!(image.metal_kind, "Buffer");
            assert_eq!(image.array_count, count);
            assert_eq!(image.metal_minimum_size, u64::from(count) * 8);
            assert_eq!(image.metal_index, 9);
            assert!(
                entries[0]
                    .metal_source
                    .contains("NagaArgumentBufferWrapper")
            );
        }
    }
    #[test]
    fn test_offline_protocol_bounds_and_unknown_fields() {
        let invalid = compile_json(&[]).expect("bounded error reply");
        let answer: serde_json::Value = serde_json::from_slice(&invalid).expect("valid error JSON");
        assert_eq!(answer["error"], "Invalid_Request");
        let request=br#"{"abi":1,"source":"invalid","selections":[{"name":"main","stage":"Compute"}],"extra":true}"#;
        let result = compile_json(request).expect("owned reply");
        let answer: serde_json::Value = serde_json::from_slice(&result).expect("valid error JSON");
        assert_eq!(answer["error"], "Invalid_Request");
    }
}
