use std::rc::Rc;

use katla_ecs::{EntityId, World};
use katla_math::Transform;
use katla_vulkan::{MaterialHandle, MaterialRegistry, MeshHandle, VulkanContext, VulkanRenderer};
use log::{debug, info, warn};

use crate::{
    components::{DrawableComponent, NameComponent, TransformComponent},
    rendering::{Material, Mesh},
    util::GLTFModel,
};

pub struct Model {
    pub entity: EntityId,
    /// Handle after registration (MeshHandle(0) if no renderer provided)
    pub mesh_handle: MeshHandle,
    /// Handle after registration (MaterialHandle(0) if no renderer provided)
    pub material_handle: MaterialHandle,
}

impl Model {
    pub fn new(
        world: &mut World,
        mut meshes: Vec<Mesh>,
        material: Material,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
        color: Option<katla_math::Color>,
    ) -> Self {
        // Register assets with renderer if available
        let (mesh_handle, material_handle) =
            Self::register_with_renderer(meshes.first_mut(), material, renderer);

        // Create DrawableComponent with optional color and PBR values
        let drawable = DrawableComponent::with_handles_and_material(
            mesh_handle,
            material_handle,
            color,
            0.0, // metallic - will be overridden by GLTF loading
            0.5, // roughness - will be overridden by GLTF loading
            1.0, // ao
        );

        // Spawn entity with all components
        let entity = world.spawn((
            TransformComponent::new(transform),
            drawable,
            NameComponent::new("Model"),
        ));

        Self {
            entity,
            mesh_handle,
            material_handle,
        }
    }

    /// Register mesh and material with the renderer.
    ///
    /// Returns (MeshHandle, MaterialHandle) - uses dummy handles if no renderer.
    fn register_with_renderer(
        first_mesh: Option<&mut Mesh>,
        material: Material,
        renderer: Option<&mut VulkanRenderer>,
    ) -> (MeshHandle, MaterialHandle) {
        if let Some(r) = renderer {
            // Register mesh - take buffers from first mesh
            let mesh_h = if let Some(mesh) = first_mesh {
                let vertex_buffer = mesh.vertex_buffer.take();
                let index_buffer = mesh.index_buffer.take();
                r.register_mesh(vertex_buffer, index_buffer)
            } else {
                MeshHandle(0)
            };

            // Register material with optional PBR textures
            let (
                pipeline,
                _texture,
                vertex_binding,
                pbr_textures,
                _pbr_texture_refs,
                texture_indices,
                emission_index,
                is_bindless,
            ) = material.get_registration_data();

            // Vertex binding and pipeline are required for material registration
            let vertex_binding = vertex_binding.expect("Material must have vertex binding");

            // Use PBR registration if PBR textures are present
            let mat_h = if let Some(pbr) = pbr_textures {
                r.register_material_pbr(
                    pipeline,
                    vertex_binding,
                    is_bindless,
                    pbr,
                    texture_indices,
                    emission_index,
                )
            } else {
                r.register_material_full(
                    pipeline,
                    vertex_binding,
                    is_bindless,
                    texture_indices,
                    emission_index,
                )
            };

            (mesh_h, mat_h)
        } else {
            (MeshHandle(0), MaterialHandle(0))
        }
    }

    /// Create a model with explicit PBR material values.
    pub fn new_with_pbr(
        world: &mut World,
        mut meshes: Vec<Mesh>,
        material: Material,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
        color: Option<katla_math::Color>,
        metallic: f32,
        roughness: f32,
        ao: f32,
    ) -> Self {
        // Register assets with renderer if available
        let (mesh_handle, material_handle) =
            Self::register_with_renderer(meshes.first_mut(), material, renderer);

        // Create DrawableComponent with PBR values
        let drawable = DrawableComponent::with_handles_and_material(
            mesh_handle,
            material_handle,
            color,
            metallic,
            roughness,
            ao,
        );

        // Spawn entity with all components
        let entity = world.spawn((
            TransformComponent::new(transform),
            drawable,
            NameComponent::new("Model"),
        ));

        Self {
            entity,
            mesh_handle,
            material_handle,
        }
    }

