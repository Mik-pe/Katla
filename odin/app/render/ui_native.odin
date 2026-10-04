//! Application UI resources upload immutable geometry and atlas versions through generic GPU APIs.
package render

import gfx "../../gfx"
import ui "../../ui"
import "core:mem"

UI_GPU_Ops :: struct($R:typeid) {
    create_pipeline:proc(^R,gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error),
    destroy_pipeline:proc(^R,gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error,
    create_buffer:proc(^R,gfx.Buffer_Desc,[]byte)->(gfx.Buffer_Handle,gfx.Gpu_Error),
    destroy_buffer:proc(^R,gfx.Buffer_Handle)->gfx.Gpu_Error,
    create_texture:proc(^R,gfx.Texture_Desc,[]byte)->(gfx.Texture_Handle,gfx.Gpu_Error),
    destroy_texture:proc(^R,gfx.Texture_Handle)->gfx.Gpu_Error,
    create_sampler:proc(^R,gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error),
    destroy_sampler:proc(^R,gfx.Sampler_Handle)->gfx.Gpu_Error,
    create_target:proc(^R,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error),
}
/// Texture identities refer to either an existing graph role or an initialized imported allocation.
UI_Encoding :: enum { Display, Linear }
UI_Texture :: struct { handle:gfx.Texture_Handle, desc:gfx.Texture_Desc, resource:gfx.Image_Id, encoding:UI_Encoding }
UI_GPU :: struct($R:typeid) { renderer:^R, ops:UI_GPU_Ops(R), pipeline,transfer,decode:gfx.Graphics_Pipeline_Handle, sampler:gfx.Sampler_Handle, atlas:UI_Texture, atlas_revision,frame_generation:u64, prepared:bool, textures:map[ui.Texture_Id]UI_Texture, allocator:mem.Allocator }
/// Immutable per-frame handles can be removed after acceptance because native execution retains them.
UI_Texture_Binding :: struct { id:ui.Texture_Id, texture:UI_Texture }
UI_GPU_Frame :: struct { owner:rawptr, generation:u64, vertices:gfx.Buffer_Handle, desc:gfx.Buffer_Desc, atlas,linear:UI_Texture, atlas_revision:u64, textures:[]UI_Texture_Binding, allocator:mem.Allocator }
/// Creates a stationary UI owner without scene/editor resources in the GPU core.
ui_gpu_init :: proc(owner:^UI_GPU($R),renderer:^R,ops:UI_GPU_Ops(R),compiled:^UI_Shader,allocator:=context.allocator)->gfx.Gpu_Error {
    if renderer==nil || compiled==nil || ops.create_target==nil || ops.create_pipeline==nil || ops.destroy_pipeline==nil || ops.create_buffer==nil || ops.destroy_buffer==nil || ops.create_texture==nil || ops.destroy_texture==nil || ops.create_sampler==nil || ops.destroy_sampler==nil { return .Unsupported }
    pipeline,error:=ops.create_pipeline(renderer,compiled.mapped.descriptor); if error!=.None { return error }
    transfer,error_transfer:=ops.create_pipeline(renderer,compiled.final_mapped.descriptor); if error_transfer!=.None { ops.destroy_pipeline(renderer,pipeline); return error_transfer }
    decode,error_decode:=ops.create_pipeline(renderer,compiled.decode_mapped.descriptor); if error_decode!=.None { ops.destroy_pipeline(renderer,transfer); ops.destroy_pipeline(renderer,pipeline); return error_decode }
    sampler:gfx.Sampler_Handle
    sampler,error=ops.create_sampler(renderer,{min_filter=.Linear,mag_filter=.Linear,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1})
    if error!=.None { ops.destroy_pipeline(renderer,decode); ops.destroy_pipeline(renderer,transfer); ops.destroy_pipeline(renderer,pipeline); return error }
    owner^={renderer=renderer,ops=ops,pipeline=pipeline,transfer=transfer,decode=decode,sampler=sampler,textures=make(map[ui.Texture_Id]UI_Texture,allocator),allocator=allocator}; return .None
}
/// Registers a host-owned texture and its current graph identity without taking ownership of it.
ui_gpu_texture :: proc(owner:^UI_GPU($R),id:ui.Texture_Id,texture:UI_Texture)->UI_Error {
    if id==0 || id==UI_TEXTURE_ATLAS || texture.handle.owner==nil || .Sampled not_in texture.desc.usage { return .Missing_Texture }
    owner.textures[id]=texture; return .None
}
/// Removes a registry identity while already prepared frames keep their frozen texture selection.
ui_gpu_remove_texture :: proc(owner:^UI_GPU($R),id:ui.Texture_Id)->UI_Error {
    if id==0 || id==UI_TEXTURE_ATLAS { return .Missing_Texture }
    _,found:=owner.textures[id]; if !found { return .Missing_Texture }
    delete_key(&owner.textures,id); return .None
}
/// Uploads new immutable resources before publication; stale prepared atlases fail explicitly.
ui_gpu_prepare :: proc(owner:^UI_GPU($R),fonts:^UI_Font_System,mesh:^UI_Mesh)->(UI_GPU_Frame,gfx.Gpu_Error) {
    if owner.prepared { return {},.Busy }
    if mesh.atlas_revision!=fonts.atlas.revision || fonts.error!=.None { return {},.Invalid_Resource }
    for batch in mesh.batches { if batch.texture!=UI_TEXTURE_ATLAS { _,ok:=owner.textures[batch.texture]; if !ok { return {},.Invalid_Resource } } }
    if owner.atlas_revision!=fonts.atlas.revision {
        desc:=gfx.Texture_Desc{width=fonts.atlas.width,height=fonts.atlas.height,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Sampled,.Transfer_Destination}}
        handle,error:=owner.ops.create_texture(owner.renderer,desc,fonts.atlas.pixels); if error!=.None { return {},error }
        if owner.atlas.handle.owner!=nil { cleanup:=owner.ops.destroy_texture(owner.renderer,owner.atlas.handle); if cleanup!=.None { owner.ops.destroy_texture(owner.renderer,handle); return {},cleanup } }
        owner.atlas={handle=handle,desc=desc,encoding=.Linear}; owner.atlas_revision=fonts.atlas.revision
    }
    frame:=UI_GPU_Frame{owner=owner,generation=owner.frame_generation+1,atlas=owner.atlas,atlas_revision=owner.atlas_revision,allocator=owner.allocator}
    bindings:=make([dynamic]UI_Texture_Binding,owner.allocator); defer delete(bindings)
    for batch in mesh.batches {
        if batch.texture==UI_TEXTURE_ATLAS { continue }
        found:=false; for binding in bindings { if binding.id==batch.texture { found=true; break } }
        if !found { append(&bindings,UI_Texture_Binding{batch.texture,owner.textures[batch.texture]}) }
    }
    frame.textures=make([]UI_Texture_Binding,len(bindings),owner.allocator); copy(frame.textures,bindings[:])
    if len(mesh.vertices)>0 {
        frame.desc={size=u64(len(mesh.vertices))*u64(size_of(ui.Vertex)),usage={.Storage,.Transfer_Destination},memory=.GPU_Private}
        error:gfx.Gpu_Error
        frame.vertices,error=owner.ops.create_buffer(owner.renderer,frame.desc,mem.slice_to_bytes(mesh.vertices[:])); if error!=.None { delete(frame.textures,frame.allocator); return {},error }
    }
    owner.prepared=true; owner.frame_generation=frame.generation
    return frame,.None
}
/// Removes one public immutable upload after native acceptance or graph preparation rejection.
ui_gpu_frame_destroy :: proc(owner:^UI_GPU($R),frame:^UI_GPU_Frame)->gfx.Gpu_Error {
    if !owner.prepared || frame.owner!=owner || frame.generation!=owner.frame_generation { return .Invalid_Resource }
    if frame.linear.handle.owner!=nil { error:=owner.ops.destroy_texture(owner.renderer,frame.linear.handle); if error!=.None { return error }; frame.linear={} }
    if frame.vertices.owner!=nil { error:=owner.ops.destroy_buffer(owner.renderer,frame.vertices); if error!=.None { return error } }
    delete(frame.textures,frame.allocator); owner.prepared=false; frame^={}; return .None
}
/// Drained host submissions or native retention govern the actual resource destruction lifetime.
ui_gpu_destroy :: proc(owner:^UI_GPU($R))->gfx.Gpu_Error {
    if owner.prepared { return .Busy }
    if owner.atlas.handle.owner!=nil { error:=owner.ops.destroy_texture(owner.renderer,owner.atlas.handle); if error!=.None { return error }; owner.atlas={} }
    if owner.transfer.owner!=nil { error:=owner.ops.destroy_pipeline(owner.renderer,owner.transfer); if error!=.None { return error }; owner.transfer={} }
    if owner.decode.owner!=nil { error:=owner.ops.destroy_pipeline(owner.renderer,owner.decode); if error!=.None { return error }; owner.decode={} }
    if owner.pipeline.owner!=nil { error:=owner.ops.destroy_pipeline(owner.renderer,owner.pipeline); if error!=.None { return error }; owner.pipeline={} }
    if owner.sampler.owner!=nil { error:=owner.ops.destroy_sampler(owner.renderer,owner.sampler); if error!=.None { return error }; owner.sampler={} }
    delete(owner.textures); owner^={}; return .None
}
