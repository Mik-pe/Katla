//! Source-model uploads retain exact texture, sampler and acquired-slot ownership.
package render

import app ".."
import ecs "../../ecs"
import gfx "../../gfx"
import "core:mem"

Model_GPU_Ops :: struct($R:typeid) {
    gpu:GPU_Ops(R),
    create_sampler:proc(^R,gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error),
    destroy_sampler:proc(^R,gfx.Sampler_Handle)->gfx.Gpu_Error,
}
Model_Config :: struct($R:typeid) { shader:^Model_Shader, operations:Model_GPU_Ops(R) }
@(private="package")
Model_Slot :: struct { frame,objects,geometry:gfx.Buffer_Handle }
@(private="package")
Model_Texture :: struct { entity:ecs.Entity_Id,image:i32,srgb:bool,native:Native_Texture, encoded:[]byte,digest:[32]byte }
@(private="package")
Model_Sampler :: struct { desc:gfx.Sampler_Desc,handle:gfx.Sampler_Handle }
@(private="package")
Model_Receipt :: struct { textures,samplers:[5]int }
/// A candidate owns decoded uploads and mutable geometry for every native slot before publication.
Native_Model :: struct($R:typeid) {
    renderer:^R,operations:Model_GPU_Ops(R),batch:Model_Batch,
    pipelines,reverse_pipelines:[8]gfx.Graphics_Pipeline_Handle,slots:[]Model_Slot,
    textures:[dynamic]Model_Texture,samplers:[dynamic]Model_Sampler,receipts:[]Model_Receipt,
    frame_desc,object_desc,geometry_desc:gfx.Buffer_Desc,
    graph:^Scene_Graph,frame,objects,geometry:gfx.Resource_Id,
    image_ids:[]gfx.Image_Id,passes:[]gfx.Pass_Id,order:[]int,
    inputs:[3]gfx.Buffer_Input,texture_inputs:[]gfx.Texture_Input,
    allocator:mem.Allocator,
}
@(private="package")
model_sampler_prepare :: proc(cache:^Native_Model($R),owner:^app.Authoring,entry:Model_Entry,role,texture:int)->(int,Native_Error) {
    source:=app.Gltf_Sampler{}
    view:=entry.views[role]
    if view.texture>=0 {
        model:=ecs.get_component_mut(&owner.world,entry.entity,app.Scene_Model); if model==nil { return 0,{scene=.Invalid_Geometry} }
        index:=model.model.textures[view.texture].sampler
        if index>=0 { if int(index)>=len(model.model.samplers) { return 0,{scene=.Invalid_Material} }; source=model.model.samplers[index] }
    }
    desc:=model_sampler_desc(source,cache.textures[texture].native.desc.mip_levels)
    if entry.has_sampling { desc=entry.samplers[role];desc.max_lod=min(desc.max_lod,f32(cache.textures[texture].native.desc.mip_levels-1)) }
    for sampler,i in cache.samplers { if sampler.desc==desc { return i,{} } }
    handle,error:=cache.operations.create_sampler(cache.renderer,desc); if error!=.None { return 0,{gpu=error} }
    index:=len(cache.samplers); append(&cache.samplers,Model_Sampler{desc,handle}); return index,{}
}
/// Prepares every active primitive and sampled image without mutating authored components.
model_native_init :: proc(cache:^Native_Model($R),owner:^app.Authoring,ids:[]ecs.Entity_Id,renderer:^R,config:Model_Config(R),slots:int,allocator:mem.Allocator=context.allocator)->Native_Error {
    if config.shader==nil || config.operations.create_sampler==nil || config.operations.destroy_sampler==nil || renderer==nil || slots<1 { return {gpu=.Unsupported} }
    gpu:=config.operations.gpu
    if gpu.create_pipeline==nil || gpu.destroy_pipeline==nil || gpu.create_buffer==nil || gpu.destroy_buffer==nil || gpu.write_buffer==nil || gpu.create_texture==nil || gpu.destroy_texture==nil || gpu.acquire==nil || gpu.abort==nil || gpu.submit==nil || gpu.wait==nil || gpu.release_exports==nil { return {gpu=.Unsupported} }
    cache.renderer=renderer; cache.operations=config.operations; cache.allocator=allocator
    cache.textures=make([dynamic]Model_Texture,allocator); cache.samplers=make([dynamic]Model_Sampler,allocator)
    success:=false; defer { if !success { model_native_destroy(cache) } }
    batch_error:Model_Batch_Error
    cache.batch,batch_error=model_batch_prepare_entities(owner,ids,allocator)
    if batch_error.kind!=.None { return {scene=.Invalid_Geometry} }
    if len(cache.batch.entries)==0 { success=true; return {} }
    for &pipeline,i in cache.pipelines { error:gfx.Gpu_Error; pipeline,error=cache.operations.gpu.create_pipeline(renderer,config.shader.descriptors[i]); if error!=.None { return {gpu=error} } }
    for &pipeline,i in cache.reverse_pipelines { error:gfx.Gpu_Error; pipeline,error=cache.operations.gpu.create_pipeline(renderer,depth_descriptor(config.shader.descriptors[i],.Reverse)); if error!=.None { return {gpu=error} } }
    cache.frame_desc={size=u64(size_of(Frame_Data)),usage={.Uniform},memory=.CPU_Visible}
    cache.object_desc={size=u64(len(cache.batch.objects))*u64(size_of(Model_Object)),usage={.Storage},memory=.CPU_Visible}
    cache.geometry_desc={size=u64(len(cache.batch.vertices))*u64(size_of(Model_Vertex)),usage={.Storage},memory=.CPU_Visible}
    cache.slots=make([]Model_Slot,slots,allocator)
    zero_frame:=[1]Frame_Data{}
    for &slot in cache.slots {
        error:gfx.Gpu_Error
        slot.frame,error=cache.operations.gpu.create_buffer(renderer,cache.frame_desc,mem.slice_to_bytes(zero_frame[:])); if error!=.None { return {gpu=error} }
        slot.objects,error=cache.operations.gpu.create_buffer(renderer,cache.object_desc,mem.slice_to_bytes(cache.batch.objects)); if error!=.None { return {gpu=error} }
        slot.geometry,error=cache.operations.gpu.create_buffer(renderer,cache.geometry_desc,mem.slice_to_bytes(cache.batch.vertices)); if error!=.None { return {gpu=error} }
    }
    cache.receipts=make([]Model_Receipt,len(cache.batch.entries),allocator)
    for entry,i in cache.batch.entries {
        for role in 0..<5 {
            texture,error:=model_texture_prepare(cache,owner,entry,role); if error!={} { return error }
            sampler,sampler_error:=model_sampler_prepare(cache,owner,entry,role,texture); if sampler_error!={} { return sampler_error }
            cache.receipts[i].textures[role]=texture; cache.receipts[i].samplers[role]=sampler
        }
    }
    success=true; return {}
}
/// Releases prepared GPU owners after the containing scene drains accepted submissions.
model_native_destroy :: proc(cache:^Native_Model($R))->gfx.Gpu_Error {
    error:=gfx.Gpu_Error.None
    if cache.renderer!=nil {
        for slot in cache.slots { for handle in ([3]gfx.Buffer_Handle{slot.frame,slot.objects,slot.geometry}) { if handle.owner!=nil { e:=cache.operations.gpu.destroy_buffer(cache.renderer,handle); if e!=.None { error=e } } } }
        for pipeline in cache.pipelines { if pipeline.owner!=nil { e:=cache.operations.gpu.destroy_pipeline(cache.renderer,pipeline); if e!=.None { error=e } } }
        for pipeline in cache.reverse_pipelines { if pipeline.owner!=nil { e:=cache.operations.gpu.destroy_pipeline(cache.renderer,pipeline); if e!=.None { error=e } } }
        for texture in cache.textures { e:=cache.operations.gpu.destroy_texture(cache.renderer,texture.native.texture); if e!=.None { error=e } }
        for sampler in cache.samplers { e:=cache.operations.destroy_sampler(cache.renderer,sampler.handle); if e!=.None { error=e } }
    }
    for texture in cache.textures { delete(texture.encoded,cache.allocator) }
    model_batch_destroy(&cache.batch)
    delete(cache.slots,cache.allocator); delete(cache.textures); delete(cache.samplers); delete(cache.receipts,cache.allocator)
    model_graph_release(cache); cache^={}; return error
}
