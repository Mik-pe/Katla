//! Runnable host-thread JSON submission composed with caller-thread scene ownership.
package main

import agent "../../agent"
import editor "../../editor"
import ecs "../../ecs"
import "core:thread"
import "core:fmt"

Position :: struct { x,y,z:f32, scale_x,scale_y,scale_z:f32 }
producer :: proc(th:^thread.Thread) {
    harness:=cast(^editor.Agent_Harness)th.data
    err:=agent.submit_call(harness,{"spawn-1","spawn_entity",transmute([]byte)string(`{"position":[3,2,1]}`)})
    assert(err==.None)
    editor.agent_finish(harness)
}
main :: proc() {
    world:ecs.World; ecs.world_init(&world); defer ecs.world_destroy(&world)
    registry:editor.Component_Registry; editor.editor_registry_init(&registry); defer editor.editor_registry_destroy(&registry)
    editor.editor_register(&world,&registry,"Position",Position{})
    existing:=ecs.spawn(&world,struct { position:Position }{Position{9,9,9,1,1,1}})
    harness:editor.Agent_Harness; editor.agent_harness_init(&harness); defer editor.agent_harness_destroy(&harness)
    worker:=thread.create(producer); worker.data=&harness; thread.start(worker); thread.join(worker); thread.destroy(worker)
    assert(world.live_count==1 && ecs.entity_exists(&world,existing))
    assert(editor.agent_tick(&harness,&world,&registry)==1)
    response,ok:=editor.agent_take_result(&harness); assert(ok && response.result.error==.None)
    defer editor.tool_result_destroy(&response.result)
    id:=response.result.entities[0]
    position,present:=ecs.get_component(&world,id,Position)
    assert(present && position.x==3 && position.y==2 && position.z==1)
    observation:=agent.scene_context(&world,&registry,id,true); defer agent.scene_context_destroy(&observation)
    assert(observation.entity_count==2 && observation.has_selection)
    fmt.printf("Agent action %d: entity %d at (%g, %g, %g)\n",response.id,u64(id),position.x,position.y,position.z)
    assert(editor.agent_undo_all(&harness.session,&world,&registry)==.None)
    assert(world.live_count==1 && ecs.entity_exists(&world,existing))
    fmt.println("Undo preserved the pre-existing scene")
}
