use std::io::Write;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

use katla_ecs::EntityId;
use katla_math::Vec3;

use crate::bindings::script_world::{ScriptWorldProxy, SharedWorldData};
use crate::bindings::world::ScriptCommand;
use crate::component::ScriptComponent;
use crate::engine::ScriptEngine;

static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

fn make_test_entity(index: u32) -> EntityId {
    EntityId::from_raw(index as u64)
}

fn unique_script_name() -> String {
    let id = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("test_{}_{}", std::process::id(), id)
}

struct TempScript {
    path: std::path::PathBuf,
}

impl TempScript {
    fn new(content: &str) -> Self {
        let dir = std::env::temp_dir().join("katla_script_test");
        std::fs::create_dir_all(&dir).unwrap();
        let name = unique_script_name();
        let path = dir.join(format!("{name}.luau"));
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        TempScript { path }
    }

    fn to_str(&self) -> &str {
        self.path.to_str().unwrap()
    }
}

impl Drop for TempScript {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

impl std::ops::Deref for TempScript {
    type Target = std::path::Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

impl AsRef<std::path::Path> for TempScript {
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

fn make_proxy() -> ScriptWorldProxy {
    ScriptWorldProxy::from_shared(Rc::new(SharedWorldData {
        transforms: Default::default(),
        live_entities: Vec::new(),
        component_entities: Default::default(),
        input_state: Default::default(),
    }))
}

#[test]
fn test_engine_creation() {
    let engine = ScriptEngine::new();
    assert!(engine.is_ok());
}

#[test]
fn test_load_valid_script() {
    let script = TempScript::new("local x = 1 + 1\n");
    let mut engine = ScriptEngine::new().unwrap();
    let result = engine.load_script(script.to_str());
    assert!(result.is_ok(), "load_script failed: {:?}", result.err());
}

#[test]
fn test_load_nonexistent_script() {
    let mut engine = ScriptEngine::new().unwrap();
    let result = engine.load_script("nonexistent_script_12345");
    assert!(result.is_err());
}

#[test]
fn test_create_instance_with_hooks() {
    let script = TempScript::new("function on_update(entity, world, dt)\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let result = engine.create_instance(entity, script.to_str());
    assert!(result.is_ok(), "create_instance failed: {:?}", result.err());
    let handle = result.unwrap();
    assert_eq!(handle.index, 0);
    assert_eq!(handle.generation, 0);
}

#[test]
fn test_execute_on_update_with_commands() {
    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(1, 2, 3))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let proxy = make_proxy();
    let result = engine.execute_on_update(handle, entity, proxy, 0.016);
    assert!(
        result.is_ok(),
        "execute_on_update failed: {:?}",
        result.err()
    );
    let commands = result.unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        ScriptCommand::SetPosition(e, pos) => {
            assert_eq!(*e, entity);
            assert_eq!(*pos, Vec3::new(1.0, 2.0, 3.0));
        }
        _ => panic!("Expected SetPosition command"),
    }
}

#[test]
fn test_load_syntax_error_script() {
    let script = TempScript::new("function broken(\n");
    let mut engine = ScriptEngine::new().unwrap();
    let result = engine.load_script(script.to_str());
    match result {
        Err(crate::error::ScriptError::LoadFailed { .. }) => {}
        Err(other) => panic!("Expected LoadFailed, got: {other}"),
        Ok(_) => panic!("Expected load to fail"),
    }
}

#[test]
fn test_script_component() {
    let comp = ScriptComponent::new("test_script");
    assert_eq!(comp.script_path, "test_script");
    assert!(comp.instance_handle.is_none());
}

#[test]
fn test_remove_instance() {
    let script = TempScript::new("function on_update(entity, world, dt)\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    engine.remove_instance(handle);

    let proxy = make_proxy();
    let result = engine.execute_on_update(handle, entity, proxy, 0.016);
    match result {
        Err(crate::error::ScriptError::InstanceNotFound(h)) => assert_eq!(h, handle),
        Err(other) => panic!("Expected InstanceNotFound, got: {other}"),
        Ok(_) => panic!("Expected error after removal"),
    }
}

#[test]
fn test_error_count_increments_on_failure() {
    let script =
        TempScript::new("function on_update(entity, world, dt)\n  error(\"script error\")\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    for _ in 0..3 {
        let proxy = make_proxy();
        let _ = engine.execute_on_update(handle, entity, proxy, 0.016);
    }

    let inst = engine
        .instances
        .get(handle.index as usize)
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(inst.error_count, 3);
}

#[test]
fn test_error_count_starts_at_zero() {
    let script = TempScript::new("function on_update(entity, world, dt)\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let inst = engine
        .instances
        .get(handle.index as usize)
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(inst.error_count, 0);
}

fn make_proxy_with_components(
    component_entities: std::collections::HashMap<String, Vec<EntityId>>,
) -> ScriptWorldProxy {
    ScriptWorldProxy::from_shared(Rc::new(SharedWorldData {
        transforms: Default::default(),
        live_entities: Vec::new(),
        component_entities,
        input_state: Default::default(),
    }))
}

#[test]
fn test_get_all_with_returns_matching_entities() {
    let e1 = make_test_entity(1);
    let e2 = make_test_entity(2);
    let e3 = make_test_entity(3);
    let mut component_entities = std::collections::HashMap::new();
    component_entities.insert("TransformComponent".to_string(), vec![e1, e2, e3]);
    component_entities.insert("NameComponent".to_string(), vec![e1]);

    let proxy = make_proxy_with_components(component_entities);

    let result = proxy.get_all_with("TransformComponent");
    assert_eq!(result, vec![e1, e2, e3]);

    let result = proxy.get_all_with("NameComponent");
    assert_eq!(result, vec![e1]);
}

#[test]
fn test_get_all_with_returns_empty_for_unknown() {
    let proxy = make_proxy();
    let result = proxy.get_all_with("NonexistentComponent");
    assert!(result.is_empty());
}

#[test]
fn test_get_all_with_lua_binding() {
    let e1 = make_test_entity(1);
    let e2 = make_test_entity(2);
    let mut component_entities = std::collections::HashMap::new();
    component_entities.insert("TransformComponent".to_string(), vec![e1, e2]);

    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  local entities = world:get_all_with(\"TransformComponent\")\n  world:set_position(entities[1], Vec3.new(1, 2, 3))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(10);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let proxy = make_proxy_with_components(component_entities);
    let result = engine.execute_on_update(handle, entity, proxy, 0.016);
    assert!(
        result.is_ok(),
        "execute_on_update failed: {:?}",
        result.err()
    );
    let commands = result.unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        ScriptCommand::SetPosition(e, pos) => {
            assert_eq!(*e, e1);
            assert_eq!(*pos, Vec3::new(1.0, 2.0, 3.0));
        }
        _ => panic!("Expected SetPosition command"),
    }
}

// --- Integration tests ---

#[test]
fn test_on_spawn_hook_executes_and_can_queue_commands() {
    let script = TempScript::new(
        "function on_spawn(entity, world)\n  world:set_position(entity, Vec3.new(10, 20, 30))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(42);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let proxy = make_proxy();
    let result = engine.execute_on_spawn(handle, entity, proxy);
    assert!(result.is_ok(), "on_spawn failed: {:?}", result.err());
    let commands = result.unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        ScriptCommand::SetPosition(e, pos) => {
            assert_eq!(*e, entity);
            assert_eq!(*pos, Vec3::new(10.0, 20.0, 30.0));
        }
        _ => panic!("Expected SetPosition command"),
    }
}

#[test]
fn test_on_destroy_hook_executes() {
    let script = TempScript::new("function on_destroy(entity)\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let result = engine.call_on_destroy(handle, entity);
    assert!(result.is_ok(), "on_destroy failed: {:?}", result.err());
}

#[test]
fn test_on_destroy_with_no_hook_succeeds() {
    let script = TempScript::new("local x = 1\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let result = engine.call_on_destroy(handle, entity);
    assert!(result.is_ok(), "on_destroy should succeed with no hook");
}

#[test]
fn test_multiple_instances_same_script() {
    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(1, 2, 3))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();

    let e1 = make_test_entity(1);
    let e2 = make_test_entity(2);
    let e3 = make_test_entity(3);

    let h1 = engine.create_instance(e1, script.to_str()).unwrap();
    let h2 = engine.create_instance(e2, script.to_str()).unwrap();
    let h3 = engine.create_instance(e3, script.to_str()).unwrap();

    // All three should have distinct handles
    assert_ne!(h1.index, h2.index);
    assert_ne!(h2.index, h3.index);

    // All three should produce commands
    for (handle, entity) in [(h1, e1), (h2, e2), (h3, e3)] {
        let proxy = make_proxy();
        let commands = engine
            .execute_on_update(handle, entity, proxy, 0.016)
            .unwrap();
        assert_eq!(commands.len(), 1);
        match &commands[0] {
            ScriptCommand::SetPosition(e, _) => assert_eq!(*e, entity),
            _ => panic!("Expected SetPosition"),
        }
    }
}

#[test]
fn test_spawn_entity_from_script() {
    let script =
        TempScript::new("function on_update(entity, world, dt)\n  world:spawn_entity()\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        ScriptCommand::SpawnEntity { return_index: _ } => {}
        _ => panic!("Expected SpawnEntity command"),
    }
}

#[test]
fn test_destroy_entity_from_script() {
    let target = make_test_entity(99);
    let script = TempScript::new(&format!(
        "function on_update(entity, world, dt)\n  world:destroy_entity(Entity.from_raw({}))\nend\n",
        target.id()
    ));
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        ScriptCommand::DestroyEntity(e) => assert_eq!(*e, target),
        _ => panic!("Expected DestroyEntity command"),
    }
}

#[test]
fn test_script_reads_transform() {
    let entity = make_test_entity(5);
    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  local t = world:get_transform(entity)\n  if t then\n    world:set_position(entity, Vec3.new(t.position.x + 1, 0, 0))\n  end\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let mut transform = katla_math::Transform::default();
    transform.position = Vec3::new(5.0, 0.0, 0.0);
    let proxy = ScriptWorldProxy::from_shared(Rc::new(SharedWorldData {
        transforms: vec![(entity, transform)].into_iter().collect(),
        live_entities: vec![entity],
        component_entities: Default::default(),
        input_state: Default::default(),
    }));

    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        ScriptCommand::SetPosition(_, pos) => {
            assert_eq!(*pos, Vec3::new(6.0, 0.0, 0.0));
        }
        _ => panic!("Expected SetPosition"),
    }
}

#[test]
fn test_script_no_on_update_returns_empty_commands() {
    let script = TempScript::new("local x = 1\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    assert!(commands.is_empty());
}

#[test]
fn test_input_bindings_pressed() {
    let entity = make_test_entity(1);
    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  if world:is_action_pressed(\"jump\") then\n    world:set_position(entity, Vec3.new(0, 10, 0))\n  end\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let mut input = crate::bindings::script_world::InputSnapshot::default();
    input.pressed_actions.insert("jump".to_string());
    let proxy = ScriptWorldProxy::from_shared(Rc::new(SharedWorldData {
        transforms: Default::default(),
        live_entities: vec![entity],
        component_entities: Default::default(),
        input_state: input,
    }));

    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        ScriptCommand::SetPosition(_, pos) => assert_eq!(*pos, Vec3::new(0.0, 10.0, 0.0)),
        _ => panic!("Expected SetPosition"),
    }
}

#[test]
fn test_input_bindings_not_pressed() {
    let entity = make_test_entity(1);
    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  if world:is_action_pressed(\"jump\") then\n    world:set_position(entity, Vec3.new(0, 10, 0))\n  end\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    assert!(commands.is_empty());
}

#[test]
fn test_entity_exists_binding() {
    let e1 = make_test_entity(1);
    let e2 = make_test_entity(2);
    let script = TempScript::new(&format!(
        "function on_update(entity, world, dt)\n  if world:entity_exists(Entity.from_raw({})) then\n    world:set_position(entity, Vec3.new(1, 0, 0))\n  end\nend\n",
        e2.id()
    ));
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let handle = engine.create_instance(e1, script.to_str()).unwrap();

    let proxy = ScriptWorldProxy::from_shared(Rc::new(SharedWorldData {
        transforms: Default::default(),
        live_entities: vec![e2],
        component_entities: Default::default(),
        input_state: Default::default(),
    }));
    let commands = engine.execute_on_update(handle, e1, proxy, 0.016).unwrap();
    assert_eq!(commands.len(), 1);
}

#[test]
fn test_hot_reload_preserves_scalar_state() {
    // Use global assignment (not local) so state lives in the environment table
    // and can be preserved across hot reloads.
    let script = TempScript::new(
        "counter = 5\nfunction on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(counter, 0, 0))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.set_scripts_dir(script.parent().unwrap().to_str().unwrap());
    let script_name = script.file_stem().unwrap().to_str().unwrap();
    engine.load_script(script_name).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script_name).unwrap();

    // First update: counter should be 5
    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        ScriptCommand::SetPosition(_, pos) => assert_eq!(pos.x(), 5.0),
        _ => panic!("Expected SetPosition"),
    }

    // Rewrite script — new default counter is 0
    let new_content = "counter = 0\nfunction on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(counter, 0, 0))\nend\n";
    std::fs::write(&script, new_content).unwrap();

    engine.reload_script(script_name).unwrap();

    engine.reload_script(script_name).unwrap();
    let new_handles = engine.hot_reload_instances(script_name);
    assert_eq!(new_handles.len(), 1);
    let new_handle = new_handles[0];

    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(new_handle, entity, proxy, 0.016)
        .unwrap();
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        ScriptCommand::SetPosition(_, pos) => {
            assert_eq!(pos.x(), 5.0, "counter should be preserved from old env")
        }
        _ => panic!("Expected SetPosition"),
    }
}

#[test]
fn test_hot_reload_replaces_script_logic() {
    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(1, 0, 0))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.set_scripts_dir(script.parent().unwrap().to_str().unwrap());
    let script_name = script.file_stem().unwrap().to_str().unwrap();
    engine.load_script(script_name).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script_name).unwrap();

