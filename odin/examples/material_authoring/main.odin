//! Real host-thread material calls use the scene owner's shared mailbox and history.
package main

import app "../../app"
import agent "../../agent"
import ecs "../../ecs"
import editor "../../editor"
import "core:thread"
import "core:fmt"
import "core:mem"

Producer :: struct { mailbox:^editor.Agent_Harness, arguments:[]byte }
producer :: proc(th:^thread.Thread) {
    state:=cast(^Producer)th.data
    assert(agent.submit_call(state.mailbox,{"surface-1","material",state.arguments})==.None)
    editor.agent_finish(state.mailbox)
}
Position :: struct { x:f32 }
main :: proc() {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker) }
    context.allocator=mem.tracking_allocator(&tracker)
    scene:app.Authoring; app.authoring_init(&scene); defer app.authoring_destroy(&scene)
    editor.editor_register(&scene.world,&scene.registry,"Position",Position{})
    a:=ecs.spawn(&scene.world,struct { surface:app.Surface_Material, position:Position }{app.Surface_Material{roughness=0.5,ao=1},Position{4}})
    b:=ecs.spawn(&scene.world,struct { surface:app.Surface_Material }{app.Surface_Material{roughness=0.7,ao=1}})
    args:=fmt.aprintf(`{{"action":"set","entity_ids":["%d","%d"],"preset":"oak","roughness":0.3}}`,u64(a),u64(b)); defer delete(args)
    state:=Producer{&scene.agent,transmute([]byte)args}
    worker:=thread.create(producer); worker.data=&state; thread.start(worker); thread.join(worker); thread.destroy(worker)
    unchanged,_:=ecs.get_component(&scene.world,a,app.Surface_Material); assert(unchanged.roughness==0.5 && !unchanged.has_tint)
    assert(app.authoring_tick(&scene)==1)
    response,ok:=editor.agent_take_result(&scene.agent); assert(ok && response.result.error==.None && len(response.result.entities)==2)
    defer editor.tool_result_destroy(&response.result)
    fmt.println(string(response.result.data))
    for id in ([2]ecs.Entity_Id{a,b}) {
        surface,_:=ecs.get_component(&scene.world,id,app.Surface_Material); assert(surface.has_tint && surface.roughness==0.3)
    }
    ecs.get_component_mut(&scene.world,a,Position).x=9
    assert(app.authoring_undo_last(&scene)==.None)
    original_a,_:=ecs.get_component(&scene.world,a,app.Surface_Material)
    original_b,_:=ecs.get_component(&scene.world,b,app.Surface_Material)
    position,_:=ecs.get_component(&scene.world,a,Position)
    assert(!original_a.has_tint && original_a.roughness==0.5 && !original_b.has_tint && original_b.roughness==0.7 && position.x==9)
    assert(ecs.validate(&scene.world))
    fmt.println("One shared undo step restored both materials and retained the unrelated scene edit")
}
