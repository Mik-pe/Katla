use katla_ecs::Component;

#[derive(Component, serde::Serialize, serde::Deserialize)]
pub struct ScriptComponent {
    #[serde(default)]
    pub script_path: String,
    #[serde(skip)]
    pub(crate) instance_handle: Option<ScriptInstanceHandle>,
}

impl ScriptComponent {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            script_path: path.into(),
            instance_handle: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptInstanceHandle {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}
