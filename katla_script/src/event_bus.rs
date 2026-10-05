//! Script event channels with entity-owned subscriptions and deferred dispatch.

use katla_ecs::EntityId;
use std::collections::HashMap;

use mlua::RegistryKey;

/// A pending event waiting to be delivered to script handlers.
///
/// Events are queued when emitted and dispatched at the end of each frame.
#[derive(Clone)]
pub struct ScriptEvent {
    /// The event name/channel.
    pub name: String,
    /// The event data (arbitrary Lua value).
    pub data: mlua::Value,
}

/// Internal storage for event subscriptions.
/// Maps event names to lists of Lua function registry keys.
struct EventSubscription {
    handler_keys: Vec<(EntityId, RegistryKey)>,
}

/// String-keyed event bus for gameplay events.
///
/// Scripts emit events via `world:emit("name", data)` and subscribe via
/// `world:on_event("name", callback)`. Each frame, the `ScriptSystem` drains
/// pending events and dispatches them to entity-owned handlers in insertion order.
/// Callbacks receive `(name, data, world)` with a fresh deferred-command proxy.
/// Emissions from callbacks are dispatched on the next tick.
///
/// # Usage in Lua
///
/// ```lua
/// -- Emit an event
/// world:emit("player_died", { killer = "dragon", score = 100 })
///
/// -- Subscribe to an event
/// world:on_event("player_died", function(name, data, world)
///     print("Player died! Killer: " .. data.killer)
/// end)
/// ```
///
/// # Thread Safety
///
/// **Warning:** `EventBus` is NOT thread-safe. All event emission and subscription
/// should happen on the same thread as script execution.
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
    pub fn subscribe(&mut self, name: String, owner: EntityId, handler_key: RegistryKey) {
        self.subscriptions
            .entry(name)
            .or_insert_with(|| EventSubscription {
                handler_keys: Vec::new(),
            })
            .handler_keys
            .push((owner, handler_key));
    }

    /// Drain all pending events, returning them for dispatch.
    /// Callers should iterate and invoke handlers for each event.
    pub fn drain_pending(&mut self) -> Vec<ScriptEvent> {
        std::mem::take(&mut self.pending)
    }

    /// Get the handler registry keys for a given event name.
    pub fn handlers(&self, name: &str) -> &[(EntityId, RegistryKey)] {
        match self.subscriptions.get(name) {
            Some(sub) => &sub.handler_keys,
            None => &[],
        }
    }

    /// Release callbacks when an entity's script is destroyed, disabled or reloaded.
    pub fn remove_owner(&mut self, owner: EntityId) {
        for sub in self.subscriptions.values_mut() {
            sub.handler_keys.retain(|(entity, _)| *entity != owner);
        }
        self.subscriptions
            .retain(|_, sub| !sub.handler_keys.is_empty());
    }

    /// Discard undelivered events when script execution is suspended.
    pub fn discard_pending(&mut self) {
        self.pending.clear();
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
