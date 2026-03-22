use ash::{Device, util::read_spv, vk};
use naga::{
    back::spv::{self, WriterFlags},
    front::wgsl,
};
use std::{
    collections::HashSet,
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

fn find_common_dir(from: &Path) -> Option<PathBuf> {
    let mut dir = from;
    loop {
        let candidate = dir.join("common");
        if candidate.is_dir() {
            return Some(candidate);
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return None,
        }
    }
}

fn resolve_includes(
    source: &str,
    file_path: &Path,
    included: &mut HashSet<PathBuf>,
) -> Result<String, ShaderError> {
    let canonical = file_path.canonicalize().map_err(ShaderError::IoError)?;

    if !included.insert(canonical.clone()) {
        return Ok(String::new());
    }

    let mut result = String::new();
    let base_dir = file_path.parent().unwrap_or(Path::new("."));

    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(path_str) = trimmed
            .strip_prefix("//include ")
            .or_else(|| trimmed.strip_prefix("#include "))
        {
            let path_str = path_str.trim();

            let include_path = if let Some(quoted) =
                path_str.strip_prefix('"').and_then(|s| s.strip_suffix('"'))
            {
                base_dir.join(quoted)
            } else if let Some(bracketed) =
                path_str.strip_prefix('<').and_then(|s| s.strip_suffix('>'))
            {
                let common_dir = find_common_dir(base_dir).ok_or_else(|| {
                    ShaderError::IoError(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!(
                            "Could not find common/ directory searching up from {:?}",
                            base_dir
                        ),
                    ))
                })?;
                common_dir.join(bracketed)
            } else {
                continue;
            };

            let include_source =
                std::fs::read_to_string(&include_path).map_err(ShaderError::IoError)?;
            let expanded = resolve_includes(&include_source, &include_path, included)?;
            result.push_str(&expanded);
            result.push('\n');
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    Ok(result)
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

    pub fn from_wgsl(
        device: Device,
        path: impl AsRef<Path>,
        stage: vk::ShaderStageFlags,
        entry_point: impl Into<String>,
    ) -> Result<Self, ShaderError> {
        let path = path.as_ref();
        let raw = std::fs::read_to_string(path).map_err(ShaderError::IoError)?;
        let resolved = resolve_includes(&raw, path, &mut HashSet::new())?;

        // Dump resolved shader for debugging
        if std::env::var("KATLA_DUMP_SHADERS").is_ok() {
            let dump_path = path.with_extension("resolved.wgsl");
            std::fs::write(&dump_path, &resolved).ok();
        }

        Self::from_wgsl_string(device, &resolved, stage, entry_point)
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
    shaders: std::collections::HashMap<(PathBuf, vk::ShaderStageFlags), vk::ShaderModule>,
}

impl ShaderCache {
    pub fn new(device: Device) -> Self {
        Self {
            device,
            shaders: std::collections::HashMap::new(),
        }
    }

    /// Get the entry point name for a given shader stage from a WGSL file.
    fn get_entry_point(stage: vk::ShaderStageFlags) -> &'static str {
        match stage {
            vk::ShaderStageFlags::VERTEX => "vs_main",
            vk::ShaderStageFlags::FRAGMENT => "fs_main",
            vk::ShaderStageFlags::COMPUTE => "cs_main",
            _ => "main",
        }
    }

    pub fn load_shader(
        &mut self,
        path: impl AsRef<Path>,
        stage: vk::ShaderStageFlags,
    ) -> Result<vk::ShaderModule, ShaderError> {
        let path = path.as_ref();
        let cache_key = (path.to_path_buf(), stage);

        log::debug!(
            "load_shader: checking cache for {:?} stage={:?}",
            path,
            stage
        );
        if let Some(&module) = self.shaders.get(&cache_key) {
            return Ok(module);
        }

        log::debug!("load_shader: loading from disk {:?}", path);
        if let Some(extension) = path.extension()
            && extension == "wgsl"
        {
            let entry_point = Self::get_entry_point(stage);
            let shader = ShaderModule::from_wgsl(self.device.clone(), path, stage, entry_point)?;
            log::debug!("load_shader: WGSL compiled successfully");
            let module = shader.module;

            // Prevent drop from destroying the module
            std::mem::forget(shader);
            self.shaders.insert(cache_key, module);
            return Ok(module);
        }

        let entry_point = Self::get_entry_point(stage);
        let shader = ShaderModule::from_file(self.device.clone(), path, stage, entry_point)?;
        let module = shader.module;

        // Prevent drop from destroying the module
        std::mem::forget(shader);

        self.shaders.insert(cache_key, module);
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
    WgslValidationError(String),
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
            Self::WgslValidationError(s) => write!(f, "WGSL validation error: {}", s),
        }
    }
}

impl std::error::Error for ShaderError {}
