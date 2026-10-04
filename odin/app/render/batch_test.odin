#+test
package render

import app ".."
import ecs "../../ecs"
import km "../../math"
import "core:testing"

@(test)
test_world_mesh_batch_preserves_material_edits_and_detects_geometry_replacement :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.scene_mesh_register(&owner)
    mesh,error:=app.scene_mesh_prepare(&owner,{kind=.Geometry,geometry=transmute([]byte)string(`{"kind":"cube","size":[1,1,1]}`)})
    testing.expect_value(t,error,app.Mesh_Error.None)
    entity:=ecs.spawn(&owner.world,struct { mesh:app.Scene_Mesh, transform:app.Scene_Transform, surface:app.Surface_Material }{mesh,{km.TRANSFORM_IDENTITY},{linear_color=km.color_to_linear({0.7,0.2,0.1,1}),has_tint=true,metallic=0,roughness=0.5,ao=1}})
    empty:=ecs.spawn(&owner.world,struct { mesh:app.Scene_Mesh }{{source={kind=.Empty},geometry={allocator=context.allocator}}})
    batch,batch_error:=scene_batch_prepare(&owner); defer scene_batch_destroy(&batch)
    testing.expect_value(t,batch_error,Batch_Error{})
    testing.expect_value(t,len(batch.entries),1)
    testing.expect_value(t,batch.entries[0].entity,entity)
    testing.expect_value(t,len(batch.geometry.vertices),36)
    surface:=ecs.get_component_mut(&owner.world,entity,app.Surface_Material); surface.roughness=0.9
    testing.expect_value(t,scene_batch_refresh(&batch,&owner),Batch_Error{})
    testing.expect_value(t,batch.objects[0].factors[1],f32(0.9))
    current:=batch.objects[0]
    ecs.destroy_entity(&owner.world,entity)
    testing.expect_value(t,scene_batch_refresh(&batch,&owner).kind,Batch_Error_Kind.Rebuild_Required)
    testing.expect_value(t,batch.objects[0],current)
    ecs.destroy_entity(&owner.world,empty)
    cleared,clear_error:=scene_batch_prepare(&owner); defer scene_batch_destroy(&cleared)
    testing.expect_value(t,clear_error,Batch_Error{})
    testing.expect_value(t,len(cleared.draws),0)
    testing.expect_value(t,len(cleared.geometry.vertices),0)
}

@(test)
test_world_batch_rejects_stale_duplicate_and_unrendered_model_candidates :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    id:=ecs.spawn(&owner.world,struct { value:i32 }{})
    candidate,error:=scene_batch_prepare_entities(&owner,{id,id}); defer scene_batch_destroy(&candidate)
    testing.expect_value(t,error.kind,Batch_Error_Kind.Invalid_Scene)
    ecs.destroy_entity(&owner.world,id)
    candidate,error=scene_batch_prepare_entities(&owner,{id})
    testing.expect_value(t,error.kind,Batch_Error_Kind.Invalid_Scene)
    model:=ecs.spawn(&owner.world,struct { model:app.Scene_Model }{})
    candidate,error=scene_batch_prepare_entities(&owner,{model})
    testing.expect_value(t,error.kind,Batch_Error_Kind.Unsupported_Model)
}
