use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use log::{debug, warn};

use katla_ecs::EntityId;
use katla_ecs::World;
use katla_gfx::{PoseComputeBuffers, PoseComputePipeline};

use crate::animation::components::{AnimatedModel, AnimationPlayer};
use crate::animation::gpu_clip_loader::{
    GpuAnimData, build_skeleton_params, prepare_gpu_anim_data,
};
use crate::animation::skin::{Skeleton, Skin};

struct ClipLookup {
    clip_name_to_index: HashMap<String, u32>,
    joint_offset: u32,
    joint_count: u32,
}

/// Per-entity GPU animation info needed for buffer copies.
pub(crate) struct GpuAnimEntityInfo {
    pub joint_offset: u32,
    pub joint_count: u32,
}

/// ECS-side GPU animation system.
///
/// Handles querying the ECS world for animated entities, preparing animation
/// data, and updating per-frame parameters. Does NOT own GPU resources —
/// the pipeline and buffers live on the VulkanRenderer.
pub(crate) struct GpuAnimationSystem {
    entity_clip_map: HashMap<EntityId, ClipLookup>,
    /// Entities in the order they were registered by prepare().
    /// Must match the joint offset order used in update_params().
    entity_order: Vec<EntityId>,

    gpu_data: Option<GpuAnimData>,

    /// Fingerprint of the static data that was last uploaded.
    /// Used to detect when clip/joint data changes without entity add/remove.
    upload_fingerprint: u64,

    max_skeletons: usize,
    max_joints: usize,
}

impl GpuAnimationSystem {
    pub fn new() -> Self {
        Self {
            entity_clip_map: HashMap::new(),
            entity_order: Vec::new(),
            gpu_data: None,
            upload_fingerprint: 0,
            max_skeletons: 0,
            max_joints: 0,
        }
    }

