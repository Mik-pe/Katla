//! Pass types for render graph execution.

/// Internal pass descriptor.
///
/// Currently only stores the pass name since reads/writes/execute
/// are handled separately during graph construction.
pub(crate) struct PassDesc {
    /// Human-readable name for debugging.
    pub name: String,
}

impl PassDesc {
    /// Create a new pass descriptor.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass_desc_new() {
        let desc = PassDesc::new("test_pass");
        assert_eq!(desc.name, "test_pass");
    }
}
