//! Backend-independent animation requests shared by MCP and the editor agent.

use serde::{Deserialize, Serialize};

/// Semantic animation operations. Clip durations and blend weights belong to the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnimationOp {
    /// List the entity's clips and inspect its playback and transition state.
    Inspect { entity_id: u64 },
    /// Start a named clip, fading from the active clip when one exists.
    Play {
        entity_id: u64,
        clip: String,
        #[serde(default = "default_fade")]
        fade_seconds: f32,
        #[serde(default = "default_looping")]
        looping: bool,
        #[serde(default = "default_speed")]
        speed: f32,
    },
}

pub(crate) fn default_fade() -> f32 {
    0.25
}
pub(crate) fn default_looping() -> bool {
    true
}
pub(crate) fn default_speed() -> f32 {
    1.0
}

impl AnimationOp {
    /// Function-call schema for the same operations accepted by MCP.
    pub fn tool_schema() -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["inspect", "play"] },
                "entity_id": { "type": "integer", "minimum": 0 },
                "clip": { "type": "string", "description": "Required for play; use inspect to discover exact clip names" },
                "fade_seconds": { "type": "number", "minimum": 0, "default": 0.25, "description": "Seconds of linear local-pose crossfade; zero switches immediately" },
                "looping": { "type": "boolean", "default": true },
                "speed": { "type": "number", "minimum": 0, "default": 1 }
            },
            "required": ["action", "entity_id"],
            "additionalProperties": false
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animation_request_defaults_and_required_clip() {
        let request: AnimationOp = serde_json::from_value(serde_json::json!({
            "action": "play", "entity_id": 42, "clip": "Run"
        }))
        .unwrap();
        assert!(matches!(
            request,
            AnimationOp::Play {
                entity_id: 42,
                fade_seconds: 0.25,
                looping: true,
                speed: 1.0,
                ..
            }
        ));
        assert!(
            serde_json::from_value::<AnimationOp>(serde_json::json!({
                "action": "play", "entity_id": 42
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<AnimationOp>(serde_json::json!({
                "action": "play", "entity_id": 42, "clip": "Run", "blend_weight": 0.5
            }))
            .is_err()
        );
    }
}
