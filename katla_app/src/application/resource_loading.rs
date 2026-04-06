use katla_gfx::TextureDescriptor;
use log::info;

use crate::animation::AnimationClip;
use crate::animation::gltf_loader::load_animation_clip;
use crate::error::{AppError, AppResult};
use crate::util::{GLTFModel, gltf_parser::AttributeParser};

impl super::Application {
    /// Load a texture from an image file (PNG, JPEG, etc.) and upload to GPU.
    ///
    /// The texture is created in sRGB color space, suitable for albedo/emission maps.
    /// For non-color data (normals, roughness), use `load_texture_unorm`.
    ///
    /// Returns a [`TextureHandle`] that can be used as a bindless texture index
    /// via [`VulkanRenderer::get_texture_bindless_index`].
    pub fn load_texture(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> AppResult<katla_gfx::TextureHandle> {
        let path = path.as_ref();
        let img = image::open(path).map_err(|e| AppError::Other {
            message: format!("Failed to load image '{}': {}", path.display(), e),
        })?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        let desc = TextureDescriptor::rgba8_srgb(width, height);
        let handle = self.renderer.create_texture(&desc, rgba.as_raw());

        info!(
            "Loaded texture '{}' ({}x{}) -> handle {}",
            path.display(),
            width,
            height,
            handle.index()
        );

        Ok(handle)
    }

    /// Load a texture from an image file in linear (UNORM) color space.
    ///
    /// Use this for non-color data textures such as normal maps, roughness maps,
    /// and ambient occlusion maps where the pixel values should not be gamma-corrected.
    pub fn load_texture_unorm(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> AppResult<katla_gfx::TextureHandle> {
        let path = path.as_ref();
        let img = image::open(path).map_err(|e| AppError::Other {
            message: format!("Failed to load image '{}': {}", path.display(), e),
        })?;
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();

        let desc = TextureDescriptor::rgba8_unorm(width, height);
        let handle = self.renderer.create_texture(&desc, rgba.as_raw());

        info!(
            "Loaded UNORM texture '{}' ({}x{}) -> handle {}",
            path.display(),
            width,
            height,
            handle.index()
        );

        Ok(handle)
    }

    /// Load a GLTF/GLB mesh from disk and upload vertex/index data to the GPU.
    ///
    /// Returns a [`MeshHandle`] for the loaded geometry. The handle can be used
    /// with [`Spawner::spawn_primitive`] or [`DrawableComponent::with_handles`]
    /// to create renderable entities.
    ///
    /// This does **not** spawn an entity or set up materials/textures/skinning.
    /// For full GLTF import with textures and animation, use
    /// [`Application::spawn_gltf_model`].
    pub fn load_mesh(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> AppResult<katla_gfx::MeshHandle> {
        let path_ref = path.as_ref();
        let path_buf = path_ref.to_path_buf();
        let model = GLTFModel::new(&path_buf).map_err(|e| AppError::ModelLoadFailed {
            path: path_ref.to_string_lossy().to_string(),
            reason: format!("{}", e),
        })?;

        let vertex_count = if model.has_skinning {
            model.skinned_vertex_data.len()
        } else {
            model.vertex_data.len()
        };

        let indices = Self::convert_indices_to_u32_with_vertex_count(
            &model.index_data,
            model.index_stride,
            vertex_count,
        );

        let mesh_handle = if model.has_skinning {
            self.renderer.create_mesh_soa(
                &model.skinned_vertex_attributes,
                model.skinned_vertex_data.len() as u32,
                &indices,
            )
        } else {
            self.renderer.create_mesh_soa(
                &model.vertex_attributes,
                model.vertex_data.len() as u32,
                &indices,
            )
        };

        info!(
            "Loaded mesh '{}' ({} vertices, {} indices, skinned={}) -> handle {}",
            path_ref.display(),
            vertex_count,
            indices.len(),
            model.has_skinning,
            mesh_handle.index()
        );

        Ok(mesh_handle)
    }

    /// Load an animation clip by name from a GLTF/GLB file.
    ///
    /// If `clip_name` is `None`, loads the first animation clip in the file.
    /// Returns the parsed [`AnimationClip`] containing all channels (translation,
    /// rotation, scale, morph target weights) and their sample data.
    ///
    /// # Errors
    ///
    /// Returns `AppError::ModelLoadFailed` if the GLTF file cannot be loaded.
    /// Returns `AppError::Other` if no animation with the given name exists.
    pub fn load_animation(
        &mut self,
        path: impl AsRef<std::path::Path>,
        clip_name: Option<&str>,
    ) -> AppResult<AnimationClip> {
        let path_ref = path.as_ref();
        let path_buf = path_ref.to_path_buf();
        let model = GLTFModel::new(&path_buf).map_err(|e| AppError::ModelLoadFailed {
            path: path_ref.to_string_lossy().to_string(),
            reason: format!("{}", e),
        })?;

        let animations: Vec<_> = model.document.animations().collect();
        if animations.is_empty() {
            return Err(AppError::Other {
                message: format!("No animations found in '{}'", path_ref.to_string_lossy()),
            });
        }

        let parser = AttributeParser::new(&model.buffers);

        if let Some(name) = clip_name {
            for gltf_animation in &animations {
                let anim_name = gltf_animation.name().unwrap_or("Animation_0").to_string();
                if anim_name == name {
                    let clip = load_animation_clip(&parser, gltf_animation);
                    info!(
                        "Loaded animation '{}' from '{}' ({:.2}s, {} channels)",
                        name,
                        path_ref.display(),
                        clip.duration,
                        clip.channels.len()
                    );
                    return Ok(clip);
                }
            }
            Err(AppError::Other {
                message: format!(
                    "Animation '{}' not found in '{}'",
                    name,
                    path_ref.to_string_lossy()
                ),
            })
        } else {
            let clip = load_animation_clip(&parser, &animations[0]);
            let name = animations[0].name().unwrap_or("Animation_0");
            info!(
                "Loaded first animation '{}' from '{}' ({:.2}s, {} channels)",
                name,
                path_ref.display(),
                clip.duration,
                clip.channels.len()
            );
            Ok(clip)
        }
    }
}
