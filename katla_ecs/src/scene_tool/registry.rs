use std::collections::HashMap;
use std::fmt;

use crate::inspect::FieldInfo;

/// Stored value for undo/redo of component fields.
#[derive(Debug, Clone)]
pub enum FieldValue {
    F32(f32),
    F64(f64),
    I32(i32),
    U32(u32),
    I64(i64),
    U64(u64),
    Bool(bool),
    String(String),
    Unknown,
}

impl FieldValue {
    /// Convert a serde_json::Value into a FieldValue based on the target field kind.
    pub fn from_json(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    FieldValue::F32(f as f32)
                } else {
                    FieldValue::Unknown
                }
            }
            serde_json::Value::Bool(b) => FieldValue::Bool(*b),
            serde_json::Value::String(s) => FieldValue::String(s.clone()),
            _ => FieldValue::Unknown,
        }
    }

    /// Convert a serde_json::Value to a specific FieldValue variant.
    pub fn from_json_typed(value: &serde_json::Value, target: &FieldValue) -> Option<FieldValue> {
        match target {
            FieldValue::F32(_) => value.as_f64().map(|v| FieldValue::F32(v as f32)),
            FieldValue::F64(_) => value.as_f64().map(FieldValue::F64),
            FieldValue::I32(_) => value.as_i64().map(|v| FieldValue::I32(v as i32)),
            FieldValue::U32(_) => value.as_u64().map(|v| FieldValue::U32(v as u32)),
            FieldValue::I64(_) => value.as_i64().map(FieldValue::I64),
            FieldValue::U64(_) => value.as_u64().map(FieldValue::U64),
            FieldValue::Bool(_) => value.as_bool().map(FieldValue::Bool),
            FieldValue::String(_) => value.as_str().map(|s| FieldValue::String(s.to_string())),
            FieldValue::Unknown => None,
        }
    }

    /// Try to extract an f32 value.
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            FieldValue::F32(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to extract a bool value.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            FieldValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Try to extract a String value.
    pub fn as_string(&self) -> Option<&str> {
        match self {
            FieldValue::String(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            FieldValue::F32(_) => "f32",
            FieldValue::F64(_) => "f64",
            FieldValue::I32(_) => "i32",
            FieldValue::U32(_) => "u32",
            FieldValue::I64(_) => "i64",
            FieldValue::U64(_) => "u64",
            FieldValue::Bool(_) => "bool",
            FieldValue::String(_) => "String",
            FieldValue::Unknown => "unknown",
        }
    }
}

impl fmt::Display for FieldValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FieldValue::F32(v) => write!(f, "{v}"),
            FieldValue::F64(v) => write!(f, "{v}"),
            FieldValue::I32(v) => write!(f, "{v}"),
            FieldValue::U32(v) => write!(f, "{v}"),
            FieldValue::I64(v) => write!(f, "{v}"),
            FieldValue::U64(v) => write!(f, "{v}"),
            FieldValue::Bool(v) => write!(f, "{v}"),
            FieldValue::String(v) => write!(f, "{v}"),
            FieldValue::Unknown => write!(f, "<unknown>"),
        }
    }
}

/// A single entry in the component registry, mapping a type name to accessor functions.
pub struct ComponentRegistryEntry {
    pub type_name: &'static str,
    pub has_component: fn(&crate::World, crate::EntityId) -> bool,
    pub create_default: fn(&mut crate::World, crate::EntityId),
    pub remove_component: fn(&mut crate::World, crate::EntityId),
    pub get_fields: fn(&crate::World, crate::EntityId) -> Vec<FieldInfo>,
    pub get_field_value: fn(&mut crate::World, crate::EntityId, &str) -> Option<FieldValue>,
    pub set_field_value: fn(
        &mut crate::World,
        crate::EntityId,
        &str,
        FieldValue,
    ) -> Result<(), super::SceneToolError>,
}

/// Registry mapping component type names to accessor functions.
///
/// Components register themselves to enable the scene tool system to find and
/// manipulate them by string name at runtime.
pub struct ComponentRegistry {
    entries: HashMap<&'static str, ComponentRegistryEntry>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Register a component type by providing a registry entry.
    pub fn register(&mut self, entry: ComponentRegistryEntry) {
        self.entries.insert(entry.type_name, entry);
    }

    /// Look up a registry entry by component type name.
    pub fn get(&self, type_name: &str) -> Option<&ComponentRegistryEntry> {
        self.entries.get(type_name)
    }

    /// Iterate over all registered entries.
    pub fn entries(&self) -> impl Iterator<Item = &ComponentRegistryEntry> {
        self.entries.values()
    }

    /// Check if a component type is registered.
    pub fn is_registered(&self, type_name: &str) -> bool {
        self.entries.contains_key(type_name)
    }

    /// List all registered component type names.
    pub fn type_names(&self) -> Vec<&'static str> {
        self.entries.keys().copied().collect()
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
