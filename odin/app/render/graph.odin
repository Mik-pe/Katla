//! Application-authored PBR attachments and ordinary buffer bindings use the generic graph.
package render

import gfx "../../gfx"
import "core:mem"

/// Owns one stationary scene graph; native resource handles remain in the app renderer.
Scene_Graph :: struct {
    graph:gfx.Graph,
    plan:gfx.Compiled_Graph,
    frame,objects,geometry:gfx.Resource_Id,
    color,depth:gfx.Image_Id,
    pass:gfx.Pass_Id,
    frame_desc,object_desc,geometry_desc:gfx.Buffer_Desc,
    color_desc,depth_desc:gfx.Texture_Desc,
    pipeline:gfx.Graphics_Pipeline_Handle,
    allocator:mem.Allocator,
    has_draws:bool,
}
/// Declares the actual scene buffer, color and depth resources without installing backend features.
scene_graph_init :: proc(scene:^Scene_Graph,pipeline:gfx.Graphics_Pipeline_Handle,vertex_count,object_capacity:int,width,height:u32,format:=gfx.Texture_Format.RGBA8_Unorm,allocator:=context.allocator)->Scene_Error {
    if width==0 || height==0 || !(format in (bit_set[gfx.Texture_Format]{.RGBA8_Unorm,.BGRA8_Unorm})) || pipeline.owner==nil { return .Invalid_Geometry }
    frame_desc,object_desc,geometry_desc,ok:=scene_buffer_descs(vertex_count,object_capacity)
    empty:=vertex_count==0 && object_capacity==0
    if !ok && !empty { return .Invalid_Geometry }
    scene.allocator=allocator; scene.pipeline=pipeline
    scene.frame_desc,scene.object_desc,scene.geometry_desc=frame_desc,object_desc,geometry_desc
    scene.color_desc={width=width,height=height,layers=1,mip_levels=1,format=format,usage={.Color_Attachment,.Transfer_Source},depth=1}
    scene.depth_desc={width=width,height=height,layers=1,mip_levels=1,format=.D32_Float,usage={.Depth_Attachment},depth=1}
    gfx.graph_init(&scene.graph,allocator)
    success:=false; defer { if !success { scene_graph_destroy(scene) } }
    error:gfx.Graph_Error
    if !empty {
        scene.frame,error=gfx.graph_buffer(&scene.graph,frame_desc,true,false); if error!=.None { return .Invalid_Geometry }
        scene.objects,error=gfx.graph_buffer(&scene.graph,object_desc,true,false); if error!=.None { return .Invalid_Geometry }
        scene.geometry,error=gfx.graph_buffer(&scene.graph,geometry_desc,true,false); if error!=.None { return .Invalid_Geometry }
    }
    scene.color,error=gfx.graph_image(&scene.graph,scene.color_desc,{},false,true); if error!=.None { return .Invalid_Geometry }
    scene.depth,error=gfx.graph_image(&scene.graph,scene.depth_desc,{},false,false); if error!=.None { return .Invalid_Geometry }
    buffers:=[3]gfx.Buffer_Access{
        {scene.frame,{0,frame_desc.size},.Read,.Uniform},
        {scene.objects,{0,object_desc.size},.Read,.Storage},
        {scene.geometry,{0,geometry_desc.size},.Read,.Storage},
    }
    images:=[2]gfx.Image_Access{
        {scene.color,gfx.image_full_range(scene.color_desc),.Write,.Color_Attachment},
        {scene.depth,gfx.image_full_range(scene.depth_desc),.Write,.Depth_Attachment},
    }
    reads:=buffers[:] if !empty else nil
    scene.pass,error=gfx.graph_pass(&scene.graph,"Scene surfaces",.Graphics,reads,images=images[:]); if error!=.None { return .Invalid_Geometry }
    draws:[]gfx.Draw_Op
    initial:=[1]gfx.Draw_Op{gfx.Draw{u32(vertex_count),1,0,0}}
    if !empty { draws=initial[:] }
    if scene_graph_draws(scene,draws)!=.None { return .Invalid_Geometry }
    scene.plan,error=gfx.graph_compile(&scene.graph); if error!=.None { return .Invalid_Geometry }
    success=true; return .None
}
/// Freezes one actual geometry range and global object slot for each ordered draw.
scene_graph_draws :: proc(scene:^Scene_Graph,draws:[]gfx.Draw_Op)->gfx.Packet_Error {
    vertex_count:=scene.geometry_desc.size/u64(size_of(Vertex)); object_count:=scene.object_desc.size/u64(size_of(Object_Data))
    for operation in draws {
        draw,generated:=operation.(gfx.Draw); if !generated { return .Invalid_Packet }
        if draw.vertex_count==0 || draw.instance_count==0 || u64(draw.first_vertex)>vertex_count || u64(draw.vertex_count)>vertex_count-u64(draw.first_vertex) || u64(draw.first_instance)>object_count || u64(draw.instance_count)>object_count-u64(draw.first_instance) { return .Invalid_Packet }
    }
    color:=[1]gfx.Color_Attachment{{{scene.color,gfx.image_full_range(scene.color_desc),.Write,.Color_Attachment},.Clear,.Store,{0.035,0.04,0.05,1}}}
    depth:=gfx.Depth_Attachment{true,{scene.depth,gfx.image_full_range(scene.depth_desc),.Write,.Depth_Attachment},.Clear,.Store,1,0}
    buffers:=[3]gfx.Stage_Buffer_Binding{
        {0,0,{.Vertex,.Fragment},{scene.frame,{0,scene.frame_desc.size},.Read,.Uniform}},
        {0,1,{.Vertex,.Fragment},{scene.objects,{0,scene.object_desc.size},.Read,.Storage}},
        {0,2,{.Vertex},{scene.geometry,{0,scene.geometry_desc.size},.Read,.Storage}},
    }
    phases:=[1]gfx.Render_Phase{{pipeline=scene.pipeline,draws=draws}}
    packet:=gfx.Render{colors=color[:],depth=depth,buffers=buffers[:],phases=phases[:]}
    has_draws:=len(draws)>0
    if !has_draws { packet.buffers=nil; packet.phases=nil }
    if scene.has_draws==has_draws { return gfx.graph_set_packet(&scene.graph,scene.pass,packet) }
    accesses:=[3]gfx.Buffer_Access{buffers[0].access,buffers[1].access,buffers[2].access}
    images:=[2]gfx.Image_Access{color[0].access,depth.access}
    reads:=accesses[:] if has_draws else nil
    packet_error,graph_error:=gfx.graph_set_commands(&scene.graph,scene.pass,packet,reads,images[:])
    if packet_error!=.None { return packet_error }
    if graph_error!=.None { return .Undeclared_Access }
    scene.has_draws=has_draws
    if scene.plan.owner!=nil {
        gfx.compiled_graph_destroy(&scene.plan)
        scene.plan,graph_error=gfx.graph_compile(&scene.graph)
        if graph_error!=.None { return .Invalid_Plan }
    }
    return .None
}
/// Releases the graph plan and owned declarations after native submissions retain their resources.
scene_graph_destroy :: proc(scene:^Scene_Graph) { gfx.compiled_graph_destroy(&scene.plan); gfx.graph_destroy(&scene.graph); scene^={} }
/// Selects one acquired frame slot's actual allocations without rewriting earlier submissions.
scene_graph_inputs :: proc(scene:^Scene_Graph,frame,objects,geometry:gfx.Buffer_Handle)->[3]gfx.Buffer_Input {
    return {{scene.frame,frame},{scene.objects,objects},{scene.geometry,geometry}}
}
/// Selects the slot's independent color/depth allocations for the same graph image roles.
scene_graph_textures :: proc(scene:^Scene_Graph,color,depth:gfx.Texture_Handle)->[2]gfx.Texture_Input { return {{scene.color,color},{scene.depth,depth}} }
