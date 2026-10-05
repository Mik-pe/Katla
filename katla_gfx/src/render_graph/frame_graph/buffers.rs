//! Imported buffer ownership, allocation, and pass declarations.

use super::*;

pub(super) fn validate_buffer_access_descriptor(
    pass: &str,
    resource: &str,
    mode: ResourceAccessMode,
    usage: BufferUsage,
    stage: ResourceAccessStage,
    range: BufferByteRange,
    desc: BufferDesc,
) -> Result<(), RenderGraphError> {
    if range.is_empty()
        || range.offset >= desc.size
        || (range.size != u64::MAX && range.end() > desc.size)
    {
        return Err(GraphValidationError::InvalidBufferAccessRange {
            pass: pass.to_string(),
            resource: resource.to_string(),
            offset: range.offset,
            size: range.size,
            capacity: desc.size,
        }
        .into());
    }

    let (required, usage_name) = match usage {
        BufferUsage::Uniform => (BufferUsages::UNIFORM, "uniform"),
        BufferUsage::Storage => (BufferUsages::STORAGE, "storage"),
        BufferUsage::Vertex => (BufferUsages::VERTEX, "vertex"),
        BufferUsage::Index => (BufferUsages::INDEX, "index"),
        BufferUsage::Indirect => (BufferUsages::INDIRECT, "indirect"),
        BufferUsage::TransferSource => (BufferUsages::TRANSFER_SOURCE, "transfer-source"),
        BufferUsage::TransferDestination => {
            (BufferUsages::TRANSFER_DESTINATION, "transfer-destination")
        }
        BufferUsage::Readback => (BufferUsages::READBACK, "readback"),
    };
    let usage_declared = desc.usages.contains(required)
        && (usage != BufferUsage::Readback || desc.memory == BufferMemoryPolicy::Readback);
    if !usage_declared {
        return Err(GraphValidationError::BufferUsageNotDeclared {
            pass: pass.to_string(),
            resource: resource.to_string(),
            usage: usage_name.to_string(),
        }
        .into());
    }

    let mode_valid = match usage {
        BufferUsage::Uniform
        | BufferUsage::Vertex
        | BufferUsage::Index
        | BufferUsage::Indirect
        | BufferUsage::TransferSource
        | BufferUsage::Readback => mode == ResourceAccessMode::Read,
        BufferUsage::TransferDestination => mode == ResourceAccessMode::Write,
        BufferUsage::Storage => true,
    };
    if !mode_valid {
        return Err(GraphValidationError::InvalidBufferAccessMode {
            pass: pass.to_string(),
            resource: resource.to_string(),
            usage: usage_name.to_string(),
        }
        .into());
    }

    let stage_valid = match usage {
        BufferUsage::Uniform | BufferUsage::Storage => matches!(
            stage,
            ResourceAccessStage::VertexShader
                | ResourceAccessStage::FragmentShader
                | ResourceAccessStage::ComputeShader
                | ResourceAccessStage::AllGraphics
        ),
        BufferUsage::Vertex | BufferUsage::Index => stage == ResourceAccessStage::VertexInput,
        BufferUsage::Indirect => stage == ResourceAccessStage::DrawIndirect,
        BufferUsage::TransferSource | BufferUsage::TransferDestination => {
            stage == ResourceAccessStage::Transfer
        }
        BufferUsage::Readback => stage == ResourceAccessStage::Host,
    };
    if !stage_valid {
        return Err(GraphValidationError::InvalidBufferAccessStage {
            pass: pass.to_string(),
            resource: resource.to_string(),
            usage: usage_name.to_string(),
            stage,
        }
        .into());
    }

    Ok(())
}

impl<B: RenderGraphBackend> FrameGraph<B> {
    /// Get a graph-owned buffer by resource id for one frame slot.
    pub fn transient_buffer_by_id(
        &self,
        id: ResourceId,
        frame_idx: usize,
    ) -> Option<&B::TransientBuffer> {
        self.transient_buffers_by_frame.get(frame_idx)?.get(&id)
    }

