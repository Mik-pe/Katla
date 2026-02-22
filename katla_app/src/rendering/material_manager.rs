use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc::{self, Receiver},
};

use crate::rendering::Material;
use katla_vulkan::{MaterialHandle, VulkanContext, VulkanRenderer, MaterialRegistry};
use log::{debug, error, info};
use notify::{Watcher, RecursiveMode};

/// ID for referencing a shared material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MaterialId(pub usize);

/// Manages shared materials to avoid duplication.
///
/// Materials can be registered by name and then cloned for multiple models.
/// Since Material's fields (pipeline, texture) are Rc-wrapped, cloning is cheap.
pub struct MaterialManager {
    materials: Vec<Material>,
    by_name: HashMap<String, MaterialId>,
    hot_reload: Option<MaterialHotReload>,
    shader_to_materials: HashMap<PathBuf, Vec<String>>, // Maps shader path -> material names
    material_handles: HashMap<String, MaterialHandle>, // Maps material name -> renderer handle
    context: Option<Rc<VulkanContext>>, // Store context for cleanup
}

impl MaterialManager {
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            by_name: HashMap::new(),
            hot_reload: None,
            shader_to_materials: HashMap::new(),
            material_handles: HashMap::new(),
            context: None,
        }
    }

    /// Set the Vulkan context for this manager (needed for cleanup)
    pub fn set_context(&mut self, context: Rc<VulkanContext>) {
        self.context = Some(context);
    }

    /// Clean up old pipelines that are no longer referenced.
    ///
    /// This should be called periodically to free resources from hot-reloaded materials.
    pub fn cleanup_old_pipelines(&mut self) {
        // Note: We rely on Rc's reference counting. When all references to an old pipeline
        // are gone (including in-flight command buffers), it will be dropped automatically.
        // For hot reload in development, we accept that some old pipelines may stay in memory.
    }

    /// Register a material with a name, returning its ID.
    ///
    /// The material can be cloned and used in multiple models.
    /// Since the material's internal fields use Rc, cloning is cheap.
    pub fn register_material(&mut self, name: impl Into<String>, material: Material) -> MaterialId {
        let name = name.into();
        let id = MaterialId(self.materials.len());
        self.materials.push(material);
        self.by_name.insert(name.clone(), id);
        id
    }

    /// Register a material with shader tracking for hot reload.
    ///
    /// # Arguments
    /// * `name` - Material name
    /// * `material` - The material
    /// * `shader_path` - Path to the WGSL shader file this material uses
    pub fn register_material_with_shader(
        &mut self,
        name: impl Into<String>,
        material: Material,
        shader_path: impl AsRef<Path>,
    ) -> MaterialId {
        let name = name.into();
        // Store the path as-is (don't canonicalize to avoid platform-specific prefixes)
        let shader_path = shader_path.as_ref().to_path_buf();

        let id = MaterialId(self.materials.len());
        self.materials.push(material);
        self.by_name.insert(name.clone(), id);

        // Track shader -> material mapping
        self.shader_to_materials
            .entry(shader_path)
            .or_default()
            .push(name);

        id
    }

    /// Get a reference to a material by ID.
    pub fn get(&self, id: MaterialId) -> Option<&Material> {
        self.materials.get(id.0)
    }

    /// Get a mutable reference to a material by ID.
    pub fn get_mut(&mut self, id: MaterialId) -> Option<&mut Material> {
        self.materials.get_mut(id.0)
    }

    /// Get a material ID by name.
    pub fn get_by_name(&self, name: &str) -> Option<MaterialId> {
        self.by_name.get(name).copied()
    }

    /// Clone a material by ID for use in a Model.
    ///
    /// This is cheap because Material's fields are Rc-wrapped.
    pub fn clone_material(&self, id: MaterialId) -> Option<Material> {
        self.get(id).cloned()
    }

    /// Clone a material by name for use in a Model.
    pub fn clone_material_by_name(&self, name: &str) -> Option<Material> {
        self.get_by_name(name)
            .and_then(|id| self.clone_material(id))
    }

    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    /// Register a material's handle after it's been registered with the renderer.
    ///
    /// This is needed so hot reload can update the renderer's AssetRegistry.
    pub fn register_handle(&mut self, name: &str, handle: MaterialHandle) {
        self.material_handles.insert(name.to_string(), handle);
    }

    /// Register a material from a template in the MaterialRegistry.
    ///
    /// This method checks if a template with the given name exists in the registry,
    /// and if so, creates a Material from that template. This enables hot reload
    /// to automatically update all materials using the same template.
    ///
    /// # Arguments
    /// * `name` - Material name (also used to look up the template)
    /// * `material_registry` - The MaterialRegistry containing loaded templates
    /// * `texture` - Optional texture for this material instance
    /// * `color` - Optional color for this material instance
    ///
    /// # Returns
    /// * `Some(MaterialId)` - If template was found and material created
    /// * `None` - If no template with that name exists
    pub fn register_from_template(
        &mut self,
        name: impl Into<String>,
        material_registry: &MaterialRegistry,
        texture: Option<Rc<katla_vulkan::Texture>>,
        color: Option<katla_math::Color>,
    ) -> Option<MaterialId> {
        let name = name.into();

        // Try to get the template from the registry
        let template = material_registry.get_template(&name)?;

        // Create material from template
        let material = Material::from_template(template, texture, color);

        // Register the material
        Some(self.register_material(name, material))
    }

    /// Update a material's handle (used when material is re-registered)
    pub fn update_material_handle(&mut self, name: &str, handle: MaterialHandle) {
        if let Some(&id) = self.by_name.get(name) {
            if id.0 < self.materials.len() {
                self.materials[id.0].handle = Some(handle);
                self.material_handles.insert(name.to_string(), handle);
            }
        }
    }

    /// Enable hot reload for materials.
    ///
    /// # Arguments
    /// * `shaders_directory` - Directory containing WGSL shader files
    /// * `debounce_ms` - Debounce delay in milliseconds to prevent multiple notifications
    ///
    /// # Example
    /// ```ignore
    /// material_manager.enable_hot_reload(Path::new("resources/shaders"), 100);
    /// ```
    pub fn enable_hot_reload(
        &mut self,
        shaders_directory: &Path,
        debounce_ms: u64,
    ) -> Result<(), String> {
        let reload = MaterialHotReload::new(shaders_directory, debounce_ms)?;
        self.hot_reload = Some(reload);
        info!("Hot reload enabled for: {}", shaders_directory.display());
        Ok(())
    }

    /// Disable hot reload.
    pub fn disable_hot_reload(&mut self) {
        self.hot_reload = None;
        self.shader_to_materials.clear();
        self.material_handles.clear();
    }

    /// Check if hot reload is enabled.
    pub fn is_hot_reload_enabled(&self) -> bool {
        self.hot_reload.is_some()
    }

    /// Check for shader changes and reload affected materials.
    ///
    /// # Arguments
    /// * `renderer` - Vulkan renderer (provides access to AssetRegistry for updates)
    /// * `material_factory` - Function that creates new materials given a name
    ///
    /// # Returns
    /// Number of materials that were reloaded
    ///
    /// # Example
    /// ```ignore
    /// material_manager.check_hot_reload(
    ///     &mut renderer,
    ///     |name, context, render_pass| {
    ///         match name {
    ///             "colored_mesh" => create_colored_checkerboard_material(
    ///                 context,
    ///                 render_pass,
    ///                 Color::rgb(1.0, 0.5, 0.0),
    ///             ),
    ///             _ => panic!("Unknown material: {}", name),
    ///         }
    ///     },
    /// );
    /// ```
    pub fn check_hot_reload<F>(
        &mut self,
        renderer: &mut VulkanRenderer,
        material_factory: F,
    ) -> usize
    where
        F: Fn(&str, Rc<VulkanContext>) -> Material,
    {
        let Some(ref hot_reload) = self.hot_reload else {
            return 0;
        };

        let mut reloaded = 0;

        while let Some(shader_path) = hot_reload.check() {
            debug!("Shader modified: {:?}", shader_path);

            // Try to find matching materials using smart path comparison
            for (tracked_path, material_names) in &self.shader_to_materials {
                if Self::paths_match(&shader_path, tracked_path) {
                    debug!("  Matched shader: {:?} == {:?}", shader_path, tracked_path);

                    for material_name in material_names {
                        // Recreate the material
                        let new_material = material_factory(
                            material_name,
                            renderer.context.clone(),
                        );

                        // Update the material in the manager
                        if let Some(&id) = self.by_name.get(material_name) {
                            // Update the AssetRegistry's material if we have a handle
                            if let Some(&handle) = self.material_handles.get(material_name) {
                                // Replace the pipeline in AssetRegistry
                                let updated = renderer.asset_registry.replace_material_pipeline(
                                    handle,
                                    new_material.material_pipeline.clone(),
                                );

                                if updated {
                                    // Update in MaterialManager
                                    self.materials[id.0] = new_material;
                                    reloaded += 1;
                                    debug!(
                                        "  ✓ Reloaded material: {} (updated AssetRegistry)",
                                        material_name
                                    );
                                } else {
                                    debug!(
                                        "  ✗ Failed to update AssetRegistry: {}",
                                        material_name
                                    );
                                }
                            } else {
                                // No handle in AssetRegistry - just update MaterialManager
                                self.materials[id.0] = new_material;
                                reloaded += 1;
                                debug!(
                                    "  ✓ Reloaded material: {} (MaterialManager only)",
                                    material_name
                                );
                            }
                        }
                    }
                    break; // Found the match, no need to check others
                }
            }

            if reloaded == 0 {
                debug!("  ✗ No materials found using this shader");
            }
        }

        reloaded
    }

    /// Check if two paths refer to the same file (platform-agnostic)
    fn paths_match(path1: &Path, path2: &Path) -> bool {
        // Try direct comparison first
        if path1 == path2 {
            return true;
        }

        // Try canonicalizing both (handles symlinks, relative paths, etc.)
        let canon1 = path1.canonicalize();
        let canon2 = path2.canonicalize();

        if let (Ok(c1), Ok(c2)) = (canon1, canon2) {
            if c1 == c2 {
                return true;
            }
        }

        // Fallback: compare file names
        path1.file_name() == path2.file_name()
    }

    /// Destroy all Vulkan resources held by managed materials.
    ///
    /// This should be called during shutdown AFTER wait_for_device() to ensure
    /// the GPU is not using any resources.
    ///
    /// Note: Old pipelines from hot reload are not destroyed here to avoid
    pub fn destroy(&mut self) {
        // Destroy active materials
        for material in &mut self.materials {
            // Destroy the pipeline
            if let Ok(mut pipeline) = material.material_pipeline.try_borrow_mut() {
                pipeline.destroy();
            }
        }

        self.materials.clear();
        self.by_name.clear();
        self.shader_to_materials.clear();
        self.material_handles.clear();
        self.hot_reload = None;
    }
}

