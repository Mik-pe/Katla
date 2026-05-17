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

fn write_temp_script(content: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("katla_script_test");
    std::fs::create_dir_all(&dir).unwrap();
    let name = unique_script_name();
    let path = dir.join(format!("{name}.luau"));
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    path
}

fn cleanup_temp_script(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
}

fn make_proxy() -> ScriptWorldProxy {
    ScriptWorldProxy::from_shared(Rc::new(SharedWorldData {
        transforms: Default::default(),
        live_entities: Vec::new(),
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
    let path = write_temp_script("local x = 1 + 1\n");
    let mut engine = ScriptEngine::new().unwrap();
    let result = engine.load_script(path.to_str().unwrap());
    assert!(result.is_ok(), "load_script failed: {:?}", result.err());
    cleanup_temp_script(&path);
}

#[test]
fn test_load_nonexistent_script() {
    let mut engine = ScriptEngine::new().unwrap();
    let result = engine.load_script("nonexistent_script_12345");
    assert!(result.is_err());
}

#[test]
fn test_create_instance_with_hooks() {
    let path = write_temp_script("function on_update(entity, world, dt)\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(path.to_str().unwrap()).unwrap();
    let entity = make_test_entity(1);
    let result = engine.create_instance(entity, path.to_str().unwrap());
    assert!(result.is_ok(), "create_instance failed: {:?}", result.err());
    let handle = result.unwrap();
    assert_eq!(handle.index, 0);
    assert_eq!(handle.generation, 0);
    cleanup_temp_script(&path);
}

#[test]
fn test_execute_on_update_with_commands() {
    let path = write_temp_script(
        "function on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(1, 2, 3))\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(path.to_str().unwrap()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine
        .create_instance(entity, path.to_str().unwrap())
        .unwrap();

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
    cleanup_temp_script(&path);
}

#[test]
fn test_load_syntax_error_script() {
    let path = write_temp_script("function broken(\n");
    let mut engine = ScriptEngine::new().unwrap();
    let result = engine.load_script(path.to_str().unwrap());
    match result {
        Err(crate::error::ScriptError::LoadFailed { .. }) => {}
        Err(other) => panic!("Expected LoadFailed, got: {other}"),
        Ok(_) => panic!("Expected load to fail"),
    }
    cleanup_temp_script(&path);
}

#[test]
fn test_script_component() {
    let comp = ScriptComponent::new("test_script");
    assert_eq!(comp.script_path, "test_script");
    assert!(comp.instance_handle.is_none());
}

#[test]
fn test_remove_instance() {
    let path = write_temp_script("function on_update(entity, world, dt)\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(path.to_str().unwrap()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine
        .create_instance(entity, path.to_str().unwrap())
        .unwrap();

    engine.remove_instance(handle);

    let proxy = make_proxy();
    let result = engine.execute_on_update(handle, entity, proxy, 0.016);
    match result {
        Err(crate::error::ScriptError::InstanceNotFound(h)) => assert_eq!(h, handle),
        Err(other) => panic!("Expected InstanceNotFound, got: {other}"),
        Ok(_) => panic!("Expected error after removal"),
    }
    cleanup_temp_script(&path);
}

#[test]
fn test_error_count_increments_on_failure() {
    let path = write_temp_script(
        "function on_update(entity, world, dt)\n  error(\"script error\")\nend\n",
    );
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(path.to_str().unwrap()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine
        .create_instance(entity, path.to_str().unwrap())
        .unwrap();

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

    cleanup_temp_script(&path);
}

#[test]
fn test_error_count_starts_at_zero() {
    let path = write_temp_script("function on_update(entity, world, dt)\nend\n");
    let mut engine = ScriptEngine::new().unwrap();
    engine.load_script(path.to_str().unwrap()).unwrap();
    let entity = make_test_entity(1);
    let handle = engine
        .create_instance(entity, path.to_str().unwrap())
        .unwrap();

    let inst = engine
        .instances
        .get(handle.index as usize)
        .unwrap()
        .as_ref()
        .unwrap();
    assert_eq!(inst.error_count, 0);

    cleanup_temp_script(&path);
}
