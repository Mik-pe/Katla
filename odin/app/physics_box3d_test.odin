#+test
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import box3d "../physics/box3d"
import "core:testing"

BOX3D_LIBRARY :: #config(BOX3D_LIBRARY,"")
@(test)
test_box3d_missing_dependency_preserves_selection_and_scene :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    scene_components_register(&app); physics_register(&app)
    entity:=ecs.spawn(&app.world,struct { transform:Scene_Transform }{Scene_Transform{km.TRANSFORM_IDENTITY}})
    testing.expect_value(t,physics_select_box3d(&app,"__katla_missing_box3d_dependency__"),editor.Scene_Error.Application_Owned)
    testing.expect(t,ecs.entity_exists(&app.world,entity) && !ecs.contains_resource(&app.world,box3d.Backend) && !ecs.contains_resource(&app.world,Physics_Selection))
}
when BOX3D_LIBRARY!="" {
@(test)
test_box3d_scene_parent_local_commit_contact_and_sensor_transitions :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    scene_components_register(&app); physics_register(&app)
    testing.expect_value(t,physics_select_box3d(&app,BOX3D_LIBRARY),editor.Scene_Error.None)
    parent:=ecs.spawn(&app.world,struct { transform:Scene_Transform }{Scene_Transform{km.Transform{position={4,0,0},scale={1,1,1},rotation=km.QUAT_IDENTITY}}})
    ground:=ecs.spawn(&app.world,struct { transform:Scene_Transform,body:Physics_Body }{
        Scene_Transform{km.Transform{position={0,-0.5,0},scale={1,1,1},rotation=km.QUAT_IDENTITY}},physics_body({kind=.Box,half_extents={10,0.5,10}},.Fixed)})
    ball:=ecs.spawn(&app.world,struct { transform:Scene_Transform,parent:Scene_Parent,body:Physics_Body }{
        Scene_Transform{km.Transform{position={-4,3,0},scale={1,1,1},rotation=km.QUAT_IDENTITY}},Scene_Parent{parent},physics_body({kind=.Sphere,radius=0.5})})
    sensor:=ecs.spawn(&app.world,struct { transform:Scene_Transform,body:Physics_Body }{
        Scene_Transform{km.Transform{position={0,1.5,0},scale={1,1,1},rotation=km.QUAT_IDENTITY}},physics_body({kind=.Box,half_extents={1,0.2,1}},.Fixed,true)})
    entered,left:=false,false
    for _ in 0..<240 {
        step:=physics_step(&app,1.0/60); testing.expect_value(t,step.error,editor.Scene_Error.None)
        for event in step.events { if event.trigger==sensor && event.other==ball { if event.phase==.Enter { entered=true } else { left=true } } }
        physics_step_result_destroy(&step)
    }
    local,_:=ecs.get_component(&app.world,ball,Scene_Transform)
    world,err:=scene_world_matrix(&app,ball); testing.expect_value(t,err,editor.Scene_Error.None)
    testing.expect(t,abs(local.local.position[0]+4)<0.01 && abs(world[3][0])<0.01 && abs(world[3][1]-0.5)<0.03)
    testing.expect(t,entered && left && ecs.entity_exists(&app.world,ground))
    testing.expect_value(t,len(app.agent.session.actions),0)
}
@(test)
test_box3d_scene_preflight_rejects_whole_batch_and_backend_switch_gate :: proc(t:^testing.T) {
    app:Authoring; authoring_init(&app); defer authoring_destroy(&app)
    scene_components_register(&app); physics_register(&app)
    testing.expect_value(t,physics_select_box3d(&app,BOX3D_LIBRARY),editor.Scene_Error.None)
    a:=ecs.spawn(&app.world,struct { transform:Scene_Transform,body:Physics_Body }{Scene_Transform{km.TRANSFORM_IDENTITY},physics_body({kind=.Sphere,radius=1})})
    testing.expect_value(t,physics_box3d_sync(&app),editor.Scene_Error.None)
    owner:=ecs.get_resource_mut(&app.world,box3d.Backend); testing.expect_value(t,len(owner.entries),1)
    bad:=ecs.spawn(&app.world,struct { transform:Scene_Transform,body:Physics_Body }{Scene_Transform{km.TRANSFORM_IDENTITY},physics_body({kind=.Box,half_extents={1,0,1}})})
    ecs.get_component_mut(&app.world,a,Scene_Transform).local.position={9,9,9}
    testing.expect_value(t,physics_box3d_sync(&app),editor.Scene_Error.Invalid_Field_Value)
    testing.expect(t,len(owner.entries)==1 && owner.entries[u64(a)].body.position==[3]f32{})
    ecs.destroy_entity(&app.world,bad)
    testing.expect_value(t,physics_box3d_sync(&app),editor.Scene_Error.None)
    testing.expect_value(t,owner.entries[u64(a)].body.position,[3]f32{9,9,9})
    app.mode=.Playing
    testing.expect_value(t,physics_select_rapier(&app),editor.Scene_Error.Editing_Required)
    testing.expect_value(t,physics_select_box3d(&app,BOX3D_LIBRARY),editor.Scene_Error.Editing_Required)
    app.mode=.Editing
    testing.expect_value(t,physics_select_rapier(&app),editor.Scene_Error.Application_Owned)
    selection,_:=ecs.get_resource(&app.world,Physics_Selection); testing.expect_value(t,selection.backend,Physics_Backend.Box3D)
}
}
