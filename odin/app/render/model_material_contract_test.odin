#+test
package render

import app ".."
import km "../../math"
import gfx "../../gfx"
import ecs "../../ecs"
import editor "../../editor"
import "core:strings"
import "core:testing"

@(test)
test_model_material_authored_surface_and_independent_sampling_replace_imported_properties :: proc(t:^testing.T) {
    default_model:app.Gltf_Model
    material,_:=model_material(&default_model,-1)
    material.emissive={0.2,0.3,0.4};material.alpha_mode=.Mask;material.alpha_cutoff=.7
    material.normal_texture={texture=2,texcoord=0,scale=.3,uv_scale={1,1}}
    material.occlusion_texture={texture=3,scale=.6,uv_scale={1,1}}
    surface:=app.Surface_Material{metallic=1,roughness=1,ao=.8}
    testing.expect_value(t,model_material_surface(material,surface),material)
    surface.has_surface=true;surface.surface=app.material_surface_default()
    surface.surface.emissive_factor={2,3,4};surface.surface.normal_scale= -2
    surface.surface.occlusion_strength=.25;surface.surface.alpha_mode=.Blend
    surface.surface.alpha_cutoff=.9;surface.surface.double_sided=true
    resolved:=model_material_surface(material,surface)
    testing.expect_value(t,resolved.emissive,km.Vec3{2,3,4})
    testing.expect_value(t,resolved.normal_texture.scale,f32(-2))
    testing.expect_value(t,resolved.occlusion_texture.scale,f32(.25))
    testing.expect_value(t,resolved.alpha_mode,app.Gltf_Alpha_Mode.Blend)
    testing.expect_value(t,resolved.alpha_cutoff,f32(.9));testing.expect(t,resolved.double_sided)
    object,error:=model_object(km.identity(km.Mat4),surface,resolved)
    testing.expect_value(t,error,Model_Batch_Error{})
    testing.expect_value(t,object.emissive,km.Vec4{2,3,4,.9})
    testing.expect_value(t,object.factors[3],f32(-2));testing.expect_value(t,object.flags[1],u32(2))
    surface.has_sampling=true;surface.sampling=app.material_sampling_default()
    surface.sampling.normal.uv={tex_coord=1,offset={.2,.3},rotation=km.PI/2,scale={-2,3}}
    surface.sampling.normal.sampler.address_u=.Mirror_Repeat
    surface.sampling.albedo.sampler.mag_filter=.Nearest
    views,samplers:=model_material_sampling(resolved,surface)
    testing.expect_value(t,views[1].texture,i32(2));testing.expect_value(t,views[1].texcoord,i32(1))
    testing.expect_value(t,views[1].offset,km.Vec2{.2,.3})
    testing.expect_value(t,views[1].uv_scale,km.Vec2{-2,3})
    testing.expect_value(t,samplers[1].address_u,gfx.Address_Mode.Mirror_Repeat)
    testing.expect_value(t,samplers[0].mag_filter,gfx.Filter.Nearest)
    testing.expect_value(t,samplers[4],app.texture_sampling_default().sampler)
}

@(test)
test_authored_primitive_uses_model_material_stream_and_actual_image_snapshot_revision :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner)
    testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None)
    geometry,error:=app.mesh_triangles({{0,0,0},{1,0,0},{0,1,0}},{0,1,2},uvs={{0,0},{1,0},{0,1}})
    testing.expect_value(t,error,app.Mesh_Error.None);if error!=.None { return }
    surface:=app.Surface_Material{metallic=.2,roughness=.3,ao=.4,has_surface=true,surface=app.material_surface_default(),has_sampling=true,sampling=app.material_sampling_default()}
    surface.surface.emissive_factor={2,3,4};surface.surface.normal_scale= -1;surface.surface.occlusion_strength=.25;surface.surface.alpha_mode=.Mask;surface.surface.alpha_cutoff=.6
    surface.sampling.albedo.uv={scale={2,3},offset={.2,.3}};surface.sampling.normal.uv={scale={-1,1}}
    entity:=ecs.spawn(&owner.world,struct {mesh:app.Scene_Mesh,transform:app.Scene_Transform,surface:app.Surface_Material}{{source={kind=.Geometry},geometry=geometry},{km.TRANSFORM_IDENTITY},surface})
    images:app.Material_Images
    images.roles[1].source.kind=.Neutral
    images.roles[3].source.kind=.File;images.roles[3].source.path=strings.clone("textures/ao.png")
    images.roles[3].image={width=1,height=1,pixels=make([]byte,4),allocator=context.allocator};images.roles[3].digest[0]=42
    ecs.add_component(&owner.world,entity,images)
    batch,batch_error:=model_batch_prepare(&owner);defer model_batch_destroy(&batch)
    testing.expect_value(t,batch_error,Model_Batch_Error{});if batch_error.kind!=.None { return }
    testing.expect_value(t,len(batch.entries),1);testing.expect(t,batch.entries[0].primitive_mesh)
    testing.expect_value(t,batch.vertices[1].uvs[0],km.Vec4{2.2,.3,0,0})
    testing.expect_value(t,batch.vertices[0].uvs[3][2],f32(.25))
    testing.expect_value(t,batch.objects[0].flags[3],u32(3))
    testing.expect_value(t,batch.objects[0].emissive,km.Vec4{2,3,4,.6})
    prior:=batch.objects[0]
    mutable:=ecs.get_component_mut(&owner.world,entity,app.Material_Images);mutable.roles[3].digest[0]=43
    testing.expect_value(t,model_batch_refresh(&batch,&owner).kind,Model_Batch_Error_Kind.Rebuild_Required)
    testing.expect_value(t,batch.objects[0],prior)
}