    // Verify original logic
    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    match &commands[0] {
        ScriptCommand::SetPosition(_, pos) => assert_eq!(*pos, Vec3::new(1.0, 0.0, 0.0)),
        _ => panic!("Expected SetPosition"),
    }

    // Rewrite with different logic
    let new_content = "function on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(2, 0, 0))\nend\n";
    std::fs::write(&script, new_content).unwrap();

    engine.reload_script(script_name).unwrap();
    let new_handles = engine.hot_reload_instances(script_name);
    let new_handle = new_handles[0];

    // New logic should produce Vec3(2, 0, 0)
    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(new_handle, entity, proxy, 0.016)
        .unwrap();
    match &commands[0] {
        ScriptCommand::SetPosition(_, pos) => assert_eq!(*pos, Vec3::new(2.0, 0.0, 0.0)),
        _ => panic!("Expected SetPosition"),
    }
}

#[test]
fn test_reload_script_replaces_chunk() {
    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(1, 0, 0))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.set_scripts_dir(script.parent().unwrap().to_str().unwrap());
    let script_name = script.file_stem().unwrap().to_str().unwrap();
    engine.load_script(script_name).unwrap();

    // Rewrite
    let new_content = "function on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(9, 0, 0))\nend\n";
    std::fs::write(&script, new_content).unwrap();

