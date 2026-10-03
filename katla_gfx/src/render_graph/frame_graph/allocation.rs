//! Physical texture allocation contracts and resize lifetimes.

use super::*;

#[derive(Debug, PartialEq)]
pub(super) struct TransientAllocationMember {
    resource: ResourceId,
    format: crate::texture::ImageFormat,
    width: u32,
    height: u32,
    resource_class: u8,
}

#[derive(Debug, PartialEq)]
pub(super) struct TransientAllocationContract {
    members: Vec<TransientAllocationMember>,
    policy: crate::render_graph::backend::TransientSlotPolicy,
}

impl<B: RenderGraphBackend> FrameGraph<B> {
    /// Group transient resources into physical allocation slots.
    ///
    /// Driven by the compiled allocation plan: compatible resources whose
    /// live intervals do not overlap share a slot. Resources without a
    /// compiled lifetime (culled or unused) receive no allocation. A disabled
    /// optimization policy gives each live texture an independent group.
    fn transient_allocation_groups(&self) -> Result<Vec<Vec<GraphResourceDesc>>, RenderGraphError> {
        let plan = self.build_execution_plan()?;
        let allocation = TransientAllocationPlan::build(
            &self.resources,
            &self.transient_resources,
            &self.exported_resources,
            &plan.resource_lifetimes,
            &plan.live_image_accesses,
        );

        let mut standalone = Vec::new();
        let mut by_slot = BTreeMap::<u32, Vec<GraphResourceDesc>>::new();
        for desc in &self.transient_resources {
            let resource_id = self
                .resource_by_name
                .get(&desc.name)
                .copied()
                .ok_or_else(|| {
                    RenderGraphError::Validation(
                        GraphValidationError::MissingResourceNamespaceEntry(desc.name.clone()),
                    )
                })?;
            match allocation.physical_allocation_id(resource_id) {
                Some(_) if !self.transient_aliasing => standalone.push(vec![desc.clone()]),
                Some(slot) => by_slot.entry(slot).or_default().push(desc.clone()),
                None => {}
            }
        }

        Ok(standalone
            .into_iter()
            .chain(by_slot.into_values())
            .collect())
    }

    fn build_transient_allocation_contract(
        &self,
        groups: &[Vec<GraphResourceDesc>],
    ) -> Result<Vec<TransientAllocationContract>, RenderGraphError> {
        let plan = self.build_execution_plan()?;
        let mut allocation = TransientAllocationPlan::build(
            &self.resources,
            &self.transient_resources,
            &self.exported_resources,
            &plan.resource_lifetimes,
            &plan.live_image_accesses,
        );
        allocation.apply_attachment_storage(&self.passes, &plan.resource_lifetimes);
        groups
            .iter()
            .enumerate()
            .map(|(group_index, group)| {
                let members = group
                    .iter()
                    .map(|desc| {
                        let resource = self.resource_id(&desc.name).ok_or_else(|| {
                            RenderGraphError::Validation(
                                GraphValidationError::MissingResourceNamespaceEntry(
                                    desc.name.clone(),
                                ),
                            )
                        })?;
                        let resource_class = match desc.resource_type {
                            crate::render_graph::resource::GraphResourceType::ColorAttachment {
                                ..
                            } => 0,
                            crate::render_graph::resource::GraphResourceType::DepthAttachment {
                                sampled: false,
                                ..
                            } => 1,
                            crate::render_graph::resource::GraphResourceType::DepthAttachment {
                                sampled: true,
                                ..
                            } => 2,
                            crate::render_graph::resource::GraphResourceType::SampledImage => 3,
                        };
                        Ok(TransientAllocationMember {
                            resource,
                            format: desc.format,
                            width: desc.width,
                            height: desc.height,
                            resource_class,
                        })
                    })
                    .collect::<Result<Vec<_>, RenderGraphError>>()?;
                let uses = |usage| {
                    members.iter().any(|member| {
                        plan.live_image_accesses.iter().any(|access| {
                            access.resource == member.resource && access.usage == usage
                        })
                    })
                };
                let policy = crate::render_graph::backend::TransientSlotPolicy {
                    frame_slot: 0,
                    allocation_slot: group_index as u32,
                    optimize: self.transient_aliasing,
                    memoryless: self.transient_aliasing
                        && members.iter().all(|member| {
                            allocation
                                .persistence(member.resource)
                                .is_some_and(|persistence| persistence.tile_memory.is_eligible())
                        }),
                    storage: uses(crate::render_graph::access::ResourceAccessUsage::Storage),
                    transfer_destination: uses(
                        crate::render_graph::access::ResourceAccessUsage::TransferDestination,
                    ),
                };
                Ok(TransientAllocationContract { members, policy })
            })
            .collect()
    }

