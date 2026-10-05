//! Lossless agent authoring of script and particle attachments, and preview control.
use serde::{Deserialize, Serialize};

/// Configure durable attachments or issue an explicit particle preview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BehaviorOp {
    Describe,
    Inspect {
        entity_id: String,
    },
    /// A resource-relative scripts/*.luau path; null detaches the script.
    SetScript {
        entity_id: String,
        #[serde(deserialize_with = "explicit_option")]
        path: Option<String>,
    },
    /// Full scene particle descriptor; null detaches the emitter.
    SetParticles {
        entity_id: String,
        #[serde(deserialize_with = "explicit_option")]
        document: Option<serde_json::Value>,
    },
    Burst {
        entity_id: String,
        count: u32,
    },
    SetActive {
        entity_id: String,
        active: bool,
    },
}

impl BehaviorOp {
    /// Tool arguments shared by the co-creator and MCP.
    pub fn tool_schema() -> serde_json::Value {
        serde_json::json!({"type":"object","additionalProperties":false,"required":["action"],"properties":{
            "action":{"type":"string","enum":["describe","inspect","set_script","set_particles","burst","set_active"]},
            "entity_id":{"type":"string"},
            "path":{"type":["string","null"],"description":"Resource-relative scripts/name.luau; null detaches"},
            "document":{"type":["object","null"],"description":"Particle descriptor from describe/inspect; null detaches"},
            "count":{"type":"integer","minimum":1,"maximum":100000},
            "active":{"type":"boolean"}
        }})
    }
}

/// Explicit idempotent editor preview transitions; stop restores authored state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum SimulationOp {
    Inspect,
    Play,
    Pause,
    Resume,
    Stop,
}

impl SimulationOp {
    /// The same action schema used by MCP.
    pub fn tool_schema() -> serde_json::Value {
        serde_json::json!({"type":"object","additionalProperties":false,"required":["action"],"properties":{
            "action":{"type":"string","enum":["inspect","play","pause","resume","stop"]}
        }})
    }
}

fn explicit_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_detach_requires_explicit_null_and_ids_are_lossless() {
        assert!(
            serde_json::from_value::<BehaviorOp>(
                serde_json::json!({"action":"set_script","entity_id":"1"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<BehaviorOp>(
                serde_json::json!({"action":"set_particles","entity_id":"1"})
            )
            .is_err()
        );
        assert!(matches!(serde_json::from_value::<BehaviorOp>(serde_json::json!({"action":"set_script","entity_id":u64::MAX.to_string(),"path":null})).unwrap(), BehaviorOp::SetScript { path:None,.. }));
        assert!(
            serde_json::from_value::<BehaviorOp>(
                serde_json::json!({"action":"inspect","entity_id":u64::MAX})
            )
            .is_err()
        );
    }
}
