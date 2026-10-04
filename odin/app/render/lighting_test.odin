#+test
package render

import app ".."
import ecs "../../ecs"
import gfx "../../gfx"
import km "../../math"
import "core:testing"
import "core:mem"

@(test)
test_light_snapshot_preserves_hierarchy_and_prospective_capacity :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    parent:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform}{{km.transform(position={2,0,0})}})
    child:=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,parent:app.Scene_Parent,light:app.Scene_Point_Light}{{km.transform(position={1,2,3})},{parent},{{0.2,0.4,0.6},3,4}})
    first:=ecs.spawn(&owner.world,struct {light:app.Scene_Directional_Light}{{{0,-2,0},{1,1,1},4}})
    _=ecs.spawn(&owner.world,struct {light:app.Scene_Directional_Light}{{{1,-1,0},{1,0,0},2}})
    snapshot,error:=lighting_collect_entities(&owner,{child,first})
    testing.expect_value(t,error,Scene_Error.None)
    testing.expect_value(t,snapshot.count,u32(1))
    testing.expect_value(t,snapshot.points[0].position,km.Vec3{3,2,3})
    testing.expect_value(t,snapshot.sun.direction,km.Vec3{0,-1,0})
    ids:=make([]ecs.Entity_Id,MAX_POINT_LIGHTS+1); defer delete(ids)
    for &id in ids { id=ecs.spawn(&owner.world,struct {transform:app.Scene_Transform,light:app.Scene_Point_Light}{{km.transform()},app.point_light_default()}) }
    _,overflow:=lighting_collect_entities(&owner,ids)
    testing.expect_value(t,overflow,Scene_Error.Invalid_Material)
    prospective,accepted:=lighting_collect_entities(&owner,ids[:MAX_POINT_LIGHTS])
    testing.expect_value(t,accepted,Scene_Error.None)
    testing.expect_value(t,prospective.count,u32(MAX_POINT_LIGHTS))
}

@(test)
test_shadow_packet_freezes_distinct_cascade_phase_constants :: proc(t:^testing.T) {
    owner:int
    scene:Scene_Graph
    testing.expect_value(t,scene_graph_init(&scene,{&owner,0,0},{&owner,1,0},3,1,32,24),Scene_Error.None)
    defer scene_graph_destroy(&scene)
    scene.features.pipelines.geometry[0][0]={&owner,2,0}
    error:=feature_shadow_packet(&scene,0,scene.objects,scene.geometry,scene.object_desc,scene.geometry_desc,{gfx.Draw{3,1,0,0}},-1,true)
    testing.expect_value(t,error,Native_Error{})
    packet:=scene.graph.passes[scene.features.shadows[0].index].packet.(gfx.Render)
    testing.expect_value(t,len(packet.phases),4)
    for phase,i in packet.phases {
        words:=mem.slice_data_cast([]u32,phase.constants[0].bytes)
        testing.expect_value(t,words[0],u32(i))
        testing.expect_value(t,transmute(f32)words[1],f32(-1))
        testing.expect_value(t,phase.viewport.x,f64(i%2)*1024)
        testing.expect_value(t,phase.viewport.y,f64(i/2)*1024)
    }
}
