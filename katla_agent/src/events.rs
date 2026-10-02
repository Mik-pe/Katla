//! Declarative trigger rules shared by scene files, MCP and the editor agent.

use crate::animation::{default_fade, default_looping, default_speed};
use serde::{Deserialize, Serialize};

/// Overlap transition that activates a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TriggerPhase {
    Enter,
    Exit,
}

/// An action's recipient. Scene files use names; live rules use generational IDs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventTarget<T = u64> {
    Trigger,
    Other,
    Entity { entity: T },
}

/// A semantic action, executed in list order after physics finishes its step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum EventAction<T = u64> {
    /// Reuses the engine's named animation playback and fade contract.
    PlayAnimation {
        target: EventTarget<T>,
        clip: String,
        #[serde(default = "default_fade")]
        fade_seconds: f32,
        #[serde(default = "default_looping")]
        looping: bool,
        #[serde(default = "default_speed")]
        speed: f32,
    },
    /// Delivers a named Luau event with trigger_entity and other_entity fields.
    Emit { name: String },
}
/// A trigger's event, optional visitor filter and ordered actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
pub struct TriggerRule<T = u64> {
    pub event: TriggerPhase,
    #[serde(default)]
    pub other_entity: Option<T>,
    /// Runs once per play session, consumed when matched even if an action fails.
    #[serde(default)]
    pub once: bool,
    pub actions: Vec<EventAction<T>>,
}

impl<T> TriggerRule<T> {
    /// Resolve persistent names to live IDs, or map live IDs back to scene names.
    pub fn map_entities<U, E>(
        &self,
        mut resolve: impl FnMut(&T) -> Result<U, E>,
    ) -> Result<TriggerRule<U>, E> {
        let other_entity = self.other_entity.as_ref().map(&mut resolve).transpose()?;
        let mut actions = Vec::with_capacity(self.actions.len());
        for action in &self.actions {
            actions.push(match action {
                EventAction::Emit { name } => EventAction::Emit { name: name.clone() },
                EventAction::PlayAnimation {
                    target,
                    clip,
                    fade_seconds,
                    looping,
                    speed,
                } => {
                    let target = match target {
                        EventTarget::Trigger => EventTarget::Trigger,
                        EventTarget::Other => EventTarget::Other,
                        EventTarget::Entity { entity } => EventTarget::Entity {
                            entity: resolve(entity)?,
                        },
                    };
                    EventAction::PlayAnimation {
                        target,
                        clip: clip.clone(),
                        fade_seconds: *fade_seconds,
                        looping: *looping,
                        speed: *speed,
                    }
                }
            });
        }
        Ok(TriggerRule {
            event: self.event,
            other_entity,
            once: self.once,
            actions,
        })
    }

