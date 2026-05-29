use katla_ecs::Component;

/// ECS component that attaches a Lua script to an entity.
///
/// When an entity has this component, the `ScriptSystem` will:
/// 1. Load the script from disk
/// 2. Create a script instance with its own Lua environment
/// 3. Call `on_spawn` when the entity is created
/// 4. Call `on_update` each frame (when `ScriptsActive(true)`)
/// 5. Call `on_destroy` when the entity is destroyed
///
/// The script path is relative to the scripts directory configured in `ScriptSystem`.
#[derive(Component, serde::Serialize, serde::Deserialize)]
pub struct ScriptComponent {
    /// Path to the script file, relative to the scripts directory.
    /// Can be a bare name (e.g., "player") which will be resolved as "player.luau".
    #[serde(default)]
    pub script_path: String,
    /// Internal handle to the script instance. Managed by `ScriptSystem`.
    #[serde(skip)]
    pub(crate) instance_handle: Option<ScriptInstanceHandle>,
}

impl ScriptComponent {
    /// Create a new script component with the given path.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            script_path: path.into(),
            instance_handle: None,
        }
    }
}

/// Handle to a script instance, used to reference a specific script on an entity.
///
/// Uses an index + generation pattern to allow safe reuse of slots after instances
/// are removed. The generation is incremented each time a slot is reused, preventing
/// use-after-free bugs.
///
/// # Safety
///
/// A handle is valid only if:
/// - The index is within bounds of the instances vector
/// - The generation matches the instance at that index
/// - The instance has not been removed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScriptInstanceHandle {
    /// Index into the instances vector.
    pub(crate) index: u32,
    /// Generation counter to detect stale handles.
    pub(crate) generation: u32,
}
