use ash::{Device, util::read_spv, vk};
use naga::{
    back::spv::{self, WriterFlags},
    front::wgsl,
};
use std::{
    ffi::CString,
    io::Cursor,
    path::{Path, PathBuf},
};

use crate::vulkan::pipeline_state::ShaderStages;

pub struct ShaderModule {
    pub(crate) module: vk::ShaderModule,
    pub stage: vk::ShaderStageFlags,
    pub entry_point: CString,
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
        stage: vk::ShaderStageFlags,
        entry_point: &str,
    ) -> Result<Self, ShaderError> {
        let mut cursor = Cursor::new(bytes);
        let code = read_spv(&mut cursor).map_err(ShaderError::InvalidSpirv)?;

        let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
        let module = unsafe { device.create_shader_module(&create_info, None) }
            .map_err(ShaderError::CreationFailed)?;

        Ok(Self {
            module,
            stage,
            entry_point: CString::new(entry_point).unwrap(),
            device,
        })
    }

    pub fn from_wgsl(
        device: Device,
        path: impl AsRef<Path>,
        stage: vk::ShaderStageFlags,
        entry_point: impl Into<String>,
    ) -> Result<Self, ShaderError> {
        let wgsl_str = std::fs::read_to_string(path.as_ref()).map_err(ShaderError::IoError)?;
        Self::from_wgsl_string(device, &wgsl_str, stage, entry_point)
    }

    pub fn from_wgsl_string(
        device: Device,
        wgsl_str: &str,
        stage: vk::ShaderStageFlags,
        entry_point: impl Into<String>,
    ) -> Result<Self, ShaderError> {
        Self::from_wgsl_string_impl(device, wgsl_str, stage, entry_point)
    }

    /// Create a shader module from WGSL source string using wrapper types.
    pub fn from_wgsl_string_wrapped(
        device: Device,
        wgsl_str: &str,
        stage: ShaderStages,
        entry_point: impl Into<String>,
    ) -> Result<Self, ShaderError> {
        let vk_stage: vk::ShaderStageFlags = stage.into();
        Self::from_wgsl_string_impl(device, wgsl_str, vk_stage, entry_point)
    }

    fn from_wgsl_string_impl(
        device: Device,
        wgsl_str: &str,
        stage: vk::ShaderStageFlags,
        entry_point: impl Into<String>,
    ) -> Result<Self, ShaderError> {
        let wgsl_module = wgsl::parse_str(wgsl_str).map_err(ShaderError::WgslParseError)?;

        let module_info: naga::valid::ModuleInfo = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .subgroup_stages(naga::valid::ShaderStages::all())
        .subgroup_operations(naga::valid::SubgroupOperationSet::all())
        .validate(&wgsl_module)
        .unwrap();
        let entry_point = entry_point.into();
        let options = spv::Options {
            flags: WriterFlags::LABEL_VARYINGS | WriterFlags::CLAMP_FRAG_DEPTH,
            ..Default::default()
        };
        let spirv = spv::write_vec(
            &wgsl_module,
            &module_info,
            &options,
            Some(&spv::PipelineOptions {
                shader_stage: shader_stage_to_naga(stage),
                entry_point: entry_point.clone(),
            }),
        )
        .map_err(ShaderError::SpvWriteError)?;
        let bytes = bytemuck::cast_slice(&spirv);
        Self::from_bytes(device, bytes, stage, &entry_point)
    }

    pub fn from_file(
        device: Device,
        path: impl AsRef<Path>,
        stage: vk::ShaderStageFlags,
        entry_point: &str,
    ) -> Result<Self, ShaderError> {
        let bytes = std::fs::read(path.as_ref()).map_err(ShaderError::IoError)?;
        Self::from_bytes(device, &bytes, stage, entry_point)
    }

    pub fn stage_info<'a>(
        &'a self,
        entry_point: &'a CString,
    ) -> vk::PipelineShaderStageCreateInfo<'a> {
        vk::PipelineShaderStageCreateInfo::default()
            .stage(self.stage)
            .module(self.module)
            .name(entry_point)
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
    shaders: std::collections::HashMap<PathBuf, vk::ShaderModule>,
}

impl ShaderCache {
    pub fn new(device: Device) -> Self {
        Self {
            device,
            shaders: std::collections::HashMap::new(),
        }
    }

    pub fn load_shader(
        &mut self,
        path: impl AsRef<Path>,
        stage: vk::ShaderStageFlags,
    ) -> Result<vk::ShaderModule, ShaderError> {
        let path = path.as_ref();

        if let Some(&module) = self.shaders.get(path) {
            return Ok(module);
        }

        if let Some(extension) = path.extension()
            && extension == "wgsl"
        {
            let shader = ShaderModule::from_wgsl(self.device.clone(), path, stage, "main")?;
            let module = shader.module;

            // Prevent drop from destroying the module
            std::mem::forget(shader);
            self.shaders.insert(path.to_path_buf(), module);
            return Ok(module);
        }

        let shader = ShaderModule::from_file(self.device.clone(), path, stage, "main")?;
        let module = shader.module;

        // Prevent drop from destroying the module
        std::mem::forget(shader);

        self.shaders.insert(path.to_path_buf(), module);
        Ok(module)
    }

    pub fn clear(&mut self) {
        for (_, module) in self.shaders.drain() {
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
    IoError(std::io::Error),
    InvalidSpirv(std::io::Error),
    CreationFailed(vk::Result),
    WgslParseError(wgsl::ParseError),
    SpvWriteError(spv::Error),
}

impl std::fmt::Display for ShaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error loading shader: {}", e),
            Self::InvalidSpirv(e) => write!(f, "Invalid SPIR-V: {}", e),
            Self::CreationFailed(e) => write!(f, "Failed to create shader module: {:?}", e),
            Self::WgslParseError(e) => write!(f, "WGSL parse error: {}", e),
            Self::SpvWriteError(e) => write!(f, "SPIR-V write error: {}", e),
        }
    }
}

impl std::error::Error for ShaderError {}