    /// Get a graph-owned buffer by name for one frame slot.
    pub fn transient_buffer(&self, name: &str, frame_idx: usize) -> Option<&B::TransientBuffer> {
        let id = self.resource_by_name.get(name)?;
        self.transient_buffer_by_id(*id, frame_idx)
    }

    /// Imported allocation currently selected for a graph resource.
    pub(crate) fn imported_buffer_handle(&self, resource: ResourceId) -> Option<BufferHandle> {
        self.imported_buffers.get(&resource).copied()
    }

    /// Resolve either a transient allocation or an imported buffer handle.
    pub fn buffer_by_id<'a>(
        &'a self,
        backend: &'a B,
        id: ResourceId,
        frame_idx: usize,
    ) -> Option<crate::render_graph::backend::ResolvedGraphBuffer<'a, B>> {
        self.transient_buffer_by_id(id, frame_idx)
            .or_else(|| {
                self.imported_buffer_handle(id)
                    .and_then(|handle| B::buffer_by_handle(backend, handle))
            })
            .map(crate::render_graph::backend::ResolvedGraphBuffer::Borrowed)
    }

    /// Import an application-owned buffer without prescribing a feature role.
    pub fn import_buffer(
        &mut self,
        name: impl Into<String>,
        handle: BufferHandle,
        desc: BufferDesc,
    ) -> Result<ResourceId, RenderGraphError> {
        let name = name.into();
        if name.is_empty() {
            return Err(GraphValidationError::EmptyResourceName.into());
        }
        if self.resource_by_name.contains_key(&name) {
            return Err(GraphValidationError::DuplicateResourceName(name).into());
        }
        if handle.is_none() {
            return Err(GraphValidationError::InvalidImportedBuffer(name).into());
        }
        if desc.size == 0 || desc.usages.is_empty() {
            return Err(GraphValidationError::InvalidBufferDescriptor {
                resource: name,
                size: desc.size,
            }
            .into());
        }
        let id = self.create_resource_id(name);
        self.imported_buffers.insert(id, handle);
        self.buffer_desc_by_id.insert(id, desc);
        self.compiled = false;
        self.execution_plan = None;
        Ok(id)
    }

    /// Select another allocation with the same declared capacity and capabilities.
    ///
    /// Applications select their acquired frame slot before execution. Native
    /// descriptor validation remains mandatory before any buffer is encoded.
    pub fn rebind_imported_buffer(
        &mut self,
        resource: ResourceId,
        handle: BufferHandle,
    ) -> Result<(), RenderGraphError> {
        let name = self.resource_name(resource).unwrap_or("?").to_string();
        if handle.is_none() {
            return Err(GraphValidationError::InvalidImportedBuffer(name).into());
        }
        let target = self
            .imported_buffers
            .get_mut(&resource)
            .ok_or(RenderGraphError::ResourceNotFound(name))?;
        *target = handle;
        Ok(())
    }

    /// Replace an imported buffer after application-controlled reallocation.
    ///
    /// Unlike rebinding a frame slot, a capacity or usage change recompiles all
    /// access and synchronization contracts before execution.
    pub fn redefine_imported_buffer(
        &mut self,
        resource: ResourceId,
        handle: BufferHandle,
        desc: BufferDesc,
    ) -> Result<(), RenderGraphError> {
        let name = self.resource_name(resource).unwrap_or("?").to_string();
        if desc.size == 0 || desc.usages.is_empty() {
            return Err(GraphValidationError::InvalidBufferDescriptor {
                resource: name,
                size: desc.size,
            }
            .into());
        }
        self.rebind_imported_buffer(resource, handle)?;
        self.buffer_desc_by_id.insert(resource, desc);
        self.compiled = false;
        self.execution_plan = None;
        Ok(())
    }

    /// Retire an import after every command, packet and export stops using it.
    ///
    /// Its logical resource ID remains reserved so other graph IDs never move.
    pub fn remove_imported_buffer(&mut self, resource: ResourceId) -> Result<(), RenderGraphError> {
        let name = self.resource_name(resource).unwrap_or("?").to_string();
        if !self.imported_buffers.contains_key(&resource) {
            return Err(RenderGraphError::ResourceNotFound(name));
        }
        let consumer = self.passes.iter().find(|pass| {
            pass.reads.contains(&resource) || pass.writes.contains(&resource)
                || pass.buffer_accesses.iter().any(|access| access.resource == resource)
                || pass.commands.iter().any(|command| match command {
                    crate::render_graph::ComputeCommand::Dispatch(dispatch) => dispatch.bindings.iter().any(|binding| binding.resource == resource) || matches!(dispatch.size, crate::render_graph::ComputeDispatchSize::Indirect { resource: source, .. } if source == resource),
                    crate::render_graph::ComputeCommand::FillBuffer { resource: target, .. } => *target == resource,
                    crate::render_graph::ComputeCommand::CopyBuffer { source, destination, .. } => *source == resource || *destination == resource,
                })
                || pass.bindings.buffers.iter().any(|binding| binding.resource == resource)
                || pass.bindings.phases.iter().any(|phase| matches!(phase.draw, crate::renderer::frame_bindings::PassDraw::Indirect { resource: source, .. } if source == resource))
        }).map(|pass| pass.name.clone()).or_else(|| self.exported_resources.contains(&resource).then(|| "graph export".to_string()));
        if let Some(consumer) = consumer {
            return Err(GraphValidationError::ImportedBufferStillInUse {
                resource: name,
                consumer,
            }
            .into());
        }
        self.imported_buffers.remove(&resource);
        self.compiled = false;
        self.execution_plan = None;
        Ok(())
    }

    /// Replace explicit commands and their complete buffer access declaration.
    pub fn set_pass_commands(
        &mut self,
        pass_id: PassId,
        commands: Vec<crate::render_graph::compute::ComputeCommand>,
        accesses: Vec<crate::render_graph::BufferAccess>,
    ) -> Result<(), RenderGraphError> {
        let index = self
            .pass_position(pass_id)
            .ok_or_else(|| RenderGraphError::PassNotFound(format!("{pass_id:?}")))?;
        let pass = &mut self.passes[index];
        pass.commands = commands;
        pass.set_buffer_accesses(accesses);
        self.compiled = false;
        self.execution_plan = None;
        Ok(())
    }

    /// Replace graphics inputs after validating the declared resource contract.
    ///
    /// Valid packets reuse the compiled plan. Failure preserves the previous
    /// packet and does not invalidate compilation.
    pub fn set_pass_bindings(
        &mut self,
        pass_id: PassId,
        bindings: crate::renderer::frame_bindings::PassBindings,
    ) -> Result<(), RenderGraphError> {
        let index = self
            .pass_position(pass_id)
            .ok_or_else(|| RenderGraphError::PassNotFound(format!("{pass_id:?}")))?;
        let pass = &mut self.passes[index];
        crate::render_graph::pass_bindings::validate(pass, &bindings)?;
        pass.bindings = bindings;
        Ok(())
    }

    /// Add buffer consumers to an existing pass while preserving its image contract.
    pub fn extend_pass_buffer_accesses(
        &mut self,
        name: &str,
        accesses: impl IntoIterator<Item = crate::render_graph::access::BufferAccess>,
    ) -> Result<(), RenderGraphError> {
        let pass = self
            .passes
            .iter_mut()
            .find(|pass| pass.name == name)
            .ok_or_else(|| {
                RenderGraphError::InvalidConfiguration(format!("Missing pass '{name}'"))
            })?;
        let mut combined = pass.buffer_accesses.clone();
        combined.extend(accesses);
        pass.set_buffer_accesses(combined);
        self.execution_plan = None;
        self.compiled = false;
        Ok(())
    }

    /// Buffer descriptor declared for a named graph resource.
    pub fn buffer_desc(&self, name: &str) -> Option<BufferDesc> {
        let id = self.resource_by_name.get(name)?;
        self.buffer_desc_by_id.get(id).copied()
    }

    pub(super) fn validate_declared_buffer_accesses(&self) -> Result<(), RenderGraphError> {
        for pass in &self.passes {
            let declared_buffers = pass
                .buffer_accesses
                .iter()
                .map(|access| access.resource)
                .collect::<BTreeSet<_>>();

            for &resource in pass.reads.iter().chain(&pass.writes) {
                if self.buffer_desc_by_id.contains_key(&resource)
                    && !declared_buffers.contains(&resource)
                {
                    return Err(GraphValidationError::MissingTypedBufferAccess {
                        pass: pass.name.clone(),
                        resource: self.resource_name(resource).unwrap_or("?").to_string(),
                    }
                    .into());
                }
            }

            for access in &pass.image_accesses {
                if self.buffer_desc_by_id.contains_key(&access.resource) {
                    return Err(GraphValidationError::ImageAccessOnNonImage {
                        pass: pass.name.clone(),
                        resource: self
                            .resource_name(access.resource)
                            .unwrap_or("?")
                            .to_string(),
                    }
                    .into());
                }
            }

            for access in &pass.buffer_accesses {
                let resource_name = self.resource_name(access.resource).unwrap_or("?");
                let Some(desc) = self.buffer_desc_by_id.get(&access.resource).copied() else {
                    return Err(GraphValidationError::BufferAccessOnNonBuffer {
                        pass: pass.name.clone(),
                        resource: resource_name.to_string(),
                    }
                    .into());
                };
                validate_buffer_access_descriptor(
                    &pass.name,
                    resource_name,
                    access.mode,
                    access.usage,
                    access.stage,
                    access.range,
                    desc,
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn validate_pass_bindings(&self) -> Result<(), RenderGraphError> {
        for pass in &self.passes {
            crate::render_graph::pass_bindings::validate(pass, &pass.bindings)?;
        }
        Ok(())
    }

    /// Allocate the graph's declared transient buffers for every frame slot.
    pub fn initialize_transient_buffers(&mut self, backend: &B) -> Result<(), RenderGraphError> {
        for (&resource, &handle) in &self.imported_buffers {
            let Some(buffer) = B::buffer_by_handle(backend, handle) else {
                return Err(RenderGraphError::InvalidConfiguration(format!(
                    "Imported buffer '{}' has a stale handle",
                    self.resource_name(resource).unwrap_or("?")
                )));
            };
            let expected = self.buffer_desc_by_id.get(&resource).copied();
            if expected != Some(B::buffer_desc(buffer)) {
                return Err(RenderGraphError::InvalidConfiguration(format!(
                    "Imported buffer '{}' descriptor does not match its handle",
                    self.resource_name(resource).unwrap_or("?")
                )));
            }
        }

        if !self.transient_buffers_by_frame.is_empty() {
            return Ok(());
        }

        let mut frame_slots = Vec::with_capacity(B::transient_texture_frames());
        for _ in 0..B::transient_texture_frames() {
            let mut frame_buffers = HashMap::with_capacity(self.transient_buffers.len());
            for desc in &self.transient_buffers {
                let id = self
                    .resource_by_name
                    .get(&desc.name)
                    .copied()
                    .ok_or_else(|| {
                        RenderGraphError::Validation(
                            GraphValidationError::MissingResourceNamespaceEntry(desc.name.clone()),
                        )
                    })?;
                let buffer = B::create_transient_buffer(backend, desc.buffer)?;
                frame_buffers.insert(id, buffer);
            }
            frame_slots.push(frame_buffers);
        }
        self.transient_buffers_by_frame = frame_slots;

        Ok(())
    }
}
