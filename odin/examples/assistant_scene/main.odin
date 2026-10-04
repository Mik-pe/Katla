//! Exercises provider-driven scene publication and staged loading in a confined temporary project.
package main

import app "../../app"
import agent "../../agent"
import ecs "../../ecs"
import "core:os"
import "core:fmt"
import "core:mem"
import "core:time"
import "core:encoding/json"

main :: proc() {
    if len(os.args)!=4 { fmt.eprintln("usage: assistant_scene <llm.toml> <temporary-project> <resources>"); os.exit(2) }
    backing:=context.allocator; tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); context.allocator=mem.tracking_allocator(&tracker)
    authoring:app.Authoring; app.authoring_init(&authoring,agent_capacity=2); assert(app.authoring_services_init(&authoring)==.None)
    assert(app.asset_resources_init(&authoring,os.args[2],os.args[3])==.None)
    schemas,schema_error:=agent.tools_select({"spawn_entity","query_entities","material","save_scene","load_scene"}); assert(schema_error==.None)
    owner:app.Assistant; assert(app.assistant_init_path(&owner,&authoring.agent,os.args[1],schemas,"Use actual scene results. Save scene.katla, edit, verify a failed load preserves the world, then load scene.katla and query fresh identities.")==.None); delete(schemas)
    assert(app.assistant_start(&owner,"Create a fox, save it, edit its material, check a missing-file load, restore the saved scene and report actual identities.")==.None)
    start:=time.tick_now(); ticks:=0; loaded,failed_load_preserved:bool; original:ecs.Entity_Id; saw_original:bool
    for owner.job.worker!=nil {
        accepted:=app.authoring_tick(&authoring); ticks+=accepted
        ids:=ecs.entity_ids(&authoring.world)
        if len(ids)==1 {
            if !saw_original { original=ids[0]; saw_original=true }
            material,_:=ecs.get_component(&authoring.world,ids[0],app.Surface_Material)
            if ids[0]==original && material.metallic==0.8 && ticks>=5 { failed_load_preserved=true }
            if ids[0]!=original { loaded=true; assert(material.metallic==0 && material.roughness==0.5) }
        }
        delete(ids)
        app.assistant_poll(&owner); assert(time.tick_since(start)<8*time.Second); time.sleep(time.Millisecond)
    }
    app.authoring_tick(&authoring); app.assistant_poll(&owner)
    assert(owner.state==.Completed && owner.error==.None && owner.conversation.pending==0 && authoring.agent.outstanding==0)
    assert(string(owner.output[:])=="Scene restored with fresh IDs.")
    assert(loaded && failed_load_preserved && authoring.world.live_count==1 && !ecs.entity_exists(&authoring.world,original))
    edits:=0; for action in authoring.agent.session.actions { if action.undo.state!=nil { edits+=1 } }; assert(edits==0 && len(authoring.agent.session.actions)==2)
    state,present:=ecs.get_resource(&authoring.world,app.Scene_File_State); assert(present && state.path=="scene.katla")
    assert(app.assistant_reset(&owner)==.None && owner.state==.Idle && len(owner.conversation.history)==1 && authoring.world.live_count==1)
    bytes,error:=json.marshal(struct { text:string,ticks,entities,edits:int,loaded,failed_load_preserved,reset:bool }{ "Scene restored with fresh IDs.",ticks,authoring.world.live_count,edits,loaded,failed_load_preserved,true }); assert(error==nil); fmt.println(string(bytes)); delete(bytes)
    assert(app.assistant_destroy(&owner)==0); app.authoring_destroy(&authoring)
    context.allocator=backing; assert(len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0); mem.tracking_allocator_destroy(&tracker)
}