    /// Validate bounded action lists and finite animation parameters.
    pub fn validate(&self) -> Result<(), String> {
        if self.actions.is_empty() || self.actions.len() > 32 {
            return Err("A rule requires 1..32 actions".into());
        }
        for action in &self.actions {
            match action {
                EventAction::Emit { name } if name.trim().is_empty() || name.len() > 128 => {
                    return Err("Event name requires 1..128 bytes".into());
                }
                EventAction::PlayAnimation {
                    clip,
                    fade_seconds,
                    speed,
                    ..
                } if clip.trim().is_empty()
                    || !fade_seconds.is_finite()
                    || *fade_seconds < 0.0
                    || !speed.is_finite()
                    || *speed < 0.0 =>
                {
                    return Err(
                        "Animation action needs a clip and finite nonnegative fade_seconds/speed"
                            .into(),
                    );
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Agent operations for authoring and inspecting event-driven trigger volumes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp-server", derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum TriggerOp<T = u64> {
    CreateBox {
        name: String,
        position: [f32; 3],
        half_extents: [f32; 3],
        rules: Vec<TriggerRule<T>>,
    },
    SetRules {
        entity_id: T,
        rules: Vec<TriggerRule<T>>,
    },
    Inspect {
        entity_id: T,
    },
}

impl<T> TriggerOp<T> {
    /// Decode transport entity references without exposing native physics handles.
    pub fn map_entities<U, E>(
        self,
        mut resolve: impl FnMut(&T) -> Result<U, E>,
    ) -> Result<TriggerOp<U>, E> {
        Ok(match self {
            Self::Inspect { entity_id } => TriggerOp::Inspect {
                entity_id: resolve(&entity_id)?,
            },
            Self::SetRules { entity_id, rules } => TriggerOp::SetRules {
                entity_id: resolve(&entity_id)?,
                rules: rules
                    .iter()
                    .map(|rule| rule.map_entities(&mut resolve))
                    .collect::<Result<_, _>>()?,
            },
            Self::CreateBox {
                name,
                position,
                half_extents,
                rules,
            } => TriggerOp::CreateBox {
                name,
                position,
                half_extents,
                rules: rules
                    .iter()
                    .map(|rule| rule.map_entities(&mut resolve))
                    .collect::<Result<_, _>>()?,
            },
        })
    }
}

impl TriggerOp<String> {
    /// Resolve lossless decimal IDs supplied by agent transports.
    pub fn resolve_ids(self) -> Result<TriggerOp, String> {
        self.map_entities(|id| {
            id.parse::<u64>()
                .map_err(|_| format!("Invalid entity ID '{id}'; use a decimal u64 string"))
        })
    }
}

impl TriggerOp {
    /// Function-call schema for the same operations accepted by MCP.
    pub fn tool_schema() -> serde_json::Value {
        let target = serde_json::json!({
            "type":"object","required":["kind"],"additionalProperties":false,"properties":{
                "kind":{"type":"string","enum":["trigger","other","entity"]},
                "entity":{"type":"string"}
            }
        });
        let action = serde_json::json!({
            "type":"object","additionalProperties":false,"required":["action"],
            "properties": {
                "action":{"type":"string","enum":["play_animation","emit"]},
                "target":target, "clip":{"type":"string"},
                "fade_seconds":{"type":"number","minimum":0,"default":0.25},
                "looping":{"type":"boolean","default":true},
                "speed":{"type":"number","minimum":0,"default":1},
                "name":{"type":"string","description":"Required for emit; Lua receives trigger_entity/other_entity"}
            }
        });
        let rule = serde_json::json!({
            "type":"object","additionalProperties":false,"required":["event","actions"],
            "properties": {
                "event":{"type":"string","enum":["enter","exit"]},
                "other_entity":{"type":"string","description":"Optional visitor entity filter"},
                "once":{"type":"boolean","default":false},
                "actions":{"type":"array","minItems":1,"maxItems":32,"items":action}
            }
        });
        serde_json::json!({
            "type":"object","additionalProperties":false,"required":["action"],
            "properties": {
                "action":{"type":"string","enum":["create_box","set_rules","inspect"]},
                "entity_id":{"type":"string","description":"Required for set_rules/inspect"},
                "name":{"type":"string","description":"Required unique name for create_box"},
                "position":{"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3},
                "half_extents":{"type":"array","items":{"type":"number","exclusiveMinimum":0},"minItems":3,"maxItems":3},
                "rules":{"type":"array","maxItems":64,"items":rule}
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_trigger_transport_preserves_full_entity_ids() {
        let wire: TriggerOp<String> = serde_json::from_value(serde_json::json!({
            "action":"set_rules", "entity_id":u64::MAX.to_string(), "rules":[{
                "event":"enter", "other_entity":u64::MAX.to_string(), "actions":[{
                    "action":"play_animation", "target":{"kind":"entity", "entity":u64::MAX.to_string()}, "clip":"Run"
                }]
            }]
        })).unwrap();
        let TriggerOp::SetRules { entity_id, rules } = wire.resolve_ids().unwrap() else {
            panic!("Unexpected operation")
        };
        assert_eq!(entity_id, u64::MAX);
        assert_eq!(rules[0].other_entity, Some(u64::MAX));
        assert!(matches!(
            &rules[0].actions[0],
            EventAction::PlayAnimation {
                target: EventTarget::Entity { entity: u64::MAX },
                ..
            }
        ));
        assert!(
            TriggerOp::Inspect {
                entity_id: "18446744073709551616".into()
            }
            .resolve_ids()
            .is_err()
        );
        assert!(
            TriggerOp::Inspect {
                entity_id: "missing".into()
            }
            .resolve_ids()
            .is_err()
        );
        assert!(
            serde_json::from_value::<TriggerOp<String>>(
                serde_json::json!({"action":"inspect", "entity_id":17})
            )
            .is_err()
        );
    }

    #[test]
    fn test_trigger_action_defaults_and_unknown_fields() {
        let rule: TriggerRule =
            serde_json::from_value(serde_json::json!({"event":"enter", "actions":[
            {"action":"play_animation","target":{"kind":"other"},"clip":"Run"}]}))
            .unwrap();
        assert!(matches!(
            &rule.actions[0],
            EventAction::PlayAnimation {
                fade_seconds: 0.25,
                looping: true,
                speed: 1.0,
                ..
            }
        ));
        assert!(
            serde_json::from_value::<TriggerRule>(
                serde_json::json!({"event":"enter","delay":5,"actions":[]})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<TriggerOp>(
                serde_json::json!({"action":"create_box","name":"Box"})
            )
            .is_err()
        );
        assert!(rule.validate().is_ok());
    }
}
