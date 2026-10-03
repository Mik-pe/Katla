//! Reusable material authoring without renderer or application dependencies.

use serde::{Deserialize, Serialize};

/// Describe, validate, publish, capture and apply portable `.katmat` surfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum MaterialAssetOp {
    /// Return a complete example, format limits and explicit reuse semantics.
    Describe,
    /// Read a validated definition without changing live objects.
    Read { path: String },
    /// Check a definition and its images without uploading GPU resources.
    Validate {
        path: String,
        document: serde_json::Value,
    },
    /// Atomically publish a validated definition without changing live objects.
    Write {
        path: String,
        document: serde_json::Value,
    },
    /// Capture the effective surface, resolving inherited images for reuse.
    Capture { path: String, entity_id: String },
    /// Apply a complete definition to a preflighted batch as one undoable edit.
    Apply {
        path: String,
        entity_ids: Vec<String>,
    },
}
impl MaterialAssetOp {
    /// Shared discriminated input schema for MCP and the in-editor assistant.
    pub fn tool_schema() -> serde_json::Value {
        use serde_json::json;
        let fields = json!({
            "path":{"type":"string","minLength":1,"description":"Project-relative .katmat path; no absolute path, . or .. segments"},
            "document":{"type":"object","description":"Complete material JSON from describe/read: version, name, values, sampling and textures"},
            "entity_id":{"type":"string","pattern":"^[0-9]+$"},
            "entity_ids":{"type":"array","items":{"type":"string","pattern":"^[0-9]+$"},"minItems":1,"maxItems":256,"uniqueItems":true}
        });
        let mut properties = fields.as_object().cloned().unwrap_or_default();
        properties.insert("action".into(),json!({"type":"string","enum":["describe","read","validate","write","capture","apply"]}));
        let branches:Vec<_>=[("describe",vec![]),("read",vec!["path"]),("validate",vec!["path","document"]),("write",vec!["path","document"]),("capture",vec!["path","entity_id"]),("apply",vec!["path","entity_ids"])].into_iter().map(|(action, names)|{
            let mut props=serde_json::Map::new(); props.insert("action".into(),json!({"const":action}));
            let mut required=vec!["action"];
            for name in names {props.insert(name.into(),fields[name].clone());required.push(name);}
            json!({"type":"object","properties":props,"required":required,"additionalProperties":false})
        }).collect();
        json!({"type":"object","properties":properties,"required":["action"],"additionalProperties":false,"oneOf":branches})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_material_asset_transport_is_strict_and_discriminated() {
        assert!(serde_json::from_value::<MaterialAssetOp>(serde_json::json!({"action":"apply","path":"assets/a.katmat","entity_ids":["4294967297"]})).is_ok());
        for bad in [
            serde_json::json!({"action":"capture","path":"a.katmat","entity_id":1}),
            serde_json::json!({"action":"apply","path":"a.katmat"}),
            serde_json::json!({"action":"read","path":"a.katmat","document":{}}),
        ] {
            assert!(serde_json::from_value::<MaterialAssetOp>(bad).is_err());
        }
        assert_eq!(
            MaterialAssetOp::tool_schema()["oneOf"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
    }
}