    /// Smart GLTF importer that automatically detects the best material type.
    ///
    /// This unified importer handles all GLTF models by:
    /// - Detecting if the model has skinning (skeletal animation)
    /// - Always using full PBR with default textures for missing maps
    /// - Selecting the appropriate shader template automatically
    /// - Registering textures with BindlessTextureManager when available
    pub fn from_gltf(
        world: &mut World,
        model: Rc<GLTFModel>,
        context: Rc<VulkanContext>,
        mut renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
        material_registry: &std::cell::RefCell<MaterialRegistry>,
    ) -> Self {
        use katla_vulkan::bindless_texture::{
            DEFAULT_ALBEDO_SLOT, DEFAULT_AO_SLOT, DEFAULT_EMISSION_SLOT, DEFAULT_MR_SLOT,
            DEFAULT_NORMAL_SLOT,
        };
        use katla_vulkan::material::PbrTextureSet;

        // Check if model has skinning
        let has_skinning = model.has_skinning;

        // Check if bindless is available and register textures in one pass
        let (use_bindless, texture_indices, emission_index) = if let Some(ref mut r) = renderer {
            if r.bindless_manager().is_some() {
                // Will register textures later after loading them
                (
                    true,
                    [
                        DEFAULT_ALBEDO_SLOT,
                        DEFAULT_NORMAL_SLOT,
                        DEFAULT_MR_SLOT,
                        DEFAULT_AO_SLOT,
                    ],
                    DEFAULT_EMISSION_SLOT,
                )
            } else {
                (
                    false,
                    [
                        DEFAULT_ALBEDO_SLOT,
                        DEFAULT_NORMAL_SLOT,
                        DEFAULT_MR_SLOT,
                        DEFAULT_AO_SLOT,
                    ],
                    DEFAULT_EMISSION_SLOT,
                )
            }
        } else {
            (
                false,
                [
                    DEFAULT_ALBEDO_SLOT,
                    DEFAULT_NORMAL_SLOT,
                    DEFAULT_MR_SLOT,
                    DEFAULT_AO_SLOT,
                ],
                DEFAULT_EMISSION_SLOT,
            )
        };

        // Helper to load a texture from a GLTF image index
        let load_texture = |image_index: Option<usize>,
                            default_texture: &Rc<katla_vulkan::Texture>|
         -> Rc<katla_vulkan::Texture> {
            if let Some(idx) = image_index {
                if let Some(image) = model.images.get(idx) {
                    if let Some(tex) = Self::load_texture_from_gltf(image, &context) {
                        return tex;
                    }
                }
            }
            Rc::clone(default_texture)
        };

        // Create default textures for missing maps
        let default_albedo = Rc::new(katla_vulkan::Texture::create_default_albedo(
            context.clone(),
        ));
        let default_normal = Rc::new(katla_vulkan::Texture::create_default_normal(
            context.clone(),
        ));
        let default_mr = Rc::new(katla_vulkan::Texture::create_default_metallic_roughness(
            context.clone(),
        ));
        let default_occlusion = Rc::new(katla_vulkan::Texture::create_default_occlusion(
            context.clone(),
        ));
        let default_emission = Rc::new(katla_vulkan::Texture::create_default_emission(
            context.clone(),
        ));

        // Get material info from GLTF
        let mat_info = model.materials.first();
        let (metallic, roughness) = mat_info
            .map(|m| (m.metallic_factor, m.roughness_factor))
            .unwrap_or((0.0, 0.5));

        // Log material detection
        if let Some(mat) = mat_info {
            info!(
                "GLTF import: {} skinning, {} bindless, material: {}",
                if has_skinning { "with" } else { "no" },
                if use_bindless { "with" } else { "no" },
                mat.summary()
            );
        } else {
            info!(
                "GLTF import: {} skinning, {} bindless, no material info (using defaults: M={:.2}, R={:.2})",
                if has_skinning { "with" } else { "no" },
                if use_bindless { "with" } else { "no" },
                metallic, roughness
            );
        }

        // Load all textures (use defaults for missing maps)
        let albedo_tex = load_texture(mat_info.and_then(|m| m.base_color_texture), &default_albedo);
        let normal_tex = load_texture(mat_info.and_then(|m| m.normal_texture), &default_normal);
        let mr_tex = load_texture(
            mat_info.and_then(|m| m.metallic_roughness_texture),
            &default_mr,
        );
        let occlusion_tex = load_texture(
            mat_info.and_then(|m| m.occlusion_texture),
            &default_occlusion,
        );
        let emission_tex =
            load_texture(mat_info.and_then(|m| m.emission_texture), &default_emission);

        // Register textures with bindless manager and get indices
        let (texture_indices, emission_index) = if use_bindless {
            if let Some(ref mut r) = renderer {
                if let Some(manager) = r.bindless_manager_mut() {
                    // Register textures with bindless manager
                    let albedo_idx = manager
                        .register_texture(albedo_tex.image_view)
                        .unwrap_or(DEFAULT_ALBEDO_SLOT);
                    let normal_idx = manager
                        .register_texture(normal_tex.image_view)
                        .unwrap_or(DEFAULT_NORMAL_SLOT);
                    let mr_idx = manager
                        .register_texture(mr_tex.image_view)
                        .unwrap_or(DEFAULT_MR_SLOT);
                    let ao_idx = manager
                        .register_texture(occlusion_tex.image_view)
                        .unwrap_or(DEFAULT_AO_SLOT);
                    let emiss_idx = manager
                        .register_texture(emission_tex.image_view)
                        .unwrap_or(DEFAULT_EMISSION_SLOT);

                    debug!(
                        "  Bindless texture slots: albedo={}, normal={}, mr={}, ao={}, emission={}",
                        albedo_idx, normal_idx, mr_idx, ao_idx, emiss_idx
                    );

                    ([albedo_idx, normal_idx, mr_idx, ao_idx], emiss_idx)
                } else {
                    (texture_indices, emission_index)
                }
            } else {
                (texture_indices, emission_index)
            }
        } else {
            (texture_indices, emission_index)
        };

        // Create PbrTextureSet for legacy mode (also needed to keep texture refs)
        let pbr_textures = PbrTextureSet::from_wrapped_shared_sampler(
            albedo_tex.image_view,
            normal_tex.image_view,
            mr_tex.image_view,
            occlusion_tex.image_view,
            emission_tex.image_view,
            albedo_tex.image_sampler,
        );

        // Keep texture refs alive
        let texture_refs = vec![
            Rc::clone(&albedo_tex),
            Rc::clone(&normal_tex),
            Rc::clone(&mr_tex),
            Rc::clone(&occlusion_tex),
            Rc::clone(&emission_tex),
        ];

        // Create material based on skinning detection and bindless mode
        let material = {
            if has_skinning {
                // Skinned mesh material selection
                if use_bindless {
                    // Bindless mode - use bindless skinned template with full PBR
                    if let Some(template) = material_registry
                        .borrow()
                        .get_template("gltf_skinned_pbr_bindless")
                    {
                        info!("  Using gltf_skinned_pbr_bindless template");
                        Material::from_template_skinned_pbr_bindless(
                            template,
                            pbr_textures,
                            texture_refs,
                            None,
                            texture_indices,
                            emission_index,
                        )
                    } else {
                        warn!("  Template 'gltf_skinned_pbr_bindless' not found, falling back to gltf_skinned");
                        // Fallback to albedo-only skinned template
                        if let Some(template) =
                            material_registry.borrow().get_template("gltf_skinned")
                        {
                            Material::from_template_skinned_with_bindless(
                                template,
                                Some(albedo_tex),
                                None,
                                texture_indices,
                                emission_index,
                            )
                        } else {
                            panic!("Neither 'gltf_skinned_pbr_bindless' nor 'gltf_skinned' templates found. Ensure materials are loaded.");
                        }
                    }
                } else {
                    // Legacy mode - use per-material texture descriptors
                    // TODO: Create a gltf_skinned_pbr_full template for full PBR on skinned models
                    if let Some(template) = material_registry.borrow().get_template("gltf_skinned")
                    {
                        info!("  Using gltf_skinned template (albedo only)");
                        Material::from_template_skinned(template, Some(albedo_tex), None)
                    } else {
                        warn!("  Template 'gltf_skinned' not found, falling back to gltf_default");
                        if let Some(template) =
                            material_registry.borrow().get_template("gltf_default")
                        {
                            Material::from_template_with_optional_texture(
                                template,
                                Some(albedo_tex),
                                None,
                            )
                        } else {
                            panic!("Neither 'gltf_skinned' nor 'gltf_default' templates found. Ensure materials are loaded.");
                        }
                    }
                }
            } else {
                // Use full PBR template for static models
                if use_bindless {
                    // Bindless mode - use bindless templates
                    if let Some(template) =
                        material_registry.borrow().get_template("gltf_pbr_bindless")
                    {
                        info!("  Using gltf_pbr_bindless template");
                        Material::from_template_pbr_bindless(
                            template,
                            pbr_textures,
                            texture_refs,
                            None,
                            texture_indices,
                            emission_index,
                        )
                    } else {
                        warn!("  Template 'gltf_pbr_bindless' not found, falling back to gltf_default");
                        if let Some(template) =
                            material_registry.borrow().get_template("gltf_default")
                        {
                            Material::from_template_with_optional_texture(
                                template,
                                Some(albedo_tex),
                                None,
                            )
                        } else {
                            panic!("Neither 'gltf_pbr_bindless' nor 'gltf_default' templates found. Ensure materials are loaded.");
                        }
                    }
                } else {
                    // Legacy mode - now also uses bindless (all materials are bindless)
                    if let Some(template) =
                        material_registry.borrow().get_template("gltf_pbr_bindless")
                    {
                        info!("  Using gltf_pbr_bindless template");
                        Material::from_template_pbr_bindless(
                            template,
                            pbr_textures,
                            texture_refs,
                            None,
                            texture_indices,
                            emission_index,
                        )
                    } else {
                        warn!("  Template 'gltf_pbr_bindless' not found, falling back to gltf_default");
                        if let Some(template) =
                            material_registry.borrow().get_template("gltf_default")
                        {
                            Material::from_template_with_optional_texture(
                                template,
                                Some(albedo_tex),
                                None,
                            )
                        } else {
                            panic!("Neither 'gltf_pbr_bindless' nor 'gltf_default' templates found. Ensure materials are loaded.");
                        }
                    }
                }
            }
        };

        // Get root transform before moving model to mesh creation
        let root_transform = model.root_transform.clone();

        // Create appropriate mesh type
        let mesh = if has_skinning {
            Mesh::new_skinned_from_model(model, context.clone())
        } else {
            Mesh::new_from_model(model, context.clone())
        };
        log::info!("Root is: {:?}", root_transform);
        // Combine user transform with model's root transform from GLTF
        // Root transform is applied first (model space), then user transform
        let combined_matrix = transform.make_mat4() * root_transform;
        let final_transform = combined_matrix.decompose();

        Self::new_with_pbr(
            world,
            vec![mesh],
            material,
            renderer,
            final_transform,
            None,
            metallic,
            roughness,
            1.0,
        )
    }

    /// Load a texture from GLTF image data.
    ///
    /// Returns None if the format is unsupported.
    pub fn load_texture_from_gltf(
        image: &gltf::image::Data,
        context: &Rc<VulkanContext>,
    ) -> Option<Rc<katla_vulkan::Texture>> {
        match image.format {
            gltf::image::Format::R8G8B8 => Some(Rc::new(katla_vulkan::Texture::create_image_rgb(
                context.clone(),
                image.width,
                image.height,
                &image.pixels,
            ))),
            gltf::image::Format::R8G8B8A8 => Some(Rc::new(katla_vulkan::Texture::create_image(
                context.clone(),
                image.width,
                image.height,
                katla_vulkan::ImageFormat::R8G8B8A8Srgb,
                &image.pixels,
            ))),
            _ => {
                debug!("Unsupported texture format: {:?}", image.format);
                None
            }
        }
    }
}
