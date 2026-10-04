#+test
package render

import app ".."
import ecs "../../ecs"
import km "../../math"
import resources "../../resources"
import "core:testing"
import "core:strings"

@(test)
test_model_batch_actual_textures_factors_uvs_and_atomic_revisions :: proc(t:^testing.T) {
    testing.expect_value(t,size_of(Model_Vertex),144); testing.expect_value(t,size_of(Model_Object),208)
    root,root_error:=resources.root_open(#config(GLTF_RESOURCE_ROOT,"resources")); testing.expect_value(t,root_error,resources.Error.None); if root_error!=.None { return }; defer resources.root_destroy(&root)
    model,error:=app.gltf_load(&root,"models/DamagedHelmet.glb"); testing.expect_value(t,error,app.Gltf_Error.None); if error!=.None { return }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.scene_model_register(&owner)
    entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform,surface:app.Surface_Material}{{model=model},{km.TRANSFORM_IDENTITY},{metallic=1,roughness=1,ao=1}})
    batch,batch_error:=model_batch_prepare(&owner); defer model_batch_destroy(&batch)
    testing.expect_value(t,batch_error,Model_Batch_Error{}); if batch_error.kind!=.None { return }
    testing.expect(t,len(batch.entries)>0 && len(batch.vertices)>0 && !batch.streaming)
    entry:=batch.entries[0]; testing.expect_value(t,entry.entity,entity)
    testing.expect(t,entry.views[0].texture>=0 && entry.views[1].texture>=0 && entry.views[2].texture>=0)
    testing.expect_value(t,batch.objects[0].base_color,entry.material.base_color)
    testing.expect_value(t,batch.objects[0].flags[3],u32(1))
    primitive:=model.primitives[entry.primitive]; vertex_index:=primitive.geometry.indices[0]
    uv,uv_error:=model_vertex_uv(primitive,vertex_index,entry.views[0]); testing.expect_value(t,uv_error,Model_Batch_Error{})
    testing.expect_value(t,km.Vec2{batch.vertices[entry.first_vertex].uvs[0][0],batch.vertices[entry.first_vertex].uvs[0][1]},uv)
    previous_revision:=batch.geometry_revision
    surface:=ecs.get_component_mut(&owner.world,entity,app.Surface_Material); surface.roughness=0.25
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    testing.expect_value(t,batch.objects[0].factors[1],entry.material.roughness*0.25)
    testing.expect(t,!batch.geometry_changed && batch.geometry_revision==previous_revision)
    transform:=ecs.get_component_mut(&owner.world,entity,app.Scene_Transform); transform.local.position={2,3,4}
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    testing.expect(t,!batch.geometry_changed)
    testing.expect_value(t,batch.objects[0].model[3],km.matrix_mul(km.transform_to_mat4(transform.local),model.nodes[entry.node].world_matrix)[3])
    preserved:=batch.objects[0]; transform.local.scale={0,1,1}
    testing.expect_value(t,model_batch_refresh(&batch,&owner).kind,Model_Batch_Error_Kind.Invalid_Scene)
    testing.expect_value(t,batch.objects[0],preserved)
    transform.local.scale={1,1,1}
    current:=ecs.get_component_mut(&owner.world,entity,app.Scene_Model); current.model.materials[primitive.material].base_color_texture.texture= -1
    testing.expect_value(t,model_batch_refresh(&batch,&owner).kind,Model_Batch_Error_Kind.Rebuild_Required)
    testing.expect_value(t,batch.objects[0],preserved)
    ecs.destroy_entity(&owner.world,entity)
    testing.expect_value(t,model_batch_refresh(&batch,&owner).kind,Model_Batch_Error_Kind.Rebuild_Required)
}

