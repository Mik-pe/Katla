//! The application owns scene resources and composes ordinary GPU operations per acquired slot.
package render

import gfx "../../gfx"
import "core:mem"

/// Generic GPU operations are explicit; scene/material policy stays in this application owner.
GPU_Ops :: struct($Renderer:typeid) {
    create_pipeline:proc(^Renderer,gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error),
    destroy_pipeline:proc(^Renderer,gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error,
    create_buffer:proc(^Renderer,gfx.Buffer_Desc,[]byte)->(gfx.Buffer_Handle,gfx.Gpu_Error),
    destroy_buffer:proc(^Renderer,gfx.Buffer_Handle)->gfx.Gpu_Error,
    write_buffer:proc(^Renderer,gfx.Frame_Token,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error,
    create_texture:proc(^Renderer,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error),
    destroy_texture:proc(^Renderer,gfx.Texture_Handle)->gfx.Gpu_Error,
    acquire:proc(^Renderer)->(gfx.Frame_Token,gfx.Gpu_Error),
    abort:proc(^Renderer,gfx.Frame_Token)->gfx.Gpu_Error,
    submit:proc(^Renderer,gfx.Frame_Token,^gfx.Graph,^gfx.Compiled_Graph,[]gfx.Buffer_Input,[]gfx.Texture_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error),
    wait:proc(^Renderer,gfx.Submission)->gfx.Gpu_Error,
    release_exports:proc(^Renderer,^gfx.Graph)->gfx.Gpu_Error,
}
/// One application-owned allocation for every mutable resource in every native frame slot.
Native_Slot :: struct { frame,objects:gfx.Buffer_Handle, color,depth:gfx.Texture_Handle }
/// A stationary, exclusive scene consumer retains native resources and accepted submission order.
Native_Scene :: struct($Renderer:typeid) {
    renderer:^Renderer,
    operations:GPU_Ops(Renderer),
    graph:Scene_Graph,
    slots:[]Native_Slot,
    geometry:gfx.Buffer_Handle,
    pipeline:gfx.Graphics_Pipeline_Handle,
    pending:[dynamic]gfx.Submission,
    allocator:mem.Allocator,
}
/// Keeps GPU, graph and CPU input failures distinct for host diagnostics.
Native_Error :: struct { gpu:gfx.Gpu_Error, packet:gfx.Packet_Error, scene:Scene_Error }
/// Prepares native scene resources without compiling anything on the frame path.
native_scene_init :: proc(scene:^Native_Scene($R),renderer:^R,operations:GPU_Ops(R),descriptor:gfx.Graphics_Desc,geometry:^Geometry,object_capacity,slot_count:int,width,height:u32,allocator:mem.Allocator=context.allocator)->Native_Error {
    if renderer==nil || slot_count<1 || slot_count>16 || len(descriptor.colors)!=1 || (len(geometry.vertices)==0 && object_capacity!=0) { return {scene=.Invalid_Geometry} }
    if operations.create_pipeline==nil || operations.destroy_pipeline==nil || operations.create_buffer==nil || operations.destroy_buffer==nil || operations.write_buffer==nil || operations.create_texture==nil || operations.destroy_texture==nil || operations.acquire==nil || operations.abort==nil || operations.submit==nil || operations.wait==nil || operations.release_exports==nil { return {gpu=.Unsupported} }
    scene.renderer=renderer; scene.operations=operations; scene.allocator=allocator
    scene.pending=make([dynamic]gfx.Submission,0,slot_count,allocator)
    success:=false; defer { if !success { native_scene_destroy(scene) } }
    error:gfx.Gpu_Error
    scene.pipeline,error=operations.create_pipeline(renderer,descriptor); if error!=.None { return {gpu=error} }
    scene_error:=scene_graph_init(&scene.graph,scene.pipeline,len(geometry.vertices),object_capacity,width,height,descriptor.colors[0].format,allocator)
    if scene_error!=.None { return {scene=scene_error} }
    vertex_bytes:=mem.slice_to_bytes(geometry.vertices)
    if len(vertex_bytes)>0 { scene.geometry,error=operations.create_buffer(renderer,scene.graph.geometry_desc,vertex_bytes); if error!=.None { return {gpu=error} } }
    scene.slots=make([]Native_Slot,slot_count,allocator)
    zero_frame:=make([]byte,int(scene.graph.frame_desc.size),allocator); defer delete(zero_frame,allocator)
    zero_objects:=make([]byte,int(scene.graph.object_desc.size),allocator); defer delete(zero_objects,allocator)
    for &slot in scene.slots {
        if len(vertex_bytes)>0 {
            slot.frame,error=operations.create_buffer(renderer,scene.graph.frame_desc,zero_frame); if error!=.None { return {gpu=error} }
            slot.objects,error=operations.create_buffer(renderer,scene.graph.object_desc,zero_objects); if error!=.None { return {gpu=error} }
        }
        slot.color,error=operations.create_texture(renderer,scene.graph.color_desc); if error!=.None { return {gpu=error} }
        slot.depth,error=operations.create_texture(renderer,scene.graph.depth_desc); if error!=.None { return {gpu=error} }
    }
    success=true; return {}
}
/// Retires an exact accepted scene submission, preserving other queued frame owners.
native_scene_wait :: proc(scene:^Native_Scene($R),submission:gfx.Submission)->gfx.Gpu_Error {
    found_index:= -1
    for accepted,i in scene.pending { if accepted==submission { found_index=i; break } }
    if found_index<0 { return .Invalid_Resource }
    error:=scene.operations.wait(scene.renderer,submission)
    if error==.None { ordered_remove(&scene.pending,found_index) }
    return error
}
/// Acquires the next exact native slot, retiring only the scene's oldest accepted owner when busy.
native_scene_acquire :: proc(scene:^Native_Scene($R))->(gfx.Frame_Token,gfx.Gpu_Error) {
    if scene.renderer==nil { return {},.Invalid_Resource }
    token,error:=scene.operations.acquire(scene.renderer)
    if error==.Busy && len(scene.pending)>0 {
        error=native_scene_wait(scene,scene.pending[0]); if error!=.None { return {},error }
        token,error=scene.operations.acquire(scene.renderer)
    }
    return token,error
}
/// Writes the acquired slot's buffers, freezes actual draws and consumes the token on accepted submission.
/// Failure preserves the acquisition for host retry or explicit abort, including surface cleanup.
native_scene_render :: proc(scene:^Native_Scene($R),token:gfx.Frame_Token,frame:Frame_Data,objects:[]Object_Data,draws:[]gfx.Draw_Op,color_override:=gfx.Texture_Handle{})->(gfx.Submission,Native_Error) {
    if scene.renderer==nil || u64(len(objects))>scene.graph.object_desc.size/u64(size_of(Object_Data)) { return {},{scene=.Invalid_Geometry} }
    for operation in draws { draw,generated:=operation.(gfx.Draw); if !generated { return {},{scene=.Invalid_Geometry} }; if u64(draw.first_instance)>u64(len(objects)) || u64(draw.instance_count)>u64(len(objects))-u64(draw.first_instance) { return {},{scene=.Invalid_Geometry} } }
    packet_error:=scene_graph_draws(&scene.graph,draws); if packet_error!=.None { return {},{packet=packet_error} }
    error:gfx.Gpu_Error
    if token.slot<0 || token.slot>=len(scene.slots) { return {},{gpu=.Invalid_Resource} }
    slot:=scene.slots[token.slot]
    buffers:=scene_graph_inputs(&scene.graph,slot.frame,slot.objects,scene.geometry)
    inputs:[]gfx.Buffer_Input
    if len(draws)>0 {
        frame_values:=[1]Frame_Data{frame}
        error=scene.operations.write_buffer(scene.renderer,token,slot.frame,0,mem.slice_to_bytes(frame_values[:])); if error!=.None { return {},{gpu=error} }
        error=scene.operations.write_buffer(scene.renderer,token,slot.objects,0,mem.slice_to_bytes(objects)); if error!=.None { return {},{gpu=error} }
        inputs=buffers[:]
    }
    color:=slot.color; if color_override.owner!=nil { color=color_override }
    textures:=scene_graph_textures(&scene.graph,color,slot.depth)
    submission:gfx.Submission
    submission,error,packet_error=scene.operations.submit(scene.renderer,token,&scene.graph.graph,&scene.graph.plan,inputs,textures[:])
    if error!=.None || packet_error!=.None { return {},{gpu=error,packet=packet_error} }
    append(&scene.pending,submission)
    return submission,{}
}
/// Drains accepted work before releasing scene-owned handles; the native renderer remains caller-owned.
native_scene_destroy :: proc(scene:^Native_Scene($R))->gfx.Gpu_Error {
    error:=gfx.Gpu_Error.None
    if scene.renderer!=nil {
        for len(scene.pending)>0 {
            wait_error:=native_scene_wait(scene,scene.pending[0]); if wait_error!=.None { error=wait_error; break }
        }
        if scene.operations.release_exports!=nil { release_error:=scene.operations.release_exports(scene.renderer,&scene.graph.graph); if release_error!=.None { error=release_error } }
        for slot in scene.slots {
            if slot.frame.owner!=nil { destroy_error:=scene.operations.destroy_buffer(scene.renderer,slot.frame); if destroy_error!=.None { error=destroy_error } }
            if slot.objects.owner!=nil { destroy_error:=scene.operations.destroy_buffer(scene.renderer,slot.objects); if destroy_error!=.None { error=destroy_error } }
            if slot.color.owner!=nil { destroy_error:=scene.operations.destroy_texture(scene.renderer,slot.color); if destroy_error!=.None { error=destroy_error } }
            if slot.depth.owner!=nil { destroy_error:=scene.operations.destroy_texture(scene.renderer,slot.depth); if destroy_error!=.None { error=destroy_error } }
        }
        if scene.geometry.owner!=nil { destroy_error:=scene.operations.destroy_buffer(scene.renderer,scene.geometry); if destroy_error!=.None { error=destroy_error } }
        if scene.pipeline.owner!=nil { destroy_error:=scene.operations.destroy_pipeline(scene.renderer,scene.pipeline); if destroy_error!=.None { error=destroy_error } }
    }
    scene_graph_destroy(&scene.graph); delete(scene.slots,scene.allocator); delete(scene.pending); scene^={}
    return error
}
/// Replaces scene attachments before acquiring another frame, preserving separately retained readback tickets.
native_scene_resize :: proc(scene:^Native_Scene($R),width,height:u32)->Native_Error {
    if scene.renderer==nil || width==0 || height==0 { return {scene=.Invalid_Geometry} }
    if width==scene.graph.color_desc.width && height==scene.graph.color_desc.height { return {} }
    for len(scene.pending)>0 { error:=native_scene_wait(scene,scene.pending[0]); if error!=.None { return {gpu=error} } }
    replacements:=make([][2]gfx.Texture_Handle,len(scene.slots),scene.allocator)
    allocator:=scene.allocator; defer delete(replacements,allocator)
    color_desc,depth_desc:=scene.graph.color_desc,scene.graph.depth_desc
    color_desc.width,color_desc.height=width,height; depth_desc.width,depth_desc.height=width,height
    installed:=false; defer {
        if !installed {
            for pair in replacements {
                for handle in pair { if handle.owner!=nil { scene.operations.destroy_texture(scene.renderer,handle) } }
            }
        }
    }
    for &pair in replacements {
        error:gfx.Gpu_Error
        pair[0],error=scene.operations.create_texture(scene.renderer,color_desc); if error!=.None { return {gpu=error} }
        pair[1],error=scene.operations.create_texture(scene.renderer,depth_desc); if error!=.None { return {gpu=error} }
    }
    vertex_count:=int(scene.graph.geometry_desc.size/u64(size_of(Vertex)))
    object_count:=int(scene.graph.object_desc.size/u64(size_of(Object_Data)))
    release_error:=scene.operations.release_exports(scene.renderer,&scene.graph.graph); if release_error!=.None { return {gpu=release_error} }
    scene_graph_destroy(&scene.graph)
    scene_error:=scene_graph_init(&scene.graph,scene.pipeline,vertex_count,object_count,width,height,color_desc.format,allocator)
    if scene_error!=.None { return {scene=scene_error} }
    for &slot,i in scene.slots {
        scene.operations.destroy_texture(scene.renderer,slot.color); scene.operations.destroy_texture(scene.renderer,slot.depth)
        slot.color,slot.depth=replacements[i][0],replacements[i][1]
    }
    installed=true; return {}
}
