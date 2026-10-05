use std::collections::HashMap;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{MTLCompileOptions, MTLDevice, MTLFunction, MTLLanguageVersion, MTLLibrary};

use naga::back::msl;
use naga::front::wgsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

use crate::error::RendererError;

pub(crate) use super::binding_schema::ShaderProfile;

pub(crate) struct MetalShaderModule {
    pub(crate) entry_points: HashMap<String, Retained<ProtocolObject<dyn MTLFunction>>>,
}

pub(crate) struct CompiledMetalShader {
    pub(crate) module: MetalShaderModule,
}

pub(crate) fn compile_wgsl_to_metal(
    device: &ProtocolObject<dyn MTLDevice>,
    wgsl_source: &str,
    entry_points: &[&str],
    profile: ShaderProfile,
) -> Result<CompiledMetalShader, RendererError> {
    let module = wgsl::parse_str(wgsl_source)
        .map_err(|e| RendererError::InvalidOperation(format!("WGSL parse error: {:?}", e)))?;

    let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
    let info = validator
        .validate(&module)
        .map_err(|e| RendererError::InvalidOperation(format!("Shader validation: {:?}", e)))?;

    let msl_options = super::binding_schema::options_for_module(profile, &module);
    let table_layouts = super::binding_schema::reflect_table_layout(&module, &info, &msl_options)?;
    let pipeline_options = msl::PipelineOptions::default();
    let (msl_source, _translation_info) =
        msl::write_string(&module, &info, &msl_options, &pipeline_options)
            .map_err(|e| RendererError::InvalidOperation(format!("MSL generation: {:?}", e)))?;

    log::debug!(
        "Generated MSL for {} ({} bytes, profile={:?})",
        entry_points.join(","),
        msl_source.len(),
        profile
    );

    // Dump MSL to /tmp when debug logging is enabled
    if log::log_enabled!(log::Level::Debug) {
        let debug_name = entry_points.first().unwrap_or(&"");
        let _ = std::fs::write(
            format!("/tmp/katla_msl_{debug_name}_{}.metal", msl_source.len()),
            &msl_source,
        );
    }

    let shader_key = super::pipeline_archive::hash_bytes(
        format!(
            "wgsl={};msl={};language=3.0;profile={profile:?};constants=[];options=default;abi={}",
            super::pipeline_archive::hash_bytes(wgsl_source.as_bytes()),
            super::pipeline_archive::hash_bytes(msl_source.as_bytes()),
            super::binding_schema::BINDING_ABI_VERSION
        )
        .as_bytes(),
    );
    let library_key = format!("{}:{shader_key}", device.registryID());
    let mut library_cache = library_cache()
        .lock()
        .map_err(|_| RendererError::InvalidOperation("Library cache poisoned".into()))?;
    let compiled =
        if let Some((_, library)) = library_cache.iter().find(|(key, _)| *key == library_key) {
            library.clone()
        } else {
            let source = NSString::from_str(&msl_source);
            let compile_options = MTLCompileOptions::new();
            compile_options.setLanguageVersion(MTLLanguageVersion::Version3_0);
            let descriptor = objc2_metal::MTL4LibraryDescriptor::new();
            descriptor.setSource(Some(&source));
            descriptor.setName(Some(&NSString::from_str(&shader_key)));
            descriptor.setOptions(Some(&compile_options));
            let compiler = super::pipeline_archive::new_compiler(device, None)?;
            let started = std::time::Instant::now();
            let completed = super::pipeline_archive::compile_on_worker(
                compiler.clone(),
                super::pipeline_archive::CompilerInput::Library(descriptor.clone()),
            )?;
            let library: Retained<ProtocolObject<dyn MTLLibrary>> =
                unsafe { Retained::cast_unchecked(completed.0) };
            log::debug!(
                "pipeline_cache event=library_compile key={shader_key} elapsed_us={}",
                started.elapsed().as_micros()
            );
            let compiled = std::sync::Arc::new(ImmutableLibrary {
                library,
                compiler,
                descriptor,
            });
            library_cache.push_back((library_key, compiled.clone()));
            while library_cache.len() > 32 {
                library_cache.pop_front();
            }
            compiled
        };
    drop(library_cache);
    let library = &compiled.library;

    let mut functions = HashMap::new();
    for name in entry_points {
        let ns_name = NSString::from_str(name);
        let function = library.newFunctionWithName(&ns_name).ok_or_else(|| {
            RendererError::InvalidOperation(format!(
                "Entry point '{}' not found in compiled library",
                name
            ))
        })?;
        let layout = table_layouts
            .iter()
            .find(|layout| layout.entry_point == *name)
            .cloned()
            .ok_or_else(|| {
                RendererError::InvalidOperation(format!("No reflection for entry '{name}'"))
            })?;
        register_function(&function, compiled.clone(), &shader_key, layout)?;
        functions.insert(name.to_string(), function);
    }

    Ok(CompiledMetalShader {
        module: MetalShaderModule {
            entry_points: functions,
        },
    })
}

struct ImmutableLibrary {
    library: Retained<ProtocolObject<dyn MTLLibrary>>,
    compiler: Retained<ProtocolObject<dyn objc2_metal::MTL4Compiler>>,
    descriptor: Retained<objc2_metal::MTL4LibraryDescriptor>,
}
// SAFETY: A completed Metal library is immutable and thread-safe.
unsafe impl Send for ImmutableLibrary {}
// SAFETY: Metal documents libraries and compiler contexts as thread-safe.
unsafe impl Sync for ImmutableLibrary {}
type LibraryCache =
    std::sync::Mutex<std::collections::VecDeque<(String, std::sync::Arc<ImmutableLibrary>)>>;
