//! Shared asset-authoring protocol for MCP and the in-editor AI.

use serde::{Deserialize, Serialize};

/// Author recipes, validate feedback, preview instances, and export edited subtrees.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum PrefabOp {
    /// Returns the supported formats, JSON examples, limits and iteration workflow.
    Describe,
    Read {
        path: String,
    },
    Validate {
        path: String,
        document: serde_json::Value,
    },
    Write {
        path: String,
        document: serde_json::Value,
    },
    Instantiate {
        path: String,
        #[serde(default)]
        position: [f32; 3],
        #[serde(default = "identity_rotation")]
        rotation: [f32; 4],
        #[serde(default = "unit_scale")]
        scale: [f32; 3],
    },
    /// Export one live root and every descendant; references must remain internal.
    Capture {
        path: String,
        root_entity: String,
    },
    /// Remove a preview root and every descendant, retaining other instances.
    Remove {
        root_entity: String,
    },
}

fn identity_rotation() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}
fn unit_scale() -> [f32; 3] {
    [1.0; 3]
}

impl PrefabOp {
    /// The same JSON arguments accepted by MCP. Describe supplies asset examples.
    pub fn tool_schema() -> serde_json::Value {
        serde_json::json!({
            "type":"object", "additionalProperties":false, "required":["action"],
            "properties":{
                "action":{"type":"string","enum":["describe","read","validate","write","instantiate","capture","remove"]},
                "path":{"type":"string","description":"Project-relative .katmesh or .katprefab path"},
                "document":{"type":"object","description":"Mesh/prefab JSON object from describe or read; required by validate/write"},
                "position":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},
                "rotation":{"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4,"description":"Normalized quaternion XYZW"},
                "scale":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},
                "root_entity":{"type":"string","description":"Full decimal generational entity ID from instantiate/scene context"}
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_prefab_defaults_and_strict_transport() {
        let op: PrefabOp = serde_json::from_value(
            serde_json::json!({"action":"instantiate","path":"assets/chair.katprefab"}),
        )
        .unwrap();
        assert!(matches!(
            op,
            PrefabOp::Instantiate {
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
                ..
            }
        ));
        assert!(
            serde_json::from_value::<PrefabOp>(
                serde_json::json!({"action":"capture","path":"a.katprefab","root_entity":u64::MAX})
            )
            .is_err()
        );
        assert!(serde_json::from_value::<PrefabOp>(serde_json::json!({"action":"remove","root_entity":u64::MAX.to_string(),"unknown":true})).is_err());
    }
}
