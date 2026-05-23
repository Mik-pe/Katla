use std::collections::HashMap;

use mlua::RegistryKey;

/// A pending event waiting to be delivered to script handlers.
#[derive(Clone)]
pub struct ScriptEvent {
    pub name: String,
    pub data: mlua::Value,
}

/// Stores event subscriptions (script path -> list of handler registry keys).
struct EventSubscription {
    handler_keys: Vec<RegistryKey>,
}

/// String-keyed event bus for gameplay events.
///
/// Scripts emit events via `world:emit("name", data)` and subscribe via
/// `world:on_event("name", callback)`. Each frame, the `ScriptSystem` drains
/// pending events and dispatches them to all registered handlers in insertion order.
pub struct EventBus {
    subscriptions: HashMap<String, EventSubscription>,
    pending: Vec<ScriptEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
            pending: Vec::new(),
        }
    }

    /// Queue an event for delivery at the next drain cycle.
    pub fn emit(&mut self, name: String, data: mlua::Value) {
        self.pending.push(ScriptEvent { name, data });
    }

    /// Register a Lua function as a handler for the given event name.
    pub fn subscribe(&mut self, name: String, handler_key: RegistryKey) {
        self.subscriptions
            .entry(name)
            .or_insert_with(|| EventSubscription {
                handler_keys: Vec::new(),
            })
            .handler_keys
            .push(handler_key);
    }

    /// Drain all pending events, returning them for dispatch.
    /// Callers should iterate and invoke handlers for each event.
    pub fn drain_pending(&mut self) -> Vec<ScriptEvent> {
        std::mem::take(&mut self.pending)
    }

    /// Get the handler registry keys for a given event name.
    pub fn handlers(&self, name: &str) -> &[RegistryKey] {
        match self.subscriptions.get(name) {
            Some(sub) => &sub.handler_keys,
            None => &[],
        }
    }

    /// Remove all subscriptions for a given script path (used during hot reload
    /// to clear old handlers before re-registering).
    pub fn clear_subscriptions_for_keys(&mut self, keys_to_remove: &[RegistryKey]) {
        for sub in self.subscriptions.values_mut() {
            sub.handler_keys
                .retain(|k| !keys_to_remove.iter().any(|r| r == k));
        }
        self.subscriptions
            .retain(|_, sub| !sub.handler_keys.is_empty());
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
