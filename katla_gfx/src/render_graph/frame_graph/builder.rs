//! Graph authoring and named resource resolution.

use super::*;

/// One imported external image, with its state contract.
#[derive(Debug, Clone)]
struct ImportedResource {
    name: String,
    handle: TextureHandle,
    contract: ImportedImageContract,
}

/// One external renderer-owned buffer imported into the graph namespace.
#[derive(Debug, Clone)]
struct ImportedBuffer {
    name: String,
    handle: BufferHandle,
    desc: BufferDesc,
}

/// Builder for constructing a frame graph.
///
/// Provides a fluent API for adding passes before building the executable [`FrameGraph`].
pub struct FrameGraphBuilder {
    pass_builders: Vec<InternalPassBuilder>,
    resources: Vec<ImportedResource>,
    buffers: Vec<ImportedBuffer>,
    transient_resources: Vec<GraphResourceDesc>,
    transient_buffers: Vec<GraphBufferDesc>,
    exported_resources: BTreeSet<String>,
    backbuffer_contract: ImportedImageContract,
}

impl FrameGraphBuilder {
    /// Create a new frame graph builder.
    pub fn new() -> Self {
        Self {
            pass_builders: Vec::new(),
            resources: Vec::new(),
            buffers: Vec::new(),
            transient_resources: Vec::new(),
            transient_buffers: Vec::new(),
            exported_resources: BTreeSet::from([BACKBUFFER_NAME.to_string()]),
            backbuffer_contract: DEFAULT_BACKBUFFER_CONTRACT,
        }
    }

    /// Add a pass to the graph.
    pub fn add_pass(mut self, pass: impl PassBuilder + 'static) -> Self {
        self.pass_builders.push(pass.as_builder());
        self
    }

    /// Add a pass whose observable effect is not represented by a resource write.
    ///
    /// Side effects are explicit liveness roots. Ordinary render work should expose
    /// an output resource instead, so the compiler can remove unused branches.
    pub fn add_side_effect_pass(mut self, pass: impl PassBuilder + 'static) -> Self {
        let mut pass = pass.as_builder();
        pass.side_effect = true;
        self.pass_builders.push(pass);
        self
    }

    /// Mark a resource's final value as externally observable.
    ///
    /// The swapchain backbuffer is exported by default. Offscreen outputs used for
    /// picking, readback, streaming, or interop must be exported explicitly.
    pub fn export_resource(mut self, name: impl Into<String>) -> Self {
        self.exported_resources.insert(name.into());
        self
    }

    /// Import an external image into the graph with an explicit state contract.
    ///
    /// `contract.initial` declares the state the image arrives in; loading its
    /// contents from a pass requires a non-`Undefined` initial state.
    /// `contract.required_final` declares the state the graph must leave the
    /// image in (e.g. `ResourceState::PresentSrc` for an image presented
    /// after the frame). Use `ImportedImageContract::undefined()` when
    /// neither side of the contract is observable.
    pub fn import_resource(
        mut self,
        name: impl Into<String>,
        handle: TextureHandle,
        contract: ImportedImageContract,
    ) -> Self {
        self.resources.push(ImportedResource {
            name: name.into(),
            handle,
            contract,
        });
        self
    }

    /// Import a renderer-owned typed buffer into the graph under a name.
    pub fn import_buffer(
        mut self,
        name: impl Into<String>,
        handle: BufferHandle,
        desc: BufferDesc,
    ) -> Self {
        self.buffers.push(ImportedBuffer {
            name: name.into(),
            handle,
            desc,
        });
        self
    }

    /// Override the state contract of the built-in backbuffer.
    ///
    /// By default the backbuffer is imported with observable contents (the
    /// previously presented frame), so passes may load it without an in-graph
    /// producer. Applications that present the backbuffer declare the final
    /// state here, e.g.
    /// `backbuffer_contract(ImportedImageContract::arrives_in(ResourceState::ColorAttachment).must_end_in(ResourceState::PresentSrc))`.
    pub fn backbuffer_contract(mut self, contract: ImportedImageContract) -> Self {
        self.backbuffer_contract = contract;
        self
    }

    /// Create a transient resource in the frame graph.
    pub fn create_resource(mut self, desc: GraphResourceDesc) -> Self {
        self.transient_resources.push(desc);
        self
    }

    /// Create a graph-owned transient buffer.
    pub fn create_buffer(mut self, desc: GraphBufferDesc) -> Self {
        self.transient_buffers.push(desc);
        self
    }

