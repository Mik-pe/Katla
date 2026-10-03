//! Spawn imported primitive drawables with independent surfaces and persistent origins.

use super::ModelTextures;
use crate::{
    application::Application,
    components::{Children, DrawableComponent, NameComponent, Parent, TransformComponent},
    error::{AppError, AppResult},
    scene::{AssetRef, EntitySource},
    util::{GLTFModel, GltfPrimitive},
};
use katla_ecs::EntityId;
use katla_gfx::{GpuRenderer, ImageFormat, PipelineDescriptor};
use katla_math::{Color, Vec3};
use std::{collections::HashSet, path::Path};

impl Application {
    pub(crate) fn prepare_gltf_group_collider(
        &mut self,
        entity: EntityId,
        path: &Path,
    ) -> AppResult<katla_gfx::MeshHandle> {
        let model = self.gltf_cache.read(std::fs::canonicalize(path)?)?;
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let transforms = crate::util::modelcache::build_world_transforms(
            &model.document.nodes().collect::<Vec<_>>(),
        );
        for primitive in &model.primitives {
            let offset = u32::try_from(vertices.len()).map_err(|_| AppError::Other {
                message: "Model collision geometry exceeds u32 indices".into(),
            })?;
            let positions = match &primitive.vertices {
                crate::util::GltfVertices::Static(_) => primitive.vertices.positions(),
                crate::util::GltfVertices::Skinned(source) => {
                    let skin = primitive
                        .skin_index
                        .and_then(|index| model.document.skins().nth(index))
                        .ok_or_else(|| AppError::Other {
                            message: "Skinned collision geometry has no skin".into(),
                        })?;
                    let inverse: Vec<_> = skin
                        .reader(|buffer| model.buffers.get(buffer.index()).map(|data| &data.0[..]))
                        .read_inverse_bind_matrices()
                        .map(|matrices| {
                            matrices
                                .map(|matrix| katla_math::Mat4(matrix.map(katla_math::Vec4::from)))
                                .collect()
                        })
                        .unwrap_or_else(|| {
                            vec![katla_math::Mat4::identity(); skin.joints().count()]
                        });
                    let joints: Vec<_> = skin
                        .joints()
                        .enumerate()
                        .map(|(index, node)| transforms[&node.index()] * inverse[index])
                        .collect();
                    source
                        .iter()
                        .map(|vertex| {
                            let point = katla_math::Vec4::new(
                                vertex.position[0],
                                vertex.position[1],
                                vertex.position[2],
                                1.0,
                            );
                            let mut position = katla_math::Vec4::new(0.0, 0.0, 0.0, 0.0);
                            for (&joint, &weight) in
                                vertex.joint_indices.iter().zip(&vertex.joint_weights)
                            {
                                position += joints[usize::from(joint)] * point * weight;
                            }
                            [position.x(), position.y(), position.z()]
                        })
                        .collect()
                }
            };
            for position in positions {
                vertices.push(katla_gfx::VertexPosition { position });
            }
            for &index in &primitive.indices {
                indices.push(offset.checked_add(index).ok_or_else(|| AppError::Other {
                    message: "Model collision geometry exceeds u32 indices".into(),
                })?);
            }
        }
        let mesh = self
            .renderer
            .create_mesh(
                &vertices,
                &indices,
                katla_gfx::PrimitiveTopology::TriangleList,
            )
            .map_err(|source| AppError::Graphics { source })?;
        self.gpu_resource_tracker.track_mesh(mesh);
        let geometry = crate::geometry_cache::MeshGeometryData {
            positions: vertices.iter().map(|vertex| vertex.position).collect(),
            triangles: indices.as_chunks::<3>().0.to_vec(),
        };
        self.geometry_cache.insert(mesh, geometry.clone());
        if let Some(cache) = self
            .world
            .get_resource_mut::<crate::geometry_cache::GeometryCache>()
        {
            cache.insert(mesh, geometry);
        }
        self.world
            .add_component(entity, super::CollisionMesh { handle: mesh });
        Ok(mesh)
    }

