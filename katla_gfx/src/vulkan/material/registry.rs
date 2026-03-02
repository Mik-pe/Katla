//! Material registry for central template and instance management.
//!
//! This module provides a centralized registry for material templates,
//! supporting loading from files, bulk loading from directories, and
//! event-driven template hot reload using filesystem watching.

use super::{
    FileWatcher, MaterialDescriptor, MaterialError, MaterialTemplate, ShaderReflection,
    load_material_from_file,
};
use crate::material::{DynamicMaterialConfig, MaterialDefinition, MaterialPipelineCache};
use ash::vk;
use log::{debug, info};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

/// Central registry for material templates
pub struct MaterialRegistry {
    templates: HashMap<String, Rc<MaterialTemplate>>,
    template_paths: HashMap<String, PathBuf>,
    file_watcher: Option<FileWatcher>,
    watch_directory: Option<PathBuf>,
}

impl MaterialRegistry {
    /// Create a new material registry
    pub fn new() -> Self {
        Self {
            templates: HashMap::new(),
            template_paths: HashMap::new(),
            file_watcher: None,
            watch_directory: None,
        }
    }

    /// Register a material template
    pub fn register_template(&mut self, template: MaterialTemplate) {
        let name = template.name().to_string();
        self.templates.insert(name.clone(), Rc::new(template));
    }

    /// Register a template with a specific path (for hot reload tracking)
    pub fn register_template_with_path(&mut self, template: MaterialTemplate, path: &Path) {
        let name = template.name().to_string();
        self.templates.insert(name.clone(), Rc::new(template));
        self.template_paths.insert(name, path.to_path_buf());
    }

    /// Get a template by name
    pub fn get_template(&self, name: &str) -> Option<&Rc<MaterialTemplate>> {
        self.templates.get(name)
    }

    /// Get a mutable reference to a template by name
    pub fn get_template_mut(&mut self, name: &str) -> Option<&mut Rc<MaterialTemplate>> {
        self.templates.get_mut(name)
    }

    /// Check if a template exists
    pub fn has_template(&self, name: &str) -> bool {
        self.templates.contains_key(name)
    }

    /// Get all template names
    pub fn template_names(&self) -> Vec<String> {
        self.templates.keys().cloned().collect()
    }