    engine.reload_script(script_name).unwrap();

    // New instance should use new code
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script_name).unwrap();
    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    match &commands[0] {
        ScriptCommand::SetPosition(_, pos) => assert_eq!(*pos, Vec3::new(9.0, 0.0, 0.0)),
        _ => panic!("Expected SetPosition"),
    }
}

#[test]
fn test_instance_reuse_after_removal() {
    let script = TempScript::new("function on_update(entity, world, dt)\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();

    let e1 = make_test_entity(1);
    let h1 = engine.create_instance(e1, script.to_str()).unwrap();
    engine.remove_instance(h1);

    // Slot should be reused
    let e2 = make_test_entity(2);
    let h2 = engine.create_instance(e2, script.to_str()).unwrap();
    assert_eq!(h2.index, h1.index); // same slot reused
    assert_ne!(h2.generation, h1.generation); // different generation

    // h1 should no longer work
    let proxy = make_proxy();
    assert!(engine.execute_on_update(h1, e1, proxy, 0.016).is_err());

    // h2 should work
    let proxy = make_proxy();
    assert!(engine.execute_on_update(h2, e2, proxy, 0.016).is_ok());
}

#[test]
fn test_script_error_recovery_continues() {
    let script = TempScript::new(
        "local call_count = 0\nfunction on_update(entity, world, dt)\n  call_count = call_count + 1\n  if call_count == 1 then error(\"first call fails\") end\n  world:set_position(entity, Vec3.new(call_count, 0, 0))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    // First call fails
    let proxy = make_proxy();
    assert!(
        engine
            .execute_on_update(handle, entity, proxy, 0.016)
            .is_err()
    );

    // Second call succeeds (error recovery works)
    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    assert_eq!(commands.len(), 1);
}

#[test]
fn test_script_dt_passed_correctly() {
    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(dt, 0, 0))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.033)
        .unwrap();
    match &commands[0] {
        ScriptCommand::SetPosition(_, pos) => {
            assert!((pos.x() - 0.033).abs() < 0.0001);
        }
        _ => panic!("Expected SetPosition"),
    }
}

#[test]
fn test_script_with_no_hooks_only_top_level() {
    let script = TempScript::new("local x = 42\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);

    // Should succeed even with no hooks
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    // on_update should return empty vec
    let proxy = make_proxy();
    let commands = engine
        .execute_on_update(handle, entity, proxy, 0.016)
        .unwrap();
    assert!(commands.is_empty());

    // on_spawn should return empty vec
    let proxy = make_proxy();
    let commands = engine.execute_on_spawn(handle, entity, proxy).unwrap();
    assert!(commands.is_empty());

    // on_destroy should succeed silently
    engine.call_on_destroy(handle, entity).unwrap();
}

#[test]
fn test_env_table_pairs_visibility() {
    let script = TempScript::new("my_var = 42\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(script.to_str()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine.create_instance(entity, script.to_str()).unwrap();

    let state = engine.gather_scalar_state(handle).unwrap();
    let has_my_var = state
        .iter()
        .any(|(k, v)| k == "my_var" && matches!(v, mlua::Value::Integer(42)));
    assert!(
        has_my_var,
        "Expected 'my_var = 42' in gathered state, got: {:?}",
        state
    );
}
