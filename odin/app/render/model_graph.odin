//! Model materials compose ordered ordinary passes on the scene's existing attachments.
package render

import gfx "../../gfx"
import km "../../math"
import "core:fmt"
import "core:mem"

@(private="package")
model_graph_release :: proc(cache:^Native_Model($R)) {
    delete(cache.image_ids,cache.allocator); delete(cache.passes,cache.allocator); delete(cache.order,cache.allocator); delete(cache.texture_inputs,cache.allocator)
    cache.image_ids=nil; cache.passes=nil; cache.order=nil; cache.texture_inputs=nil; cache.graph=nil
}
@(private="package")
model_graph_order :: proc(cache:^Native_Model($R),view_projection:km.Mat4,allocator:mem.Allocator=context.allocator)->[]int {
    model_batch_update_depths(&cache.batch,view_projection)
    result:=make([]int,len(cache.batch.entries),allocator)
    for &item,i in result { item=i }
    for i in 1..<len(result) {
        item:=result[i]
        transparent:=cache.batch.entries[item].material.alpha_mode==.Blend
        distance:=cache.batch.entries[item].camera_depth
        cursor:=i
        for cursor>0 {
            previous:=result[cursor-1]
            previous_transparent:=cache.batch.entries[previous].material.alpha_mode==.Blend
            previous_distance:=cache.batch.entries[previous].camera_depth
            if (!transparent && previous_transparent) || (transparent && previous_transparent && distance>previous_distance) { result[cursor]=previous; cursor-=1 } else { break }
        }
        result[cursor]=item
    }
    return result
}
@(private="package")
model_pass_name :: proc(index:int)->string { return fmt.aprintf("Model surface %d",index) }
@(private="package")
model_graph_packet :: proc(cache:^Native_Model($R),scene:^Scene_Graph,pass:gfx.Pass_Id,index:int,replace:bool)->Native_Error {
    buffers:=[3]gfx.Stage_Buffer_Binding{
        {0,0,{.Vertex,.Fragment},{cache.frame,{0,cache.frame_desc.size},.Read,.Uniform}},
        {0,1,{.Vertex,.Fragment},{cache.objects,{0,cache.object_desc.size},.Read,.Storage}},
        {0,2,{.Vertex},{cache.geometry,{0,cache.geometry_desc.size},.Read,.Storage}},
    }
    colors:=[1]gfx.Color_Attachment{{{cache.linear_color,gfx.image_full_range(cache.linear_desc),.Read_Write,.Color_Attachment},.Load,.Store,{}}}
    depth:=gfx.Depth_Attachment{enabled=true,access={scene.depth,gfx.image_full_range(scene.depth_desc),.Read_Write,.Depth_Attachment},load=.Load,store=.Store,clear_depth=1}
    image_accesses:[5]gfx.Image_Access
    bindings:[5]gfx.Image_Binding
    samplers:[5]gfx.Sampler_Binding
    declared:[7]gfx.Image_Access
    declared[0],declared[1]=colors[0].access,depth.access
    declared_count:=2
    receipt:=cache.receipts[index]
    for role in 0..<5 {
        texture:=receipt.textures[role]
        image_accesses[role]={cache.image_ids[texture],gfx.image_full_range(cache.textures[texture].native.desc),.Read,.Sampled}
        bindings[role]={group=1,slot=u32(role),stages={.Fragment},accesses=image_accesses[role:role+1]}
        samplers[role]={1,u32(role+5),{.Fragment},cache.samplers[receipt.samplers[role]].handle}
        duplicate:=false; for access in declared[:declared_count] { if access==image_accesses[role] { duplicate=true; break } }
        if !duplicate { declared[declared_count]=image_accesses[role]; declared_count+=1 }
    }
    entry:=cache.batch.entries[index]
    draws:=[1]gfx.Draw_Op{gfx.Draw{entry.vertex_count,1,entry.first_vertex,entry.object_index}}
    object_model:=cache.batch.objects[entry.object_index].model
    mirrored:=km.dot(km.cross(km.xyz(object_model[0]),km.xyz(object_model[1])),km.xyz(object_model[2]))<0
    variant:=model_pipeline_variant(entry.material.double_sided,entry.material.alpha_mode==.Blend,mirrored)
    phases:=[1]gfx.Render_Phase{{pipeline=cache.pipelines[variant],draws=draws[:]}}
    packet:=gfx.Render{colors=colors[:],depth=depth,buffers=buffers[:],images=bindings[:],samplers=samplers[:],phases=phases[:]}
    if replace {
        reads:=[3]gfx.Buffer_Access{buffers[0].access,buffers[1].access,buffers[2].access}
        packet_error,graph_error:=gfx.graph_set_commands(&scene.graph,pass,packet,reads[:],declared[:declared_count])
        if packet_error!=.None { return {packet=packet_error} }
        if graph_error!=.None { return {gpu=.Invalid_Graph} }
        return {}
    }
    return {packet=gfx.graph_set_packet(&scene.graph,pass,packet)}
}
/// Declares ready images and each material's actual sampled resources on the stationary scene graph.
model_native_bind_graph :: proc(cache:^Native_Model($R),scene:^Scene_Graph)->Native_Error {
    model_graph_release(cache); cache.graph=scene
    if len(cache.batch.entries)==0 { return {} }
    error:gfx.Graph_Error
    cache.frame,error=gfx.graph_buffer(&scene.graph,cache.frame_desc,true,false); if error!=.None { return {gpu=.Invalid_Graph} }
    cache.objects,error=gfx.graph_buffer(&scene.graph,cache.object_desc,true,false); if error!=.None { return {gpu=.Invalid_Graph} }
    cache.geometry,error=gfx.graph_buffer(&scene.graph,cache.geometry_desc,true,false); if error!=.None { return {gpu=.Invalid_Graph} }
    cache.image_ids=make([]gfx.Image_Id,len(cache.textures),cache.allocator)
    cache.texture_inputs=make([]gfx.Texture_Input,len(cache.textures),cache.allocator)
    for texture,i in cache.textures {
        cache.image_ids[i],error=gfx.graph_image(&scene.graph,texture.native.desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
        if error!=.None { return {gpu=.Invalid_Graph} }
        cache.texture_inputs[i]={cache.image_ids[i],texture.native.texture}
    }
    composite_error:=model_composite_begin(cache,scene); if composite_error!={} { return composite_error }
    cache.passes=make([]gfx.Pass_Id,len(cache.batch.entries),cache.allocator)
    cache.order=model_graph_order(cache,{},cache.allocator)
    for index,i in cache.order {
        name:=model_pass_name(i); defer delete(name)
        cache.passes[i],error=gfx.graph_pass(&scene.graph,name,.Graphics,nil)
        if error!=.None { return {gpu=.Invalid_Graph} }
        packet_error:=model_graph_packet(cache,scene,cache.passes[i],index,true); if packet_error!={} { return packet_error }
    }
    composite_error=model_composite_pass(cache,scene,true); if composite_error!={} { return composite_error }
    replacement,compile_error:=gfx.graph_compile(&scene.graph); if compile_error!=.None { return {gpu=.Invalid_Graph} }
    previous:=scene.plan; scene.plan=replacement; gfx.compiled_graph_destroy(&previous)
    return {}
}
/// Streams the actual sampled model revision into the acquired slot and updates transparent order.
model_native_prepare :: proc(cache:^Native_Model($R),scene:^Scene_Graph,token:gfx.Frame_Token,frame:Frame_Data)->(Scene_Inputs,Native_Error) {
    if cache.graph!=scene { return {},{gpu=.Invalid_Graph} }
    if len(cache.batch.entries)==0 { return {},{} }
    if token.slot<0 || token.slot>=len(cache.slots) { return {},{gpu=.Invalid_Resource} }
    order:=model_graph_order(cache,frame.view_projection,cache.allocator); defer delete(order,cache.allocator)
    for index,i in order {
        error:=model_graph_packet(cache,scene,cache.passes[i],index,index!=cache.order[i])
        if error!={} { return {},error }
        cache.order[i]=index
    }
    if scene.plan.revision!=scene.graph.revision {
        plan,error:=gfx.graph_compile(&scene.graph); if error!=.None { return {},{gpu=.Invalid_Graph} }
        previous:=scene.plan; scene.plan=plan; gfx.compiled_graph_destroy(&previous)
    }
    slot:=cache.slots[token.slot]
    frames:=[1]Frame_Data{frame}
    handles:=[3]gfx.Buffer_Handle{slot.frame,slot.objects,slot.geometry}
    bytes:=[3][]byte{mem.slice_to_bytes(frames[:]),mem.slice_to_bytes(cache.batch.objects),mem.slice_to_bytes(cache.batch.vertices)}
    for handle,i in handles { error:=cache.operations.gpu.write_buffer(cache.renderer,token,handle,0,bytes[i]); if error!=.None { return {},{gpu=error} } }
    cache.inputs={{cache.frame,slot.frame},{cache.objects,slot.objects},{cache.geometry,slot.geometry}}
    if len(cache.frame_textures)!=len(cache.texture_inputs)+2 { delete(cache.frame_textures,cache.allocator); cache.frame_textures=make([]gfx.Texture_Input,len(cache.texture_inputs)+2,cache.allocator) }
    extra_textures:=cache.frame_textures
    copy(extra_textures,cache.texture_inputs)
    extra_textures[len(cache.texture_inputs)]={cache.copied_color,slot.copied_color}
    extra_textures[len(cache.texture_inputs)+1]={cache.linear_color,slot.linear_color}
    cache.frame_inputs[0]={cache.color_staging,slot.color_staging}
    for input,i in cache.inputs { cache.frame_inputs[i+1]=input }
    return {cache.frame_inputs[:],cache.frame_textures},{}
}
