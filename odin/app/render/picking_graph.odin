//! Picking records the same geometry/model/camera buffers as the visible scene submission.
package render

import gfx "../../gfx"
import ecs "../../ecs"
import "core:mem"
import "core:fmt"
import m "core:math"

/// Existing scene graph identities are reused instead of duplicating physical buffer aliases.
Picking_Buffer :: struct { resource:gfx.Resource_Id, handle:gfx.Buffer_Handle, desc:gfx.Buffer_Desc }
Picking_Alpha_Mode :: enum { Mask,Blend }
Picking_Mask :: struct { enabled:bool,texture:UI_Texture,sampler:gfx.Sampler_Handle,uv_offset,vertex_alpha_offset,object_alpha_offset:u32,vertex_alpha:bool,cutoff:f32,mode:Picking_Alpha_Mode }
/// One immutable scene range associates explicit object identity with its actual GPU geometry.
Picking_Draw :: struct {
    geometry,objects:Picking_Buffer,
    frame:Picking_Buffer,
    vertex_stride,object_stride,position_offset,model_offset:u32,
    first_vertex,vertex_count,object_index,encoded:u32,
    entity:ecs.Entity_Id,
    pipeline:gfx.Graphics_Pipeline_Handle,
    mask:Picking_Mask,
}
Picking_Graph_Input :: struct { buffers:[]gfx.Buffer_Input, textures:[]gfx.Texture_Input, entries:[]Picking_Entry, passes:[]gfx.Pass_Id, first_pass,first_buffer,first_image:int, allocator:mem.Allocator }
@(private="package")
picking_graph_buffer :: proc(g:^gfx.Graph,buffer:Picking_Buffer)->(gfx.Resource_Id,gfx.Graph_Error) {
    if buffer.handle.owner==nil || buffer.desc.size==0 { return {},.Invalid_Resource }
    if buffer.resource.owner==g {
        if buffer.resource.index<0 || buffer.resource.index>=len(g.buffers) || g.buffers[buffer.resource.index].desc!=buffer.desc { return {},.Invalid_Resource }
        return buffer.resource,.None
    }
    return gfx.graph_buffer(g,buffer.desc,true,false)
}
@(private="package")
picking_append_input :: proc(inputs:^[dynamic]gfx.Buffer_Input,resource:gfx.Resource_Id,handle:gfx.Buffer_Handle)->bool {
    for input in inputs { if input.resource==resource { return input.handle==handle } }
    append(inputs,gfx.Buffer_Input{resource,handle}); return true
}
@(private="package")
picking_cutoff_valid :: proc(value:f32)->bool { return !m.is_nan(value) && !m.is_inf(value) && value>=0 }
/// Appends integer/depth draws and captures their immutable generational entity map.
/// Position/model offsets and strides are bytes; all GPU data remains the scene's accepted allocation.
picking_graph_append :: proc(g:^gfx.Graph,pipeline:gfx.Graphics_Pipeline_Handle,frame:Picking_Buffer,draws:[]Picking_Draw,id,depth:gfx.Image_Id,allocator:=context.allocator,depth_sense:Depth_Sense=.Forward)->(Picking_Graph_Input,Native_Error) {
    if !depth_sense_valid(depth_sense) { return {},{scene=.Invalid_Camera} }
    if g==nil || pipeline.owner==nil || id.owner!=g || depth.owner!=g || id.index<0 || id.index>=len(g.images) || depth.index<0 || depth.index>=len(g.images) { return {},{gpu=.Invalid_Resource} }
    id_desc:=g.images[id.index].desc; depth_desc:=g.images[depth.index].desc
    if id_desc.format!=.R32_Uint || depth_desc.format!=.D32_Float || id_desc.width!=depth_desc.width || id_desc.height!=depth_desc.height || !g.images[id.index].exported { return {},{gpu=.Invalid_Range} }
    if frame.desc.size<u64(size_of(Frame_Data)) || .Uniform not_in frame.desc.usage { return {},{gpu=.Invalid_Range} }
    for draw,i in draws {
        if draw.encoded==0 || draw.vertex_count==0 || draw.vertex_count%3!=0 || draw.vertex_stride%4!=0 || draw.object_stride%4!=0 || draw.position_offset%4!=0 || draw.model_offset%4!=0 || draw.vertex_stride<16 || draw.object_stride<64 || draw.position_offset>draw.vertex_stride-16 || draw.model_offset>draw.object_stride-64 || .Storage not_in draw.geometry.desc.usage || .Storage not_in draw.objects.desc.usage { return {},{gpu=.Invalid_Range} }
        if (u64(draw.first_vertex)+u64(draw.vertex_count))*u64(draw.vertex_stride)>draw.geometry.desc.size || (u64(draw.object_index)+1)*u64(draw.object_stride)>draw.objects.desc.size { return {},{gpu=.Invalid_Range} }
        if draw.mask.enabled {
            if draw.pipeline.owner==nil || draw.mask.texture.handle.owner==nil || draw.mask.sampler.owner==nil || .Sampled not_in draw.mask.texture.desc.usage || draw.mask.uv_offset%4!=0 || draw.mask.object_alpha_offset%4!=0 || draw.mask.vertex_alpha_offset%4!=0 || draw.mask.uv_offset>draw.vertex_stride-8 || draw.mask.object_alpha_offset>draw.object_stride-4 || (draw.mask.vertex_alpha && draw.mask.vertex_alpha_offset>draw.vertex_stride-4) || (!picking_cutoff_valid(draw.mask.cutoff) || draw.mask.mode not_in (bit_set[Picking_Alpha_Mode]{.Mask,.Blend})) { return {},{gpu=.Invalid_Range} }
        }
        for other in draws[:i] { if other.encoded==draw.encoded && other.entity!=draw.entity { return {},{gpu=.Invalid_Resource} } }
    }
    result:=Picking_Graph_Input{first_pass=len(g.passes),first_buffer=len(g.buffers),first_image=len(g.images),allocator=allocator}
    success:=false
    defer { if !success { gfx.graph_truncate(g,result.first_pass,result.first_buffer,result.first_image) } }
    inputs:=make([dynamic]gfx.Buffer_Input,allocator); passes:=make([dynamic]gfx.Pass_Id,allocator); entries:=make([dynamic]Picking_Entry,allocator)
    textures:=make([dynamic]gfx.Texture_Input,allocator)
    defer delete(inputs); defer delete(passes); defer delete(entries); defer delete(textures)
    camera,frame_error:=picking_graph_buffer(g,frame); if frame_error!=.None { return {},{gpu=.Invalid_Graph} }
    picking_append_input(&inputs,camera,frame.handle)
    for i in 0..<max(1,len(draws)) {
        accesses:[]gfx.Buffer_Access
        bindings:[]gfx.Stage_Buffer_Binding
        phases:[]gfx.Render_Phase
        geometry,objects:gfx.Resource_Id
        uniforms:[1]Picking_Uniform
        mask_uniforms:[1]Picking_Mask_Uniform
        selected_pipeline:=pipeline
        mask_access:[1]gfx.Image_Access
        mask_binding:[1]gfx.Image_Binding
        mask_sampler:[1]gfx.Sampler_Binding
        masked:=false
        buffer_access:[3]gfx.Buffer_Access
        buffer_bindings:[3]gfx.Stage_Buffer_Binding
        constant:[1]gfx.Constant_Binding
        operation:[1]gfx.Draw_Op
        phase:[1]gfx.Render_Phase
        if len(draws)>0 {
            draw:=draws[i]
            graph_error:gfx.Graph_Error
            geometry,graph_error=picking_graph_buffer(g,draw.geometry); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
            objects,graph_error=picking_graph_buffer(g,draw.objects); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
            if !picking_append_input(&inputs,geometry,draw.geometry.handle) || !picking_append_input(&inputs,objects,draw.objects.handle) { return {},{gpu=.Invalid_Resource} }
            actual_camera:=camera
            if draw.frame.handle.owner!=nil {
                if draw.frame.desc.size<u64(size_of(Frame_Data)) || .Uniform not_in draw.frame.desc.usage { return {},{gpu=.Invalid_Range} }
                actual_camera,graph_error=picking_graph_buffer(g,draw.frame); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }
                if !picking_append_input(&inputs,actual_camera,draw.frame.handle) { return {},{gpu=.Invalid_Resource} }
            }
            buffer_access={ {actual_camera,{0,u64(size_of(Frame_Data))},.Read,.Uniform}, {objects,{0,draw.objects.desc.size},.Read,.Storage}, {geometry,{0,draw.geometry.desc.size},.Read,.Storage} }
            buffer_bindings={ {0,0,{.Vertex},buffer_access[0]}, {0,1,{.Vertex},buffer_access[1]}, {0,2,{.Vertex},buffer_access[2]} }
            uniforms[0]={draw.vertex_stride/4,draw.object_stride/4,draw.position_offset/4,draw.model_offset/4,draw.object_index,draw.encoded,{}}
            constant[0]={group=0,slot=3,stages={.Vertex,.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(uniforms[:])}
            selected_pipeline=draw.pipeline if draw.pipeline.owner!=nil else pipeline
            masked=draw.mask.enabled
            if masked {
                texture:=draw.mask.texture
                image:gfx.Image_Id
                existing:=false
                for input in textures { if input.handle==texture.handle { if g.images[input.resource.index].desc!=texture.desc { return {},{gpu=.Invalid_Resource} }; image=input.resource; existing=true; break } }
                if !existing { image,graph_error=ui_graph_texture(g,texture); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }; append(&textures,gfx.Texture_Input{image,texture.handle}) }
                mask_access[0]={image,gfx.image_full_range(texture.desc),.Read,.Sampled}
                mask_binding[0]={1,0,{.Fragment},mask_access[:]}
                mask_sampler[0]={1,1,{.Fragment},draw.mask.sampler}
                mask_uniforms[0]={uniforms[0],draw.mask.uv_offset/4,draw.mask.vertex_alpha_offset/4,draw.mask.object_alpha_offset/4,u32(draw.mask.vertex_alpha),draw.mask.cutoff,{u32(draw.mask.mode),0,0}}
                constant[0].bytes=mem.slice_to_bytes(mask_uniforms[:])
            }
            operation[0]=gfx.Draw{draw.vertex_count,1,draw.first_vertex,0}
            phase[0]={pipeline=selected_pipeline,constants=constant[:],draws=operation[:]}
            accesses=buffer_access[:]; bindings=buffer_bindings[:]; phases=phase[:]
            known:=false; for entry in entries { if entry.encoded==draw.encoded { known=true; break } }
            if !known { append(&entries,Picking_Entry{draw.encoded,draw.entity}) }
        }
        color:=gfx.Image_Access{id,gfx.image_full_range(id_desc),.Write if i==0 else .Read_Write,.Color_Attachment}
        depth_access:=gfx.Image_Access{depth,gfx.image_full_range(depth_desc),.Write if i==0 else .Read_Write,.Depth_Attachment}
        images:=[3]gfx.Image_Access{color,depth_access,mask_access[0]}
        colors:=[1]gfx.Color_Attachment{{color,.Clear if i==0 else .Load,.Store,{}}}
        packet:=gfx.Render{colors=colors[:],depth={enabled=true,access=depth_access,load=.Clear if i==0 else .Load,store=.Store,clear_depth=f64(depth_clear(depth_sense))},buffers=bindings,phases=phases,images=mask_binding[:] if masked else nil,samplers=mask_sampler[:] if masked else nil}
        name:=fmt.aprintf("Picking geometry %d",i,allocator=allocator)
        pass,error:=gfx.graph_pass(g,name,.Graphics,accesses,images=images[:] if masked else images[:2]); delete(name,allocator)
        if error!=.None { return {},{gpu=.Invalid_Graph} }
        packet_error:=gfx.graph_set_packet(g,pass,packet); if packet_error!=.None { return {},{gpu=.Invalid_Graph,packet=packet_error} }
        append(&passes,pass)
    }
    result.buffers=make([]gfx.Buffer_Input,len(inputs),allocator); copy(result.buffers,inputs[:])
    result.textures=make([]gfx.Texture_Input,len(textures),allocator); copy(result.textures,textures[:])
    result.entries=make([]Picking_Entry,len(entries),allocator); copy(result.entries,entries[:])
    result.passes=make([]gfx.Pass_Id,len(passes),allocator); copy(result.passes,passes[:])
    success=true; return result,{}
}
/// Releases the immutable CPU map/input owner after the capture queue has cloned its map.
picking_graph_input_destroy :: proc(input:^Picking_Graph_Input) { delete(input.buffers,input.allocator); delete(input.textures,input.allocator); delete(input.entries,input.allocator); delete(input.passes,input.allocator); input^={} }
