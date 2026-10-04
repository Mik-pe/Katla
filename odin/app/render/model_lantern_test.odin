#+test
package render

import app ".."
import ecs "../../ecs"
import km "../../math"
import resources "../../resources"
import "core:testing"

@(test)
test_model_lantern_accepted_geometry_refresh_retains_source_revision :: proc(t:^testing.T) {
    root,root_error:=resources.root_open(#config(GLTF_RESOURCE_ROOT,"resources")); testing.expect_value(t,root_error,resources.Error.None); if root_error!=.None { return }; defer resources.root_destroy(&root)
    model,load_error:=app.gltf_load(&root,"models/Lantern.glb"); testing.expect_value(t,load_error,app.Gltf_Error.None); if load_error!=.None { return }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.scene_model_register(&owner)
    transform:=km.TRANSFORM_IDENTITY; transform.position={30,0,0}
    entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform}{{model=model},{transform}})
    batch,batch_error:=model_batch_prepare_entities(&owner,{entity}); defer model_batch_destroy(&batch)
    testing.expect_value(t,batch_error,Model_Batch_Error{}); if batch_error.kind!=.None { return }
    batch.all_entities=true
    testing.expect(t,len(batch.entries)>0 && len(batch.vertices)>0)
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    current:=ecs.get_component_mut(&owner.world,entity,app.Scene_Model)
    retained:=batch.vertices[0]
    normal:=current.model.primitives[0].geometry.vertices[0].normal
    current.model.primitives[0].geometry.vertices[0].normal={}
    testing.expect_value(t,model_batch_refresh(&batch,&owner).kind,Model_Batch_Error_Kind.Invalid_Geometry)
    testing.expect_value(t,batch.vertices[0],retained)
    current.model.primitives[0].geometry.vertices[0].normal=normal
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
}
