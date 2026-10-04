//! Scene picking derives visibility policy from the exact prepared mesh/model owners.
package render

import gfx "../../gfx"
import ecs "../../ecs"
import app ".."
import shader "../../gfx/shader"
import km "../../math"

/// Four culling variants share the scene's double-sided and mirrored-model rules.
Picking_Pipelines :: struct { opaque,masked:[4]gfx.Graphics_Pipeline_Handle }
Picking_Native :: struct($R:typeid) { renderer:^R,create:proc(^R,gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error),destroy:proc(^R,gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error,pipelines,reverse_pipelines:Picking_Pipelines }
/// Creates integer picking variants once with the same front-face policy as visible materials.
picking_native_init :: proc(owner:^Picking_Native($R),renderer:^R,operations:GPU_Ops(R),compiler:^shader.Compiler)->Native_Error {
    if owner.renderer!=nil || renderer==nil || operations.create_pipeline==nil || operations.destroy_pipeline==nil { return {gpu=.Invalid_Resource} }
    owner.renderer=renderer; owner.create=operations.create_pipeline; owner.destroy=operations.destroy_pipeline
    success:=false; defer { if !success { picking_native_destroy(owner) } }
    for masked in 0..<2 { for variant in 0..<4 {
        compiled,error:=picking_shader_compile(compiler,masked=masked==1,cull=.None if variant%2==1 else .Back,front_counter_clockwise=variant<2)
        if error!=.None { return {gpu=.Invalid_Shader} }
        pipeline,pipeline_error:=owner.create(renderer,compiled.mapped.descriptor)
        if pipeline_error!=.None { picking_shader_destroy(&compiled); return {gpu=pipeline_error} }
        if masked==1 { owner.pipelines.masked[variant]=pipeline } else { owner.pipelines.opaque[variant]=pipeline }
        pipeline,pipeline_error=owner.create(renderer,depth_descriptor(compiled.mapped.descriptor,.Reverse)); picking_shader_destroy(&compiled)
        if pipeline_error!=.None { return {gpu=pipeline_error} }
        if masked==1 { owner.reverse_pipelines.masked[variant]=pipeline } else { owner.reverse_pipelines.opaque[variant]=pipeline }
    } }
    success=true; return {}
}
/// Accepted native recordings retain their selected variants independently of the public owner.
picking_native_destroy :: proc(owner:^Picking_Native($R))->gfx.Gpu_Error {
    for &pipeline in owner.pipelines.opaque { if pipeline.owner!=nil { error:=owner.destroy(owner.renderer,pipeline); if error!=.None { return error }; pipeline={} } }
    for &pipeline in owner.pipelines.masked { if pipeline.owner!=nil { error:=owner.destroy(owner.renderer,pipeline); if error!=.None { return error }; pipeline={} } }
    for &pipeline in owner.reverse_pipelines.opaque { if pipeline.owner!=nil { error:=owner.destroy(owner.renderer,pipeline); if error!=.None { return error }; pipeline={} } }
    for &pipeline in owner.reverse_pipelines.masked { if pipeline.owner!=nil { error:=owner.destroy(owner.renderer,pipeline); if error!=.None { return error }; pipeline={} } }
    owner^={}; return .None
}
@(private="package")
picking_mirrored :: proc(model:km.Mat4)->bool { return km.dot(km.cross(km.xyz(model[0]),km.xyz(model[1])),km.xyz(model[2]))<0 }
@(private="package")
picking_material_cutoff :: proc(material:app.Gltf_Material)->(bool,f32) {
    if material.alpha_mode==.Mask { return true,material.alpha_cutoff }
    if material.alpha_mode==.Blend { return true,0 }
    return false,0
}
@(private="package")
picking_entity_code :: proc(entities:^[dynamic]ecs.Entity_Id,entity:ecs.Entity_Id)->u32 {
    for id,index in entities { if id==entity { return u32(index)+1 } }
    append(entities,entity); return u32(len(entities))
}
/// Reads only prepared packet ranges and acquired-slot handles; it never reshapes or updates scene data.
/// The returned draw array is caller-owned; its referenced scene resources remain host-owned.
picking_scene_draws :: proc(scene:^Native_Scene($R),token:gfx.Frame_Token,pipelines:Picking_Pipelines,allocator:=context.allocator)->([]Picking_Draw,Native_Error) {
    if scene==nil || scene.renderer==nil || token.slot<0 || token.slot>=len(scene.slots) { return nil,{gpu=.Invalid_Resource} }
    graph:=scene_graph_target(&scene.graph)
    if scene.graph.pass.owner!=graph || scene.graph.pass.index<0 || scene.graph.pass.index>=len(graph.passes) { return nil,{gpu=.Invalid_Graph} }
    surface,rendered:=graph.passes[scene.graph.pass.index].packet.(gfx.Render); if !rendered { return nil,{gpu=.Invalid_Graph} }
    entities:=make([dynamic]ecs.Entity_Id,allocator); draws:=make([dynamic]Picking_Draw,allocator); defer delete(entities); defer delete(draws)
    slot:=scene.slots[token.slot]
    frame:=Picking_Buffer{scene.graph.frame,slot.frame,scene.graph.frame_desc}
    for phase in surface.phases {
        if phase.pipeline!=scene.graph.pipeline { continue }
        for operation in phase.draws {
            draw,generated:=operation.(gfx.Draw)
            if !generated || scene.batch==nil || u64(draw.first_instance)+u64(draw.instance_count)>u64(len(scene.batch.entries)) { return nil,{scene=.Invalid_Geometry} }
            for instance in 0..<draw.instance_count {
                object:=draw.first_instance+instance; entity:=scene.batch.entries[object].entity
                if pipelines.opaque[0].owner==nil { return nil,{gpu=.Invalid_Resource} }
                append(&draws,Picking_Draw{geometry={scene.graph.geometry,scene.geometry,scene.graph.geometry_desc},objects={scene.graph.objects,slot.objects,scene.graph.object_desc},frame=frame,vertex_stride=u32(size_of(Vertex)),object_stride=u32(size_of(Object_Data)),first_vertex=draw.first_vertex,vertex_count=draw.vertex_count,object_index=object,encoded=picking_entity_code(&entities,entity),entity=entity,pipeline=pipelines.opaque[0]})
            }
        }
    }
    models:=scene.models
    if models!=nil && len(models.batch.entries)>0 {
        if models.graph!=&scene.graph || token.slot>=len(models.slots) || len(models.order)!=len(models.batch.entries) { return nil,{gpu=.Invalid_Graph} }
        model_slot:=models.slots[token.slot]
        model_frame:=Picking_Buffer{models.frame,model_slot.frame,models.frame_desc}
        for index in models.order {
            if index<0 || index>=len(models.batch.entries) { return nil,{gpu=.Invalid_Resource} }
            entry:=models.batch.entries[index]
            if int(entry.object_index)>=len(models.batch.objects) { return nil,{scene=.Invalid_Geometry} }
            mirrored:=picking_mirrored(models.batch.objects[entry.object_index].model)
            variant:=int(entry.material.double_sided)+2*int(mirrored)
            masked,cutoff:=picking_material_cutoff(entry.material)
            pipeline:=pipelines.masked[variant] if masked else pipelines.opaque[variant]
            if pipeline.owner==nil { return nil,{gpu=.Invalid_Resource} }
            draw:=Picking_Draw{geometry={models.geometry,model_slot.geometry,models.geometry_desc},objects={models.objects,model_slot.objects,models.object_desc},frame=model_frame,vertex_stride=u32(size_of(Model_Vertex)),object_stride=u32(size_of(Model_Object)),first_vertex=entry.first_vertex,vertex_count=entry.vertex_count,object_index=entry.object_index,encoded=picking_entity_code(&entities,entry.entity),entity=entry.entity,pipeline=pipeline}
            if masked {
                receipt:=models.receipts[index]; texture_index,sampler_index:=receipt.textures[0],receipt.samplers[0]
                if texture_index<0 || texture_index>=len(models.textures) || sampler_index<0 || sampler_index>=len(models.samplers) || texture_index>=len(models.image_ids) { return nil,{gpu=.Invalid_Resource} }
                texture:=models.textures[texture_index].native
                draw.mask={enabled=true,texture={handle=texture.texture,desc=texture.desc,resource=models.image_ids[texture_index]},sampler=models.samplers[sampler_index].handle,uv_offset=u32(offset_of(Model_Vertex,uvs)),vertex_alpha_offset=u32(offset_of(Model_Vertex,color))+12,object_alpha_offset=u32(offset_of(Model_Object,base_color))+12,vertex_alpha=true,cutoff=cutoff,mode=.Blend if entry.material.alpha_mode==.Blend else .Mask}
            }
            append(&draws,draw)
        }
    }
    result:=make([]Picking_Draw,len(draws),allocator); copy(result,draws[:]); return result,{}
}

/// Selects the same explicit camera-depth variants as the prepared visible scene.
picking_native_pipelines :: proc(owner:^Picking_Native($R),sense:Depth_Sense)->Picking_Pipelines { return owner.reverse_pipelines if sense==.Reverse else owner.pipelines }