    /// Import a selected glTF scene, retaining a drawable and material per primitive.
    ///
    /// A single primitive returns its drawable entity. Multiple primitives return a
    /// model controller with independently editable child drawables. Animation on
    /// that controller drives children unless a child has its own animation player.
    pub fn spawn_gltf_model(
        &mut self,
        path: impl AsRef<Path>,
        position: [f32; 3],
        default_animation: Option<&str>,
    ) -> AppResult<EntityId> {
        let path = std::fs::canonicalize(path)?;
        let model = self.gltf_cache.read(path.clone())?;
        self.prepare_gltf_entities(|app| {
            if model.primitives.is_empty() {
                return Err(AppError::ModelLoadFailed {
                    path: path.display().to_string(),
                    reason: "Selected scene contains no triangle primitives".into(),
                });
            }
            if model.primitives.len() == 1 {
                return app.spawn_gltf_primitive_from_model(
                    &path,
                    &model,
                    &model.primitives[0],
                    position,
                    default_animation,
                );
            }
            let root = app.spawn_gltf_group_from_model(&path, &model, position, default_animation);
            let mut children = Vec::with_capacity(model.primitives.len());
            for primitive in &model.primitives {
                let child =
                    app.spawn_gltf_primitive_from_model(&path, &model, primitive, [0.0; 3], None)?;
                app.world.add_component(child, Parent::new(root));
                children.push(child);
            }
            app.world.add_component(root, Children::new(children));
            Ok(root)
        })
    }

    pub(crate) fn spawn_gltf_group(
        &mut self,
        path: impl AsRef<Path>,
        position: [f32; 3],
    ) -> AppResult<EntityId> {
        let path = std::fs::canonicalize(path)?;
        let model = self.gltf_cache.read(path.clone())?;
        Ok(self.spawn_gltf_group_from_model(&path, &model, position, None))
    }

    fn spawn_gltf_group_from_model(
        &mut self,
        path: &Path,
        model: &GLTFModel,
        position: [f32; 3],
        animation: Option<&str>,
    ) -> EntityId {
        let entity = self.world.spawn((
            TransformComponent::from_position(Vec3::from(position)),
            NameComponent::new(
                path.file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Model"),
            ),
            EntitySource::GltfGroup {
                path: AssetRef::File(path.into()),
            },
        ));
        crate::animation::AnimationManager::setup_animated_model(
            &mut self.world,
            entity,
            model,
            None,
            animation,
        );
        entity
    }

    pub(crate) fn spawn_gltf_primitive(
        &mut self,
        path: impl AsRef<Path>,
        node_index: usize,
        primitive_index: usize,
        position: [f32; 3],
    ) -> AppResult<EntityId> {
        let path = std::fs::canonicalize(path)?;
        let model = self.gltf_cache.read(path.clone())?;
        let primitive = model
            .primitives
            .iter()
            .find(|primitive| {
                primitive.node_index == node_index && primitive.primitive_index == primitive_index
            })
            .ok_or_else(|| AppError::ModelLoadFailed {
                path: path.display().to_string(),
                reason: format!(
                    "Selected scene has no node {node_index} primitive {primitive_index}"
                ),
            })?;
        self.prepare_gltf_entities(|app| {
            app.spawn_gltf_primitive_from_model(&path, &model, primitive, position, None)
        })
    }

