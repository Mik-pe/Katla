use std::collections::HashMap;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{MTLCompileOptions, MTLDevice, MTLFunction, MTLLanguageVersion, MTLLibrary};

use naga::back::msl;
use naga::front::wgsl;
use naga::valid::{Capabilities, ValidationFlags, Validator};

use crate::error::RendererError;

/// Shader compilation profile selecting the appropriate binding map.
#[derive(Debug, Clone)]
pub(crate) enum ShaderProfile {
    /// Standard graphics pipeline (bindless textures at buffer 9).
    Graphics,
    /// UI shaders (different binding layout).
    Ui,
    /// Outline draw shaders (outline_params instead of bindless textures).
    Outline,
    /// Skinned outline draw shaders (joints + outline_params, no bindless).
    OutlineSkinned,
}

/// Create naga MSL options configured for Katla's binding layout.
///
/// Katla's descriptor layout:
/// - Set 0, Binding 0: FrameUniforms (storage buffer) → [[buffer(0)]]
/// - Set 0, Binding 1: ObjectUniforms array (storage buffer) → [[buffer(1)]]
/// - Set 1, Binding 0: Bindless texture array → [[texture(N)]]
/// - Set 1, Binding 1: Shared sampler → [[sampler(0)]]
/// - Set 2, Binding 0: Joint matrices (storage buffer) → [[buffer(2)]]
pub(crate) fn katla_msl_options() -> msl::Options {
    let mut options = msl::Options {
        lang_version: (2, 0),
        fake_missing_bindings: true,
        ..msl::Options::default()
    };

    let graphics_bindings = create_graphics_binding_map();
    options
        .per_entry_point_map
        .insert("vs_main".to_string(), graphics_bindings.clone());
    options
        .per_entry_point_map
        .insert("fs_main".to_string(), graphics_bindings);

    options
}

/// MSL options for UI shaders.
///
/// UI shader binding layout:
/// - Set 0, Binding 1: font_sampler → [[sampler(0)]]
/// - Set 0, Binding 3: UiUniforms (screen_size) → [[buffer(3)]]
/// - Set 1, Binding 0: Bindless texture array → [[buffer(9)]]
/// - Set 1, Binding 1: Shared sampler → [[sampler(1)]]
pub(crate) fn katla_msl_options_ui() -> msl::Options {
    let mut options = msl::Options {
        lang_version: (2, 0),
        fake_missing_bindings: true,
        ..msl::Options::default()
    };

    let ui_bindings = create_ui_binding_map();
    options
        .per_entry_point_map
        .insert("vs_main".to_string(), ui_bindings.clone());
    options
        .per_entry_point_map
        .insert("fs_main".to_string(), ui_bindings.clone());
    options
        .per_entry_point_map
        .insert("vs_instanced".to_string(), ui_bindings.clone());
    options
        .per_entry_point_map
        .insert("fs_instanced".to_string(), ui_bindings);

    options
}

fn create_graphics_binding_map() -> msl::EntryPointResources {
    let mut resources = msl::EntryPointResources::default();

    let bindings: &[(naga::ResourceBinding, msl::BindTarget)] = &[
        // Set 0: Per-frame and per-object storage buffers
        (
            naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(0),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 0,
                binding: 1,
            },
            msl::BindTarget {
                buffer: Some(1),
                ..Default::default()
            },
        ),
        // Set 1: Bindless textures (argument buffer)
        (
            naga::ResourceBinding {
                group: 1,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(9),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 1,
                binding: 1,
            },
            msl::BindTarget {
                sampler: Some(msl::BindSamplerTarget::Resource(0)),
                ..Default::default()
            },
        ),
        // Set 2: Skeletal animation / Shadow cascade params
        (
            naga::ResourceBinding {
                group: 2,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(2),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 2,
                binding: 1,
            },
            msl::BindTarget {
                buffer: Some(3),
                ..Default::default()
            },
        ),
        // Set 3: Forward+ light culling
        (
            naga::ResourceBinding {
                group: 3,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(3),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 3,
                binding: 1,
            },
            msl::BindTarget {
                buffer: Some(4),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 3,
                binding: 2,
            },
            msl::BindTarget {
                buffer: Some(5),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 3,
                binding: 3,
            },
            msl::BindTarget {
                buffer: Some(6),
                ..Default::default()
            },
        ),
        // Set 4: Shadow data
        (
            naga::ResourceBinding {
                group: 4,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(7),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 4,
                binding: 1,
            },
            msl::BindTarget {
                texture: Some(1),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 4,
                binding: 2,
            },
            msl::BindTarget {
                sampler: Some(msl::BindSamplerTarget::Resource(1)),
                ..Default::default()
            },
        ),
    ];

    for (binding, target) in bindings {
        resources.resources.insert(*binding, target.clone());
    }

    // Buffer slot for runtime array size information
    resources.sizes_buffer = Some(8);

    resources
}

