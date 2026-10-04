#+test
package editor

import ecs "../ecs"
import "core:testing"

Restore_Position :: struct { x:f32 }
Restore_Link :: struct { target:ecs.Entity_Id `inspect:"entity_ref"` }
Restore_Witness :: struct { reject:bool,prepares,commits:int,expected:f32,removed:ecs.Entity_Id,has_removed:bool }

test_restore_prepare :: proc(state:rawptr,w:^ecs.World,_:^Component_Registry,changed,removed:[]ecs.Entity_Id)->(rawptr,Scene_Error) {
    witness:=cast(^Restore_Witness)state; witness.prepares+=1
    for id in changed { value,exists:=ecs.get_component(w,id,Restore_Position); assert(exists && value.x==witness.expected) }
    if witness.has_removed { assert(len(removed)==1 && removed[0]==witness.removed && ecs.entity_exists(w,witness.removed)) }
    if witness.reject { return nil,.Invalid_Operation }
    return state,.None
}
test_restore_finish :: proc(state,token:rawptr,commit:bool) { assert(state==token && commit); (cast(^Restore_Witness)state).commits+=1 }

@(test)
test_restoration_rejection_retains_existing_values_and_removed_identity :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"RestorePosition",Restore_Position{})
    a:=ecs.spawn(&w,struct {p:Restore_Position}{{1}})
    b:=ecs.spawn(&w,struct {p:Restore_Position}{{2}})
    candidate:=ecs.spawn(&w,struct {p:Restore_Position}{{9}})
    components:=entity_components_capture(&w,&reg,candidate); defer entity_components_destroy(components,w.allocator)
    ecs.destroy_entity(&w,candidate)
    witness:=Restore_Witness{reject=true,expected=9,removed=b,has_removed=true}
    ecs.insert_resource(&w,Restoration_Participant{&witness,test_restore_prepare,test_restore_finish})
    rows:=[2]Restoration_Row{{a,true,components[:]},{b,false,nil}}
    remaps:=make([dynamic]Entity_Remap,w.allocator); defer delete(remaps)
    testing.expect_value(t,restoration_apply(&w,&reg,rows[:],&remaps),Scene_Error.Invalid_Operation)
    value,exists:=ecs.get_component(&w,a,Restore_Position)
    testing.expect(t,exists && value.x==1 && ecs.entity_exists(&w,b) && len(remaps)==0 && witness.commits==0)
    witness.reject=false
    testing.expect_value(t,restoration_apply(&w,&reg,rows[:],&remaps),Scene_Error.None)
    value,exists=ecs.get_component(&w,a,Restore_Position)
    testing.expect(t,exists && value.x==9 && !ecs.entity_exists(&w,b) && witness.commits==1)
}

@(test)
test_restoration_rejected_fresh_generation_restores_external_references :: proc(t:^testing.T) {
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:Component_Registry; editor_registry_init(&reg); defer editor_registry_destroy(&reg)
    editor_register(&w,&reg,"RestorePosition",Restore_Position{})
    editor_register(&w,&reg,"RestoreLink",Restore_Link{})
    target:=ecs.spawn(&w,struct {p:Restore_Position}{{7}})
    observer:=ecs.spawn(&w,struct {r:Restore_Link}{{target}})
    group:=removed_entities_group(&w,&reg,{target}); defer undo_group_destroy(&group)
    testing.expect_value(t,redo_group(&w,&reg,&group),Scene_Error.None)
    witness:=Restore_Witness{reject=true,expected=7}
    ecs.insert_resource(&w,Restoration_Participant{&witness,test_restore_prepare,test_restore_finish})
    testing.expect_value(t,undo_group(&w,&reg,&group),Scene_Error.Invalid_Operation)
    link,_:=ecs.get_component(&w,observer,Restore_Link)
    testing.expect(t,w.live_count==1 && link.target==target && group.entities[0]==target && len(group.remaps)==0)
    witness.reject=false
    testing.expect_value(t,undo_group(&w,&reg,&group),Scene_Error.None)
    link,_=ecs.get_component(&w,observer,Restore_Link)
    testing.expect(t,w.live_count==2 && group.entities[0]!=target && link.target==group.entities[0] && ecs.entity_exists(&w,link.target))
}