fn library_cache() -> &'static LibraryCache {
    static CACHE: std::sync::OnceLock<LibraryCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::VecDeque::new()))
}

struct RegisteredLibrary {
    compiled: std::sync::Arc<ImmutableLibrary>,
    function: objc2::rc::Weak<ProtocolObject<dyn MTLFunction>>,
    key: String,
    layout: super::binding_schema::ArgumentTableLayout,
}
// SAFETY: Metal libraries are immutable after compilation and thread-safe.
unsafe impl Send for RegisteredLibrary {}

fn function_registry() -> &'static std::sync::Mutex<HashMap<usize, RegisteredLibrary>> {
    static REGISTRY: std::sync::OnceLock<std::sync::Mutex<HashMap<usize, RegisteredLibrary>>> =
        std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn register_function(
    function: &ProtocolObject<dyn MTLFunction>,
    compiled: std::sync::Arc<ImmutableLibrary>,
    key: &str,
    layout: super::binding_schema::ArgumentTableLayout,
) -> Result<(), RendererError> {
    let mut registry = function_registry()
        .lock()
        .map_err(|_| RendererError::InvalidOperation("Shader registry poisoned".into()))?;
    registry.retain(|_, entry| entry.function.load().is_some());
    registry.insert(
        function as *const _ as usize,
        RegisteredLibrary {
            compiled,
            function: objc2::rc::Weak::new(function),
            key: key.into(),
            layout,
        },
    );
    Ok(())
}

pub(crate) fn function_layout(
    function: &ProtocolObject<dyn MTLFunction>,
) -> Result<super::binding_schema::ArgumentTableLayout, RendererError> {
    let registry = function_registry()
        .lock()
        .map_err(|_| RendererError::InvalidOperation("Shader registry poisoned".into()))?;
    registry
        .get(&(function as *const _ as usize))
        .map(|entry| entry.layout.clone())
        .ok_or_else(|| {
            RendererError::InvalidOperation("Missing function binding reflection".into())
        })
}

pub(crate) fn function_descriptor(
    function: &ProtocolObject<dyn MTLFunction>,
) -> Result<(Retained<objc2_metal::MTL4LibraryFunctionDescriptor>, String), RendererError> {
    let registry = function_registry()
        .lock()
        .map_err(|_| RendererError::InvalidOperation("Shader registry poisoned".into()))?;
    let entry = registry
        .get(&(function as *const _ as usize))
        .ok_or_else(|| {
            RendererError::InvalidOperation(
                "Function was not created by the Metal compiler service".into(),
            )
        })?;
    let descriptor = objc2_metal::MTL4LibraryFunctionDescriptor::new();
    let _compiler = (&entry.compiled.compiler, &entry.compiled.descriptor);
    descriptor.setLibrary(Some(&entry.compiled.library));
    descriptor.setName(Some(&function.name()));
    let key = format!("{};entry={}", entry.key, function.name());
    Ok((descriptor, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_metal::MTLCreateSystemDefaultDevice;

    fn headless_device() -> Retained<ProtocolObject<dyn MTLDevice>> {
        MTLCreateSystemDefaultDevice().expect("No Metal device available")
    }

    #[test]
    fn test_shader_compilation_vertex_fragment() {
        let device = headless_device();
        let wgsl = r#"
@vertex fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
    return vec4f(0.0, 0.0, 0.0, 1.0);
}
@fragment fn fs_main() -> @location(0) vec4f {
    return vec4f(1.0, 0.0, 0.0, 1.0);
}
"#;
        let result = compile_wgsl_to_metal(
            &device,
            wgsl,
            &["vs_main", "fs_main"],
            ShaderProfile::Graphics,
        );
        assert!(
            result.is_ok(),
            "Shader compilation failed: {:?}",
            result.err()
        );
        let shader = result.unwrap();
        assert!(shader.module.entry_points.contains_key("vs_main"));
        assert!(shader.module.entry_points.contains_key("fs_main"));
    }

    #[test]
    fn test_shader_compilation_compute() {
        let device = headless_device();
        let wgsl = r#"
@group(0) @binding(0) var<storage, read_write> output: array<f32>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3u) {
    output[gid.x] = f32(gid.x);
}
"#;
        let result = compile_wgsl_to_metal(&device, wgsl, &["cs_main"], ShaderProfile::Graphics);
        assert!(
            result.is_ok(),
            "Compute shader compilation failed: {:?}",
            result.err()
        );
        let shader = result.unwrap();
        assert!(shader.module.entry_points.contains_key("cs_main"));
    }

    #[test]
    fn test_shader_compilation_invalid_wgsl() {
        let device = headless_device();
        let wgsl = "this is not valid WGSL";
        let result = compile_wgsl_to_metal(&device, wgsl, &["main"], ShaderProfile::Graphics);
        assert!(result.is_err());
    }

    #[test]
    fn test_shader_compilation_missing_entry_point() {
        let device = headless_device();
        let wgsl = r#"
@vertex fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4f {
    return vec4f(0.0, 0.0, 0.0, 1.0);
}
"#;
        let result = compile_wgsl_to_metal(
            &device,
            wgsl,
            &["nonexistent_entry"],
            ShaderProfile::Graphics,
        );
        assert!(result.is_err());
    }
}
