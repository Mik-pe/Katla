//! Exercises the application-owned assistant and owner-thread scene application against explicit providers.
package main

import app "../../app"
import agent "../../agent"
import editor "../../editor"
import "core:os"
import "core:fmt"
import "core:mem"
import "core:time"
import "core:encoding/json"

main :: proc() {
    if len(os.args)<2 || len(os.args)>3 { fmt.eprintln("usage: assistant_authoring <llm.toml> [cancel|paused|cancel_after_tool]"); os.exit(2) }
    mode:=""; if len(os.args)==3 { mode=os.args[2] }
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); context.allocator=mem.tracking_allocator(&tracker)
    scene:app.Authoring; app.authoring_init(&scene,agent_capacity=2); assert(app.authoring_services_init(&scene)==.None)
    names:=[2]string{"spawn_entity","query_entities"}; schemas,schema_error:=agent.tools_select(names[:]); assert(schema_error==.None)
    owner:app.Assistant; assert(app.assistant_init_path(&owner,&scene.agent,os.args[1],schemas,"Use the supplied tools and actual scene results.")==.None); delete(schemas)
    assert(app.assistant_start(&owner,"Create one fox, inspect the scene and describe the result.")==.None)
    assert(app.assistant_start(&owner,"Duplicate request")==.Busy && app.assistant_reset(&owner)==.Busy)
    start:=time.tick_now(); saw_progress:=false; ticks:=0
    for owner.job.worker!=nil {
        app.assistant_poll(&owner); if len(owner.output)>0 && owner.job.worker!=nil { saw_progress=true }
        if mode!="paused" { app.authoring_tick(&scene); ticks+=1 }
        if (mode=="cancel" || mode=="paused") && time.tick_since(start)>100*time.Millisecond { app.assistant_cancel(&owner) }
        if mode=="cancel_after_tool" && len(scene.agent.session.actions)>0 { app.assistant_cancel(&owner) }
        assert(time.tick_since(start)<5*time.Second); time.sleep(time.Millisecond)
    }
    app.authoring_tick(&scene); app.assistant_poll(&owner)
    assert(owner.conversation.pending==0 && scene.agent.outstanding==0)
    result:=struct { error,state,text:string,entities,actions,messages,ticks:int,progress:bool }{fmt.tprintf("%s",owner.error),fmt.tprintf("%s",owner.state),string(owner.output[:]),scene.world.live_count,len(scene.agent.session.actions),len(owner.conversation.history),ticks,saw_progress}
    if owner.state==.Failed { assert(app.assistant_start(&owner,"Silent retry")==owner.error) }
    assert(app.assistant_reset(&owner)==.None && owner.state==.Idle && len(owner.output)==0 && len(owner.conversation.history)==1)
    assert(scene.world.live_count==result.entities && len(scene.agent.session.actions)==result.actions)
    encoded,error:=json.marshal(result); assert(error==nil); fmt.println(string(encoded)); delete(encoded)
    assert(editor.agent_undo_all(&scene.agent.session,&scene.world,&scene.registry)==.None && scene.world.live_count==0)
    assert(app.assistant_destroy(&owner)==0); app.authoring_destroy(&scene)
    context.allocator=backing; assert(len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0); mem.tracking_allocator_destroy(&tracker)
}