@(test)
test_model_batch_actual_fox_animation_streams_skin_geometry :: proc(t:^testing.T) {
    root,root_error:=resources.root_open(#config(GLTF_RESOURCE_ROOT,"resources")); testing.expect_value(t,root_error,resources.Error.None); if root_error!=.None { return }; defer resources.root_destroy(&root)
    model,error:=app.gltf_load(&root,"models/Fox.glb"); testing.expect_value(t,error,app.Gltf_Error.None); if error!=.None { return }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.scene_model_register(&owner); app.animation_register(&owner.world,&owner.registry)
    entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform}{{model=model},{km.TRANSFORM_IDENTITY}})
    // Player clip ownership follows the canonical component's deep value operations.
    player:=app.animation_player_stopped(); ecs.add_component(&owner.world,entity,player)
    current:=ecs.get_component_mut(&owner.world,entity,app.Animation_Player)
    current.clip=strings.clone(model.animation.clips[0].name)
    batch,batch_error:=model_batch_prepare(&owner); defer model_batch_destroy(&batch)
    testing.expect_value(t,batch_error,Model_Batch_Error{}); if batch_error.kind!=.None { return }
    testing.expect(t,batch.streaming && len(batch.vertices)>0)
    first:=batch.vertices[0]; current.time=model.animation.clips[0].duration*0.5
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    testing.expect(t,batch.geometry_changed && batch.geometry_revision==2)
    differs:=false; for vertex in batch.vertices { if vertex.position!=first.position { differs=true; break } }; testing.expect(t,differs)
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    testing.expect(t,!batch.geometry_changed && batch.geometry_revision==2)
    explicit,explicit_error:=model_batch_prepare_entities(&owner,{entity}); defer model_batch_destroy(&explicit)
    testing.expect_value(t,explicit_error,Model_Batch_Error{})
    ecs.destroy_entity(&owner.world,entity)
    testing.expect_value(t,model_batch_refresh(&explicit,&owner).kind,Model_Batch_Error_Kind.Rebuild_Required)
}


@(test)
test_model_batch_active_scene_uv_roles_and_multi_component_morphs :: proc(t:^testing.T) {
    geometry,geometry_error:=app.mesh_triangles({{0,0,0},{1,0,0},{0,1,0}},{0,1,2})
    testing.expect_value(t,geometry_error,app.Mesh_Error.None); if geometry_error!=.None { return }
    model:=app.Gltf_Model{allocator=context.allocator,default_scene=0}
    model.primitives=make([]app.Gltf_Primitive,1); primitive:=&model.primitives[0]; primitive.geometry=geometry; primitive.material=0
    primitive.uv_sets=make([][]km.Vec2,2); primitive.uv_sets[0]=make([]km.Vec2,3); primitive.uv_sets[1]=make([]km.Vec2,3)
    for &uv in primitive.uv_sets[1] { uv={0.25,0.5} }
    primitive.morphs=make([]app.Gltf_Morph,2)
    for &morph,i in primitive.morphs { morph.position=make([]km.Vec3,3); for &delta in morph.position { delta={1,0,0} if i==0 else km.Vec3{0,1,0} } }
    primitive.colors=make([]km.Vec4,3); for &color in primitive.colors { color={0.5,0.25,0.75,1} }
    model.nodes=make([]app.Gltf_Node,3); model.animation.bind_pose=make([]km.Transform,3); model.animation.parents=make([]i32,3)
    for &node,i in model.nodes { node.parent= -1; node.mesh=0; node.skin= -1; node.local=km.TRANSFORM_IDENTITY; node.local_matrix=km.identity(km.Mat4); node.world_matrix=node.local_matrix; node.weights=make([]f32,2); model.animation.bind_pose[i]=node.local; model.animation.parents[i]= -1 }
    model.scenes=make([]app.Gltf_Scene,2); model.scenes[0].roots=make([]u32,1); model.scenes[0].roots[0]=1; model.scenes[1].roots=make([]u32,1); model.scenes[1].roots[0]=2
    material,_:=model_material(&model,-1); material.workflow=.Specular_Glossiness; material.specular={0.2,0.3,0.4}; material.glossiness=0.7
    material.diffuse_texture={texture=0,texcoord=1,uv_scale={2,3},offset={0.1,0.2},rotation=0,scale=1}
    model.materials=make([]app.Gltf_Material,1); model.materials[0]=material
    model.animation.clips=make([]app.Animation_Clip,1); clip:=&model.animation.clips[0]; clip.name=strings.clone("morph"); clip.duration=1; clip.channels=make([]app.Animation_Channel,1)
    channel:=&clip.channels[0]; channel.node=1; channel.path=.Weights; channel.interpolation=.Linear; channel.weight_count=2; channel.times=make([]f32,2); channel.times[1]=1; channel.weight_values=make([]f32,4); channel.weight_values[2]=1; channel.weight_values[3]=2
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.scene_model_register(&owner); app.animation_register(&owner.world,&owner.registry)
    player:=app.animation_player_stopped(); player.clip=strings.clone("morph")
    entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform,player:app.Animation_Player}{{model=model},{km.TRANSFORM_IDENTITY},player})
    batch,batch_error:=model_batch_prepare(&owner); defer model_batch_destroy(&batch)
    testing.expect_value(t,batch_error,Model_Batch_Error{}); if batch_error.kind!=.None { return }
    testing.expect_value(t,len(batch.entries),1); testing.expect_value(t,batch.entries[0].node,u32(1)); testing.expect_value(t,len(batch.vertices),3)
    testing.expect_value(t,batch.vertices[0].uvs[0],km.Vec4{0.6,1.7,0,0}); testing.expect_value(t,batch.vertices[0].color,km.Vec4{0.5,0.25,0.75,1})
    testing.expect_value(t,batch.objects[0].flags[0],u32(1)); testing.expect_value(t,batch.objects[0].specular_glossiness,km.Vec4{0.2,0.3,0.4,0.7})
    current:=ecs.get_component_mut(&owner.world,entity,app.Animation_Player); current.time=0.5
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    testing.expect_value(t,batch.vertices[0].position,km.Vec4{0.5,1,0,1}); testing.expect(t,batch.geometry_changed)
    source:=ecs.get_component_mut(&owner.world,entity,app.Scene_Model); source.model.default_scene=1
    testing.expect_value(t,model_batch_refresh(&batch,&owner).kind,Model_Batch_Error_Kind.Rebuild_Required)
    testing.expect_value(t,batch.entries[0].node,u32(1))
}

