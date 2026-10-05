//! Scene-owned event rules. Physics detects transitions; the app composes actions.

pub use katla_agent::events::{EventAction, EventTarget, TriggerPhase, TriggerRule};
use katla_ecs::Component;

#[cfg(any(test, feature = "mcp"))]
pub(crate) mod control;
pub(crate) mod runtime;
#[cfg(test)]
mod tests;

/// Ordered trigger rules and transient diagnostics. Runtime state is never serialized.
#[derive(Component, Debug, Clone, Default)]
pub struct TriggerRules {
    #[inspect(skip)]
    pub(crate) rules: Vec<TriggerRule>,
    #[inspect(skip)]
    pub(crate) fired: Vec<usize>,
    #[inspect(skip)]
    pub(crate) last_errors: Vec<String>,
}

impl TriggerRules {
    /// Build a validated rule list. Replacing it resets once-only activation state.
    pub fn new(rules: Vec<TriggerRule>) -> Result<Self, String> {
        validate_rules(&rules)?;
        Ok(Self {
            rules,
            ..Self::default()
        })
    }
    /// Inspect authored rules without exposing runtime bookkeeping.
    pub fn rules(&self) -> &[TriggerRule] {
        &self.rules
    }
}

pub(crate) fn validate_rules<T>(rules: &[TriggerRule<T>]) -> Result<(), String> {
    if rules.len() > 64 {
        return Err("A trigger supports at most 64 rules".into());
    }
    for rule in rules {
        rule.validate()?;
    }
    Ok(())
}
