use ash::{Device, util::read_spv, vk};
use naga::{
    back::spv::{self, WriterFlags},
    front::wgsl,
};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

pub struct ShaderModule {
    pub(crate) module: vk::ShaderModule,
    device: Device,
}

fn shader_stage_to_naga(stage: vk::ShaderStageFlags) -> naga::ShaderStage {
    match stage {
        vk::ShaderStageFlags::VERTEX => naga::ShaderStage::Vertex,
        vk::ShaderStageFlags::FRAGMENT => naga::ShaderStage::Fragment,
        vk::ShaderStageFlags::COMPUTE => naga::ShaderStage::Compute,
        _ => panic!("Unsupported shader stage"),
    }
}

impl ShaderModule {
    pub fn from_bytes(
        device: Device,
        bytes: &[u8],
        _stage: vk::ShaderStageFlags,
        _entry_point: &str,
    ) -> Result<Self, ShaderError> {
        let mut cursor = Cursor::new(bytes);
        let code = read_spv(&mut cursor).map_err(ShaderError::InvalidSpirv)?;

        let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
        let module = unsafe { device.create_shader_module(&create_info, None) }
            .map_err(ShaderError::CreationFailed)?;

        Ok(Self { module, device })
    }

    pub fn from_wgsl_string(
        device: Device,
        wgsl_str: &str,
        stage: vk::ShaderStageFlags,
        entry_point: impl Into<String>,
    ) -> Result<Self, ShaderError> {
        Self::from_wgsl_string_impl(device, wgsl_str, stage, entry_point)
    }

    fn from_wgsl_string_impl(
        device: Device,
        wgsl_str: &str,
        stage: vk::ShaderStageFlags,
        entry_point: impl Into<String>,
    ) -> Result<Self, ShaderError> {
        let entry_point = entry_point.into();
        let wgsl_module = wgsl::parse_str(wgsl_str).map_err(ShaderError::WgslParseError)?;

        let module_info: naga::valid::ModuleInfo = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .subgroup_stages(naga::valid::ShaderStages::all())
        .subgroup_operations(naga::valid::SubgroupOperationSet::all())
        .validate(&wgsl_module)
        .map_err(|e| ShaderError::WgslValidationError(format!("{:?}", e)))?;
        let naga_stage = shader_stage_to_naga(stage);

        let options = spv::Options {
            flags: WriterFlags::LABEL_VARYINGS | WriterFlags::CLAMP_FRAG_DEPTH,
            ..Default::default()
        };
        let spirv = spv::write_vec(
            &wgsl_module,
            &module_info,
            &options,
            Some(&spv::PipelineOptions {
                shader_stage: naga_stage,
                entry_point: entry_point.clone(),
            }),
        )
        .map_err(ShaderError::SpvWriteError)?;
        let bytes = bytemuck::cast_slice(&spirv);
        Self::from_bytes(device, bytes, stage, &entry_point)
    }

    #[cfg(feature = "validation")]
    pub fn from_file(
        device: Device,
        path: impl AsRef<Path>,
        stage: vk::ShaderStageFlags,
        entry_point: &str,
    ) -> Result<Self, ShaderError> {
        let bytes = std::fs::read(path.as_ref()).map_err(ShaderError::IoError)?;
        Self::from_bytes(device, &bytes, stage, entry_point)
    }
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_shader_module(self.module, None);
        }
    }
}

pub struct ShaderCache {
    device: Device,
    shaders: std::collections::HashMap<
        (PathBuf, vk::ShaderStageFlags, String),
        (vk::ShaderModule, Option<String>),
    >,
}

impl ShaderCache {
    pub fn new(device: Device) -> Self {
        Self {
            device,
            shaders: std::collections::HashMap::new(),
        }
    }

