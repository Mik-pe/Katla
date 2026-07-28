//! Application-owned frame-graph selection and runtime bindings.
//!
//! The render graph is an engine facility, while graph topology belongs to the
//! application. [`ApplicationFrameGraph`] packages a graph with the optional
//! pass/resource bindings that Katla's built-in scene runtime may use. A graph
//! created with [`ApplicationFrameGraph::new`] has no hidden pass requirements
//! and executes without Katla injecting scene work.

use crate::error::AppResult;
use crate::resources::ResourceManager;
use crate::{FrameGraph, Renderer};

/// Factory invoked exactly once, after the renderer and resource paths exist.
pub type FrameGraphFactory =
    Box<dyn FnOnce(&mut Renderer, &ResourceManager) -> AppResult<ApplicationFrameGraph>>;

/// Selects who owns per-frame submissions and backend feature initialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameGraphRuntime {
    /// Execute the configured graph without Katla injecting scene, shadow,
    /// post-processing, particle, animation, or editor passes.
    #[default]
    GraphOnly,
    /// Use Katla's built-in scene renderer and submit work only to capabilities
    /// declared by [`FrameGraphBindings`].
    KatlaScene,
}

impl FrameGraphRuntime {
    pub(crate) fn uses_katla_scene(self) -> bool {
        matches!(self, Self::KatlaScene)
    }
}

/// Optional pass names consumed by Katla's built-in scene runtime.
///
/// `None` means the capability is absent. A declared name must exist in the
/// selected graph; construction fails with an actionable error otherwise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameGraphPassBindings {
    pub depth_prepass: Option<String>,
    /// Pass whose built-in scene output supports entity picking. The same
    /// pass may also serve another role, such as a depth prepass or geometry.
    pub picking: Option<String>,
    pub geometry: Option<String>,
    pub shadow: Option<String>,
    pub outline: Option<String>,
    pub stencil_indicator: Option<String>,
    pub ui: Option<String>,
    pub tonemap: Option<String>,
    pub wallhack_overlay: Option<String>,
}

/// Optional transient resource names consumed by Katla's built-in scene runtime.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameGraphResourceBindings {
    /// Object-ID image used by backends whose picking target is graph-owned.
    /// Backends with a dedicated picking target may leave this unset.
    pub object_id: Option<String>,
    pub hdr_color: Option<String>,
    pub viewport: Option<String>,
    pub shadow_atlas: Option<String>,
    pub stencil_indicator: Option<String>,
}

/// Capability map between an application graph and Katla's optional built-ins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrameGraphBindings {
    pub passes: FrameGraphPassBindings,
    pub resources: FrameGraphResourceBindings,
}

impl FrameGraphBindings {
    /// Validate every declared resource capability against the selected graph.
    pub(crate) fn validate_resources(&self, graph: &FrameGraph) -> AppResult<()> {
        for (role, name) in [
            ("object_id", &self.resources.object_id),
            ("hdr_color", &self.resources.hdr_color),
            ("viewport", &self.resources.viewport),
            ("shadow_atlas", &self.resources.shadow_atlas),
            ("stencil_indicator", &self.resources.stencil_indicator),
        ] {
            let Some(name) = name.as_deref() else {
                continue;
            };
            if graph.resource_id(name).is_none() {
                return Err(crate::AppError::Other {
                    message: format!(
                        "Frame-graph binding '{role}' references missing resource '{name}'"
                    ),
                });
            }
        }
        Ok(())
    }

