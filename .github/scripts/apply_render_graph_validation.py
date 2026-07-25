from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"expected source fragment not found: {label}")
    return text.replace(old, new, 1)


error_path = Path("katla_gfx/src/render_graph/error.rs")
error = error_path.read_text()

if "pub enum GraphValidationError" not in error:
    error = replace_once(
        error,
        "use super::resource::ResourceState;\n",
        '''use super::resource::ResourceState;

/// Structural errors detected before a render graph is compiled or allocated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphValidationError {
    /// A resource declaration or import has an empty name.
    EmptyResourceName,
    /// A resource name is declared more than once.
    DuplicateResourceName(String),
    /// A transient texture has a zero-sized extent.
    InvalidResourceExtent {
        resource: String,
        width: u32,
        height: u32,
    },
    /// An imported resource uses the sentinel NONE handle.
    InvalidImportedResource(String),
    /// A pass has an empty name.
    EmptyPassName,
    /// A pass name is declared more than once.
    DuplicatePassName(String),
    /// A pass references an empty resource name.
    EmptyPassResource { pass: String },
    /// A pass references a resource that was never declared or imported.
    UndeclaredResource { pass: String, resource: String },
    /// A transient descriptor is missing from the graph resource namespace.
    MissingResourceNamespaceEntry(String),
}

impl fmt::Display for GraphValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyResourceName => write!(f, "resource names must not be empty"),
            Self::DuplicateResourceName(name) => {
                write!(f, "resource '{}' is declared more than once", name)
            }
            Self::InvalidResourceExtent {
                resource,
                width,
                height,
            } => write!(
                f,
                "resource '{}' has invalid extent {}x{}",
                resource, width, height
            ),
            Self::InvalidImportedResource(name) => write!(
                f,
                "imported resource '{}' uses GraphResourceHandle::NONE",
                name
            ),
            Self::EmptyPassName => write!(f, "pass names must not be empty"),
            Self::DuplicatePassName(name) => {
                write!(f, "pass '{}' is declared more than once", name)
            }
            Self::EmptyPassResource { pass } => {
                write!(f, "pass '{}' references an empty resource name", pass)
            }
            Self::UndeclaredResource { pass, resource } => write!(
                f,
                "pass '{}' references undeclared resource '{}'",
                pass, resource
            ),
            Self::MissingResourceNamespaceEntry(resource) => write!(
                f,
                "transient resource '{}' is missing from the graph namespace",
                resource
            ),
        }
    }
}
''',
        "validation error type",
    )

    error = replace_once(
        error,
        "pub enum RenderGraphError {\n",
        "pub enum RenderGraphError {\n    /// Invalid graph structure discovered before compilation.\n    Validation(GraphValidationError),\n",
        "validation error variant",
    )

    error = replace_once(
        error,
        '''impl fmt::Display for RenderGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
''',
        '''impl fmt::Display for RenderGraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(error) => write!(f, "Invalid render graph: {}", error),
''',
        "validation display arm",
    )

    error = replace_once(
        error,
        "impl std::error::Error for RenderGraphError {}\n",
        '''impl std::error::Error for RenderGraphError {}

impl From<GraphValidationError> for RenderGraphError {
    fn from(error: GraphValidationError) -> Self {
        Self::Validation(error)
    }
}
''',
        "validation error conversion",
    )

error_path.write_text(error)

mod_path = Path("katla_gfx/src/render_graph/mod.rs")
mod_source = mod_path.read_text()
mod_source = mod_source.replace(
    "pub use error::RenderGraphError;",
    "pub use error::{GraphValidationError, RenderGraphError};",
)
mod_path.write_text(mod_source)

graph_path = Path("katla_gfx/src/render_graph/frame_graph.rs")
graph = graph_path.read_text()