    /// Initialize transient textures using the backend.
    ///
    /// Creates per-frame sets of textures — one per frame-in-flight —
    /// grouped into physical allocation slots by the compiled plan.
    pub fn initialize_transient_textures(&mut self, backend: &B) -> Result<(), RenderGraphError> {
        if self.compiled && self.transient_allocation_contract_validated {
            return Ok(());
        }
        let groups = self.transient_allocation_groups()?;
        let contract = self.build_transient_allocation_contract(&groups)?;
        if let Some(existing) = &self.transient_allocation_contract {
            // Fresh groups prove every old shared range still has disjoint live intervals.
            if *existing != contract {
                return Err(RenderGraphError::AllocationContractChanged);
            }
            self.transient_allocation_contract_validated = true;
            return Ok(());
        }

        let frames = B::transient_texture_frames();

        log::info!(
            "Initializing {} transient textures in {} allocation groups ({} frames in flight, aliasing {})",
            self.transient_resources.len(),
            groups.len(),
            frames,
            if self.transient_aliasing { "on" } else { "off" },
        );

        let mut frame_slots = Vec::with_capacity(frames);
        for frame_idx in 0..frames {
            let mut frame_textures = HashMap::new();
            for (group, compiled) in groups.iter().zip(&contract) {
                let policy = crate::render_graph::backend::TransientSlotPolicy {
                    frame_slot: frame_idx,
                    ..compiled.policy
                };
                let textures = B::create_transient_slot(backend, group, policy)?;
                for (desc, texture) in group.iter().zip(textures) {
                    let resource_id =
                        self.resource_by_name
                            .get(&desc.name)
                            .copied()
                            .ok_or_else(|| {
                                RenderGraphError::Validation(
                                    GraphValidationError::MissingResourceNamespaceEntry(
                                        desc.name.clone(),
                                    ),
                                )
                            })?;
                    frame_textures.insert(resource_id, texture);
                }
            }

            frame_slots.push(frame_textures);
        }
        self.transient_textures = frame_slots;
        self.transient_allocation_contract = Some(contract);
        self.transient_allocation_contract_validated = true;

        Ok(())
    }

    /// Register a transient texture with the bindless texture system.
    ///
    /// Registers ALL per-frame instances of the texture.
    /// Returns the base slot index; frame N's texture is at `base_slot + N`.
    pub fn register_transient_texture_bindless(
        &mut self,
        backend: &mut B,
        name: &str,
    ) -> Result<u32, RenderGraphError> {
        let num_frames = self.transient_textures.len();
        if num_frames == 0 {
            return Err(RenderGraphError::InvalidConfiguration(
                "Transient textures not initialized".to_string(),
            ));
        }

        log::info!(
            "Registering transient texture '{}' ({} frames) with bindless system",
            name,
            num_frames
        );

        let resource_id = self
            .resource_by_name
            .get(name)
            .copied()
            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.to_string()))?;

        let descriptor = self
            .transient_resources
            .iter()
            .find(|descriptor| descriptor.name == name)
            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.into()))?;
        if matches!(
            descriptor.resource_type,
            crate::render_graph::GraphResourceType::DepthAttachment { sampled: false, .. }
        ) {
            return Err(RenderGraphError::InvalidConfiguration(format!(
                "Texture '{name}' does not permit sampling"
            )));
        }

        for frame_idx in 0..num_frames {
            if let Some(frame_textures) = self.transient_textures.get_mut(frame_idx)
                && let Some(texture) = frame_textures.get_mut(&resource_id)
            {
                let slot = backend.register_bindless_texture(texture)?;
                B::set_transient_texture_bindless_slot(texture, slot);
                log::trace!("  Frame {}: slot {}", frame_idx, slot);
            }
        }

        let base_slot = self
            .transient_textures
            .first()
            .and_then(|textures| textures.get(&resource_id))
            .and_then(B::transient_texture_bindless_slot)
            .ok_or_else(|| RenderGraphError::ResourceNotFound(name.to_string()))?;

        Ok(base_slot)
    }

    /// Recreate transient textures with new dimensions.
    ///
    /// Old textures are destroyed and new ones are created with the updated dimensions.
    /// Returns (texture_name, bindless_slot) tuples for all recreated textures.
    pub fn recreate_transient_textures(
        &mut self,
        backend: &mut B,
        new_width: u32,
        new_height: u32,
    ) -> Result<Vec<(String, u32)>, RenderGraphError> {
        let mut existing_slots: std::collections::HashMap<String, Vec<u32>> =
            std::collections::HashMap::new();

        for frame_textures in &self.transient_textures {
            for (&resource_id, texture) in frame_textures {
                if let Some(slot) = B::transient_texture_bindless_slot(texture) {
                    let name = self
                        .resource_name(resource_id)
                        .unwrap_or("unknown")
                        .to_string();
                    existing_slots.entry(name).or_default().push(slot);
                }
            }
        }

        self.transient_textures.clear();
        self.transient_allocation_contract = None;
        self.transient_allocation_contract_validated = false;

        for desc in &mut self.transient_resources {
            if desc.tracks_swapchain_size {
                desc.width = new_width;
                desc.height = new_height;
            }
        }

        self.initialize_transient_textures(backend)?;

        let mut result = Vec::new();
        for (name, slots) in &existing_slots {
            let resource_id = match self.resource_by_name.get(name) {
                Some(&id) => id,
                None => continue,
            };
            for (frame_idx, slot) in slots.iter().enumerate() {
                if let Some(frame_textures) = self.transient_textures.get_mut(frame_idx)
                    && let Some(texture) = frame_textures.get_mut(&resource_id)
                {
                    backend.update_bindless_texture(*slot, texture)?;
                    B::set_transient_texture_bindless_slot(texture, *slot);
                }
            }

            if let Some(&base_slot) = slots.first() {
                result.push((name.clone(), base_slot));
            }
        }

        let new_texture_names: Vec<String> = self
            .transient_resources
            .iter()
            .filter(|desc| {
                !existing_slots.contains_key(&desc.name)
                    && !matches!(
                        desc.resource_type,
                        crate::render_graph::GraphResourceType::DepthAttachment {
                            sampled: false,
                            ..
                        }
                    )
            })
            .map(|desc| desc.name.clone())
            .collect();

        for name in new_texture_names {
            let slot = self.register_transient_texture_bindless(backend, &name)?;
            result.push((name, slot));
        }

        Ok(result)
    }
}