    /// Bindings used by Katla's current editor/scene preset.
    pub(crate) fn katla_editor() -> Self {
        Self {
            passes: FrameGraphPassBindings {
                depth_prepass: Some("depth_prepass".into()),
                picking: Some("depth_prepass".into()),
                geometry: Some("geometry".into()),
                shadow: Some("shadow".into()),
                outline: Some("outline".into()),
                stencil_indicator: Some("stencil_indicator".into()),
                ui: Some("ui".into()),
                tonemap: Some("tonemap".into()),
                wallhack_overlay: Some("wallhack_overlay".into()),
            },
            resources: FrameGraphResourceBindings {
                object_id: Some("object_id".into()),
                hdr_color: Some("hdr_color".into()),
                viewport: Some("viewport_0".into()),
                shadow_atlas: Some("shadow_atlas".into()),
                stencil_indicator: Some("stencil_indicator".into()),
            },
        }
    }

    /// Bindings used by the Metal variant of Katla's editor preset.
    #[cfg(target_os = "macos")]
    pub(crate) fn katla_editor_metal() -> Self {
        Self {
            passes: FrameGraphPassBindings {
                depth_prepass: Some("depth_prepass".into()),
                picking: Some("geometry".into()),
                geometry: Some("geometry".into()),
                shadow: Some("shadow".into()),
                outline: Some("outline".into()),
                stencil_indicator: None,
                ui: Some("ui".into()),
                tonemap: Some("tonemap".into()),
                wallhack_overlay: None,
            },
            resources: FrameGraphResourceBindings {
                object_id: None,
                hdr_color: Some("hdr_color".into()),
                viewport: Some("viewport_0".into()),
                shadow_atlas: None,
                stencil_indicator: None,
            },
        }
    }
}

/// A frame graph plus the explicit capabilities and runtime policy selected by
/// the application.
pub struct ApplicationFrameGraph {
    graph: FrameGraph,
    bindings: FrameGraphBindings,
    runtime: FrameGraphRuntime,
}

impl ApplicationFrameGraph {
    /// Create a fully application-owned graph.
    ///
    /// Katla will execute the graph, but will not inject built-in scene work or
    /// require any named passes/resources.
    pub fn new(graph: FrameGraph) -> Self {
        Self {
            graph,
            bindings: FrameGraphBindings::default(),
            runtime: FrameGraphRuntime::GraphOnly,
        }
    }

    /// Declare optional names that Katla's built-in runtime may target.
    pub fn with_bindings(mut self, bindings: FrameGraphBindings) -> Self {
        self.bindings = bindings;
        self
    }

    /// Select the runtime responsible for per-frame submissions.
    pub fn with_runtime(mut self, runtime: FrameGraphRuntime) -> Self {
        self.runtime = runtime;
        self
    }

    pub fn graph(&self) -> &FrameGraph {
        &self.graph
    }

    pub fn bindings(&self) -> &FrameGraphBindings {
        &self.bindings
    }

    pub fn runtime(&self) -> FrameGraphRuntime {
        self.runtime
    }

    pub(crate) fn into_parts(self) -> (FrameGraph, FrameGraphBindings, FrameGraphRuntime) {
        (self.graph, self.bindings, self.runtime)
    }
}

/// Explicit preset for Katla's current scene + editor rendering stack.
///
/// This remains the `ApplicationBuilder` default, but applications may replace
/// it with [`ApplicationBuilder::with_frame_graph`](super::builder::ApplicationBuilder::with_frame_graph).
#[derive(Debug, Clone, Copy, Default)]
pub struct KatlaEditorFrameGraphPreset;

