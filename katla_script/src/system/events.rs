//! Entity-owned Luau callback dispatch and deferred physics signal delivery.

use super::*;

impl ScriptSystem {
    fn live_handle(&self, world: &World, owner: EntityId) -> Option<ScriptInstanceHandle> {
        let handle = world
            .get_component::<ScriptComponent>(owner)?
            .instance_handle?;
        let instance = self.engine.instances.get(handle.index as usize)?.as_ref()?;
        (instance.entity == owner && instance.generation == handle.generation).then_some(handle)
    }

    /// Drain pending events from the event bus and dispatch to registered script handlers.
    pub(super) fn process_events(&mut self, world: &mut World) {
        let events = self.event_bus.drain_pending();
        if events.is_empty() {
            return;
        }
        let shared = Rc::new(self.build_shared_data(world));
        let mut commands = Vec::new();
        for event in events {
            let handlers: Vec<_> = self
                .event_bus
                .handlers(&event.name)
                .iter()
                .filter_map(|(owner, key)| {
                    self.engine
                        .vm
                        .registry_value::<mlua::Function>(key)
                        .ok()
                        .map(|function| (*owner, function))
                })
                .collect();
            for (owner, function) in handlers {
                let Some(handle) = self.live_handle(world, owner) else {
                    continue;
                };
                let mut proxy = ScriptWorldProxy::from_shared(Rc::clone(&shared));
                proxy.with_event_bus(Rc::clone(&self.shared_event_bus), owner, handle);
                let proxy = match self.engine.vm.create_userdata(proxy) {
                    Ok(proxy) => proxy,
                    Err(error) => {
                        error!("Cannot prepare event callback: {error}");
                        continue;
                    }
                };
                self.engine.reset_instruction_counter();
                match function.call::<()>((event.name.clone(), event.data.clone(), proxy.clone())) {
                    Ok(()) => {
                        if let Ok(mut proxy) = proxy.borrow_mut::<ScriptWorldProxy>() {
                            commands.append(&mut proxy.commands);
                        }
                    }
                    Err(error) => {
                        error!("Entity {owner} event '{}' failed: {error}", event.name);
                        if let Some(Some(instance)) =
                            self.engine.instances.get_mut(handle.index as usize)
                        {
                            instance.error_count += 1;
                            if instance.error_count >= MAX_SCRIPT_ERRORS {
                                self.engine.remove_instance(handle);
                                self.event_bus.remove_owner(owner);
                                log::warn!(
                                    "Disabling entity {owner} event handlers after {MAX_SCRIPT_ERRORS} errors"
                                );
                            }
                        }
                    }
                }
            }
        }
        self.apply_commands(commands, world);
    }

    /// Flush pending emits and subscriptions from a SharedEventBus into the real EventBus.
    pub(super) fn flush_script_events(
        &mut self,
        shared_bus: &Rc<RefCell<crate::bindings::script_world::SharedEventBus>>,
        world: &World,
    ) {
        let mut bus = shared_bus.borrow_mut();

        // Flush subscriptions first so handlers are registered before events arrive
        for (name, owner, instance, key) in bus.pending_subscriptions.drain(..) {
            if self.live_handle(world, owner) == Some(instance) {
                self.event_bus.subscribe(name, owner, key);
            }
        }

        // Flush emitted events into the real event bus
        for (name, data) in bus.pending_emits.drain(..) {
            self.event_bus.emit(name, data);
        }
    }

    /// Drain pending physics collision events and emit them as script events.
    pub(super) fn dispatch_physics_events(&mut self, world: &mut World) {
        let events: Vec<PhysicsCollisionEvent> =
            match world.get_resource_mut::<PendingPhysicsEvents>() {
                Some(r) => std::mem::take(&mut r.0),
                None => return,
            };

        if events.is_empty() {
            return;
        }

        for event in events {
            let event_name = match event.event_type {
                PhysicsCollisionEventType::CollisionEnter => "collision_enter",
                PhysicsCollisionEventType::CollisionExit => "collision_exit",
                PhysicsCollisionEventType::TriggerSignal(ref name) => name.as_str(),
            };

            // Skip if no handlers registered for this event type
            if self.event_bus.handlers(event_name).is_empty() {
                continue;
            }

            let table = match self.engine.vm.create_table() {
                Ok(t) => t,
                Err(e) => {
                    error!("Failed to create physics event table: {e}");
                    continue;
                }
            };

            if let Err(e) = table.set("entity_a", event.entity_a) {
                error!("Failed to set entity_a: {e}");
                continue;
            }
            if let Err(e) = table.set("entity_b", event.entity_b) {
                error!("Failed to set entity_b: {e}");
                continue;
            }

            if let Err(error) = table
                .set("trigger_entity", event.entity_a)
                .and_then(|_| table.set("other_entity", event.entity_b))
            {
                error!("Failed to build trigger event payload: {error}");
                continue;
            }

            self.event_bus
                .emit(event_name.to_string(), mlua::Value::Table(table));
        }
    }
}
