#+test
//! Projection and ray tests validate actual rendered mesh behavior independently of native pixels.
package render

import app ".."
import ecs "../../ecs"
import km "../../math"
import "core:testing"

@(test)
test_overlay_gizmo_modes_keep_screen_size_and_exact_triangle_hit :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform}{{km.transform()}})
    state:=Editor_Overlay_State{selected={entity},pivot={0,0,0},basis=km.identity(km.Mat4),gizmo=true}
    for mode in ([3]Overlay_Mode{.Translate,.Rotate,.Scale}) {
        state.mode=mode
        counts:[2]int
        extents:[2]f32
        for distance,i in ([2]f32{5,10}) {
            camera:=camera_default(); camera.position={0,0,distance}; camera.target={0,0,0}
            frame,error:=frame_data(camera,640,480,false); testing.expect_value(t,error,Scene_Error.None)
            mesh,mesh_error:=overlay_mesh_prepare(&owner,state,frame,640,480); testing.expect(t,mesh_error==.None); defer overlay_mesh_destroy(&mesh)
            counts[i]=len(mesh.vertices); max_x:f32
            for vertex in mesh.vertices { clip:=km.matrix_vector(frame.view_projection,vertex.position); max_x=max(max_x,abs(clip[0]/clip[3])) }
            extents[i]=max_x
            if mode!=.Rotate {
                point:=km.xyz(mesh.vertices[0].position+mesh.vertices[1].position+mesh.vertices[2].position)/3
                result:=overlay_hit_test(&mesh,camera.position,point-camera.position)
                testing.expect(t,result.hit && result.handle!=.None && result.entity==entity,"hit geometry must be the actual rendered gizmo triangles")
            }
        }
        testing.expect(t,counts[0]>100 && counts[0]==counts[1])
        testing.expect(t,abs(extents[0]-extents[1])<.0001,"gizmo changed projected size with camera distance")
    }
}

@(test)
test_overlay_debug_hierarchy_body_presence_and_billboard_identity :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    parent:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform}{{km.transform(position={3,0,0})}})
    box:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,parent:app.Scene_Parent,body:app.Physics_Body}{{km.transform(position={1,0,0})},{parent},app.physics_body({kind=.Box,half_extents={1,2,1}},.Fixed)})
    no_shape:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,body:app.Physics_Body}{{km.transform(position={100,0,0})},app.physics_body({kind=.None})})
    light:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,light:app.Scene_Point_Light}{{km.transform(position={0,0,0})},app.point_light_default()})
    camera:=camera_default(); camera.position={0,0,10}; camera.target={0,0,0}
    frame,_:=frame_data(camera,640,480,false)
    mesh,error:=overlay_mesh_prepare(&owner,{physics=true,billboards=true},frame,640,480); testing.expect(t,error==.None); defer overlay_mesh_destroy(&mesh)
    min_x,max_x:=max(f32),-max(f32)
    for triangle,i in mesh.triangles {
        testing.expect(t,triangle.entity!=no_shape,"body without collider fabricated debug geometry")
        if triangle.entity==box { for vertex in mesh.vertices[i*3:i*3+3] { min_x=min(min_x,vertex.position[0]); max_x=max(max_x,vertex.position[0]) } }
    }
    testing.expect(t,abs(min_x-3)<.01 && abs(max_x-5)<.01,"collider debug lost parent world transform")
    result:=overlay_hit_test(&mesh,camera.position,{0,0,-1}); testing.expect(t,result.hit && result.entity==light && result.handle==.None)
    testing.expect(t,mesh.vertices[len(mesh.vertices)-1].uv[2]==1,"actual light icon role was lost")
    testing.expect(t,len(mesh.vertices)>36 && mesh.gizmo_first==len(mesh.vertices))
}

@(test)
test_overlay_billboard_initial_zero_identity_remains_pickable :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,light:app.Scene_Point_Light}{{km.transform()},app.point_light_default()})
    testing.expect_value(t,entity,ecs.Entity_Id(0))
    camera:=camera_default(); camera.position={0,0,5}; camera.target={0,0,0}; frame,_:=frame_data(camera,640,480,false)
    mesh,error:=overlay_mesh_prepare(&owner,{billboards=true},frame,640,480); testing.expect(t,error==.None); defer overlay_mesh_destroy(&mesh)
    testing.expect_value(t,len(mesh.triangles),2)
    testing.expect(t,mesh.triangles[0].has_entity && mesh.triangles[0].entity==0)
    result:=overlay_hit_test(&mesh,camera.position,{0,0,-1}); testing.expect(t,result.hit && result.entity==entity,"zero is a valid generational identity rather than a background marker")
}

@(test)
test_overlay_authored_billboard_tint_size_and_invalid_descriptor :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    entity:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,billboard:app.Scene_Billboard}{{km.transform()},{.Fire,{.5,.25,.75,.4},2}})
    camera:=camera_default(); camera.position={0,0,5}; camera.target={0,0,0}; frame,_:=frame_data(camera,640,480,false)
    mesh,error:=overlay_mesh_prepare(&owner,{billboards=true},frame,640,480); testing.expect(t,error==.None); defer overlay_mesh_destroy(&mesh)
    testing.expect_value(t,len(mesh.vertices),6)
    expected:=km.color_to_array(km.color_to_linear({.5,.25,.75,.4}))
    for vertex in mesh.vertices { testing.expect_value(t,vertex.color,expected); testing.expect_value(t,vertex.uv[2],f32(2)) }
    a,b:=km.matrix_vector(frame.view_projection,mesh.vertices[0].position),km.matrix_vector(frame.view_projection,mesh.vertices[1].position)
    testing.expect(t,abs((b[0]/b[3]-a[0]/a[3])*320-80)<.001,"authored size multiplier lost its actual projected width")
    ecs.get_component_mut(&owner.world,entity,app.Scene_Billboard).size=0
    rejected,reject_error:=overlay_mesh_prepare(&owner,{billboards=true},frame,640,480)
    testing.expect(t,reject_error!=.None && len(rejected.vertices)==0 && len(rejected.triangles)==0,"invalid descriptor returned partial geometry")
}
