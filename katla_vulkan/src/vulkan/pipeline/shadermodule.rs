use ash::{util::read_spv, vk, Device};
use std::{
    ffi::CString,
    io::Cursor,
    path::{Path, PathBuf},
};

pub struct ShaderModule {
    pub module: vk::ShaderModule,
    pub stage: vk::ShaderStageFlags,
    device: Device,
}

impl ShaderModule {
    pub fn from_bytes(
        device: Device,
        bytes: &[u8],
        stage: vk::ShaderStageFlags,
    ) -> Result<Self, ShaderError> {
        let mut cursor = Cursor::new(bytes);
        let code = read_spv(&mut cursor).map_err(|e| ShaderError::InvalidSpirv(e))?;

        let create_info = vk::ShaderModuleCreateInfo::default().code(&code);
        let module = unsafe { device.create_shader_module(&create_info, None) }
            .map_err(|e| ShaderError::CreationFailed(e))?;

        Ok(Self {
            module,
            stage,
            device,
        })
    }

    pub fn from_file(
        device: Device,
        path: impl AsRef<Path>,
        stage: vk::ShaderStageFlags,
    ) -> Result<Self, ShaderError> {
        let bytes = std::fs::read(path.as_ref()).map_err(|e| ShaderError::IoError(e))?;
        Self::from_bytes(device, &bytes, stage)
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

        let shader = ShaderModule::from_file(self.device.clone(), path, stage)?;
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
}

impl std::fmt::Display for ShaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error loading shader: {}", e),
            Self::InvalidSpirv(e) => write!(f, "Invalid SPIR-V: {}", e),
            Self::CreationFailed(e) => write!(f, "Failed to create shader module: {:?}", e),
        }
    }
}

impl std::error::Error for ShaderError {}