fn create_ui_binding_map() -> msl::EntryPointResources {
    let mut resources = msl::EntryPointResources::default();

    let bindings: &[(naga::ResourceBinding, msl::BindTarget)] = &[
        // Set 0, Binding 1: font sampler
        (
            naga::ResourceBinding {
                group: 0,
                binding: 1,
            },
            msl::BindTarget {
                sampler: Some(msl::BindSamplerTarget::Resource(0)),
                ..Default::default()
            },
        ),
        // Set 0, Binding 3: UiUniforms (screen_size)
        (
            naga::ResourceBinding {
                group: 0,
                binding: 3,
            },
            msl::BindTarget {
                buffer: Some(3),
                ..Default::default()
            },
        ),
        // Set 0, Binding 4: Instance data storage buffer
        (
            naga::ResourceBinding {
                group: 0,
                binding: 4,
            },
            msl::BindTarget {
                buffer: Some(11),
                ..Default::default()
            },
        ),
        // Set 1, Binding 0: Bindless texture array (argument buffer)
        (
            naga::ResourceBinding {
                group: 1,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(9),
                ..Default::default()
            },
        ),
        // Set 1, Binding 1: Shared sampler
        (
            naga::ResourceBinding {
                group: 1,
                binding: 1,
            },
            msl::BindTarget {
                sampler: Some(msl::BindSamplerTarget::Resource(1)),
                ..Default::default()
            },
        ),
    ];

    for (binding, target) in bindings {
        resources.resources.insert(*binding, target.clone());
    }

    resources.sizes_buffer = Some(8);

    resources
}

/// Outline draw binding map (non-skinned).
///
/// group(0), binding(0) → buffer 0  (frame uniforms)
/// group(0), binding(1) → buffer 1  (object storage)
/// group(1), binding(0) → buffer 2  (outline_params)
fn create_outline_binding_map() -> msl::EntryPointResources {
    let mut resources = msl::EntryPointResources::default();

    let bindings: &[(naga::ResourceBinding, msl::BindTarget)] = &[
        (
            naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(0),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 0,
                binding: 1,
            },
            msl::BindTarget {
                buffer: Some(1),
                ..Default::default()
            },
        ),
        // outline_params at group(1), binding(0) → buffer 2
        (
            naga::ResourceBinding {
                group: 1,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(2),
                ..Default::default()
            },
        ),
    ];

    for (binding, target) in bindings {
        resources.resources.insert(*binding, target.clone());
    }

    resources
}

/// Outline draw binding map (skinned).
///
/// group(0), binding(0) → buffer 0  (frame uniforms)
/// group(0), binding(1) → buffer 1  (object storage)
/// group(2), binding(0) → buffer 2  (joint matrices)
/// group(3), binding(0) → buffer 3  (outline_params)
fn create_outline_skinned_binding_map() -> msl::EntryPointResources {
    let mut resources = msl::EntryPointResources::default();

    let bindings: &[(naga::ResourceBinding, msl::BindTarget)] = &[
        (
            naga::ResourceBinding {
                group: 0,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(0),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 0,
                binding: 1,
            },
            msl::BindTarget {
                buffer: Some(1),
                ..Default::default()
            },
        ),
        (
            naga::ResourceBinding {
                group: 2,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(2),
                ..Default::default()
            },
        ),
        // outline_params at group(3), binding(0) → buffer 3
        (
            naga::ResourceBinding {
                group: 3,
                binding: 0,
            },
            msl::BindTarget {
                buffer: Some(3),
                ..Default::default()
            },
        ),
    ];

    for (binding, target) in bindings {
        resources.resources.insert(*binding, target.clone());
    }

    resources
}

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

    let msl_options = match profile {
        ShaderProfile::Graphics => katla_msl_options(),
        ShaderProfile::Ui => katla_msl_options_ui(),
        ShaderProfile::Outline => {
            let mut options = msl::Options {
                lang_version: (2, 0),
                fake_missing_bindings: true,
                ..msl::Options::default()
            };
            let bindings = create_outline_binding_map();
            options
                .per_entry_point_map
                .insert("vs_main".to_string(), bindings.clone());
            options
                .per_entry_point_map
                .insert("fs_main".to_string(), bindings);
            options
        }
        ShaderProfile::OutlineSkinned => {
            let mut options = msl::Options {
                lang_version: (2, 0),
                fake_missing_bindings: true,
                ..msl::Options::default()
            };
            let bindings = create_outline_skinned_binding_map();
            options
                .per_entry_point_map
                .insert("vs_main".to_string(), bindings.clone());
            options
                .per_entry_point_map
                .insert("fs_main".to_string(), bindings);
            options
        }
    };
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

    let source = NSString::from_str(&msl_source);
    let compile_options = MTLCompileOptions::new();
    compile_options.setLanguageVersion(MTLLanguageVersion::Version3_0);

    let library = device
        .newLibraryWithSource_options_error(&source, Some(&compile_options))
        .map_err(|err| {
            let msg = err.localizedDescription().to_string();
            RendererError::InvalidOperation(format!("Metal shader compile error: {}", msg))
        })?;

    let mut functions = HashMap::new();
    for name in entry_points {
        let ns_name = NSString::from_str(name);
        let function = library.newFunctionWithName(&ns_name).ok_or_else(|| {
            RendererError::InvalidOperation(format!(
                "Entry point '{}' not found in compiled library",
                name
            ))
        })?;
        functions.insert(name.to_string(), function);
    }

    Ok(CompiledMetalShader {
        module: MetalShaderModule {
            entry_points: functions,
        },
    })
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
