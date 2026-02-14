//! Material registry for central template and instance management.
//!
//! This module provides a centralized registry for material templates,
//! supporting loading from files, bulk loading from directories, and
//! event-driven template hot reload using filesystem watching.

use super::{
    load_material_from_file, FileWatcher, MaterialError, MaterialTemplate, MaterialTemplateBuilder,
};
use crate::VulkanContext;
use log::info;
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
        self.template_paths.insert(name.clone(), path.to_path_buf());
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

    /// Load a material template from a TOML file
    pub fn load_from_file(
        &mut self,
        path: &Path,
        context: Rc<VulkanContext>,
    ) -> Result<String, MaterialError> {
        // Load descriptor from TOML
        let descriptor = load_material_from_file(path).map_err(|e| {
            MaterialError::InvalidDescriptor(format!("Failed to load {}: {}", path.display(), e))
        })?;

        // Get template name from descriptor
        let name = descriptor.name.clone();

        // Build template
        let builder = MaterialTemplateBuilder::new(name.clone())
            .with_descriptor(descriptor)
            .with_context(context);

        // Note: vertex_binding needs to be set by the caller
        // For now, we'll store the template without it
        let template = builder.build()?;

        // Register template with path for hot reload tracking
        self.register_template_with_path(template, path);

        Ok(name)
    }

    /// Load a material template from a TOML file with vertex binding
    pub fn load_from_file_with_binding(
        &mut self,
        path: &Path,
        context: Rc<VulkanContext>,
        vertex_binding: crate::VertexBinding,
    ) -> Result<String, MaterialError> {
        // Load descriptor from TOML
        let descriptor = load_material_from_file(path).map_err(|e| {
            MaterialError::InvalidDescriptor(format!("Failed to load {}: {}", path.display(), e))
        })?;

        // Get template name from descriptor
        let name = descriptor.name.clone();

        // Build template with vertex binding
        let template = MaterialTemplateBuilder::new(name.clone())
            .with_descriptor(descriptor)
            .with_context(context)
            .with_vertex_binding(vertex_binding)
            .build()?;

        // Register template with path for hot reload tracking
        self.register_template_with_path(template, path);

        Ok(name)
    }

    /// Load all material templates from a directory
    ///
    /// This method loads all .toml files from the specified directory and creates
    /// material templates from them. Templates are built with a default PBR vertex binding.
    pub fn load_directory(
        &mut self,
        dir: &Path,
        context: Rc<VulkanContext>,
    ) -> Result<usize, MaterialError> {
        self.load_directory_internal(dir, context, false)
    }

    /// Load all material templates from a directory using storage buffers with instance indexing
    ///
    /// This is the same as load_directory but creates pipelines with storage buffer
    /// layouts for modern rendering using `@builtin(instance_index)`.
    pub fn load_directory_storage(
        &mut self,
        dir: &Path,
        context: Rc<VulkanContext>,
    ) -> Result<usize, MaterialError> {
        self.load_directory_internal(dir, context, true)
    }

    fn load_directory_internal(
        &mut self,
        dir: &Path,
        context: Rc<VulkanContext>,
        use_storage: bool,
    ) -> Result<usize, MaterialError> {
        use crate::vulkan::vertexbinding::get_pbr_vertex_binding;

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

            // Only load .toml files
            if path.extension() != Some(OsStr::new("toml")) {
                continue;
            }

            // Load the descriptor from the TOML file
            let descriptor = load_material_from_file(&path).map_err(|e| {
                MaterialError::InvalidDescriptor(format!(
                    "Failed to load {}: {}",
                    path.display(),
                    e
                ))
            })?;

            // Get template name from descriptor
            let name = descriptor.name.clone();

            // Detect if this is a skinned material by checking shader filename
            let is_skinned = match &descriptor.vertex_shader {
                crate::vulkan::material::ShaderSource::WgslFile(path) => {
                    path.to_string_lossy().to_lowercase().contains("skinned")
                }
                _ => false,
            };

            // Build template with appropriate vertex binding
            let vertex_binding = if is_skinned {
                crate::vulkan::vertexbinding::get_skinned_vertex_binding()
            } else {
                get_pbr_vertex_binding()
            };

            let template = if is_skinned {
                // Skinned materials always use storage mode
                MaterialTemplateBuilder::new(name.clone())
                    .with_descriptor(descriptor)
                    .with_context(context.clone())
                    .with_vertex_binding(vertex_binding)
                    .build_storage_skinned()?
            } else if use_storage {
                MaterialTemplateBuilder::new(name.clone())
                    .with_descriptor(descriptor)
                    .with_context(context.clone())
                    .with_vertex_binding(vertex_binding)
                    .build_storage()?
            } else {
                MaterialTemplateBuilder::new(name.clone())
                    .with_descriptor(descriptor)
                    .with_context(context.clone())
                    .with_vertex_binding(vertex_binding)
                    .build()?
            };

            // Register template with path for hot reload tracking
            self.register_template_with_path(template, &path);
            loaded += 1;
        }

        Ok(loaded)
    }

    /// Enable hot reload watching for all templates
    ///
    /// This starts a filesystem watcher that will detect changes to
    /// material files (.toml and .wgsl files). The watcher runs in a
    /// background thread and sends events when files are modified.
    ///
    /// # Arguments
    /// * `directory` - The directory to watch for changes (typically the materials directory)
    /// * `debounce_ms` - Debounce delay in milliseconds to prevent multiple notifications
    ///   for the same file change (default: 100ms)
    pub fn enable_hot_reload(
        &mut self,
        directory: &Path,
        debounce_ms: u64,
    ) -> Result<(), MaterialError> {
        // Create the file watcher
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

    /// Check for file modifications and reload templates if needed
    ///
    /// This is a non-blocking call that checks if any files have been modified
    /// since the last call. If hot reload is not enabled, this returns Ok(0).
    ///
    /// # Returns
    /// The number of templates that were reloaded
    ///
    /// # Usage
    /// Call this in your update loop:
    /// ```ignore
    /// if registry.is_hot_reload_enabled() {
    ///     match registry.check_hot_reload(context.clone(), &render_pass) {
    ///         Ok(reloaded) if reloaded > 0 => {
    ///             println!("Hot reloaded {} templates", reloaded);
    ///         }
    ///         Err(e) => eprintln!("Reload failed: {}", e),
    ///         _ => {}
    ///     }
    /// }
    /// ```
    pub fn check_hot_reload(
        &mut self,
        context: Rc<VulkanContext>,
    ) -> Result<usize, MaterialError> {
        // Collect all modified paths first, releasing the watcher borrow
        let modified_paths: Vec<PathBuf> = if self.file_watcher.is_some() {
            let watcher = self.file_watcher.as_ref().unwrap();
            let mut paths = Vec::new();
            while let Some(path) = watcher.try_recv() {
                paths.push(path);
            }
            paths
        } else {
            return Ok(0);
        };

        let mut reloaded = 0;

        // Now process each modified path (no borrows held)
        for modified_path in modified_paths {
            // Find which template this file belongs to
            let templates_to_reload: Vec<_> = self
                .template_paths
                .iter()
                .filter(|(_, path)| {
                    // Check if the modified file is the material file or a shader it uses
                    modified_path == **path || Self::uses_shader(&modified_path, path.as_path())
                })
                .map(|(name, _)| name.clone())
                .collect();

            // Reload each affected template
            for name in templates_to_reload {
                info!("  Reloading template: {}", name);

                // Clone the path to release the borrow
                let path = match self.template_paths.get(&name).cloned() {
                    Some(p) => p,
                    None => continue,
                };

                // Load the descriptor to get shader paths
                let descriptor = load_material_from_file(&path).map_err(|e| {
                    MaterialError::InvalidDescriptor(format!(
                        "Failed to load {}: {}",
                        path.display(),
                        e
                    ))
                })?;

                // Get the existing descriptor set layout from the template
                // This is preserved across hot reloads to keep material instances' descriptor sets valid
                let desc_layout = if let Some(template) = self.templates.get(&name) {
                    template.desc_layout()
                } else {
                    info!("  ✗ Template not found in registry: {}", name);
                    continue;
                };

                // Rebuild only the pipeline (not the entire template)
                // This preserves the Rc so all materials see the update
                let vertex_binding = crate::vulkan::vertexbinding::get_pbr_vertex_binding();
                let mut builder =
                    super::MaterialBuilder::from_descriptor(descriptor.clone(), context.clone())?;
                builder = builder.with_vertex_binding(vertex_binding);

                // Build with the existing descriptor set layout to preserve compatibility
                let new_pipeline = builder
                    .build_with_desc_layout(desc_layout)
                    .map_err(|e| {
                        MaterialError::InvalidDescriptor(format!("Pipeline build failed: {:?}", e))
                    })?;

                // Update the existing template's pipeline in-place
                if let Some(template) = self.templates.get(&name) {
                    // Wait for the GPU to finish using the pipeline before destroying it
                    // This prevents validation errors about destroying resources that are still in use
                    info!("  Waiting for GPU idle before destroying pipeline...");
                    unsafe {
                        context
                            .device
                            .device_wait_idle()
                            .expect("Failed to wait for device idle");
                    }

                    // Use the new pipeline_mut() method to get mutable access through RefCell
                    let mut pipeline_ref = template.pipeline_mut();

                    // Destroy the old pipeline (but preserve the descriptor set layout)
                    pipeline_ref.destroy_preserving_layout();

                    // Drop the mutable borrow
                    drop(pipeline_ref);

                    // Get a new mutable borrow and replace the pipeline
                    let mut pipeline_ref = template.pipeline_mut();
                    *pipeline_ref = new_pipeline;

                    reloaded += 1;
                    info!("  ✓ Hot reloaded material template: {}", name);
                } else {
                    info!("  ✗ Template not found in registry: {}", name);
                }
            }
        }

        Ok(reloaded)
    }

    /// Check if a material file uses a specific shader file
    fn uses_shader(shader_path: &Path, material_path: &Path) -> bool {
        // Try to read the material file to check for shader references
        if let Ok(content) = fs::read_to_string(material_path) {
            // Extract just the filename from the shader path
            // This handles both absolute paths (I:\dev\...\file.wgsl) and relative paths (../shaders/file.wgsl)
            let shader_filename = shader_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            if shader_filename.is_empty() {
                return false;
            }

            // Check if the material file references this shader by filename
            content.contains(shader_filename)
        } else {
            false
        }
    }

    /// Create an instance from a template by name
    pub fn create_instance(&self, template_name: &str) -> Option<super::MaterialInstance> {
        self.get_template(template_name)
            .map(|template| super::MaterialInstance::with_template(Rc::clone(template)))
    }

    /// Remove a template from the registry
    pub fn remove_template(&mut self, name: &str) -> Option<Rc<MaterialTemplate>> {
        self.template_paths.remove(name);
        self.templates.remove(name)
    }

    /// Clear all templates from the registry
    pub fn clear(&mut self) {
        self.templates.clear();
        self.template_paths.clear();
    }

    /// Destroy the registry and clean up resources
    pub fn destroy(&mut self) {
        // Clear templates and paths
        // Note: Templates are Rc<MaterialTemplate> so we can't easily destroy them
        // In production, you'd want to use Weak references or explicit cleanup
        self.templates.clear();
        self.template_paths.clear();

        // Stop file watcher
        self.file_watcher = None;
        self.watch_directory = None;
    }
}

impl Default for MaterialRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to load materials from a directory path
///
/// This is a simplified API for common use cases
pub fn load_materials_from_directory(
    dir: impl AsRef<Path>,
    context: Rc<VulkanContext>,
) -> Result<MaterialRegistry, MaterialError> {
    let mut registry = MaterialRegistry::new();
    registry.load_directory(dir.as_ref(), context)?;
    Ok(registry)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = MaterialRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert_eq!(registry.template_names().len(), 0);
    }

    #[test]
    fn test_hot_reload_toggle() {
        let mut registry = MaterialRegistry::new();
        assert!(!registry.is_hot_reload_enabled());

        // Note: We can't actually enable hot reload without a valid directory
        // and Vulkan context, but we can test the API
        assert!(!registry.is_hot_reload_enabled());
    }
}