/// Build an empty graph for whichever renderer backend is active.
///
/// This is useful for minimal applications and tests:
///
/// ```no_run
/// use katla_app::prelude::*;
///
/// let builder = ApplicationBuilder::new().with_frame_graph(|renderer, _resources| {
///     Ok(ApplicationFrameGraph::new(empty_frame_graph(renderer)))
/// });
/// # let _ = builder;
/// ```
pub fn empty_frame_graph(renderer: &Renderer) -> FrameGraph {
    match renderer {
        katla_gfx::AnyRenderer::Vulkan(_) => {
            FrameGraph::from_vulkan(katla_gfx::render_graph::FrameGraph::<
                katla_gfx::VulkanRenderer,
            >::new())
        }
        #[cfg(target_os = "macos")]
        katla_gfx::AnyRenderer::Metal(_) => {
            FrameGraph::from_metal(katla_gfx::render_graph::FrameGraph::<
                katla_gfx::MetalRenderer,
            >::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::PassIds;
    use katla_gfx::render_graph::{PassDesc, PassType};

    fn graph_with_passes(names: &[&str]) -> FrameGraph {
        let mut graph = FrameGraph::new();
        for name in names {
            graph.add_pass(PassDesc::new(*name, PassType::Graphics, vec![], vec![]));
        }
        graph
    }

    #[test]
    fn empty_graph_has_no_implicit_pass_bindings() {
        let graph = graph_with_passes(&[]);
        let ids = PassIds::resolve(&graph, &FrameGraphPassBindings::default()).unwrap();
        assert_eq!(ids, PassIds::default());
    }

    #[test]
    fn ui_only_graph_resolves_only_ui_capability() {
        let graph = graph_with_passes(&["overlay"]);
        let bindings = FrameGraphPassBindings {
            ui: Some("overlay".into()),
            ..Default::default()
        };
        let ids = PassIds::resolve(&graph, &bindings).unwrap();
        assert_eq!(ids.ui, graph.pass_id("overlay"));
        assert!(ids.geometry.is_none());
        assert!(ids.shadow.is_none());
    }

    #[test]
    fn picking_can_share_an_existing_scene_pass() {
        let graph = graph_with_passes(&["scene"]);
        let bindings = FrameGraphPassBindings {
            geometry: Some("scene".into()),
            picking: Some("scene".into()),
            ..Default::default()
        };
        let ids = PassIds::resolve(&graph, &bindings).unwrap();
        assert_eq!(ids.geometry, graph.pass_id("scene"));
        assert_eq!(ids.picking, graph.pass_id("scene"));
    }

    #[test]
    fn geometry_only_graph_resolves_only_geometry_capability() {
        let graph = graph_with_passes(&["main_scene"]);
        let bindings = FrameGraphPassBindings {
            geometry: Some("main_scene".into()),
            ..Default::default()
        };
        let ids = PassIds::resolve(&graph, &bindings).unwrap();
        assert_eq!(ids.geometry, graph.pass_id("main_scene"));
        assert!(ids.ui.is_none());
    }

    #[test]
    fn declared_missing_pass_is_an_error() {
        let graph = graph_with_passes(&[]);
        let bindings = FrameGraphPassBindings {
            geometry: Some("missing".into()),
            ..Default::default()
        };
        let error = PassIds::resolve(&graph, &bindings).unwrap_err();
        assert!(error.to_string().contains("missing pass 'missing'"));
    }

    #[test]
    fn refresh_reindexes_and_clears_removed_capabilities() {
        let mut graph = graph_with_passes(&["geometry"]);
        let mut bindings = FrameGraphPassBindings {
            geometry: Some("geometry".into()),
            ..Default::default()
        };
        let mut ids = PassIds::resolve(&graph, &bindings).unwrap();
        assert_eq!(ids.geometry, graph.pass_id("geometry"));

        graph.insert_pass(
            0,
            PassDesc::new("before_geometry", PassType::Graphics, vec![], vec![]),
        );
        ids.refresh(&graph, &bindings).unwrap();
        assert_eq!(ids.geometry, graph.pass_id("geometry"));

        bindings.geometry = None;
        ids.refresh(&graph, &bindings).unwrap();
        assert!(ids.geometry.is_none());
    }

    #[test]
    fn declared_missing_resource_is_an_error() {
        let graph = graph_with_passes(&[]);
        let bindings = FrameGraphBindings {
            resources: FrameGraphResourceBindings {
                hdr_color: Some("not_there".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let error = bindings.validate_resources(&graph).unwrap_err();
        assert!(error.to_string().contains("missing resource 'not_there'"));
    }
}
