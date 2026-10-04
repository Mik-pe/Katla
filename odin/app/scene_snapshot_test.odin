#+test
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:testing"
import "core:strings"

@(test)
test_scene_snapshot_references_owned_labels_and_atomic_rollback :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); scene_components_register(&app)
    hidden:=ecs.spawn(&app.world,struct { hidden:Editor_Hidden }{})
    root:=ecs.spawn(&app.world,struct { name:Scene_Name, transform:Scene_Transform }{{strings.clone("Root")},{km.TRANSFORM_IDENTITY}})
    child:=ecs.spawn(&app.world,struct { name:Scene_Name, transform:Scene_Transform, parent:Scene_Parent }{{strings.clone("Child")},{km.transform(position={1,2,3})},{root}})
    snapshot,err:=scene_snapshot_capture(&app); defer scene_snapshot_destroy(&snapshot)
    testing.expect(t,err==.None && len(snapshot.entities)==2)
    ecs.get_component_mut(&app.world,child,Scene_Transform).local.position={9,9,9}
    damaged:=snapshot.entities[1].components[0].data
    malformed:string=`{`
    snapshot.entities[1].components[0].data=transmute([]byte)malformed
    testing.expect_value(t,scene_snapshot_restore(&app,&snapshot),editor.Scene_Error.Decode_Failed)
    snapshot.entities[1].components[0].data=damaged
    testing.expect(t,ecs.entity_exists(&app.world,root) && ecs.entity_exists(&app.world,child) && app.world.live_count==3)
    testing.expect_value(t,scene_snapshot_restore(&app,&snapshot),editor.Scene_Error.None)
    testing.expect(t,ecs.entity_exists(&app.world,hidden) && !ecs.entity_exists(&app.world,root) && !ecs.entity_exists(&app.world,child))
    ids:=ecs.entity_ids(&app.world); defer delete(ids)
    current_root,current_child:ecs.Entity_Id
    for id in ids { if name,ok:=ecs.get_component(&app.world,id,Scene_Name); ok { if name.name=="Root" { current_root=id } else if name.name=="Child" { current_child=id } } }
    parent,ok:=ecs.get_component(&app.world,current_child,Scene_Parent); testing.expect(t,ok && parent.entity==current_root)
    position,_:=ecs.get_component(&app.world,current_child,Scene_Transform); testing.expect_value(t,position.local.position,km.Vec3{1,2,3})
    ecs.add_component(&app.world,current_root,Scene_Parent{hidden})
    invalid,capture_error:=scene_snapshot_capture(&app); defer scene_snapshot_destroy(&invalid)
    testing.expect_value(t,capture_error,editor.Scene_Error.Invalid_Operation)
}

@(test)
test_hierarchy_retains_nonuniform_scale_shear_and_rejects_cycle :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app); scene_components_register(&app)
    parent:=ecs.spawn(&app.world,struct { transform:Scene_Transform }{{km.transform(scale={2,1,3})}})
    child:=ecs.spawn(&app.world,struct { transform:Scene_Transform }{{km.transform(position={1,2,3})}})
    testing.expect_value(t,scene_set_parent(&app,child,parent,true),editor.Scene_Error.None)
    world_matrix,err:=scene_world_matrix(&app,child); testing.expect(t,err==.None && km.xyz(world_matrix[3])==km.Vec3{2,2,9})
    testing.expect_value(t,scene_set_parent(&app,parent,child,true),editor.Scene_Error.Invalid_Operation)
    testing.expect_value(t,scene_set_parent(&app,child,0,false),editor.Scene_Error.None)
}
