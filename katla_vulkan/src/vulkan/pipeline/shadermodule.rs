use ash::{
    util::read_spv,
    vk::{self},
    Device,
};
use naga::{
    back::spv::{self, WriterFlags},
    front::wgsl,
};
use std::{
    ffi::CString,
    io::Cursor,
    path::{Path, PathBuf},
};

pub struct ShaderModule {
    pub module: vk::ShaderModule,
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
        let code = read_spv(&mut cursor).map_err(|e| ShaderError::InvalidSpirv(e))?;

        let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
        let module = unsafe { device.create_shader_module(&create_info, None) }
            .map_err(|e| ShaderError::CreationFailed(e))?;

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
        let wgsl_str =
            std::fs::read_to_string(path.as_ref()).map_err(|e| ShaderError::IoError(e))?;
        let wgsl_module = wgsl::parse_str(&wgsl_str).map_err(|e| ShaderError::WgslParseError(e))?;

        let module_info: naga::valid::ModuleInfo = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .subgroup_stages(naga::valid::ShaderStages::all())
        .subgroup_operations(naga::valid::SubgroupOperationSet::all())
        .validate(&wgsl_module)
        .unwrap();
        let entry_point = entry_point.into();
        let mut options = naga::back::spv::Options::default();
        options.flags = WriterFlags::LABEL_VARYINGS | WriterFlags::CLAMP_FRAG_DEPTH;
        let spirv = naga::back::spv::write_vec(
            &wgsl_module,
            &module_info,
            &options,
            Some(&spv::PipelineOptions {
                shader_stage: shader_stage_to_naga(stage),
                entry_point: entry_point.clone(),
            }),
        )
        .map_err(|e| ShaderError::SpvWriteError(e))?;
        let bytes = bytemuck::cast_slice(&spirv);
        Self::from_bytes(device, bytes, stage, &entry_point)
    }

    pub fn from_file(
        device: Device,
        path: impl AsRef<Path>,
        stage: vk::ShaderStageFlags,
        entry_point: &str,
    ) -> Result<Self, ShaderError> {
        let bytes = std::fs::read(path.as_ref()).map_err(|e| ShaderError::IoError(e))?;
        Self::from_bytes(device, &bytes, stage, entry_point.into())
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

        if let Some(extension) = path.extension() {
            if extension == "wgsl" {
                let shader = ShaderModule::from_wgsl(self.device.clone(), path, stage, "main")?;
                let module = shader.module;

                // Prevent drop from destroying the module
                std::mem::forget(shader);
                self.shaders.insert(path.to_path_buf(), module);
                return Ok(module);
            }
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