    fn spawn_gltf_primitive_from_model(
        &mut self,
        path: &Path,
        model: &GLTFModel,
        primitive: &GltfPrimitive,
        position: [f32; 3],
        animation: Option<&str>,
    ) -> AppResult<EntityId> {
        let mesh = self.upload_gltf_primitive_mesh(primitive)?;
        self.gpu_resource_tracker.track_mesh(mesh);
        let shader = self
            .resources
            .shader_path(if primitive.skin_index.is_some() {
                "model_pbr_skinned.wgsl"
            } else {
                "model_pbr.wgsl"
            });
        let descriptor = if primitive.skin_index.is_some() {
            PipelineDescriptor::skinned(shader.to_string_lossy())
        } else {
            PipelineDescriptor::pbr(shader.to_string_lossy())
        }
        .with_color_format(ImageFormat::R16G16B16A16Sfloat);
        let material = self
            .renderer
            .compile_material(&descriptor)
            .map_err(|error| AppError::ShaderCompileFailed {
                path: path.display().to_string(),
                reason: error.to_string(),
            })?;
        self.gpu_resource_tracker.track_material(material);
        let upload = self.upload_gltf_textures(path, &model.images, &primitive.material);
        for handle in &upload.handles {
            self.gpu_resource_tracker.track_texture(*handle);
        }
        self.renderer
            .set_material_textures(material, upload.textures);
        let factors = &primitive.material;
        let color = factors.base_color_factor;
        let mut drawable = DrawableComponent::with_handles_and_material(
            mesh,
            material,
            Some(Color::new(color[0], color[1], color[2], color[3])),
            factors.metallic_factor,
            factors.roughness_factor,
            1.0,
        );
        if primitive.skin_index.is_none() {
            drawable = drawable.with_bounds(primitive.bounds);
        }
        drawable.emission = upload.emission;
        drawable.sampling = primitive.material.sampling();
        drawable.tangent_uv = primitive.tangent_uv;
        drawable.surface = crate::rendering::MaterialSurface {
            emissive_factor: factors.emission_factor,
            normal_scale: factors.normal_scale,
            occlusion_strength: factors.occlusion_strength,
            alpha_mode: factors.alpha_mode,
            alpha_cutoff: factors.alpha_cutoff,
            double_sided: factors.double_sided,
        };
        if let Some(skin_index) = primitive.skin_index {
            let skin = model.document.skins().nth(skin_index).ok_or_else(|| {
                AppError::ModelLoadFailed {
                    path: path.display().to_string(),
                    reason: format!("Skin {skin_index} is missing"),
                }
            })?;
            drawable.skeleton_handle = self
                .renderer
                .create_skeleton(skin.joints().count())
                .map_err(|error| AppError::SkeletonCreateFailed {
                    path: path.display().to_string(),
                    reason: error.to_string(),
                })?;
            self.gpu_resource_tracker
                .track_skeleton(drawable.skeleton_handle);
        }
        let entity = self.world.spawn((
            TransformComponent::from_position(Vec3::from(position)),
            drawable,
            NameComponent::new(&primitive.name),
            EntitySource::GltfPrimitive {
                path: AssetRef::File(path.into()),
                node_index: primitive.node_index,
                primitive_index: primitive.primitive_index,
            },
            ModelTextures {
                handles: upload.handles,
            },
        ));
        if primitive.skin_index.is_some() {
            crate::animation::AnimationManager::setup_animated_model(
                &mut self.world,
                entity,
                model,
                primitive.skin_index,
                animation,
            );
        }
        Ok(entity)
    }

    fn prepare_gltf_entities(
        &mut self,
        prepare: impl FnOnce(&mut Self) -> AppResult<EntityId>,
    ) -> AppResult<EntityId> {
        let previous_entities: HashSet<_> = self.world.entity_ids().collect();
        let previous_tracker = self.gpu_resource_tracker.clone();
        match prepare(self) {
            Ok(entity) => Ok(entity),
            Err(error) => {
                let abandoned: Vec<_> = self
                    .world
                    .entity_ids()
                    .filter(|entity| !previous_entities.contains(entity))
                    .collect();
                for entity in abandoned {
                    self.world.destroy_entity(entity);
                }
                let resources = self.gpu_resource_tracker.rollback_to(previous_tracker);
                crate::scene::serialization::destroy_resources(self, resources);
                Err(error)
            }
        }
    }
}