    /// Build a hash fingerprint of the animated entity set and their clip data.
    /// Covers entity identity, clip names, joint counts, and channel counts.
    fn compute_fingerprint(
        entities: &[(
            katla_ecs::EntityId,
            &AnimatedModel,
            &Skin,
            &Skeleton,
            &AnimationPlayer,
        )],
    ) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        for (entity, model, skin, skeleton, _player) in entities {
            entity.hash(&mut hasher);
            for name in model.animations.keys() {
                name.hash(&mut hasher);
            }
            skin.joints.len().hash(&mut hasher);
            skeleton.joint_count().hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Scan the ECS world for animated entities, merge clip data, and
    /// upload static buffers if needed.
    ///
    /// The `pipeline` and `buffers` are borrowed from the renderer.
    pub fn prepare(
        &mut self,
        world: &mut World,
        pipeline: &mut PoseComputePipeline,
        buffers: &mut PoseComputeBuffers,
    ) {
        let entities: Vec<_> = world
            .query::<(&AnimatedModel, &Skin, &Skeleton, &AnimationPlayer)>()
            .collect();

        if entities.is_empty() {
            self.entity_clip_map.clear();
            self.entity_order.clear();
            self.gpu_data = None;
            self.upload_fingerprint = 0;
            self.max_skeletons = 0;
            self.max_joints = 0;
            return;
        }

        let fingerprint = Self::compute_fingerprint(&entities);
        let needs_upload = self.upload_fingerprint != fingerprint;

        if !needs_upload {
            return;
        }

        let mut all_clip_headers = Vec::new();
        let mut all_channel_infos = Vec::new();
        let mut all_keyframe_times = Vec::new();
        let mut all_keyframe_values = Vec::new();
        let mut all_joint_infos = Vec::new();
        let mut total_joints = 0usize;

        let mut new_entity_clip_map = HashMap::new();

        for (entity, animated_model, skin, skeleton, _player) in &entities {
            let data = prepare_gpu_anim_data(animated_model, skin, skeleton);
            let entity_joint_offset = total_joints as u32;
            let entity_joint_count = data.joint_count as u32;

            let clip_name_to_index: HashMap<String, u32> = animated_model
                .animations
                .keys()
                .enumerate()
                .map(|(i, name)| (name.clone(), (all_clip_headers.len() + i) as u32))
                .collect();

            new_entity_clip_map.insert(
                *entity,
                ClipLookup {
                    clip_name_to_index,
                    joint_offset: entity_joint_offset,
                    joint_count: entity_joint_count,
                },
            );

            let channel_base = all_channel_infos.len() as u32;
            let times_base = all_keyframe_times.len() as u32;
            let values_base = all_keyframe_values.len() as u32;

            all_clip_headers.extend(data.clip_headers.into_iter().map(|mut h| {
                h.channel_offset += channel_base;
                h
            }));

            all_channel_infos.extend(data.channel_infos.into_iter().map(|mut c| {
                c.time_offset += times_base;
                c.value_offset += values_base;
                c
            }));

            all_keyframe_times.extend(data.keyframe_times);
            all_keyframe_values.extend(data.keyframe_values);
            all_joint_infos.extend(data.joint_infos);
            total_joints += data.joint_count;
        }

        self.entity_clip_map = new_entity_clip_map;
        self.entity_order = entities.iter().map(|(e, _, _, _, _)| *e).collect();
        self.upload_fingerprint = fingerprint;
        self.gpu_data = Some(GpuAnimData {
            clip_headers: all_clip_headers,
            channel_infos: all_channel_infos,
            keyframe_times: all_keyframe_times,
            keyframe_values: all_keyframe_values,
            joint_infos: all_joint_infos,
            joint_count: total_joints,
        });
        self.max_skeletons = entities.len();
        self.max_joints = total_joints;

        self.upload_static_data(pipeline, buffers);
    }

    fn upload_static_data(
        &mut self,
        pipeline: &mut PoseComputePipeline,
        buffers: &mut PoseComputeBuffers,
    ) {
        let data = match &self.gpu_data {
            Some(d) => d,
            None => return,
        };

        if self.max_skeletons == 0 {
            return;
        }

        let headers_size =
            (data.clip_headers.len() * std::mem::size_of::<katla_gfx::AnimClipHeader>()) as u64;
        let channels_size =
            (data.channel_infos.len() * std::mem::size_of::<katla_gfx::AnimChannelInfo>()) as u64;
        let times_size = (data.keyframe_times.len() * std::mem::size_of::<f32>()) as u64;
        let values_size = (data.keyframe_values.len() * std::mem::size_of::<f32>()) as u64;

        if let Err(e) = buffers.allocate_params(self.max_skeletons) {
            warn!("Failed to allocate pose compute params buffer: {}", e);
            return;
        }
        if let Err(e) =
            buffers.allocate_clip_data(headers_size, channels_size, times_size, values_size)
        {
            warn!("Failed to allocate pose compute clip data buffers: {}", e);
            return;
        }
        if let Err(e) = buffers.allocate_joints(self.max_joints) {
            warn!("Failed to allocate pose compute joints buffer: {}", e);
            return;
        }
        if let Err(e) = buffers.allocate_world(self.max_joints) {
            warn!("Failed to allocate pose compute world buffer: {}", e);
            return;
        }
        if let Err(e) = buffers.allocate_output(self.max_joints) {
            warn!("Failed to allocate pose compute output buffer: {}", e);
            return;
        }

        buffers.upload_clip_data(
            &data.clip_headers,
            &data.channel_infos,
            &data.keyframe_times,
            &data.keyframe_values,
        );
        buffers.upload_joints(&data.joint_infos);

        pipeline.update_bindings(buffers);

        debug!(
            "Uploaded GPU animation static data: {} skeletons, {} joints, {} clips",
            self.max_skeletons,
            self.max_joints,
            data.clip_headers.len(),
        );
    }

    /// Update per-frame animation parameters (time, clip index) for all
    /// animated entities.
    ///
    /// Iterates in `entity_order` to guarantee joint offsets match those
    /// assigned during `prepare()`.
    pub fn update_params(&self, world: &mut World, buffers: &mut PoseComputeBuffers) {
        let mut params = Vec::with_capacity(self.entity_order.len());

        for entity in &self.entity_order {
            let clip_lookup = match self.entity_clip_map.get(entity) {
                Some(l) => l,
                None => continue,
            };
            let player = match world.get_component::<AnimationPlayer>(*entity) {
                Some(p) => p,
                None => continue,
            };

            let param = build_skeleton_params(
                player,
                &clip_lookup.clip_name_to_index,
                clip_lookup.joint_offset,
                clip_lookup.joint_count,
            );
            params.push(param);
        }

        if !params.is_empty() {
            buffers.update_params(&params);
        }
    }

    /// Number of skeletons known to the system (for workgroup count).
    pub fn skeleton_count(&self) -> usize {
        self.max_skeletons
    }

    /// Get per-entity joint offset and count for GPU buffer copies.
    pub fn entity_info(&self, entity: EntityId) -> Option<GpuAnimEntityInfo> {
        self.entity_clip_map
            .get(&entity)
            .map(|l| GpuAnimEntityInfo {
                joint_offset: l.joint_offset,
                joint_count: l.joint_count,
            })
    }

    /// Iterate all tracked entities in registration order.
    pub fn entities(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entity_order.iter().copied()
    }
}