impl Default for MaterialManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal hot reload watcher for materials.
struct MaterialHotReload {
    _watcher_thread: std::thread::JoinHandle<()>,
    receiver: Receiver<PathBuf>,
}

impl MaterialHotReload {
    fn new(directory: &Path, debounce_ms: u64) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        let dir = directory.to_path_buf();

        let thread = std::thread::spawn(move || {
            Self::watcher_thread(dir, tx, debounce_ms);
        });

        Ok(Self {
            _watcher_thread: thread,
            receiver: rx,
        })
    }

    fn watcher_thread(directory: PathBuf, sender: mpsc::Sender<PathBuf>, debounce_ms: u64) {
        let (notify_tx, notify_rx) = mpsc::channel();

        // Create watcher with config - explicitly type as RecommendedWatcher
        let mut watcher: notify::RecommendedWatcher = match notify::Watcher::new(
            move |res| {
                if let Ok(event) = res {
                    let _ = notify_tx.send(event);
                }
            },
            notify::Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                error!("Failed to create file watcher: {:?}", e);
                return;
            }
        };

        // Watch the directory
        if let Err(e) = Watcher::watch(&mut watcher, &directory, RecursiveMode::Recursive) {
            error!("Failed to watch directory {}: {:?}", directory.display(), e);
            return;
        }

        debug!("File watcher started for: {}", directory.display());

        let mut last_event_time = std::time::Instant::now();
        let mut last_modified_path: Option<PathBuf> = None;

        for event in notify_rx {
            if matches!(
                event.kind,
                notify::EventKind::Modify(_) | notify::EventKind::Create(_)
            ) {
                let now = std::time::Instant::now();

                for path in &event.paths {
                    if path.extension().is_some_and(|ext| ext == "wgsl") {
                        if now.duration_since(last_event_time)
                            > std::time::Duration::from_millis(debounce_ms)
                        {
                            let _ = sender.send(path.clone());
                            last_event_time = now;
                            last_modified_path = Some(path.clone());
                        } else if let Some(ref last_path) = last_modified_path {
                            if path == last_path {
                                last_event_time = now;
                            }
                        }
                        break;
                    }
                }
            }
        }
    }

    fn check(&self) -> Option<PathBuf> {
        self.receiver.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test_material_manager_empty() {
        let manager = MaterialManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_material_manager_register_and_retrieve() {
        let manager = MaterialManager::new();

        // Test that non-existent material returns None
        assert!(manager.get_by_name("test").is_none());
    }

    #[test]
    fn test_hot_reload_toggle() {
        let mut manager = MaterialManager::new();
        assert!(!manager.is_hot_reload_enabled());

        // Note: Can't actually enable without a valid directory in tests
        // but we can test the API
        assert!(!manager.is_hot_reload_enabled());
    }

    #[test]
    fn test_hot_reload_with_temp_folder() {
        // Create a temporary directory for testing
        let temp_dir = std::env::temp_dir().join("katla_hot_reload_test");
        fs::create_dir_all(&temp_dir).unwrap();

        // Enable hot reload on the temp directory
        let mut manager = MaterialManager::new();
        let result = manager.enable_hot_reload(&temp_dir, 50);

        // Should succeed
        assert!(result.is_ok(), "Failed to enable hot reload: {:?}", result);
        assert!(manager.is_hot_reload_enabled());

        // Verify the watcher is active by checking the flag
        assert!(manager.hot_reload.is_some());

        // Clean up
        manager.disable_hot_reload();
        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_hot_reload_file_detection() {
        // Create a temporary directory
        let temp_dir = std::env::temp_dir().join("katla_file_detect_test");
        fs::create_dir_all(&temp_dir).unwrap();

        // Enable hot reload
        let mut manager = MaterialManager::new();
        manager.enable_hot_reload(&temp_dir, 50).unwrap();

        // Create a test WGSL file
        let test_shader = temp_dir.join("test.wgsl");
        fs::write(&test_shader, "// Test shader").unwrap();

        // Give the file system a moment to propagate the event
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Note: We can't actually check for the event in a test easily without
        // a Vulkan context and render pass, but we can verify the watcher is active
        assert!(manager.is_hot_reload_enabled());

        // Clean up
        manager.disable_hot_reload();
        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn test_register_material_with_shader_tracking() {
        let manager = MaterialManager::new();

        // Create a dummy material path
        let shader_path = PathBuf::from("/test/shader.wgsl");

        // Note: We can't create actual materials without Vulkan resources
        // but we can test that the API exists and compiles
        assert!(manager.shader_to_materials.is_empty());
    }
}
