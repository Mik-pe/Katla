use std::rc::Rc;

use katla_ecs::{EntityId, World};
use katla_gfx::{
    ImageFormat, MaterialHandle, MaterialRegistry, MeshHandle, TextureDescriptor, TextureHandle,
    TextureManager, VulkanContext, VulkanRenderer,
};
use katla_math::Transform;
use log::{debug, info, warn};

use crate::{
    components::{DrawableComponent, NameComponent, TransformComponent},
    rendering::{Material, Mesh},
    util::GLTFModel,
};

pub struct Model {
    pub entity: EntityId,
    /// Handle after registration (MeshHandle::NONE if no renderer provided)
    pub mesh_handle: MeshHandle,
    /// Handle after registration (MaterialHandle::NONE if no renderer provided)
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
        renderer: &mut VulkanRenderer,
    ) -> (MeshHandle, MaterialHandle) {
        // Register mesh - take buffers from first mesh
        let mesh_h = if let Some(mesh) = first_mesh {
            let vertex_buffer = mesh.vertex_buffer.take();
            let index_buffer = mesh.index_buffer.take();
            renderer.register_mesh(vertex_buffer, index_buffer)
        } else {
            MeshHandle::NONE
        };

        // Register material with optional PBR textures
        let (
            pipeline,
            _texture,
            vertex_binding,
            pbr_textures,
            texture_indices,
            emission_index,
            is_bindless,
        ) = material.get_registration_data();

        // Vertex binding and pipeline are required for material registration
        let vertex_binding = vertex_binding.expect("Material must have vertex binding");

        // Use PBR registration if PBR textures are present
        let mat_h = if let Some(pbr) = pbr_textures {
            renderer.register_material_pbr(
                pipeline,
                vertex_binding,
                is_bindless,
                pbr,
                texture_indices,
                emission_index,
            )
        } else {
            renderer.register_material_full(
                pipeline,
                vertex_binding,
                is_bindless,
                texture_indices,
                emission_index,
            )
        };

        (mesh_h, mat_h)
    }

    /// Create a model with explicit PBR material values.
    pub fn new_with_pbr(
        world: &mut World,
        mut meshes: Vec<Mesh>,
        material: Material,
        renderer: &mut VulkanRenderer,
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
        renderer: &mut VulkanRenderer,
        transform: Transform,
        material_registry: &std::cell::RefCell<MaterialRegistry>,
    ) -> Self {
        use katla_gfx::material::PbrTextureSet;

        // Get default slot indices from renderer
        let defaults = renderer.bindless_defaults();

        // Check if model has skinning
        let has_skinning = model.has_skinning;

        // Get default texture handles (static, don't need mutable access)
        let (default_handles, loaded_handles) = {
            // Get TextureManager from renderer - required for texture creation
            let texture_manager = renderer.texture_manager_mut();

            // Get default texture handles from TextureManager
            let defaults = (
                texture_manager.default_white(),
                texture_manager.default_normal(),
                texture_manager.default_metallic_roughness(),
                texture_manager.default_occlusion(),
                texture_manager.default_emission(),
            );

            // Get material info from GLTF
            let mat_info = model.materials.first();

            // Load all textures using TextureManager (use defaults for missing maps)
            let albedo = Self::load_texture_from_gltf_with_manager_opt(
                mat_info
                    .and_then(|m| m.base_color_texture)
                    .and_then(|idx| model.images.get(idx)),
                texture_manager,
                defaults.0,
            );
            let normal = Self::load_texture_from_gltf_with_manager_opt(
                mat_info
                    .and_then(|m| m.normal_texture)
                    .and_then(|idx| model.images.get(idx)),
                texture_manager,
                defaults.1,
            );
            let mr = Self::load_texture_from_gltf_with_manager_opt(
                mat_info
                    .and_then(|m| m.metallic_roughness_texture)
                    .and_then(|idx| model.images.get(idx)),
                texture_manager,
                defaults.2,
            );
            let occlusion = Self::load_texture_from_gltf_with_manager_opt(
                mat_info
                    .and_then(|m| m.occlusion_texture)
                    .and_then(|idx| model.images.get(idx)),
                texture_manager,
                defaults.3,
            );
            let emission = Self::load_texture_from_gltf_with_manager_opt(
                mat_info
                    .and_then(|m| m.emission_texture)
                    .and_then(|idx| model.images.get(idx)),
                texture_manager,
                defaults.4,
            );

            (defaults, (albedo, normal, mr, occlusion, emission))
        };

        let (
            default_albedo_handle,
            default_normal_handle,
            default_mr_handle,
            default_occlusion_handle,
            default_emission_handle,
        ) = default_handles;
        let (albedo_handle, normal_handle, mr_handle, occlusion_handle, emission_handle) =
            loaded_handles;

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
                metallic,
                roughness
            );
        }

        // Register textures with bindless manager and get indices
        let (texture_indices, emission_index, albedo_tex_rc) = {
            // Get views from TextureManager
            let tm = renderer.texture_manager();
            let views = (
                tm.get_view(albedo_handle),
                tm.get_view(normal_handle),
                tm.get_view(mr_handle),
                tm.get_view(occlusion_handle),
                tm.get_view(emission_handle),
                tm.get_texture_rc(albedo_handle),
            );

            match views {
                (
                    Some(albedo_view),
                    Some(normal_view),
                    Some(mr_view),
                    Some(ao_view),
                    Some(emiss_view),
                    albedo_rc,
                ) => {
                    // Get bindless_manager and register textures
                    let bindless = renderer.bindless_manager_mut();
                    let albedo_idx = bindless
                        .register_texture(albedo_view)
                        .unwrap_or(defaults.albedo);
                    let normal_idx = bindless
                        .register_texture(normal_view)
                        .unwrap_or(defaults.normal);
                    let mr_idx = bindless
                        .register_texture(mr_view)
                        .unwrap_or(defaults.metallic_roughness);
                    let ao_idx = bindless
                        .register_texture(ao_view)
                        .unwrap_or(defaults.occlusion);
                    let emiss_idx = bindless
                        .register_texture(emiss_view)
                        .unwrap_or(defaults.emission);

                    debug!(
                        "  Bindless texture slots: albedo={}, normal={}, mr={}, ao={}, emission={}",
                        albedo_idx, normal_idx, mr_idx, ao_idx, emiss_idx
                    );

                    // Track bindless slots in TextureManager
                    let tm = renderer.texture_manager_mut();
                    tm.register_bindless_slot(albedo_handle, albedo_idx);
                    tm.register_bindless_slot(normal_handle, normal_idx);
                    tm.register_bindless_slot(mr_handle, mr_idx);
                    tm.register_bindless_slot(occlusion_handle, ao_idx);
                    tm.register_bindless_slot(emission_handle, emiss_idx);

                    (
                        [albedo_idx, normal_idx, mr_idx, ao_idx],
                        emiss_idx,
                        albedo_rc,
                    )
                }
                _ => (
                    [
                        defaults.albedo,
                        defaults.normal,
                        defaults.metallic_roughness,
                        defaults.occlusion,
                    ],
                    defaults.emission,
                    None,
                ),
            }
        };

        // Create PbrTextureSet with the loaded texture handles
        let pbr_textures = PbrTextureSet::new(
            albedo_handle,
            normal_handle,
            mr_handle,
            occlusion_handle,
            emission_handle,
        );

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
    ) -> Option<Rc<katla_gfx::Texture>> {
        match image.format {
            gltf::image::Format::R8G8B8 => Some(Rc::new(katla_gfx::Texture::create_image_rgb(
                context.clone(),
                image.width,
                image.height,
                &image.pixels,
            ))),
            gltf::image::Format::R8G8B8A8 => Some(Rc::new(katla_gfx::Texture::create_image(
                context.clone(),
                image.width,
                image.height,
                ImageFormat::R8G8B8A8Srgb,
                &image.pixels,
            ))),
            _ => {
                debug!("Unsupported texture format: {:?}", image.format);
                None
            }
        }
    }

    /// Load a texture from GLTF image data using TextureManager.
    ///
    /// Returns a TextureHandle for the loaded texture.
    /// Returns None if the format is unsupported.
    pub fn load_texture_from_gltf_with_manager(
        image: &gltf::image::Data,
        texture_manager: &mut TextureManager,
    ) -> Option<TextureHandle> {
        match image.format {
            gltf::image::Format::R8G8B8 => {
                Some(texture_manager.create_from_rgb(image.width, image.height, &image.pixels))
            }
            gltf::image::Format::R8G8B8A8 => {
                let desc = TextureDescriptor::rgba8_srgb(image.width, image.height);
                Some(texture_manager.create(&desc, &image.pixels))
            }
            _ => {
                debug!("Unsupported texture format: {:?}", image.format);
                None
            }
        }
    }

    /// Load a texture from optional GLTF image data using TextureManager.
    ///
    /// Returns the loaded texture handle or the default handle if:
    /// - No image is provided
    /// - The format is unsupported
    fn load_texture_from_gltf_with_manager_opt(
        image: Option<&gltf::image::Data>,
        texture_manager: &mut TextureManager,
        default_handle: TextureHandle,
    ) -> TextureHandle {
        match image {
            Some(image) => Self::load_texture_from_gltf_with_manager(image, texture_manager)
                .unwrap_or(default_handle),
            None => default_handle,
        }
    }
}
