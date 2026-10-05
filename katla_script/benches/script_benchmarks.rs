use std::io::Write;

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use katla_ecs::EntityId;
use katla_script::bindings::script_world::ScriptWorldProxy;
use katla_script::engine::ScriptEngine;

struct TempScript {
    path: std::path::PathBuf,
}

impl TempScript {
    fn new(content: &str) -> Self {
        let dir = std::env::temp_dir().join("katla_script_bench");
        let _ = std::fs::create_dir_all(&dir);
        let name = format!("bench_{}", std::process::id());
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

fn make_entity(index: u32) -> EntityId {
    EntityId::from_raw(index as u64)
}

/// Benchmark: create N script instances from the same script.
fn bench_create_instances(c: &mut Criterion) {
    let script = TempScript::new("function on_update(entity, world, dt)\nend\n");
    let mut group = c.benchmark_group("create_instances");

    for count in [10, 100, 500, 1000] {
        group.bench_with_input(BenchmarkId::new("instances", count), &count, |b, &count| {
            b.iter(|| {
                let mut engine = ScriptEngine::new().unwrap();
                engine.load_script(script.to_str()).unwrap();
                for i in 0..count {
                    let entity = make_entity(i);
                    engine.create_instance(entity, script.to_str()).unwrap();
                }
                black_box(&engine);
            });
        });
    }
    group.finish();
}

/// Benchmark: execute on_update for N instances (empty hook).
fn bench_on_update(c: &mut Criterion) {
    let script = TempScript::new("function on_update(entity, world, dt)\nend\n");
    let mut group = c.benchmark_group("on_update");

    for count in [10, 100, 500, 1000] {
        let mut engine = ScriptEngine::new().unwrap();
        engine.load_script(script.to_str()).unwrap();
        let handles: Vec<_> = (0..count)
            .map(|i| {
                let entity = make_entity(i);
                engine.create_instance(entity, script.to_str()).unwrap()
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("empty_hook", count),
            &handles,
            |b, handles| {
                b.iter(|| {
                    for (i, handle) in handles.iter().enumerate() {
                        let entity = make_entity(i as u32);
                        let proxy = ScriptWorldProxy::new();
                        let _ = black_box(engine.execute_on_update(*handle, entity, proxy, 0.016));
                    }
                });
            },
        );
    }
    group.finish();
}

/// Benchmark: on_update with command emission (world:set_position).
fn bench_on_update_with_commands(c: &mut Criterion) {
    let script = TempScript::new(
        "function on_update(entity, world, dt)\n  world:set_position(entity, Vec3.new(1, 2, 3))\nend\n",
    );
    let mut group = c.benchmark_group("on_update_commands");

    for count in [10, 100, 500, 1000] {
        let mut engine = ScriptEngine::new().unwrap();
        engine.load_script(script.to_str()).unwrap();
        let handles: Vec<_> = (0..count)
            .map(|i| {
                let entity = make_entity(i);
                engine.create_instance(entity, script.to_str()).unwrap()
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("set_position", count),
            &handles,
            |b, handles| {
                b.iter(|| {
                    for (i, handle) in handles.iter().enumerate() {
                        let entity = make_entity(i as u32);
                        let proxy = ScriptWorldProxy::new();
                        let _ = black_box(engine.execute_on_update(*handle, entity, proxy, 0.016));
                    }
                });
            },
        );
    }
    group.finish();
}

/// Benchmark: on_update with vector math operations.
fn bench_on_update_math_heavy(c: &mut Criterion) {
    let script = TempScript::new(
        r#"
function on_update(entity, world, dt)
    local pos = Vec3.new(1, 2, 3)
    local dir = Vec3.new(4, 5, 6)
    local len = dir:length()
    local normalized = dir:normalize()
    local dot = pos:dot(dir)
    local cross = pos:cross(dir)
    local result = pos + dir * dt
end
"#,
    );
    let mut group = c.benchmark_group("on_update_math");

    for count in [10, 100, 500, 1000] {
        let mut engine = ScriptEngine::new().unwrap();
        engine.load_script(script.to_str()).unwrap();
        let handles: Vec<_> = (0..count)
            .map(|i| {
                let entity = make_entity(i);
                engine.create_instance(entity, script.to_str()).unwrap()
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("vector_ops", count),
            &handles,
            |b, handles| {
                b.iter(|| {
                    for (i, handle) in handles.iter().enumerate() {
                        let entity = make_entity(i as u32);
                        let proxy = ScriptWorldProxy::new();
                        let _ = black_box(engine.execute_on_update(*handle, entity, proxy, 0.016));
                    }
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_create_instances,
    bench_on_update,
    bench_on_update_with_commands,
    bench_on_update_math_heavy,
);
criterion_main!(benches);
