use std::rc::Rc;

use katla_ecs::{EntityId, World};
use katla_math::{Mat4, Transform};
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
        let (mesh_handle, material_handle) = Self::register_with_renderer(meshes.first_mut(), material, renderer);

        // Create DrawableComponent with optional color and PBR values
        let drawable = DrawableComponent::with_handles_and_material(
            mesh_handle,
            material_handle,
            color,
            0.0,   // metallic - will be overridden by GLTF loading
            0.5,   // roughness - will be overridden by GLTF loading
            1.0,   // ao
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

            // Register material with optional per-material uniform buffer
            let (pipeline, texture, vertex_binding, uniform, pbr_textures, pbr_texture_refs) =
                material.get_registration_data();

            // Use PBR registration if PBR textures are present
            let mat_h = if let Some(pbr) = pbr_textures {
                let tex_refs = pbr_texture_refs.unwrap_or_default();
                r.register_material_pbr(pipeline, texture, vertex_binding, uniform, pbr, tex_refs)
            } else {
                r.register_material_full(pipeline, texture, vertex_binding, uniform)
            };

            (mesh_h, mat_h)
        } else {
            (MeshHandle(0), MaterialHandle(0))
        }
    }

    pub fn new_from_gltf(
        world: &mut World,
        model: Rc<GLTFModel>,
        context: Rc<VulkanContext>,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
        material_registry: Option<&std::cell::RefCell<MaterialRegistry>>,
    ) -> Self {
        // Try to create material from template first
        // Use provided registry, or get from renderer
        let registry_ref = if let Some(registry) = material_registry {
            Some(registry)
        } else {
            renderer.as_ref().map(|r| &r.material_registry)
        };

        let material = if let Some(registry) = registry_ref {
            let registry = registry.borrow();
            // Try to get "gltf_default" template
            if let Some(template) = registry.get_template("gltf_default") {
                // Get the correct texture index from material info
                // Fall back to image 0 if no material info or no base color texture
                let texture_index = model.materials.first()
                    .and_then(|m| m.base_color_texture)
                    .unwrap_or(0);

                // Extract texture from the GLTF model using the correct index
                let texture = model.images.get(texture_index)
                    .and_then(|image| Self::load_texture_from_gltf(image, &context));

                // Create material from template
                Material::from_template(template, texture, None)
            } else {
                // Fallback to direct creation if template not found
                Material::new(model.clone(), context.clone())
            }
        } else {
            // No registry provided, use direct creation
            Material::new(model.clone(), context.clone())
        };

        // Get PBR values from material info
        let (metallic, roughness) = model.materials.first()
            .map(|m| (m.metallic_factor, m.roughness_factor))
            .unwrap_or((0.0, 0.5));

        let mesh = Mesh::new_from_model(model, context.clone());

        Self::new_with_pbr(world, vec![mesh], material, renderer, transform, None, metallic, roughness, 1.0)
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
        let (mesh_handle, material_handle) = Self::register_with_renderer(meshes.first_mut(), material, renderer);

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
    pub fn from_gltf(
        world: &mut World,
        model: Rc<GLTFModel>,
        context: Rc<VulkanContext>,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
        material_registry: &std::cell::RefCell<MaterialRegistry>,
    ) -> Self {
        use katla_vulkan::material::PbrTextureSet;

        // Check if model has skinning
        let has_skinning = model.has_skinning;

        // Helper to load a texture from a GLTF image index
        let load_texture = |image_index: Option<usize>, default_texture: &Rc<katla_vulkan::Texture>| -> Rc<katla_vulkan::Texture> {
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
        let default_albedo = Rc::new(katla_vulkan::Texture::create_default_albedo(context.clone()));
        let default_normal = Rc::new(katla_vulkan::Texture::create_default_normal(context.clone()));
        let default_mr = Rc::new(katla_vulkan::Texture::create_default_metallic_roughness(context.clone()));
        let default_occlusion = Rc::new(katla_vulkan::Texture::create_default_occlusion(context.clone()));
        let default_emission = Rc::new(katla_vulkan::Texture::create_default_emission(context.clone()));

        // Get material info from GLTF
        let mat_info = model.materials.first();
        let (metallic, roughness) = mat_info
            .map(|m| (m.metallic_factor, m.roughness_factor))
            .unwrap_or((0.0, 0.5));

        // Log material detection
        if let Some(mat) = mat_info {
            info!(
                "GLTF import: {} skinning, material: {}",
                if has_skinning { "with" } else { "no" },
                mat.summary()
            );
        } else {
            info!(
                "GLTF import: {} skinning, no material info (using defaults: M={:.2}, R={:.2})",
                if has_skinning { "with" } else { "no" },
                metallic, roughness
            );
        }

        // Load all textures (use defaults for missing maps)
        let albedo_tex = load_texture(
            mat_info.and_then(|m| m.base_color_texture),
            &default_albedo,
        );
        let normal_tex = load_texture(
            mat_info.and_then(|m| m.normal_texture),
            &default_normal,
        );
        let mr_tex = load_texture(
            mat_info.and_then(|m| m.metallic_roughness_texture),
            &default_mr,
        );
        let occlusion_tex = load_texture(
            mat_info.and_then(|m| m.occlusion_texture),
            &default_occlusion,
        );
        let emission_tex = load_texture(
            mat_info.and_then(|m| m.emission_texture),
            &default_emission,
        );

        // Create PbrTextureSet
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

        // Create material based on skinning detection
        let material = {
            if has_skinning {
                // Use skinned shader template (currently only supports albedo texture)
                // TODO: Create a gltf_skinned_pbr_full template for full PBR on skinned models
                if let Some(template) = material_registry.borrow().get_template("gltf_skinned") {
                    info!("  Using gltf_skinned template");
                    Material::from_template_skinned(template, Some(albedo_tex), None)
                } else {
                    warn!("  Template 'gltf_skinned' not found, falling back to gltf_default");
                    if let Some(template) = material_registry.borrow().get_template("gltf_default") {
                        Material::from_template(template, Some(albedo_tex), None)
                    } else {
                        Material::new(model.clone(), context.clone())
                    }
                }
            } else {
                // Use full PBR template for static models
                if let Some(template) = material_registry.borrow().get_template("gltf_pbr_full") {
                    info!("  Using gltf_pbr_full template");
                    Material::from_template_pbr(template, pbr_textures, texture_refs, None)
                } else {
                    warn!("  Template 'gltf_pbr_full' not found, falling back to gltf_default");
                    if let Some(template) = material_registry.borrow().get_template("gltf_default") {
                        Material::from_template(template, Some(albedo_tex), None)
                    } else {
                        Material::new(model.clone(), context.clone())
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

        // Combine user transform with model's root transform from GLTF
        // Root transform is applied first (model space), then user transform
        let combined_matrix = transform.make_mat4() * root_transform;
        let final_transform = combined_matrix.decompose();

        Self::new_with_pbr(world, vec![mesh], material, renderer, final_transform, None, metallic, roughness, 1.0)
    }

    /// Load a texture from GLTF image data.
    ///
    /// Returns None if the format is unsupported.
    pub fn load_texture_from_gltf(
        image: &gltf::image::Data,
        context: &Rc<VulkanContext>,
    ) -> Option<Rc<katla_vulkan::Texture>> {
        match image.format {
            gltf::image::Format::R8G8B8 => Some(Rc::new(
                katla_vulkan::Texture::create_image_rgb(
                    context.clone(),
                    image.width,
                    image.height,
                    &image.pixels,
                ),
            )),
            gltf::image::Format::R8G8B8A8 => Some(Rc::new(
                katla_vulkan::Texture::create_image(
                    context.clone(),
                    image.width,
                    image.height,
                    katla_vulkan::ImageFormat::R8G8B8A8Srgb,
                    &image.pixels,
                ),
            )),
            _ => {
                debug!("Unsupported texture format: {:?}", image.format);
                None
            }
        }
    }
}
