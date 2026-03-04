//! Pass types for render graph execution.

use std::marker::PhantomData;

use super::error::RenderGraphError;
use super::resource::GraphResourceHandle;

/// Type of render pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PassType {
    /// Graphics pass (rendering to attachments).
    Graphics,
    /// Compute pass (GPU computation).
    Compute,
    /// Transfer pass (copying data).
    Transfer,
}

impl Default for PassType {
    fn default() -> Self {
        Self::Graphics
    }
}

/// Pass execution callback type.
pub(crate) type PassExecFn =
    Box<dyn FnOnce(&mut PassContext) -> Result<(), RenderGraphError> + 'static>;

/// Context provided during pass execution.
pub(crate) struct PassContext {
    /// Command buffer for recording commands.
    pub cmd: Option<ash::vk::CommandBuffer>,
    /// Resources available for reading.
    pub reads: Vec<GraphResourceHandle>,
    /// Resources being written to.
    pub writes: Vec<GraphResourceHandle>,
    /// Phantom data for !Send + !Sync.
    _marker: PhantomData<*const ()>,
}

impl PassContext {
    /// Create a new pass context.
    pub(crate) fn new(
        cmd: ash::vk::CommandBuffer,
        reads: Vec<GraphResourceHandle>,
        writes: Vec<GraphResourceHandle>,
    ) -> Self {
        Self {
            cmd: Some(cmd),
            reads,
            writes,
            _marker: PhantomData,
        }
    }

    /// Get the command buffer.
    pub fn command_buffer(&self) -> Option<ash::vk::CommandBuffer> {
        self.cmd
    }

    /// Take ownership of the command buffer.
    pub fn take_command_buffer(&mut self) -> Option<ash::vk::CommandBuffer> {
        self.cmd.take()
    }

    /// Get resources being read by this pass.
    pub fn read_resources(&self) -> &[GraphResourceHandle] {
        &self.reads
    }

    /// Get resources being written by this pass.
    pub fn write_resources(&self) -> &[GraphResourceHandle] {
        &self.writes
    }
}

/// Internal pass descriptor.
pub(crate) struct PassDesc {
    /// Human-readable name for debugging.
    pub name: String,
    /// Resources this pass reads from (by handle).
    pub reads: Vec<GraphResourceHandle>,
    /// Resources this pass writes to (by handle).
    pub writes: Vec<GraphResourceHandle>,
    /// Pass type (graphics, compute, transfer).
    pub pass_type: PassType,
    /// Execution callback.
    pub execute: PassExecFn,
}

impl PassDesc {
    /// Create a new pass descriptor.
    pub fn new(
        name: impl Into<String>,
        pass_type: PassType,
        reads: Vec<GraphResourceHandle>,
        writes: Vec<GraphResourceHandle>,
        execute: PassExecFn,
    ) -> Self {
        Self {
            name: name.into(),
            reads,
            writes,
            pass_type,
            execute,
        }
    }

    /// Create a graphics pass descriptor.
    pub fn graphics(
        name: impl Into<String>,
        reads: Vec<GraphResourceHandle>,
        writes: Vec<GraphResourceHandle>,
        execute: PassExecFn,
    ) -> Self {
        Self::new(name, PassType::Graphics, reads, writes, execute)
    }

    /// Create a compute pass descriptor.
    pub fn compute(
        name: impl Into<String>,
        reads: Vec<GraphResourceHandle>,
        writes: Vec<GraphResourceHandle>,
        execute: PassExecFn,
    ) -> Self {
        Self::new(name, PassType::Compute, reads, writes, execute)
    }

    /// Create a transfer pass descriptor.
    pub fn transfer(
        name: impl Into<String>,
        reads: Vec<GraphResourceHandle>,
        writes: Vec<GraphResourceHandle>,
        execute: PassExecFn,
    ) -> Self {
        Self::new(name, PassType::Transfer, reads, writes, execute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_type_default() {
        assert_eq!(PassType::default(), PassType::Graphics);
    }

    #[test]
    fn test_pass_type_equality() {
        assert_eq!(PassType::Graphics, PassType::Graphics);
        assert_ne!(PassType::Graphics, PassType::Compute);
        assert_ne!(PassType::Compute, PassType::Transfer);
    }

    #[test]
    fn test_pass_context_new() {
        let reads = vec![GraphResourceHandle::new(0), GraphResourceHandle::new(1)];
        let writes = vec![GraphResourceHandle::new(2)];
        let ctx = PassContext::new(
            ash::vk::CommandBuffer::null(),
            reads.clone(),
            writes.clone(),
        );

        assert!(ctx.command_buffer().is_some());
        assert_eq!(ctx.read_resources().len(), 2);
        assert_eq!(ctx.write_resources().len(), 1);
    }

    #[test]
    fn test_pass_context_take_command_buffer() {
        let mut ctx = PassContext::new(ash::vk::CommandBuffer::null(), vec![], vec![]);

        assert!(ctx.command_buffer().is_some());
        let cmd = ctx.take_command_buffer();
        assert!(cmd.is_some());
        assert!(ctx.command_buffer().is_none());
    }

    #[test]
    fn test_pass_desc_new() {
        let reads = vec![GraphResourceHandle::new(0)];
        let writes = vec![GraphResourceHandle::new(1)];
        let desc = PassDesc::new(
            "test_pass",
            PassType::Graphics,
            reads.clone(),
            writes.clone(),
            Box::new(|_ctx| Ok(())),
        );

        assert_eq!(desc.name, "test_pass");
        assert_eq!(desc.pass_type, PassType::Graphics);
        assert_eq!(desc.reads.len(), 1);
        assert_eq!(desc.writes.len(), 1);
    }

    #[test]
    fn test_pass_desc_convenience_constructors() {
        let graphics =
            PassDesc::graphics("graphics_pass", vec![], vec![], Box::new(|_ctx| Ok(())));
        assert_eq!(graphics.pass_type, PassType::Graphics);

        let compute =
            PassDesc::compute("compute_pass", vec![], vec![], Box::new(|_ctx| Ok(())));
        assert_eq!(compute.pass_type, PassType::Compute);

        let transfer =
            PassDesc::transfer("transfer_pass", vec![], vec![], Box::new(|_ctx| Ok(())));
        assert_eq!(transfer.pass_type, PassType::Transfer);
    }
}
