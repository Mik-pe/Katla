use std::collections::HashMap;

pub struct ShaderRegistry {
    shaders: HashMap<String, Vec<u8>>,
}

impl ShaderRegistry {
    pub fn new() -> Self {
        let mut shaders = HashMap::new();

        shaders.insert(
            "model_pbr.vert".to_string(),
            include_bytes!("../../../resources/shaders/model_pbr.vert.spv").to_vec(),
        );
        shaders.insert(
            "model.frag".to_string(),
            include_bytes!("../../../resources/shaders/model.frag.spv").to_vec(),
        );

        Self { shaders }
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.shaders.get(name).map(|v| v.as_slice())
    }

    pub fn get_vertex_shader(&self, name: &str) -> &[u8] {
        self.get(name).unwrap_or_else(|| panic!("Vertex shader '{}' not found", name))
    }

    pub fn get_fragment_shader(&self, name: &str) -> &[u8] {
        self.get(name).unwrap_or_else(|| panic!("Fragment shader '{}' not found", name))
    }
}

impl Default for ShaderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shader_registry() {
        let registry = ShaderRegistry::new();

        // Test that we can get the shaders
        let vert_shader = registry.get("model_pbr.vert");
        assert!(vert_shader.is_some());
        assert!(!vert_shader.unwrap().is_empty());

        let frag_shader = registry.get("model.frag");
        assert!(frag_shader.is_some());
        assert!(!frag_shader.unwrap().is_empty());

        // Test that non-existent shader returns None
        let nonexistent = registry.get("nonexistent");
        assert!(nonexistent.is_none());
    }

    #[test]
    fn test_shader_registry_helpers() {
        let registry = ShaderRegistry::new();

        // Test get_vertex_shader and get_fragment_shader helpers
        let vert = registry.get_vertex_shader("model_pbr.vert");
        assert!(!vert.is_empty());

        let frag = registry.get_fragment_shader("model.frag");
        assert!(!frag.is_empty());
    }
}
