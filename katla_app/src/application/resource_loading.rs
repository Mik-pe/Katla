use std::io::Read as _;

use katla_gfx::GpuRenderer;
use katla_gfx::TextureDescriptor;
use log::info;

use crate::animation::AnimationClip;
use crate::animation::gltf_loader::load_animation_clip;
use crate::error::{AppError, AppResult};
use crate::util::StlMesh;
use crate::util::{GLTFModel, gltf_parser::AttributeParser};

impl super::Application {
    /// Load a texture from an image file (PNG, JPEG, etc.) and upload to GPU.
    ///
    /// The texture is created in sRGB color space, suitable for albedo/emission maps.
    /// For non-color data (normals, roughness), use `load_texture_unorm`.
    ///
    /// Returns a [`TextureHandle`] that can be used as a bindless texture index
    /// via [`GpuRenderer::get_bindless_slot`].
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
        let handle = self
            .renderer
            .create_texture(&desc, rgba.as_raw())
            .map_err(|e| AppError::Graphics { source: e })?;

        info!(
            "Loaded texture '{}' ({}x{}) -> handle {}",
            path.display(),
            width,
            height,
            handle.index()
        );

        #[cfg(feature = "editor")]
        {
            self.editor.texture_paths.insert(path.to_path_buf(), handle);
        }

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
        let handle = self
            .renderer
            .create_texture(&desc, rgba.as_raw())
            .map_err(|e| AppError::Graphics { source: e })?;

        info!(
            "Loaded UNORM texture '{}' ({}x{}) -> handle {}",
            path.display(),
            width,
            height,
            handle.index()
        );

        #[cfg(feature = "editor")]
        {
            self.editor.texture_paths.insert(path.to_path_buf(), handle);
        }

        Ok(handle)
    }

    /// Upload one selected-scene node primitive from a glTF/GLB asset.
    ///
    /// Node and primitive indices must exist in the selected scene. Returns a
    /// [`katla_gfx::MeshHandle`] for that primitive's geometry. The handle can be used
    /// with [`Spawner::spawn_primitive`](crate::spawner::Spawner::spawn_primitive)
    /// or [`DrawableComponent::with_handles`](katla_app::components::rendering::DrawableComponent::with_handles)
    /// to create renderable entities.
    ///
    /// This does **not** spawn an entity or set up materials/textures/skinning.
    /// For full GLTF import with textures and animation, use
    /// [`Application::spawn_gltf_model`](crate::application::Application::spawn_gltf_model).
    pub fn load_mesh(
        &mut self,
        path: impl AsRef<std::path::Path>,
        node_index: usize,
        primitive_index: usize,
    ) -> AppResult<katla_gfx::MeshHandle> {
        let path_ref = path.as_ref();
        let path_buf = path_ref.to_path_buf();
        let model = GLTFModel::new(&path_buf).map_err(|e| AppError::ModelLoadFailed {
            path: path_ref.to_string_lossy().to_string(),
            reason: format!("{}", e),
        })?;

        let primitive = model
            .primitives
            .iter()
            .find(|primitive| {
                primitive.node_index == node_index && primitive.primitive_index == primitive_index
            })
            .ok_or_else(|| AppError::ModelLoadFailed {
                path: path_ref.to_string_lossy().into_owned(),
                reason: format!(
                    "Selected scene has no node {node_index} primitive {primitive_index}"
                ),
            })?;
        let mesh_handle = self.upload_gltf_primitive_mesh(primitive)?;
        Ok(mesh_handle)
    }

    pub(crate) fn upload_gltf_primitive_mesh(
        &mut self,
        primitive: &crate::util::GltfPrimitive,
    ) -> AppResult<katla_gfx::MeshHandle> {
        use crate::util::GltfVertices;
        let indices = &primitive.indices;
        let mesh_handle = match &primitive.vertices {
            GltfVertices::Static(vertices) => self.renderer.create_mesh(
                vertices,
                indices,
                katla_gfx::PrimitiveTopology::TriangleList,
            ),
            GltfVertices::Skinned(vertices) => self.renderer.create_mesh(
                vertices,
                indices,
                katla_gfx::PrimitiveTopology::TriangleList,
            ),
        }
        .map_err(|source| AppError::Graphics { source })?;
        let positions = primitive.vertices.positions();
        let triangles: Vec<[u32; 3]> = indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        self.geometry_cache.insert(
            mesh_handle,
            crate::geometry_cache::MeshGeometryData {
                positions: positions.clone(),
                triangles: triangles.clone(),
            },
        );
        if let Some(cache) = self
            .world
            .get_resource_mut::<crate::geometry_cache::GeometryCache>()
        {
            cache.insert(
                mesh_handle,
                crate::geometry_cache::MeshGeometryData {
                    positions,
                    triangles,
                },
            );
        }

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

    /// Load an STL mesh from disk and upload vertex/index data to the GPU.
    ///
    /// STL files contain only triangle geometry (positions + normals). Tangents are
    /// generated with default handedness and tex coords are set to (0, 0).
    ///
    /// Returns a [`katla_gfx::MeshHandle`] and the bounding [`katla_math::Sphere`].
    pub fn load_stl_mesh(
        &mut self,
        path: impl AsRef<std::path::Path>,
    ) -> AppResult<(katla_gfx::MeshHandle, katla_math::Sphere)> {
        let path_ref = path.as_ref();
        let mut file = std::fs::File::open(path_ref).map_err(|e| AppError::ModelLoadFailed {
            path: path_ref.to_string_lossy().to_string(),
            reason: e.to_string(),
        })?;

        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| AppError::ModelLoadFailed {
                path: path_ref.to_string_lossy().to_string(),
                reason: e.to_string(),
            })?;

        let mesh = StlMesh::from_bytes(&data).map_err(|e| AppError::ModelLoadFailed {
            path: path_ref.to_string_lossy().to_string(),
            reason: e.to_string(),
        })?;

        let (positions, normals, indices) = mesh.to_indexed_mesh();
        let bounds = mesh.bounds;

        let vertices: Vec<katla_gfx::vertex::VertexPBR> = positions
            .iter()
            .zip(&normals)
            .map(|(&position, &normal)| {
                katla_gfx::vertex::VertexPBR::new(
                    position,
                    normal,
                    [1.0, 0.0, 0.0, 1.0],
                    [0.0, 0.0],
                )
            })
            .collect();
        let mesh_handle = self.renderer.create_mesh(
            &vertices,
            &indices,
            katla_gfx::PrimitiveTopology::TriangleList,
        )?;

        let triangles: Vec<[u32; 3]> = indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        self.geometry_cache.insert(
            mesh_handle,
            crate::geometry_cache::MeshGeometryData {
                positions: positions.clone(),
                triangles: triangles.clone(),
            },
        );
        if let Some(cache) = self
            .world
            .get_resource_mut::<crate::geometry_cache::GeometryCache>()
        {
            cache.insert(
                mesh_handle,
                crate::geometry_cache::MeshGeometryData {
                    positions,
                    triangles,
                },
            );
        }

        info!(
            "Loaded STL mesh '{}' ({} vertices, {} triangles) -> handle {}",
            path_ref.display(),
            vertices.len(),
            mesh.triangles.len(),
            mesh_handle.index()
        );

        Ok((mesh_handle, bounds))
    }
}
