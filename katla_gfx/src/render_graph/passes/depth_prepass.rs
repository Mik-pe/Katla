//! Depth prepass template.
//!
//! Renders scene depth from camera's perspective using a depth-only pipeline.
//! The depth buffer is then reused by the geometry pass via `LoadOp::Load`.

use std::collections::HashMap;

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::pass::PassType;
use super::super::resource::GraphResourceHandle;

/// Depth prepass template.
///
/// Renders only depth from the camera's perspective. No color output.
/// The geometry pass should follow with `depth_config(LoadOp::Load, ...)`.
#[derive(Debug, Clone)]
pub struct DepthPrepass {
    name: String,
    reads: Vec<String>,
}

impl DepthPrepass {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
        }
    }

    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }
}

impl PassBuilder for DepthPrepass {
    fn as_builder(self) -> InternalPassBuilder {
        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads,
            writes: Vec::new(),
            pipeline: None,
            tonemap_params: None,
            material: None,
            output_format: None,
            build_fn: Box::new(|_resource_map: &HashMap<String, GraphResourceHandle>| {
                Ok(Box::new(DepthPrepassData))
            }),
            uses_depth: true,
            depth_attachment: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct DepthPrepassData;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_depth_prepass_new() {
        let pass = DepthPrepass::new("depth_prepass");
        assert_eq!(pass.name, "depth_prepass");
        assert!(pass.reads.is_empty());
    }

    #[test]
    fn test_depth_prepass_with_reads() {
        let pass = DepthPrepass::new("depth_prepass").read("shadow_atlas");
        assert_eq!(pass.reads, vec!["shadow_atlas"]);
    }

    #[test]
    fn test_depth_prepass_as_builder() {
        let pass = DepthPrepass::new("depth_prepass").read("shadow_atlas");
        let builder = pass.as_builder();

        assert_eq!(builder.name, "depth_prepass");
        assert_eq!(builder.pass_type, PassType::Graphics);
        assert!(builder.writes.is_empty());
        assert!(builder.pipeline.is_none());
        assert!(builder.material.is_none());
        assert!(builder.uses_depth);
    }

    #[test]
    fn test_depth_prepass_build_fn() {
        let pass = DepthPrepass::new("depth_prepass");
        let builder = pass.as_builder();

        let resource_map = HashMap::new();
        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }
}
