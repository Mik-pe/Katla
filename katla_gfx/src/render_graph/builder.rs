//! Builder types for render graph pass construction.
//!
//! This module provides the public `PassBuilder` trait that all pass templates
//! implement, and the internal `InternalPassBuilder` struct used by the
//! frame graph builder.

use std::any::Any;
use std::collections::HashMap;

use super::error::RenderGraphError;
use super::pass::PassType;
use super::resource::GraphResourceHandle;

/// Pass builder trait.
///
/// Implemented by all pass templates (GeometryPass, FullscreenPass, etc.).
/// Converts a user-friendly pass template into the internal representation
/// used by the frame graph builder.
///
/// # Example
///
/// ```ignore
/// pub struct GeometryPass {
///     // ... fields
/// }
///
/// impl PassBuilder for GeometryPass {
///     fn as_builder(self) -> InternalPassBuilder {
///         InternalPassBuilder {
///             name: self.name,
///             pass_type: PassType::Graphics,
///             reads: self.reads,
///             writes: self.writes,
///             build_fn: Box::new(move |resource_map| {
///                 // Convert string names to handles and build pass data
///                 Ok(Box::new(pass_data))
///             }),
///         }
///     }
/// }
/// ```
pub trait PassBuilder: Any {
    /// Convert this pass template into an internal pass builder.
    ///
    /// This method consumes the pass template and produces an `InternalPassBuilder`
    /// that the frame graph builder uses to construct the actual pass.
    fn as_builder(self) -> InternalPassBuilder;
}

/// Internal pass builder representation.
///
/// Created from public pass templates at graph build time. Contains
/// string-based resource references that are resolved to handles
/// during compilation.
///
/// This type is `pub(crate)` - it's an implementation detail within
/// katla_gfx and not exposed to external users.
pub(crate) struct InternalPassBuilder {
    /// Human-readable name for debugging.
    pub name: String,

    /// Type of pass (graphics, compute, transfer).
    pub pass_type: PassType,

    /// Resource names this pass reads from.
    pub reads: Vec<String>,

    /// Resource names this pass writes to.
    pub writes: Vec<String>,

    /// Build function that converts string names to handles.
    ///
    /// Called during graph compilation with a map from resource names
    /// to handles. Returns pass-specific data as a boxed `dyn Any`.
    pub build_fn: Box<
        dyn FnOnce(&HashMap<String, GraphResourceHandle>) -> Result<Box<dyn Any>, RenderGraphError>,
    >,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestPass {
        name: String,
        reads: Vec<String>,
        writes: Vec<String>,
    }

    impl TestPass {
        fn new(name: impl Into<String>) -> Self {
            Self {
                name: name.into(),
                reads: Vec::new(),
                writes: Vec::new(),
            }
        }

        fn read(mut self, name: impl Into<String>) -> Self {
            self.reads.push(name.into());
            self
        }

        fn write(mut self, name: impl Into<String>) -> Self {
            self.writes.push(name.into());
            self
        }
    }

    impl PassBuilder for TestPass {
        fn as_builder(self) -> InternalPassBuilder {
            InternalPassBuilder {
                name: self.name,
                pass_type: PassType::Graphics,
                reads: self.reads,
                writes: self.writes,
                build_fn: Box::new(|_resource_map| Ok(Box::new(()))),
            }
        }
    }

    #[test]
    fn test_pass_builder_trait() {
        let pass = TestPass::new("test").read("input").write("output");

        let builder = pass.as_builder();

        assert_eq!(builder.name, "test");
        assert_eq!(builder.pass_type, PassType::Graphics);
        assert_eq!(builder.reads, vec!["input"]);
        assert_eq!(builder.writes, vec!["output"]);
    }

    #[test]
    fn test_build_fn_execution() {
        let pass = TestPass::new("test").read("color").write("depth");

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("color".to_string(), GraphResourceHandle::new(0));
        resource_map.insert("depth".to_string(), GraphResourceHandle::new(1));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }
}
