//! Fullscreen/compute pass template.
//!
//! Post-processing, lighting, and compute-like work.

use std::collections::HashMap;

use crate::handle::PipelineHandle;
use crate::texture::ImageFormat;

use super::super::builder::{InternalPassBuilder, PassBuilder};
use super::super::error::RenderGraphError;
use super::super::pass::PassType;
use super::super::resource::GraphResourceHandle;

/// Fullscreen/compute pass template.
///
/// Post-processing, lighting, and compute-like work.
///
/// # Example
///
/// ```ignore
/// let tone_map = FullscreenPass::new("tone_map")
///     .read("hdr_color")
///     .write("ldr_output", ImageFormat::R8G8B8A8Srgb)
///     .pipeline(tone_map_pipeline);
///
/// let graph = FrameGraph::builder()
///     .add_pass(tone_map)
///     .build(&renderer)?;
///
/// graph.execute(&renderer, |ctx| {
///     ctx.pass("tone_map").dispatch();
/// })?;
/// ```
pub struct FullscreenPass {
    name: String,
    reads: Vec<String>,
    writes: Vec<(String, ImageFormat)>,
    pipeline: Option<PipelineHandle>,
}

impl FullscreenPass {
    /// Create a new fullscreen pass.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            reads: Vec::new(),
            writes: Vec::new(),
            pipeline: None,
        }
    }

    /// Read from a resource (can call multiple times).
    pub fn read(mut self, name: impl Into<String>) -> Self {
        self.reads.push(name.into());
        self
    }

    /// Write to an output resource.
    pub fn write(mut self, name: impl Into<String>, format: ImageFormat) -> Self {
        self.writes.push((name.into(), format));
        self
    }

    /// Set the graphics pipeline.
    pub fn pipeline(mut self, pipeline: PipelineHandle) -> Self {
        self.pipeline = Some(pipeline);
        self
    }
}

impl PassBuilder for FullscreenPass {
    fn as_builder(self) -> InternalPassBuilder {
        let writes: Vec<String> = self.writes.iter().map(|(n, _)| n.clone()).collect();

        InternalPassBuilder {
            name: self.name,
            pass_type: PassType::Graphics,
            reads: self.reads.clone(),
            writes,
            build_fn: Box::new(move |resource_map: &HashMap<String, GraphResourceHandle>| {
                let reads: Vec<GraphResourceHandle> = self
                    .reads
                    .iter()
                    .map(|n| {
                        resource_map
                            .get(n)
                            .copied()
                            .ok_or_else(|| RenderGraphError::ResourceNotFound(n.clone()))
                    })
                    .collect::<Result<Vec<_>, RenderGraphError>>()?;

                let writes: Vec<GraphResourceHandle> = self
                    .writes
                    .iter()
                    .map(|(n, _)| {
                        resource_map
                            .get(n)
                            .copied()
                            .ok_or_else(|| RenderGraphError::ResourceNotFound(n.clone()))
                    })
                    .collect::<Result<Vec<_>, RenderGraphError>>()?;

                Ok(Box::new(FullscreenPassData {
                    reads,
                    writes,
                    pipeline: self.pipeline,
                }))
            }),
        }
    }
}

/// Internal data for a fullscreen pass.
pub(crate) struct FullscreenPassData {
    pub reads: Vec<GraphResourceHandle>,
    pub writes: Vec<GraphResourceHandle>,
    pub pipeline: Option<PipelineHandle>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fullscreen_pass_new() {
        let pass = FullscreenPass::new("tone_map");
        assert_eq!(pass.name, "tone_map");
        assert!(pass.reads.is_empty());
        assert!(pass.writes.is_empty());
        assert!(pass.pipeline.is_none());
    }

    #[test]
    fn test_fullscreen_pass_read() {
        let pass = FullscreenPass::new("tone_map")
            .read("hdr_color")
            .read("bloom");

        assert_eq!(pass.reads.len(), 2);
        assert_eq!(pass.reads[0], "hdr_color");
        assert_eq!(pass.reads[1], "bloom");
    }

    #[test]
    fn test_fullscreen_pass_write() {
        let pass = FullscreenPass::new("tone_map").write("ldr_output", ImageFormat::R8G8B8A8Srgb);

        assert_eq!(pass.writes.len(), 1);
        assert_eq!(pass.writes[0].0, "ldr_output");
        assert_eq!(pass.writes[0].1, ImageFormat::R8G8B8A8Srgb);
    }

    #[test]
    fn test_fullscreen_pass_pipeline() {
        let pipeline = PipelineHandle::new(42);
        let pass = FullscreenPass::new("tone_map").pipeline(pipeline);

        assert!(pass.pipeline.is_some());
        assert_eq!(pass.pipeline.unwrap().index(), 42);
    }

    #[test]
    fn test_fullscreen_pass_as_builder() {
        let pipeline = PipelineHandle::new(42);
        let pass = FullscreenPass::new("tone_map")
            .read("hdr_color")
            .write("ldr_output", ImageFormat::R8G8B8A8Srgb)
            .pipeline(pipeline);

        let builder = pass.as_builder();

        assert_eq!(builder.name, "tone_map");
        assert_eq!(builder.pass_type, PassType::Graphics);
        assert_eq!(builder.reads, vec!["hdr_color"]);
        assert_eq!(builder.writes, vec!["ldr_output"]);
    }

    #[test]
    fn test_fullscreen_pass_build_fn() {
        let pipeline = PipelineHandle::new(42);
        let pass = FullscreenPass::new("tone_map")
            .read("hdr_color")
            .write("ldr_output", ImageFormat::R8G8B8A8Srgb)
            .pipeline(pipeline);

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("hdr_color".to_string(), GraphResourceHandle::new(0));
        resource_map.insert("ldr_output".to_string(), GraphResourceHandle::new(1));

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_ok());
    }

    #[test]
    fn test_fullscreen_pass_build_fn_missing_resource() {
        let pass = FullscreenPass::new("tone_map")
            .read("hdr_color")
            .write("ldr_output", ImageFormat::R8G8B8A8Srgb);

        let builder = pass.as_builder();

        let mut resource_map = HashMap::new();
        resource_map.insert("hdr_color".to_string(), GraphResourceHandle::new(0));
        // Missing "ldr_output"

        let result = (builder.build_fn)(&resource_map);
        assert!(result.is_err());
    }
}
