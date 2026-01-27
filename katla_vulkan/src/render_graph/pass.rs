use crate::{ResourceId, VulkanContext};

pub struct Pass {
    name: String,
    inputs: Vec<ResourceId>,
    outputs: Vec<ResourceId>,
    execute: Box<dyn FnMut(&mut VulkanContext)>,
}

impl Pass {
    // Methods for the pass

    pub fn new(
        name: impl Into<String>,
        inputs: Vec<ResourceId>,
        outputs: Vec<ResourceId>,
        execute: Box<dyn FnMut(&mut VulkanContext)>,
    ) -> Self {
        Self {
            name: name.into(),
            inputs,
            outputs,
            execute,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pass() {
        // Test the pass
    }
}