@(test)
test_authored_imported_factors_replace_zero_metallic_instead_of_multiplying_it :: proc(t:^testing.T) {
    source:app.Gltf_Model;material,_:=model_material(&source,-1)
    material.base_color={.2,.3,.4,.5};material.metallic=0;material.roughness=.25
    surface:=app.Surface_Material{linear_color={.7,.8,.9,.6},has_tint=true,has_factors=true,metallic=.8,roughness=.7,ao=1}
    packed,error:=model_object(km.identity(km.Mat4),surface,material)
    testing.expect_value(t,error,Model_Batch_Error{})
    testing.expect_value(t,packed.base_color,km.Vec4{.7,.8,.9,.6})
    testing.expect_value(t,packed.factors[0],f32(.8));testing.expect_value(t,packed.factors[1],f32(.7))
    surface.has_factors=false
    inherited,inherit_error:=model_object(km.identity(km.Mat4),surface,material)
    testing.expect_value(t,inherit_error,Model_Batch_Error{})
    testing.expect_value(t,inherited.base_color,material.base_color*km.Vec4{.7,.8,.9,.6})
    testing.expect_value(t,inherited.factors[0],f32(0));testing.expect_value(t,inherited.factors[1],f32(.25*.7))
}

@(test)
test_neutral_model_images_do_not_require_unused_texture_coordinate_streams :: proc(t:^testing.T) {
    owner:app.Authoring;app.authoring_init(&owner);defer app.authoring_destroy(&owner)
    testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None)
    geometry,error:=app.mesh_triangles({{0,0,0},{1,0,0},{0,1,0}},{0,1,2})
    testing.expect_value(t,error,app.Mesh_Error.None);if error!=.None { return }
    model:=app.Gltf_Model{allocator=context.allocator,default_scene= -1}
    model.primitives=make([]app.Gltf_Primitive,1)
    model.primitives[0]={geometry=geometry,material= -1}
    model.nodes=make([]app.Gltf_Node,1)
    model.nodes[0]={parent= -1,mesh=0,skin= -1,local=km.TRANSFORM_IDENTITY,local_matrix=km.identity(km.Mat4),world_matrix=km.identity(km.Mat4)}
    model.animation.bind_pose=make([]km.Transform,1);model.animation.bind_pose[0]=km.TRANSFORM_IDENTITY
    model.animation.parents=make([]i32,1);model.animation.parents[0]= -1
    surface:=app.Surface_Material{metallic=1,roughness=1,ao=1,has_sampling=true,sampling=app.material_sampling_default()}
    surface.sampling.normal.uv.tex_coord=1
    entity:=ecs.spawn(&owner.world,struct {model:app.Scene_Model,transform:app.Scene_Transform,surface:app.Surface_Material}{{model=model},{km.TRANSFORM_IDENTITY},surface})
    images:app.Material_Images;for &role in images.roles { role.source.kind=.Neutral }
    ecs.add_component(&owner.world,entity,images)
    batch,batch_error:=model_batch_prepare(&owner);defer model_batch_destroy(&batch)
    testing.expect_value(t,batch_error,Model_Batch_Error{});if batch_error.kind!=.None { return }
    testing.expect_value(t,len(batch.vertices),3)
    for vertex in batch.vertices { testing.expect_value(t,vertex.uvs[1],km.Vec4{}) }
    prior:=batch.objects[0]
    mutable:=ecs.get_component_mut(&owner.world,entity,app.Material_Images)
    mutable.roles[1].source.kind=.File
    testing.expect_value(t,model_batch_refresh(&batch,&owner).kind,Model_Batch_Error_Kind.Invalid_Material)
    testing.expect_value(t,batch.objects[0],prior)
}
