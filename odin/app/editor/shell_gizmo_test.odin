#+test
package editor_app
import app ".."
import render "../render"
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import ui "../../ui"
import "core:testing"

@(test)
test_shell_gizmo_release_commits_final_pointer_when_motion_was_coalesced :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner);app.scene_components_register(&owner)
    entity:=ecs.create_entity(&owner.world);ecs.add_component(&owner.world,entity,app.Scene_Transform{local=km.transform()})
    state:State;state_init(&state,&owner);defer state_destroy(&state);selection_set(&state,entity)
    shell:=Shell{state=&state,allocator=owner.world.allocator};defer gizmo_destroy(&shell.gizmo)
    shell.viewports.slots[0].bounds={20,30,640,480}
    frame,frame_error:=render.frame_data({position={4,3,8},target={},up={0,1,0},fov_degrees=60,near=.01,far=100},640,480,true)
    testing.expect_value(t,frame_error,render.Scene_Error.None);shell.gizmo_frames[0]=frame
    meshes:[4]render.Overlay_Mesh;shell.gizmo_meshes=&meshes
    error:editor.Scene_Error
    meshes[0],error=render.overlay_mesh_prepare(&owner,{selected={entity},gizmo=true,basis=km.identity(km.Mat4)},frame,640,480)
    testing.expect_value(t,error,editor.Scene_Error.None);defer render.overlay_mesh_destroy(&meshes[0])
    start:ui.Vec2;found:=false
    for triangle,index in meshes[0].triangles {
        if triangle.handle!=.Axis_X { continue }
        center:=km.xyz(meshes[0].vertices[index*3].position+meshes[0].vertices[index*3+1].position+meshes[0].vertices[index*3+2].position)/3
        clip:=km.matrix_vector(frame.view_projection,km.vec4(center,1));start={20+(clip[0]/clip[3]+1)*320,30+(clip[1]/clip[3]+1)*240}
        hit:=gizmo_hit(&meshes[0],frame.view_projection,shell.viewports.slots[0].bounds,start)
        if hit.hit && hit.handle==.Axis_X { found=true;break }
    };testing.expect(t,found)
    testing.expect(t,shell_gizmo_pointer(&shell,{position=start,button=.Left,pressed=true},0) && shell.gizmo.gesture.active)
    target:=shell.gizmo.start+km.Vec3{1,0,0};clip:=km.matrix_vector(frame.view_projection,km.vec4(target,1))
    finish:=ui.Vec2{20+(clip[0]/clip[3]+1)*320,30+(clip[1]/clip[3]+1)*240}
    testing.expect(t,shell_gizmo_pointer(&shell,{position=finish,button=.Left,released=true},0))
    transform,_:=ecs.get_component(&owner.world,entity,app.Scene_Transform)
    testing.expect(t,shell.state.last_error==.None && !shell.gizmo.gesture.active && abs(transform.local.position[0]-1)<.001)
    testing.expect_value(t,len(owner.agent.session.actions),1)
    testing.expect_value(t,app.authoring_undo_last(&owner),editor.Scene_Error.None)
    transform,_=ecs.get_component(&owner.world,entity,app.Scene_Transform);testing.expect_value(t,transform.local.position,km.VEC3_ZERO)
}