    /// Get the number of registered templates
    pub fn len(&self) -> usize {
        self.templates.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    /// Load all material templates from a directory using storage buffers.
    pub fn load_directory(
        &mut self,
        dir: &Path,
        cache: &mut MaterialPipelineCache,
    ) -> Result<usize, MaterialError> {
        let dir_entries = fs::read_dir(dir).map_err(|e| {
            MaterialError::InvalidDescriptor(format!(
                "Failed to read directory {}: {}",
                dir.display(),
                e
            ))
        })?;

        let mut loaded = 0;
        for entry in dir_entries {
            let entry = entry.map_err(|e| {
                MaterialError::InvalidDescriptor(format!("Failed to read directory entry: {}", e))
            })?;
            let path = entry.path();

            if path.extension() != Some(OsStr::new("toml")) {
                continue;
            }

            match self.load_material(&path, cache) {
                Ok(_) => loaded += 1,
                Err(e) => {
                    debug!("Failed to load material {}: {}", path.display(), e);
                }
            }
        }

        Ok(loaded)
    }

    /// Load all material templates using bindless textures.
    pub(crate) fn load_directory_bindless(
        &mut self,
        dir: &Path,
        cache: &mut MaterialPipelineCache,
        bindless_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<usize, MaterialError> {
        let dir_entries = fs::read_dir(dir).map_err(|e| {
            MaterialError::InvalidDescriptor(format!(
                "Failed to read directory {}: {}",
                dir.display(),
                e
            ))
        })?;

        let mut loaded = 0;
        for entry in dir_entries {
            let entry = entry.map_err(|e| {
                MaterialError::InvalidDescriptor(format!("Failed to read directory entry: {}", e))
            })?;
            let path = entry.path();

            if path.extension() != Some(OsStr::new("toml")) {
                continue;
            }

            match self.load_material_bindless(&path, cache, bindless_layout) {
                Ok(_) => loaded += 1,
                Err(e) => {
                    debug!("Failed to load bindless material {}: {}", path.display(), e);
                }
            }
        }

        Ok(loaded)
    }

    /// Load a single material from a TOML file.
    pub fn load_material(
        &mut self,
        path: &Path,
        cache: &mut MaterialPipelineCache,
    ) -> Result<Rc<MaterialTemplate>, MaterialError> {
        let descriptor = load_material_from_file(path)?;
        let name = descriptor.name.clone();

        if self.has_template(&name) {
            debug!("Template '{}' already exists, skipping", name);
            return Ok(self.get_template(&name).unwrap().clone());
        }

        let (is_skinned, is_pbr_full) = detect_material_type(&descriptor);
        debug!(
            "Template '{}' detection: is_skinned={}, is_pbr_full={}",
            name, is_skinned, is_pbr_full
        );

        let vertex_binding = if is_skinned {
            crate::vulkan::vertexbinding::get_skinned_vertex_binding()
        } else {
            get_pbr_vertex_binding()
        };

        let config = if is_skinned {
            DynamicMaterialConfig::skinned(&descriptor, vertex_binding)
        } else if is_pbr_full {
            DynamicMaterialConfig::full_pbr(&descriptor, vertex_binding)
        } else {
            DynamicMaterialConfig::pbr(&descriptor, vertex_binding)
        };

        let pipeline_handle = cache.get_or_create(&config).map_err(|e| {
            MaterialError::InvalidDescriptor(format!("Failed to create pipeline: {}", e))
        })?;

        let reflection = generate_reflection(&descriptor)?;

        let is_bindless = config.uses_bindless();
        let template = MaterialTemplate::from_cached_pipeline_with_layouts(
            name.clone(),
            descriptor,
            reflection,
            pipeline_handle,
            vk::DescriptorSetLayout::null(),
            None,
            None,
            is_bindless,
        );

        self.register_template_with_path(template, path);

        Ok(self.get_template(&name).unwrap().clone())
    }

    /// Load a single bindless material from a TOML file.
    pub(crate) fn load_material_bindless(
        &mut self,
        path: &Path,
        cache: &mut MaterialPipelineCache,
        bindless_layout: crate::sync::VkDescriptorSetLayout,
    ) -> Result<Rc<MaterialTemplate>, MaterialError> {
        let descriptor = load_material_from_file(path)?;
        let name = descriptor.name.clone();

        if self.has_template(&name) {
            debug!("Template '{}' already exists, skipping", name);
            return Ok(self.get_template(&name).unwrap().clone());
        }

        let is_skinned = detect_material_type(&descriptor).0;

        let vertex_binding = if is_skinned {
            crate::vulkan::vertexbinding::get_skinned_vertex_binding()
        } else {
            get_pbr_vertex_binding()
        };

        let config = if is_skinned {
            DynamicMaterialConfig::bindless_skinned(&descriptor, vertex_binding)
        } else {
            DynamicMaterialConfig::bindless(&descriptor, vertex_binding)
        };

        let pipeline_handle = cache
            .get_or_create_bindless(&config, bindless_layout)
            .map_err(|e| {
                MaterialError::InvalidDescriptor(format!(
                    "Failed to create bindless pipeline: {}",
                    e
                ))
            })?;

        let reflection = generate_reflection(&descriptor)?;

        let template = MaterialTemplate::from_cached_pipeline_with_layouts(
            name.clone(),
            descriptor,
            reflection,
            pipeline_handle,
            vk::DescriptorSetLayout::null(),
            None,
            None,
            true,
        );

        self.register_template_with_path(template, path);

        Ok(self.get_template(&name).unwrap().clone())
    }

    /// Enable hot reload watching for all templates
    pub fn enable_hot_reload(
        &mut self,
        directory: &Path,
        debounce_ms: u64,
    ) -> Result<(), MaterialError> {
        let watcher = FileWatcher::new(directory, debounce_ms).map_err(|e| {
            MaterialError::InvalidDescriptor(format!("Failed to create file watcher: {}", e))
        })?;
        self.file_watcher = Some(watcher);
        self.watch_directory = Some(directory.to_path_buf());
        info!("Hot reload enabled for directory: {}", directory.display());
        Ok(())
    }

    /// Disable hot reload watching
    pub fn disable_hot_reload(&mut self) {
        self.file_watcher = None;
        self.watch_directory = None;
    }

    /// Check if hot reload is enabled
    pub fn is_hot_reload_enabled(&self) -> bool {
        self.file_watcher.is_some()
    }

    /// Check for file changes and reload modified materials
    pub fn check_reload(
        &mut self,
        cache: &mut MaterialPipelineCache,
    ) -> Result<Vec<String>, MaterialError> {
        let changed_paths: Vec<PathBuf> = {
            let watcher = match &self.file_watcher {
                Some(w) => w,
                None => return Ok(Vec::new()),
            };
            std::iter::from_fn(|| watcher.try_events()).collect()
        };

        let mut reloaded = Vec::new();

        for path in changed_paths {
            if let Some(name) = self.find_template_by_path(&path) {
                debug!("Reloading material: {}", name);

                self.templates.remove(&name);
                self.template_paths.remove(&name);

                match self.load_material(&path, cache) {
                    Ok(_) => {
                        reloaded.push(name);
                    }
                    Err(e) => {
                        debug!("Failed to reload material: {}", e);
                    }
                }
            }
        }

        Ok(reloaded)
    }

    /// Convenience method for hot reload - returns count of reloaded materials
    pub fn check_hot_reload(
        &mut self,
        cache: &mut MaterialPipelineCache,
    ) -> Result<usize, MaterialError> {
        Ok(self.check_reload(cache)?.len())
    }

    /// Find a template name by its file path
    fn find_template_by_path(&self, path: &Path) -> Option<String> {
        self.template_paths
            .iter()
            .find(|(_, p)| *p == path)
            .map(|(name, _)| name.clone())
    }

    /// Destroy all templates and clean up resources
    pub fn destroy(&mut self) {
        self.templates.clear();
        self.template_paths.clear();
        self.file_watcher = None;
        self.watch_directory = None;
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Detect material type from shader path
fn detect_material_type(descriptor: &MaterialDescriptor) -> (bool, bool) {
    let is_skinned = match &descriptor.vertex_shader {
        crate::vulkan::material::ShaderSource::WgslFile(path) => {
            path.to_string_lossy().to_lowercase().contains("skinned")
        }
        _ => false,
    };

    let is_pbr_full = match &descriptor.vertex_shader {
        crate::vulkan::material::ShaderSource::WgslFile(path) => {
            path.to_string_lossy().to_lowercase().contains("pbr_full")
        }
        _ => false,
    };

    (is_skinned, is_pbr_full)
}

/// Generate shader reflection from descriptor
fn generate_reflection(descriptor: &MaterialDescriptor) -> Result<ShaderReflection, MaterialError> {
    use crate::vulkan::material::ShaderSource;

    match &descriptor.vertex_shader {
        ShaderSource::WgslFile(path) => {
            let wgsl = std::fs::read_to_string(path)
                .map_err(|e| MaterialError::ShaderLoadFailed(path.clone(), e))?;
            ShaderReflection::from_wgsl(&wgsl).map_err(|e| {
                MaterialError::InvalidDescriptor(format!("Reflection failed: {:?}", e))
            })
        }
        _ => Ok(ShaderReflection::default()),
    }
}

/// Get PBR vertex binding helper
fn get_pbr_vertex_binding() -> crate::VertexBinding {
    crate::vulkan::vertexbinding::get_pbr_vertex_binding()
}
