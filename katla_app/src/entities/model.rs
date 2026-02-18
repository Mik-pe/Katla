use std::rc::Rc;

use katla_ecs::{EntityId, World};
use katla_math::Transform;
use katla_vulkan::{MaterialHandle, MaterialRegistry, MeshHandle, VulkanContext, VulkanRenderer};
use log::{info, warn};

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
        let (mesh_handle, material_handle) = if let Some(r) = renderer {
            // Register mesh - take buffers from first mesh
            // Note: For now we only support single-mesh models
            let mesh_h = if let Some(first_mesh) = meshes.first_mut() {
                let vertex_buffer = first_mesh.vertex_buffer.take();
                let index_buffer = first_mesh.index_buffer.take();
                r.register_mesh(vertex_buffer, index_buffer)
            } else {
                MeshHandle(0) // Dummy handle if no meshes
            };

            // Register material with optional per-material uniform buffer
            let (pipeline, texture, vertex_binding, uniform) = material.get_registration_data();
            let mat_h = r.register_material_full(
                pipeline,
                texture,
                vertex_binding,
                uniform, // Move ownership to renderer
            );

            (mesh_h, mat_h)
        } else {
            // Use dummy handles when no renderer provided
            (MeshHandle(0), MaterialHandle(0))
        };

        // Create DrawableComponent with optional color
        let drawable = if let Some(c) = color {
            DrawableComponent::with_handles_and_color(mesh_handle, material_handle, c)
        } else {
            DrawableComponent::with_handles(mesh_handle, material_handle)
        };

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
                let texture = if texture_index < model.images.len() {
                    let image = &model.images[texture_index];
                    let pixels = &image.pixels;

                    match image.format {
                        gltf::image::Format::R8G8B8 => {
                            let tex = katla_vulkan::Texture::create_image_rgb(
                                context.clone(),
                                image.width,
                                image.height,
                                pixels.as_slice(),
                            );
                            Some(Rc::new(tex))
                        }
                        gltf::image::Format::R8G8B8A8 => {
                            let tex = katla_vulkan::Texture::create_image(
                                context.clone(),
                                image.width,
                                image.height,
                                katla_vulkan::ImageFormat::R8G8B8A8Srgb,
                                pixels.as_slice(),
                            );
                            Some(Rc::new(tex))
                        }
                        _ => {
                            info!("Unsupported texture format: {:?}", image.format);
                            None
                        }
                    }
                } else {
                    None
                };

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

        let mesh = Mesh::new_from_model(model, context.clone());

        Self::new(world, vec![mesh], material, renderer, transform, None)
    }
    /// Create a GLTF model using a raw pointer to MaterialRegistry.
    ///
    /// This version avoids borrow checker issues by using a raw pointer,
    /// similar to the MeshBuilder approach.
    ///
    /// # Safety
    /// The registry_ptr must point to a valid MaterialRegistry that outlives
    /// this function call.
    pub(crate) fn new_from_gltf_with_ptr(
        world: &mut World,
        model: Rc<GLTFModel>,
        context: Rc<VulkanContext>,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
        material_registry_ptr: *const std::cell::RefCell<MaterialRegistry>,
    ) -> Self {
        // SAFETY: The raw pointer points to the MaterialRegistry in VulkanRenderer
        // which is guaranteed to be valid for the lifetime of the application.
        // We only access it during this function call.
        let material = unsafe {
            let registry = &*material_registry_ptr;

            // Try to get the "gltf_default" template
            if let Some(template) = registry.borrow().get_template("gltf_default") {
                // Get the correct texture index from material info
                // Fall back to image 0 if no material info or no base color texture
                let texture_index = model.materials.first()
                    .and_then(|m| m.base_color_texture)
                    .unwrap_or(0);

                // Extract texture from the GLTF model using the correct index
                let texture = if texture_index < model.images.len() {
                    let image = &model.images[texture_index];
                    let pixels = &image.pixels;

                    match image.format {
                        gltf::image::Format::R8G8B8 => {
                            let tex = katla_vulkan::Texture::create_image_rgb(
                                context.clone(),
                                image.width,
                                image.height,
                                pixels.as_slice(),
                            );
                            Some(Rc::new(tex))
                        }
                        gltf::image::Format::R8G8B8A8 => {
                            let tex = katla_vulkan::Texture::create_image(
                                context.clone(),
                                image.width,
                                image.height,
                                katla_vulkan::ImageFormat::R8G8B8A8Srgb,
                                pixels.as_slice(),
                            );
                            Some(Rc::new(tex))
                        }
                        _ => {
                            info!("Unsupported texture format: {:?}", image.format);
                            None
                        }
                    }
                } else {
                    None
                };

                // Create material from template
                Material::from_template(template, texture, None)
            } else {
                info!("  Model: Template 'gltf_default' not found, creating directly");
                // Fall back to direct creation if template not found
                Material::new(model.clone(), context.clone())
            }
        };

        let mesh = Mesh::new_from_model(model, context.clone());

        Self::new(world, vec![mesh], material, renderer, transform, None)
    }

    /// Create a skinned GLTF model with skeletal animation support.
    ///
    /// This uses the skinned shader and vertex format for GPU skeletal animation.
    ///
    /// # Safety
    /// The registry_ptr must point to a valid MaterialRegistry that outlives
    /// this function call.
    pub(crate) fn new_skinned_from_gltf_with_ptr(
        world: &mut World,
        model: Rc<GLTFModel>,
        context: Rc<VulkanContext>,
        renderer: Option<&mut VulkanRenderer>,
        transform: Transform,
        material_registry_ptr: *const std::cell::RefCell<MaterialRegistry>,
    ) -> Self {
        // SAFETY: The raw pointer points to the MaterialRegistry in VulkanRenderer
        // which is guaranteed to be valid for the lifetime of the application.
        let material = unsafe {
            let registry = &*material_registry_ptr;

            // Try to get the "gltf_skinned" template for animated models
            if let Some(template) = registry.borrow().get_template("gltf_skinned") {
                info!("  Model: Using skinned material template");

                // Extract texture from the GLTF model
                let texture = if !model.images.is_empty() {
                    let image = &model.images[0];
                    let pixels = &image.pixels;

                    match image.format {
                        gltf::image::Format::R8G8B8 => {
                            let tex = katla_vulkan::Texture::create_image_rgb(
                                context.clone(),
                                image.width,
                                image.height,
                                pixels.as_slice(),
                            );
                            Some(Rc::new(tex))
                        }
                        gltf::image::Format::R8G8B8A8 => {
                            let tex = katla_vulkan::Texture::create_image(
                                context.clone(),
                                image.width,
                                image.height,
                                katla_vulkan::ImageFormat::R8G8B8A8Srgb,
                                pixels.as_slice(),
                            );
                            Some(Rc::new(tex))
                        }
                        _ => None,
                    }
                } else {
                    None
                };

                // Create material from template with skinned vertex binding
                Material::from_template_skinned(template, texture, None)
            } else {
                info!("  Model: Template 'gltf_skinned' not found, falling back to default");
                // Fall back to default template
                if let Some(template) = registry.borrow().get_template("gltf_default") {
                    let texture = if !model.images.is_empty() {
                        let image = &model.images[0];
                        let pixels = &image.pixels;

                        match image.format {
                            gltf::image::Format::R8G8B8 => {
                                let tex = katla_vulkan::Texture::create_image_rgb(
                                    context.clone(),
                                    image.width,
                                    image.height,
                                    pixels.as_slice(),
                                );
                                Some(Rc::new(tex))
                            }
                            gltf::image::Format::R8G8B8A8 => {
                                let tex = katla_vulkan::Texture::create_image(
                                    context.clone(),
                                    image.width,
                                    image.height,
                                    katla_vulkan::ImageFormat::R8G8B8A8Srgb,
                                    pixels.as_slice(),
                                );
                                Some(Rc::new(tex))
                            }
                            _ => None,
                        }
                    } else {
                        None
                    };
                    Material::from_template(template, texture, None)
                } else {
                    Material::new(model.clone(), context.clone())
                }
            }
        };

        // Create skinned mesh with joint indices and weights
        let mesh = Mesh::new_skinned_from_model(model, context.clone());

        Self::new(world, vec![mesh], material, renderer, transform, None)
    }
}