if "fn validate(&self) -> Result<(), RenderGraphError>" not in graph:
    graph = replace_once(
        graph,
        "use std::collections::HashMap;",
        "use std::collections::{HashMap, HashSet};",
        "HashSet import",
    )
    graph = replace_once(
        graph,
        "use super::error::RenderGraphError;",
        "use super::error::{GraphValidationError, RenderGraphError};",
        "validation error import",
    )
    graph = replace_once(
        graph,
        "    resources: HashMap<String, GraphResourceHandle>,",
        "    resources: Vec<(String, GraphResourceHandle)>,",
        "ordered imported resources",
    )
    graph = replace_once(
        graph,
        "            resources: HashMap::new(),",
        "            resources: Vec::new(),",
        "ordered resource initialization",
    )
    graph = replace_once(
        graph,
        '''    pub fn import_resource(mut self, name: impl Into<String>, handle: GraphResourceHandle) -> Self {
        self.resources.insert(name.into(), handle);
        self
    }
''',
        '''    pub fn import_resource(mut self, name: impl Into<String>, handle: GraphResourceHandle) -> Self {
        self.resources.push((name.into(), handle));
        self
    }
''',
        "ordered import insertion",
    )
    graph = replace_once(
        graph,
        '''                let resource_id = self
                    .resource_by_name
                    .get(&desc.name)
                    .copied()
                    .unwrap_or(ResourceId(frame_textures.len() as u32));
''',
        '''                let resource_id = self
                    .resource_by_name
                    .get(&desc.name)
                    .copied()
                    .ok_or_else(|| {
                        RenderGraphError::Validation(
                            GraphValidationError::MissingResourceNamespaceEntry(desc.name.clone()),
                        )
                    })?;
''',
        "transient namespace fallback",
    )

    build_start = graph.index(
        "    /// Build the frame graph.\n    pub fn build<B: RenderGraphBackend>"
    )
    build_end = graph.index("\n}\n\nimpl Default for FrameGraphBuilder", build_start)

    replacement = r'''    fn validate(&self) -> Result<(), RenderGraphError> {
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

        for (name, handle) in &self.resources {
            if name.trim().is_empty() {
                return Err(GraphValidationError::EmptyResourceName.into());
            }
            if handle.is_none() {
                return Err(GraphValidationError::InvalidImportedResource(name.clone()).into());
            }
            if !resource_names.insert(name.clone()) {
                return Err(GraphValidationError::DuplicateResourceName(name.clone()).into());
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

            for resource in pass.reads.iter().chain(&pass.writes) {
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

        Ok(())
    }

    /// Build the frame graph after validating its complete resource namespace.
    pub fn build<B: RenderGraphBackend>(self) -> Result<FrameGraph<B>, RenderGraphError> {
        self.validate()?;

        let FrameGraphBuilder {
            pass_builders,
            resources,
            transient_resources,
        } = self;

        let transient_names = transient_resources
            .iter()
            .map(|desc| desc.name.clone())
            .collect::<Vec<_>>();

        let mut graph = FrameGraph::new();
        graph.transient_resources = transient_resources;

        // The swapchain backbuffer is the only built-in resource. Every other
        // name has already been declared or imported by the validated builder.
        graph.create_resource_id(BACKBUFFER_NAME);
        for name in transient_names {
            graph.create_resource_id(name);
        }
        for (name, _) in &resources {
            graph.create_resource_id(name.clone());
        }

        let mut global_resource_map = HashMap::new();
        for (name, &resource_id) in &graph.resource_by_name {
            global_resource_map.insert(name.clone(), GraphResourceHandle::new(resource_id.0));
        }
        for (name, handle) in &resources {
            global_resource_map.insert(name.clone(), *handle);
        }

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

            let mut pass = PassDesc::new(
                pass_builder.name,
                pass_builder.pass_type,
                read_ids,
                write_ids,
            );

            pass.pipeline = pass_builder.pipeline;
            pass.tonemap_params = pass_builder.tonemap_params;
            pass.overlay_params = pass_builder.overlay_params;
            pass.material = pass_builder.material;
            pass.output_format = pass_builder.output_format;
            pass.uses_depth = pass_builder.uses_depth;
            pass.depth_attachment = pass_builder.depth_attachment;
            pass.kind = pass_builder.kind;

            if let Some(geom_data) = pass_data.downcast_ref::<GeometryPassData>() {
                for (handle, format, load_op, store_op, clear_value) in &geom_data.colors {
                    pass.color_attachments.push((
                        ResourceId(handle.index()),
                        *format,
                        *load_op,
                        *store_op,
                        *clear_value,
                    ));
                }
            } else if let Some(dp_data) =
                pass_data.downcast_ref::<crate::render_graph::passes::depth_prepass::DepthPrepassData>()
            {
                for (handle, format, load_op, store_op, clear_value) in &dp_data.colors {
                    pass.color_attachments.push((
                        ResourceId(handle.index()),
                        *format,
                        *load_op,
                        *store_op,
                        *clear_value,
                    ));
                }
            }

            if let Some(comp_data) =
                pass_data.downcast_ref::<crate::render_graph::passes::CompositePassData>()
            {
                pass.compositing_viewports = Some(comp_data.viewports.clone());
            }

            graph.add_pass(pass);
        }

        graph.compile()?;
        Ok(graph)
    }
'''
    graph = graph[:build_start] + replacement + graph[build_end:]

    tests = r'''

    fn validation_resource(name: &str, width: u32, height: u32) -> GraphResourceDesc {
        GraphResourceDesc {
            name: name.to_string(),
            resource_type: super::super::resource::GraphResourceType::ColorAttachment {
                clear_value: None,
            },
            format: crate::texture::ImageFormat::R8G8B8A8Unorm,
            width,
            height,
            tracks_swapchain_size: true,
        }
    }

    fn validation_error(builder: FrameGraphBuilder) -> GraphValidationError {
        match builder.build::<MockBackend>() {
            Err(RenderGraphError::Validation(error)) => error,
            Err(error) => panic!("expected graph validation error, got {error}"),
            Ok(_) => panic!("expected graph validation to fail"),
        }
    }

    #[test]
    fn builder_rejects_duplicate_resource_names() {
        let error = validation_error(
            FrameGraphBuilder::new()
                .create_resource(validation_resource("color", 1, 1))
                .import_resource("color", GraphResourceHandle::new(7)),
        );
        assert_eq!(
            error,
            GraphValidationError::DuplicateResourceName("color".to_string())
        );
    }

    #[test]
    fn builder_rejects_repeated_imports() {
        let error = validation_error(
            FrameGraphBuilder::new()
                .import_resource("external", GraphResourceHandle::new(1))
                .import_resource("external", GraphResourceHandle::new(2)),
        );
        assert_eq!(
            error,
            GraphValidationError::DuplicateResourceName("external".to_string())
        );
    }

    #[test]
    fn builder_rejects_duplicate_pass_names() {
        let error = validation_error(
            FrameGraphBuilder::new()
                .add_pass(super::super::builder::SimplePass::new("same", PassType::Graphics))
                .add_pass(super::super::builder::SimplePass::new("same", PassType::Graphics)),
        );
        assert_eq!(
            error,
            GraphValidationError::DuplicatePassName("same".to_string())
        );
    }

    #[test]
    fn builder_rejects_undeclared_pass_resources() {
        let error = validation_error(FrameGraphBuilder::new().add_pass(
            super::super::builder::SimplePass::new("geometry", PassType::Graphics)
                .write("typo_color"),
        ));
        assert_eq!(
            error,
            GraphValidationError::UndeclaredResource {
                pass: "geometry".to_string(),
                resource: "typo_color".to_string(),
            }
        );
    }

    #[test]
    fn builder_rejects_invalid_resource_descriptors_and_imports() {
        assert_eq!(
            validation_error(
                FrameGraphBuilder::new()
                    .create_resource(validation_resource("color", 0, 64))
            ),
            GraphValidationError::InvalidResourceExtent {
                resource: "color".to_string(),
                width: 0,
                height: 64,
            }
        );
        assert_eq!(
            validation_error(
                FrameGraphBuilder::new()
                    .import_resource("external", GraphResourceHandle::NONE)
            ),
            GraphValidationError::InvalidImportedResource("external".to_string())
        );
    }

    #[test]
    fn builder_rejects_empty_names() {
        assert_eq!(
            validation_error(
                FrameGraphBuilder::new().create_resource(validation_resource("", 1, 1))
            ),
            GraphValidationError::EmptyResourceName
        );
        assert_eq!(
            validation_error(FrameGraphBuilder::new().add_pass(
                super::super::builder::SimplePass::new("", PassType::Graphics)
            )),
            GraphValidationError::EmptyPassName
        );
    }

    #[test]
    fn builder_accepts_declared_resources_and_builtin_backbuffer() {
        let result = FrameGraphBuilder::new()
            .create_resource(validation_resource("color", 64, 64))
            .add_pass(
                super::super::builder::SimplePass::new("geometry", PassType::Graphics)
                    .write("color"),
            )
            .add_pass(
                super::super::builder::SimplePass::new("present", PassType::Graphics)
                    .read("color")
                    .write(BACKBUFFER_NAME),
            )
            .build::<MockBackend>();
        assert!(result.is_ok());
    }

    #[test]
    fn transient_initialization_rejects_a_missing_namespace_entry() {
        let mut graph = TestGraph::new();
        graph
            .transient_resources
            .push(validation_resource("orphan", 1, 1));

        let error = graph.initialize_transient_textures(&MockBackend).unwrap_err();
        assert!(matches!(
            error,
            RenderGraphError::Validation(
                GraphValidationError::MissingResourceNamespaceEntry(resource)
            ) if resource == "orphan"
        ));
    }
'''

    final_brace = graph.rfind("\n}")
    graph = graph[:final_brace] + tests + graph[final_brace:]

graph_path.write_text(graph)