@(test)
test_model_batch_real_primitive_bounds_supply_camera_depth_instead_of_node_origin :: proc(t:^testing.T) {
    root,root_error:=resources.root_open(#config(GLTF_RESOURCE_ROOT,"resources")); testing.expect_value(t,root_error,resources.Error.None); if root_error!=.None { return }; defer resources.root_destroy(&root)
    model,error:=app.gltf_load(&root,"models/TangentUV.gltf"); testing.expect_value(t,error,app.Gltf_Error.None); if error!=.None { return }
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    app.scene_components_register(&owner); app.scene_model_register(&owner)
    entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform}{{model=model},{km.TRANSFORM_IDENTITY}})
    batch,batch_error:=model_batch_prepare(&owner); defer model_batch_destroy(&batch)
    testing.expect_value(t,batch_error,Model_Batch_Error{}); if batch_error.kind!=.None { return }
    testing.expect_value(t,len(batch.entries),2)
    testing.expect_value(t,batch.objects[0].model,batch.objects[1].model)
    testing.expect_value(t,batch.entries[0].world_center,km.Vec3{-0.8,0,0})
    testing.expect_value(t,batch.entries[1].world_center,km.Vec3{0.8,0,-0.5})
    frame,frame_error:=frame_data(camera_default(),256,256,false); testing.expect_value(t,frame_error,Scene_Error.None)
    model_batch_update_depths(&batch,frame.view_projection)
    testing.expect(t,batch.entries[1].camera_depth>batch.entries[0].camera_depth)
    testing.expect_value(t,batch.entries[0].camera_depth,f32(4))
    testing.expect_value(t,batch.entries[1].camera_depth,f32(4.5))
    for &entry in batch.entries { entry.material.alpha_mode=.Blend }
    cache:=Native_Model(u8){batch=batch}
    order:=model_graph_order(&cache,frame.view_projection); defer delete(order)
    testing.expect_value(t,order[0],1); testing.expect_value(t,order[1],0)
    for &entry in batch.entries { entry.material.alpha_mode=.Opaque }
    transform:=ecs.get_component_mut(&owner.world,entity,app.Scene_Transform); transform.local.position={0,0,-2}
    testing.expect_value(t,model_batch_refresh(&batch,&owner),Model_Batch_Error{})
    model_batch_update_depths(&batch,frame.view_projection)
    testing.expect_value(t,batch.entries[0].camera_depth,f32(6))
    testing.expect_value(t,batch.entries[1].camera_depth,f32(6.5))
}
