#+test
package render

import app ".."
import ecs "../../ecs"
import gfx "../../gfx"
import km "../../math"
import "core:testing"
import "core:mem"

@(test)
test_model_shadow_phases_preserve_images_mirror_culling_and_blend_exclusion :: proc(t:^testing.T) {
    marker:int;scene:Scene_Graph
    testing.expect_value(t,scene_graph_init(&scene,{&marker,0,0},{&marker,1,0},3,1,32,24),Scene_Error.None);defer scene_graph_destroy(&scene)
    cache:=Native_Model(u8){graph=&scene,allocator=context.allocator,frame_desc={size=128,usage={.Uniform}},object_desc={size=3*208,usage={.Storage}},geometry_desc={size=9*144,usage={.Storage}}}
    cache.frame,_=gfx.graph_buffer(&scene.graph,cache.frame_desc,true,false);cache.objects,_=gfx.graph_buffer(&scene.graph,cache.object_desc,true,false);cache.geometry,_=gfx.graph_buffer(&scene.graph,cache.geometry_desc,true,false)
    cache.batch.allocator=context.allocator
    cache.batch.entries=make([]Model_Entry,3);cache.batch.objects=make([]Model_Object,3);defer model_batch_destroy(&cache.batch)
    for &entry,i in cache.batch.entries { entry={entity=ecs.Entity_Id(i+1),object_index=u32(i),vertex_count=3,first_vertex=u32(i)*3};cache.batch.objects[i].model=km.identity(km.Mat4) }
    cache.batch.entries[0].material.alpha_mode=.Mask;cache.batch.entries[0].material.unlit=true
    cache.batch.entries[1].material.double_sided=true;cache.batch.objects[1].model[0][0]= -1
    cache.batch.entries[2].material.alpha_mode=.Blend
    cache.textures=make([dynamic]Model_Texture);defer delete(cache.textures)
    cache.samplers=make([dynamic]Model_Sampler);defer delete(cache.samplers)
    cache.receipts=make([]Model_Receipt,3);defer delete(cache.receipts)
    cache.image_ids=make([]gfx.Image_Id,2);defer delete(cache.image_ids)
    desc:=gfx.Texture_Desc{width=4,height=4,depth=1,layers=1,mip_levels=3,format=.RGBA8_Srgb,usage={.Sampled}}
    for i in 0..<2 {
        append(&cache.textures,Model_Texture{native={desc=desc}});append(&cache.samplers,Model_Sampler{handle={&marker,u32(i),1}})
        cache.image_ids[i],_=gfx.graph_image(&scene.graph,desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
        cache.receipts[i].textures[0]=i;cache.receipts[i].samplers[0]=i
    }
    scene.features.pipelines.geometry[1][0]={&marker,2,1};scene.features.pipelines.model_variants[0][2]={&marker,3,1}
    testing.expect_value(t,feature_model_coverage_packet(&cache,&scene,0,true,-1),Native_Error{})
    frozen:=scene.graph.passes[scene.features.shadows[1].index].packet.(gfx.Render)
    testing.expect_value(t,len(frozen.phases),8)
    testing.expect_value(t,frozen.buffers[0].stages,gfx.Shader_Stages{.Vertex,.Fragment})
    for phase,i in frozen.phases {
        entry:=i/4;testing.expect_value(t,phase.images[0].accesses[0].resource,cache.image_ids[entry])
        testing.expect_value(t,phase.samplers[0].handle,cache.samplers[entry].handle)
        testing.expect_value(t,phase.pipeline,scene.features.pipelines.geometry[1][0] if entry==0 else scene.features.pipelines.model_variants[0][2])
        testing.expect_value(t,phase.draws[0].(gfx.Draw).first_instance,u32(entry))
    }
    cache.receipts[0].textures[0]=9
    testing.expect_value(t,feature_model_coverage_packet(&cache,&scene,0,true,-1).gpu,gfx.Gpu_Error.Invalid_Resource)
    current:=scene.graph.passes[scene.features.shadows[1].index].packet.(gfx.Render)
    testing.expect_value(t,len(current.phases),8);testing.expect_value(t,current.phases[0].images[0].accesses[0].resource,cache.image_ids[0])
    cache.receipts[0].textures[0]=0
    names:=[4]string{"Model stencil","Model occlusion","Model outline","Model mask"}
    scene.features.late_bound=true
    for effect in 1..<5 {
        scene.features.selection[1][effect-1],_=gfx.graph_pass(&scene.graph,names[effect-1],.Graphics,nil)
        scene.features.pipelines.geometry[1][effect]={&marker,u32(effect+5),1}
        scene.features.pipelines.model_variants[effect][2]={&marker,u32(effect+10),1}
        testing.expect_value(t,feature_model_coverage_packet(&cache,&scene,effect,true,1,{cache.batch.entries[1].entity}),Native_Error{})
    }
    passes:=[5]gfx.Pass_Id{scene.features.shadows[1],scene.features.selection[1][0],scene.features.selection[1][1],scene.features.selection[1][2],scene.features.selection[1][3]}
    for mip_levels in ([2]u32{1,3}) {
        for &texture,i in cache.textures {
            texture.native.desc.mip_levels=mip_levels
            testing.expect_value(t,gfx.graph_replace_image(&scene.graph,cache.image_ids[i],texture.native.desc),gfx.Graph_Error.None)
            cache.samplers[i].handle={&marker,u32(i)+20*mip_levels,1}
        }
        testing.expect_value(t,model_coverage_rebind(&cache),Native_Error{})
        for pass,effect in passes {
            current_pass:=scene.graph.passes[pass.index]; rebound:=current_pass.packet.(gfx.Render)
            testing.expect_value(t,len(rebound.phases),8 if effect==0 else 1)
            for access in current_pass.images { if access.usage==.Sampled { testing.expect_value(t,access.range.mip_count,mip_levels) } }
            for phase,i in rebound.phases {
                entry:=i/4 if effect==0 else 1
                testing.expect_value(t,phase.images[0].accesses[0].range.mip_count,mip_levels)
                testing.expect_value(t,phase.samplers[0].handle,cache.samplers[entry].handle)
                testing.expect_value(t,phase.draws[0].(gfx.Draw).first_instance,u32(entry))
                if effect==0 {
                    constants:=[1]Model_Cascade_Uniform{};copy(mem.slice_to_bytes(constants[:]),phase.constants[0].bytes)
                    testing.expect_value(t,constants[0].sign,f32(-1))
                    testing.expect_value(t,constants[0].index,u32(i%4))
                }
            }
        }
    }
    masked,cutoff:=picking_material_cutoff(app.Gltf_Material{alpha_mode=.Blend});testing.expect(t,masked && cutoff==0)
}

@(test)
test_blended_model_order_is_far_to_near_in_both_orthographic_depth_senses :: proc(t:^testing.T) {
    entries:=[3]Model_Entry{{world_center={0,0,.25},material={alpha_mode=.Blend}},{world_center={0,0,.75},material={alpha_mode=.Blend}},{world_center={0,0,.5},material={alpha_mode=.Opaque}}}
    cache:=Native_Model(u8){batch={entries=entries[:]}}
    forward:=model_graph_order(&cache,km.identity(km.Mat4));defer delete(forward)
    testing.expect_value(t,forward[0],2);testing.expect_value(t,forward[1],1);testing.expect_value(t,forward[2],0)
    reverse:=km.identity(km.Mat4);reverse[2][2]= -1;reverse[3][2]=1
    reversed:=model_graph_order(&cache,reverse,depth_sense=.Reverse);defer delete(reversed)
    testing.expect_value(t,reversed[0],2);testing.expect_value(t,reversed[1],1);testing.expect_value(t,reversed[2],0)
}