    /// Load and cache a shader module for a specific entry point.
    #[cfg(feature = "validation")]
    pub fn load_shader_with_entry(
        &mut self,
        path: impl AsRef<Path>,
        stage: vk::ShaderStageFlags,
        entry_point: &str,
    ) -> Result<vk::ShaderModule, ShaderError> {
        let path = path.as_ref();
        if path.extension().is_some_and(|ext| ext == "wgsl") {
            let source = crate::renderer::shader_source::ShaderSource::load(path)
                .map_err(ShaderError::IoError)?;
            return self.load_shader_from_source(path, &source.code, stage, entry_point);
        }
        let cache_key = (path.to_path_buf(), stage, entry_point.to_owned());
        if let Some((module, _)) = self.shaders.get(&cache_key) {
            return Ok(*module);
        }
        let shader = ShaderModule::from_file(self.device.clone(), path, stage, entry_point)?;
        let module = shader.module;
        std::mem::forget(shader);
        self.shaders.insert(cache_key, (module, None));
        Ok(module)
    }

    pub(crate) fn load_shader_from_source(
        &mut self,
        path: &Path,
        source: &str,
        stage: vk::ShaderStageFlags,
        entry_point: &str,
    ) -> Result<vk::ShaderModule, ShaderError> {
        let key = (path.to_path_buf(), stage, entry_point.to_owned());
        if let Some((module, cached)) = self.shaders.get(&key)
            && cached.as_deref() == Some(source)
        {
            return Ok(*module);
        }
        let shader =
            ShaderModule::from_wgsl_string(self.device.clone(), source, stage, entry_point)?;
        let module = shader.module;
        std::mem::forget(shader);
        if let Some((previous, _)) = self.shaders.insert(key, (module, Some(source.to_owned()))) {
            unsafe {
                self.device.destroy_shader_module(previous, None);
            }
        }
        Ok(module)
    }

    #[cfg(feature = "validation")]
    pub fn invalidate(&mut self, path: &Path) {
        self.shaders.retain(|(shader_path, _, _), (module, _)| {
            if shader_path == path {
                unsafe {
                    self.device.destroy_shader_module(*module, None);
                }
                false
            } else {
                true
            }
        });
    }

    pub fn clear(&mut self) {
        for (_, (module, _)) in self.shaders.drain() {
            unsafe {
                self.device.destroy_shader_module(module, None);
            }
        }
    }
}

impl Drop for ShaderCache {
    fn drop(&mut self) {
        self.clear();
    }
}

#[derive(Debug)]
pub enum ShaderError {
    #[cfg(feature = "validation")]
    IoError(std::io::Error),
    InvalidSpirv(std::io::Error),
    CreationFailed(vk::Result),
    WgslParseError(wgsl::ParseError),
    WgslValidationError(String),
    SpvWriteError(spv::Error),
}

impl std::fmt::Display for ShaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            #[cfg(feature = "validation")]
            Self::IoError(e) => write!(f, "IO error loading shader: {}", e),
            Self::InvalidSpirv(e) => write!(f, "Invalid SPIR-V: {}", e),
            Self::CreationFailed(e) => write!(f, "Failed to create shader module: {:?}", e),
            Self::WgslParseError(e) => write!(f, "WGSL parse error: {}", e),
            Self::SpvWriteError(e) => write!(f, "SPIR-V write error: {}", e),
            Self::WgslValidationError(s) => write!(f, "WGSL validation error: {}", s),
        }
    }
}

impl std::error::Error for ShaderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_and_skinned_pbr_shader_interfaces_validate() {
        for shader in ["model_pbr.wgsl", "model_pbr_skinned.wgsl"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../resources/shaders")
                .join(shader);
            let source = crate::renderer::shader_source::ShaderSource::load(&path)
                .unwrap()
                .code;
            let interface = crate::renderer::graphics_interface::GraphicsInterface::reflect(
                &source,
                &crate::renderer::pipeline_descriptor::PipelineStages::Graphics {
                    vertex_entry: "vs_main".into(),
                    fragment_entry: Some("fs_main".into()),
                },
            )
            .unwrap_or_else(|error| panic!("{shader}: {error}"));
            assert_eq!(interface.color_outputs, vec![0]);
            assert!(interface.bindings.iter().any(|slot| slot.group == 3));
        }
    }
}