    fn validate_buffer_accesses(&self) -> Result<(), RenderGraphError> {
        let buffer_names = self
            .transient_buffers
            .iter()
            .map(|buffer| buffer.name.as_str())
            .chain(self.buffers.iter().map(|buffer| buffer.name.as_str()))
            .collect::<HashSet<_>>();
        for pass in &self.pass_builders {
            let typed_buffer_names = pass
                .buffer_accesses
                .iter()
                .map(|access| access.resource.as_str())
                .collect::<HashSet<_>>();
            for resource in pass.reads.iter().chain(&pass.writes) {
                if buffer_names.contains(resource.as_str())
                    && !typed_buffer_names.contains(resource.as_str())
                {
                    return Err(GraphValidationError::MissingTypedBufferAccess {
                        pass: pass.name.clone(),
                        resource: resource.clone(),
                    }
                    .into());
                }
            }
            for access in &pass.image_accesses {
                if buffer_names.contains(access.resource.as_str()) {
                    return Err(GraphValidationError::ImageAccessOnNonImage {
                        pass: pass.name.clone(),
                        resource: access.resource.clone(),
                    }
                    .into());
                }
            }
            for access in &pass.buffer_accesses {
                let desc = self
                    .transient_buffers
                    .iter()
                    .find(|buffer| buffer.name == access.resource)
                    .map(|buffer| buffer.buffer)
                    .or_else(|| {
                        self.buffers
                            .iter()
                            .find(|buffer| buffer.name == access.resource)
                            .map(|buffer| buffer.desc)
                    })
                    .ok_or_else(|| GraphValidationError::BufferAccessOnNonBuffer {
                        pass: pass.name.clone(),
                        resource: access.resource.clone(),
                    })?;

                validate_buffer_access_descriptor(
                    &pass.name,
                    &access.resource,
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

    fn validate(&self) -> Result<(), RenderGraphError> {
        let mut resource_names = HashSet::from([BACKBUFFER_NAME.to_string()]);

        for desc in &self.transient_resources {
            if desc.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyResourceName.into());
            }
            if desc.width == 0 || desc.height == 0 {
                return Err(GraphValidationError::InvalidResourceExtent {
                    resource: desc.name.clone(),
                    width: desc.width,
                    height: desc.height,
                }
                .into());
            }
            if !resource_names.insert(desc.name.clone()) {
                return Err(GraphValidationError::DuplicateResourceName(desc.name.clone()).into());
            }
        }

        for desc in &self.transient_buffers {
            if desc.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyResourceName.into());
            }
            if desc.buffer.size == 0 || desc.buffer.usages.is_empty() {
                return Err(GraphValidationError::InvalidBufferDescriptor {
                    resource: desc.name.clone(),
                    size: desc.buffer.size,
                }
                .into());
            }
            if !resource_names.insert(desc.name.clone()) {
                return Err(GraphValidationError::DuplicateResourceName(desc.name.clone()).into());
            }
        }

        let mut buffer_identities = HashMap::new();
        for buffer in &self.buffers {
            if buffer.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyResourceName.into());
            }
            if buffer.handle.is_none() {
                return Err(
                    GraphValidationError::InvalidImportedBuffer(buffer.name.clone()).into(),
                );
            }
            if buffer.desc.size == 0 || buffer.desc.usages.is_empty() {
                return Err(GraphValidationError::InvalidBufferDescriptor {
                    resource: buffer.name.clone(),
                    size: buffer.desc.size,
                }
                .into());
            }
            if !resource_names.insert(buffer.name.clone()) {
                return Err(
                    GraphValidationError::DuplicateResourceName(buffer.name.clone()).into(),
                );
            }
            if let Some(first) = buffer_identities.insert(
                (buffer.handle.index(), buffer.handle.generation()),
                buffer.name.clone(),
            ) {
                return Err(GraphValidationError::DuplicateImportedIdentity {
                    kind: "buffer",
                    first,
                    duplicate: buffer.name.clone(),
                }
                .into());
            }
        }

        let mut image_identities = HashMap::new();
        for resource in &self.resources {
            if resource.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyResourceName.into());
            }
            if resource.handle.is_none() {
                return Err(
                    GraphValidationError::InvalidImportedResource(resource.name.clone()).into(),
                );
            }
            if !resource_names.insert(resource.name.clone()) {
                return Err(
                    GraphValidationError::DuplicateResourceName(resource.name.clone()).into(),
                );
            }
            if let Some(first) = image_identities.insert(
                (resource.handle.index(), resource.handle.generation()),
                resource.name.clone(),
            ) {
                return Err(GraphValidationError::DuplicateImportedIdentity {
                    kind: "image",
                    first,
                    duplicate: resource.name.clone(),
                }
                .into());
            }
        }

        for resource in &self.exported_resources {
            if !resource_names.contains(resource) {
                return Err(
                    GraphValidationError::UndeclaredExportedResource(resource.clone()).into(),
                );
            }
        }

        let mut pass_names = HashSet::new();
        for pass in &self.pass_builders {
            if pass.name.trim().is_empty() {
                return Err(GraphValidationError::EmptyPassName.into());
            }
            if !pass_names.insert(pass.name.clone()) {
                return Err(GraphValidationError::DuplicatePassName(pass.name.clone()).into());
            }

            for resource in pass
                .reads
                .iter()
                .chain(&pass.writes)
                .chain(pass.image_accesses.iter().map(|access| &access.resource))
                .chain(pass.buffer_accesses.iter().map(|access| &access.resource))
            {
                if resource.trim().is_empty() {
                    return Err(GraphValidationError::EmptyPassResource {
                        pass: pass.name.clone(),
                    }
                    .into());
                }
                if !resource_names.contains(resource) {
                    return Err(GraphValidationError::UndeclaredResource {
                        pass: pass.name.clone(),
                        resource: resource.clone(),
                    }
                    .into());
                }
            }
        }

        self.validate_buffer_accesses()?;

        Ok(())
    }

    /// Build the frame graph after validating its complete resource namespace.
    pub fn build<B: RenderGraphBackend>(self) -> Result<FrameGraph<B>, RenderGraphError> {
        self.validate()?;

        let FrameGraphBuilder {
            pass_builders,
            resources,
            buffers,
            transient_resources,
            transient_buffers,
            exported_resources,
            backbuffer_contract,
        } = self;

        let transient_names = transient_resources
            .iter()
            .map(|desc| desc.name.clone())
            .collect::<Vec<_>>();
        let buffer_names = transient_buffers
            .iter()
            .map(|desc| desc.name.clone())
            .chain(buffers.iter().map(|buffer| buffer.name.clone()))
            .collect::<Vec<_>>();

        let mut graph = FrameGraph::new();
        graph.transient_resources = transient_resources;
        graph.transient_buffers = transient_buffers;

        // The swapchain backbuffer is the only built-in resource. Every other
        // name has already been declared or imported by the validated builder.
        let backbuffer_id = graph.create_resource_id(BACKBUFFER_NAME);
        graph
            .imported_contracts
            .insert(backbuffer_id, backbuffer_contract);
        for name in transient_names {
            graph.create_resource_id(name);
        }
        for name in buffer_names {
            graph.create_resource_id(name);
        }
        for resource in &resources {
            let id = graph.create_resource_id(resource.name.clone());
            graph.imported_contracts.insert(id, resource.contract);
            graph.imported_images.insert(id, resource.handle);
        }
        for buffer in buffers {
            let id = graph.create_resource_id(buffer.name);
            graph.buffer_desc_by_id.insert(id, buffer.desc);
            graph.imported_buffers.insert(id, buffer.handle);
        }
        for buffer in &graph.transient_buffers {
            let id = graph.resource_by_name[&buffer.name];
            graph.buffer_desc_by_id.insert(id, buffer.buffer);
        }

        let mut global_resource_map = HashMap::new();
        for (name, &resource_id) in &graph.resource_by_name {
            global_resource_map.insert(name.clone(), GraphResourceHandle::new(resource_id.0));
        }

        let exported_resource_ids = exported_resources
            .iter()
            .map(|name| {
                graph.resource_by_name.get(name).copied().ok_or_else(|| {
                    RenderGraphError::ResourceNotFound(format!(
                        "Exported resource '{}' was not created",
                        name
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        graph.configure_pass_culling(exported_resource_ids);

        for pass_builder in pass_builders {
            let pass_data = (pass_builder.build_fn)(&global_resource_map)?;
            let pass_name = pass_builder.name.clone();

            let read_ids = pass_builder
                .reads
                .iter()
                .map(|name| {
                    graph.resource_by_name.get(name).copied().ok_or_else(|| {
                        RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                            pass: pass_name.clone(),
                            resource: name.clone(),
                        })
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let write_ids = pass_builder
                .writes
                .iter()
                .map(|name| {
                    graph.resource_by_name.get(name).copied().ok_or_else(|| {
                        RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                            pass: pass_name.clone(),
                            resource: name.clone(),
                        })
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let explicit_image_accesses = pass_builder
                .image_accesses
                .iter()
                .map(|access| {
                    graph
                        .resource_by_name
                        .get(&access.resource)
                        .copied()
                        .map(|resource| access.resolve(resource))
                        .ok_or_else(|| {
                            RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                                pass: pass_name.clone(),
                                resource: access.resource.clone(),
                            })
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let has_explicit_image_accesses = !explicit_image_accesses.is_empty();

            let explicit_buffer_accesses = pass_builder
                .buffer_accesses
                .iter()
                .map(|access| {
                    graph
                        .resource_by_name
                        .get(&access.resource)
                        .copied()
                        .map(|resource| access.resolve(resource))
                        .ok_or_else(|| {
                            RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                                pass: pass_name.clone(),
                                resource: access.resource.clone(),
                            })
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let mut pass = PassDesc::new(
                pass_builder.name,
                pass_builder.pass_type,
                read_ids,
                write_ids,
            );

            if has_explicit_image_accesses {
                pass.set_image_accesses(explicit_image_accesses);
            }
            pass.set_buffer_accesses(explicit_buffer_accesses);

            pass.material = pass_builder.material;
            pass.output_format = pass_builder.output_format;
            pass.uses_depth = pass_builder.uses_depth;
            pass.depth_target = pass_builder
                .depth_target
                .as_ref()
                .map(|name| {
                    graph
                        .resource_id(name)
                        .ok_or_else(|| RenderGraphError::ResourceNotFound(name.clone()))
                })
                .transpose()?;
            pass.depth_attachment = pass_builder.depth_attachment;
            pass.kind = pass_builder.kind;
            pass.side_effect = pass_builder.side_effect;
            if let Some(commands) =
                pass_data.downcast_ref::<Vec<crate::render_graph::compute::ComputeCommand>>()
            {
                pass.commands = commands.clone();
            }

            pass.color_attachments = pass_builder
                .color_attachments
                .iter()
                .map(|(name, ops)| {
                    graph
                        .resource_by_name
                        .get(name)
                        .copied()
                        .map(|resource| (resource, *ops))
                        .ok_or_else(|| {
                            RenderGraphError::Validation(GraphValidationError::UndeclaredResource {
                                pass: pass_name.clone(),
                                resource: name.clone(),
                            })
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Every graphics pass that uses depth gets an explicit depth
            // contract: the canonical reverse-Z default when the template
            // declares nothing. Execution never guesses.
            if pass_builder.pass_type == PassType::Graphics
                && pass.uses_depth
                && pass.depth_attachment.is_none()
            {
                pass.depth_attachment = Some(DepthStencilAttachmentOps::reverse_z_default());
            }

            if !has_explicit_image_accesses {
                pass.refine_inferred_image_accesses();
            }
            if let Some(resource) = pass.depth_target {
                let ops = pass.depth_attachment.ok_or_else(|| {
                    RenderGraphError::InvalidConfiguration(format!(
                        "Pass '{}' declares a depth target without attachment operations",
                        pass.name
                    ))
                })?;
                let mut access =
                    crate::render_graph::access::ImageAccess::depth_attachment_write(resource);
                if ops.depth.load == LoadOp::Load || ops.stencil.load == LoadOp::Load {
                    access.mode = ResourceAccessMode::ReadWrite;
                }
                pass.image_accesses.retain(|existing| {
                    existing.resource != resource
                        || existing.usage
                            != crate::render_graph::access::ResourceAccessUsage::DepthStencilAttachment
                });
                if graph
                    .resource_format_for_target(resource)
                    .is_some_and(|format| matches!(format, crate::texture::ImageFormat::D32Sfloat))
                {
                    access.range = crate::render_graph::access::ImageSubresourceRange::WHOLE_DEPTH;
                }
                pass.image_accesses.push(access);
                pass.set_image_accesses(pass.image_accesses.clone());
            }

            if let Some(comp_data) =
                pass_data.downcast_ref::<crate::render_graph::passes::CompositePassData>()
            {
                pass.compositing_viewports = Some(comp_data.viewports.clone());
            }

            graph.add_pass(pass)?;
        }

        graph.compile()?;
        Ok(graph)
    }
}

impl Default for FrameGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}
