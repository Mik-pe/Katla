//! UI graph composition preserves command order, popup layering and explicit sampled image roles.
package render

import gfx "../../gfx"
import ui "../../ui"
import "core:mem"
import m "core:math"
import "core:fmt"

/// Host-owned graph extensions return only the additional immutable resource inputs.
UI_Graph_Input :: struct { buffers:[]gfx.Buffer_Input, textures:[]gfx.Texture_Input, passes:[]gfx.Pass_Id, first_pass,first_buffer,first_image:int, allocator:mem.Allocator }
@(private="package")
ui_graph_texture :: proc(g:^gfx.Graph,texture:UI_Texture)->(gfx.Image_Id,gfx.Graph_Error) {
    if texture.resource.owner==g {
        if texture.resource.index<0 || texture.resource.index>=len(g.images) || g.images[texture.resource.index].desc!=texture.desc { return {},.Invalid_Resource }
        return texture.resource,.None
    }
    return gfx.graph_image(g,texture.desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
}
@(private="package")
ui_texture_lookup :: proc(frame:^UI_GPU_Frame,id:ui.Texture_Id)->(UI_Texture,bool) {
    if id==UI_TEXTURE_ATLAS { return frame.atlas,true }
    for binding in frame.textures { if binding.id==id { return binding.texture,true } }
    return {},false
}
@(private="package")
ui_graph_scissor :: proc(clip:ui.Rect,scale:f32,desc:gfx.Texture_Desc)->(gfx.Scissor,bool) {
    x:=u32(clamp(f32(m.floor(clip.x*scale)),f32(0),f32(desc.width)))
    y:=u32(clamp(f32(m.floor(clip.y*scale)),f32(0),f32(desc.height)))
    right:=u32(clamp(f32(m.ceil((clip.x+clip.width)*scale)),f32(0),f32(desc.width)))
    bottom:=u32(clamp(f32(m.ceil((clip.y+clip.height)*scale)),f32(0),f32(desc.height)))
    if x>=right || y>=bottom { return {},false }
    return {true,x,y,right-x,bottom-y},true
}
@(private="package")
ui_graph_pass_name :: proc(index:int,allocator:mem.Allocator)->string { return fmt.aprintf("UI composition %d",index,allocator=allocator) }
@(private="package")
ui_graph_transfer :: proc(g:^gfx.Graph,pipeline:gfx.Graphics_Pipeline_Handle,source,target:gfx.Image_Id,source_desc,target_desc:gfx.Texture_Desc,name:string)->(gfx.Pass_Id,Native_Error) {
    read:=gfx.Image_Access{source,gfx.image_full_range(source_desc),.Read,.Sampled}
    write:=gfx.Image_Access{target,gfx.image_full_range(target_desc),.Write,.Color_Attachment}
    images:=[2]gfx.Image_Access{read,write}
    colors:=[1]gfx.Color_Attachment{{write,.Clear,.Store,{}}}
    operations:=[1]gfx.Draw_Op{gfx.Draw{3,1,0,0}}
    phases:=[1]gfx.Render_Phase{{pipeline=pipeline,draws=operations[:]}}
    bindings:=[1]gfx.Image_Binding{{0,0,{.Fragment},images[:1]}}
    pass,error:=gfx.graph_pass(g,name,.Graphics,nil,images=images[:]); if error!=.None { return {},{gpu=.Invalid_Graph} }
    packet_error:=gfx.graph_set_packet(g,pass,gfx.Render{colors=colors[:],images=bindings[:],phases=phases[:]}); if packet_error!=.None { return {},{gpu=.Invalid_Graph,packet=packet_error} }
    return pass,{}
}
/// Appends final UI passes after the scene output; the caller recompiles the composed graph once.
ui_graph_append :: proc(g:^gfx.Graph,owner:^UI_GPU($R),frame:^UI_GPU_Frame,mesh:^UI_Mesh,target:gfx.Image_Id,desc:gfx.Texture_Desc,clear:bool,clip_y_down:bool,allocator:=context.allocator)->(UI_Graph_Input,Native_Error) {
    if !owner.prepared || frame.owner!=owner || frame.generation!=owner.frame_generation || frame.atlas_revision!=mesh.atlas_revision || target.owner!=g || target.index<0 || target.index>=len(g.images) || g.images[target.index].desc!=desc || desc.depth!=1 || .Color_Attachment not_in desc.usage { return {},{gpu=.Invalid_Resource} }
    if desc.format!=.RGBA8_Unorm && desc.format!=.BGRA8_Unorm { return {},{gpu=.Unsupported} }
    if frame.linear.handle.owner!=nil { return {},{gpu=.Invalid_Resource} }
    if !clear && .Sampled not_in desc.usage { return {},{gpu=.Unsupported} }
    for batch in mesh.batches { texture,ok:=ui_texture_lookup(frame,batch.texture); if !ok || texture.handle.owner==nil { return {},{gpu=.Invalid_Resource} } }
    result:=UI_Graph_Input{first_pass=len(g.passes),first_buffer=len(g.buffers),first_image=len(g.images),allocator=allocator}
    success:=false
    defer { if !success { gfx.graph_truncate(g,result.first_pass,result.first_buffer,result.first_image) } }
    textures:=make([dynamic]gfx.Texture_Input,allocator); passes:=make([dynamic]gfx.Pass_Id,allocator)
    defer delete(textures); defer delete(passes)
    linear_desc:=gfx.Texture_Desc{width=desc.width,height=desc.height,depth=1,mip_levels=1,layers=1,format=.RGBA16_Float,usage={.Color_Attachment,.Sampled}}
    linear_handle,allocation_error:=owner.ops.create_target(owner.renderer,linear_desc); if allocation_error!=.None { return {},{gpu=allocation_error} }
    frame.linear={handle=linear_handle,desc=linear_desc,encoding=.Linear}
    linear,linear_error:=gfx.graph_image(g,linear_desc,{initial=.Undefined,final=.Shader_Read},false,false); if linear_error!=.None { return {},{gpu=.Invalid_Graph} }
    append(&textures,gfx.Texture_Input{linear,linear_handle})
    if !clear {
        initial,initial_error:=ui_graph_transfer(g,owner.decode,target,linear,desc,linear_desc,"UI initial display decode"); if initial_error!={} { return {},initial_error }; append(&passes,initial)
    }
    vertex:gfx.Resource_Id
    if len(mesh.vertices)>0 {
        id,error:=gfx.graph_buffer(g,frame.desc,true,false); if error!=.None { return {},{gpu=.Invalid_Graph} }; vertex=id
        result.buffers=make([]gfx.Buffer_Input,1,allocator); result.buffers[0]={vertex,frame.vertices}
    }
    atlas,atlas_error:=ui_graph_texture(g,frame.atlas); if atlas_error!=.None { delete(result.buffers,allocator); return {},{gpu=.Invalid_Graph} }
    append(&textures,gfx.Texture_Input{atlas,frame.atlas.handle})
    images:=make(map[ui.Texture_Id]gfx.Image_Id,allocator); defer delete(images); images[UI_TEXTURE_ATLAS]=atlas
    for binding in frame.textures {
        image:gfx.Image_Id
        existing:=false
        for input in textures { if input.handle==binding.texture.handle { if g.images[input.resource.index].desc!=binding.texture.desc { delete(result.buffers,allocator); return {},{gpu=.Invalid_Resource} }; image=input.resource; existing=true; break } }
        if !existing { error:gfx.Graph_Error; image,error=ui_graph_texture(g,binding.texture); if error!=.None { delete(result.buffers,allocator); return {},{gpu=.Invalid_Graph} } }
        images[binding.id]=image
        found:=false; for input in textures { if input.resource==image { found=true; break } }
        if !found { append(&textures,gfx.Texture_Input{image,binding.texture.handle}) }
    }
    start:=0
    first:=true
    for first || start<len(mesh.batches) {
        ids:[64]ui.Texture_Id; ids[0]=UI_TEXTURE_ATLAS
        count:=1; end:=start
        for end<len(mesh.batches) {
            id:=mesh.batches[end].texture
            found:=false; for current in ids[:count] { if current==id { found=true; break } }
            if !found { if count==64 { break }; ids[count]=id; count+=1 }
            end+=1
        }
        color:=gfx.Image_Access{linear,gfx.image_full_range(linear_desc),.Write if first && clear else .Read_Write,.Color_Attachment}
        accesses:=make([]gfx.Image_Access,count+1,allocator); accesses[0]=color
        elements:[64]gfx.Image_Access
        for i in 0..<count {
            id:=images[ids[i]]
            texture,_:=ui_texture_lookup(frame,ids[i]); accesses[i+1]={id,gfx.image_full_range(texture.desc),.Read,.Sampled}
        }
        for i in 0..<64 { elements[i]=accesses[1] if i>=count else accesses[i+1] }
        phases:=make([]gfx.Render_Phase,end-start,allocator)
        uniforms:=make([]UI_Frame_Data,end-start,allocator)
        constants:=make([]gfx.Constant_Binding,end-start,allocator)
        draws:=make([]gfx.Draw_Op,end-start,allocator)
        for batch,index in mesh.batches[start:end] {
            texture_index:u32
            for id,i in ids[:count] { if id==batch.texture { texture_index=u32(i); break } }
            texture,_:=ui_texture_lookup(frame,batch.texture)
            decode:=texture.encoding==.Display && texture.desc.format!=.RGBA8_Srgb && texture.desc.format!=.BGRA8_Srgb
            uniforms[index]={logical_size=mesh.logical_size,texture_index=texture_index,clip_y=-1 if clip_y_down else 1,decode_sample=u32(decode)}
            constants[index]={group=0,slot=0,stages={.Vertex,.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(uniforms[index:index+1])}
            scissor,valid:=ui_graph_scissor(batch.clip,mesh.pixel_scale,desc)
            if !valid { delete(accesses,allocator); delete(phases,allocator); delete(uniforms,allocator); delete(constants,allocator); delete(draws,allocator); delete(result.buffers,allocator); return {},{gpu=.Invalid_Range} }
            draws[index]=gfx.Draw{batch.count,1,batch.first,0}
            phases[index]={pipeline=owner.pipeline,constants=constants[index:index+1],scissor=scissor,draws=draws[index:index+1]}
        }
        reads:=[1]gfx.Buffer_Access{{vertex,{0,frame.desc.size},.Read,.Storage}}
        packet:=gfx.Render{colors={{color,.Clear if first && clear else .Load,.Store,{0.002709,0.003096,0.003936,1}}},phases=phases}
        if end>start {
            packet.buffers={{0,1,{.Vertex},reads[0]}}
            packet.images={{1,0,{.Fragment},elements[:]}}
            packet.samplers={{1,1,{.Fragment},owner.sampler}}
        }
        name:=ui_graph_pass_name(len(passes),allocator)
        pass,error:=gfx.graph_pass(g,name,.Graphics,reads[:] if end>start else nil,images=accesses if end>start else accesses[:1])
        delete(name,allocator)
        packet_error:=gfx.Packet_Error.None
        if error==.None { packet_error=gfx.graph_set_packet(g,pass,packet) }
        delete(accesses,allocator); delete(phases,allocator); delete(uniforms,allocator); delete(constants,allocator); delete(draws,allocator)
        if error!=.None || packet_error!=.None { delete(result.buffers,allocator); return {},{gpu=.Invalid_Graph,packet=packet_error} }
        append(&passes,pass); start=end; first=false
    }
    final,final_error:=ui_graph_transfer(g,owner.transfer,linear,target,linear_desc,desc,"UI final display encode"); if final_error!={} { delete(result.buffers,allocator); return {},final_error }; append(&passes,final)
    result.textures=make([]gfx.Texture_Input,len(textures),allocator); copy(result.textures,textures[:])
    result.passes=make([]gfx.Pass_Id,len(passes),allocator); copy(result.passes,passes[:])
    success=true; return result,{}
}
/// Releases additional CPU input lists; frame uploads and graph declarations have separate owners.
ui_graph_input_destroy :: proc(input:^UI_Graph_Input) { delete(input.buffers,input.allocator); delete(input.textures,input.allocator); delete(input.passes,input.allocator); input^={} }
