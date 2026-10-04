//! Application-authored PBR attachments and ordinary buffer bindings use the generic graph.
package render

import gfx "../../gfx"
import "core:mem"
import "core:strings"
import "core:fmt"

/// Owns one stationary scene graph; native resource handles remain in the app renderer.
Scene_Graph :: struct {
    graph:gfx.Graph,
    target:^gfx.Graph,
    namespace:string,
    plan:gfx.Compiled_Graph,
    frame,objects,geometry:gfx.Resource_Id,
    color,depth,output:gfx.Image_Id,
    pass,display_pass:gfx.Pass_Id,
    frame_desc,object_desc,geometry_desc:gfx.Buffer_Desc,
    color_desc,depth_desc,output_desc:gfx.Texture_Desc,
    pipeline,display_pipeline:gfx.Graphics_Pipeline_Handle,
    postprocess:Postprocess_Settings,
    allocator:mem.Allocator,
    has_draws,wallhack:bool,
    depth_sense:Depth_Sense,
    features:Feature_Graph,
}
/// Declares the actual scene buffer, color and depth resources without installing backend features.
scene_graph_init :: proc(scene:^Scene_Graph,pipeline,display_pipeline:gfx.Graphics_Pipeline_Handle,vertex_count,object_capacity:int,width,height:u32,format:=gfx.Texture_Format.RGBA8_Unorm,allocator:=context.allocator,features:Feature_Pipelines={},sampler:gfx.Sampler_Handle={},settings:Feature_Settings=FEATURE_SETTINGS_DEFAULT,depth_sense:Depth_Sense=.Forward)->Scene_Error {
    gfx.graph_init(&scene.graph,allocator)
    error:=scene_graph_append(scene,&scene.graph,"",pipeline,display_pipeline,vertex_count,object_capacity,width,height,format,allocator,features,sampler,settings,depth_sense)
    if error!=.None { scene_graph_destroy(scene) }
    return error
}
/// Appends one view directly into a caller-owned graph; each view keeps distinct camera and image roles.
scene_graph_append :: proc(scene:^Scene_Graph,destination:^gfx.Graph,namespace:string,pipeline,display_pipeline:gfx.Graphics_Pipeline_Handle,vertex_count,object_capacity:int,width,height:u32,format:gfx.Texture_Format,allocator:mem.Allocator,features:Feature_Pipelines,sampler:gfx.Sampler_Handle,settings:Feature_Settings,depth_sense:Depth_Sense=.Forward)->Scene_Error {
    if width==0 || height==0 || !(format in (bit_set[gfx.Texture_Format]{.RGBA8_Unorm,.BGRA8_Unorm})) || pipeline.owner==nil { return .Invalid_Geometry }
    frame_desc,object_desc,geometry_desc,ok:=scene_buffer_descs(vertex_count,object_capacity)
    empty:=vertex_count==0 && object_capacity==0
    if !ok && !empty { return .Invalid_Geometry }
    scene.depth_sense=depth_sense; scene.allocator=allocator; scene.pipeline=pipeline; scene.display_pipeline=display_pipeline; scene.postprocess=postprocess_default()
    if empty { frame_desc={size=128,usage={.Uniform},memory=.CPU_Visible} }
    scene.frame_desc,scene.object_desc,scene.geometry_desc=frame_desc,object_desc,geometry_desc
    scene.color_desc={width=width,height=height,layers=1,mip_levels=1,format=.RGBA16_Float,usage={.Color_Attachment,.Transfer_Source,.Sampled},depth=1}
    scene.depth_desc={width=width,height=height,layers=1,mip_levels=1,format=.D32_Float_S8_Uint,usage={.Depth_Attachment,.Transfer_Source},depth=1}
    if destination==nil { return .Invalid_Geometry }
    scene.target=destination if destination!=&scene.graph else nil
    scene.namespace=strings.clone(namespace,allocator)
    pass_count,buffer_count,image_count:=len(destination.passes),len(destination.buffers),len(destination.images)
    success:=false; defer { if !success { gfx.graph_truncate(destination,pass_count,buffer_count,image_count); delete(scene.namespace,allocator); scene.namespace="" } }
    error:gfx.Graph_Error
    scene.frame,error=gfx.graph_buffer(scene_graph_target(scene),frame_desc,true,false); if error!=.None { return .Invalid_Geometry }
    if !empty {
        scene.objects,error=gfx.graph_buffer(scene_graph_target(scene),object_desc,true,false); if error!=.None { return .Invalid_Geometry }
        scene.geometry,error=gfx.graph_buffer(scene_graph_target(scene),geometry_desc,true,false); if error!=.None { return .Invalid_Geometry }
    }
    scene.color,error=gfx.graph_image(scene_graph_target(scene),scene.color_desc,{},false,false); if error!=.None { return .Invalid_Geometry }
    scene.output_desc=scene.color_desc; scene.output_desc.format=format; scene.output_desc.usage={.Color_Attachment,.Transfer_Source,.Sampled}
    scene.output,error=gfx.graph_image(scene_graph_target(scene),scene.output_desc,{},false,true); if error!=.None { return .Invalid_Geometry }
    scene.depth,error=gfx.graph_image(scene_graph_target(scene),scene.depth_desc,{},false,false); if error!=.None { return .Invalid_Geometry }
    feature_error:=feature_graph_begin(scene,features,sampler,settings); if feature_error!={} { return .Invalid_Geometry }
    buffers:=[3]gfx.Buffer_Access{
        {scene.frame,{0,frame_desc.size},.Read,.Uniform},
        {scene.objects,{0,object_desc.size},.Read,.Storage},
        {scene.geometry,{0,geometry_desc.size},.Read,.Storage},
    }
    images:=[2]gfx.Image_Access{
        {scene.color,gfx.image_full_range(scene.color_desc),.Write,.Color_Attachment},
        {scene.depth,gfx.image_full_range(scene.depth_desc),.Write,.Depth_Attachment},
    }
    reads:=buffers[:] if !empty else buffers[:1]
    scene.pass,error=scene_graph_pass(scene,"Scene surfaces",.Graphics,reads,images=images[:]); if error!=.None { return .Invalid_Geometry }
    draws:[]gfx.Draw_Op
    initial:=[1]gfx.Draw_Op{gfx.Draw{u32(vertex_count),1,0,0}}
    if !empty { draws=initial[:] }
    if scene_graph_draws(scene,draws)!=.None { return .Invalid_Geometry }
    if scene.display_pipeline.owner!=nil { display_error:=scene_graph_finalize(scene); if display_error!={} { return .Invalid_Geometry } }
    if scene.target==nil && scene.plan.owner==nil { scene.plan,error=gfx.graph_compile(scene_graph_target(scene)); if error!=.None { return .Invalid_Geometry } }
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
    depth:=gfx.Depth_Attachment{true,{scene.depth,gfx.image_full_range(scene.depth_desc),.Write,.Depth_Attachment},.Clear,.Store,f64(depth_clear(scene.depth_sense)),0}
    buffers:=[3]gfx.Stage_Buffer_Binding{
        {0,0,{.Vertex,.Fragment},{scene.frame,{0,scene.frame_desc.size},.Read,.Uniform}},
        {0,1,{.Vertex,.Fragment},{scene.objects,{0,scene.object_desc.size},.Read,.Storage}},
        {0,2,{.Vertex},{scene.geometry,{0,scene.geometry_desc.size},.Read,.Storage}},
    }
    phases:=[2]gfx.Render_Phase{{pipeline=scene.features.pipelines.sky,draws={gfx.Draw{3,1,0,0}}},{pipeline=scene.pipeline,draws=draws}}
    bound:=make([dynamic]gfx.Stage_Buffer_Binding,scene.allocator); defer delete(bound)
    if len(draws)==0 { buffers[0].stages={.Fragment} }
    append(&bound,buffers[0])
    lighting:=feature_material_buffers(scene)
    append(&bound,lighting[0])
    has_draws:=len(draws)>0
    if has_draws { append(&bound,..buffers[1:]); append(&bound,..lighting[1:]) }
    sky_ready:=scene.features.pipelines.sky.owner!=nil
    packet:=gfx.Render{colors=color[:],depth=depth,buffers=bound[:],phases=phases[:2] if has_draws else phases[:1]}
    if !sky_ready { packet.buffers=buffers[:] if has_draws else nil; packet.phases=phases[1:] if has_draws else nil }
    shadow:=feature_material_image(scene)
    if has_draws && sky_ready { packet.images={{group=4,slot=1,stages={.Fragment},accesses={shadow}}}; packet.samplers={{4,2,{.Fragment},scene.features.sampler}} }
    if scene.has_draws==has_draws && scene_graph_target(scene).passes[scene.pass.index].has_packet { return gfx.graph_set_packet(scene_graph_target(scene),scene.pass,packet) }
    accesses:=make([dynamic]gfx.Buffer_Access,scene.allocator); defer delete(accesses)
    for binding in packet.buffers { append(&accesses,binding.access) }
    images:=make([dynamic]gfx.Image_Access,scene.allocator); defer delete(images)
    append(&images,color[0].access,depth.access)
    if has_draws && sky_ready { append(&images,shadow) }
    packet_error,graph_error:=gfx.graph_set_commands(scene_graph_target(scene),scene.pass,packet,accesses[:],images[:])
    if packet_error!=.None { return packet_error }
    if graph_error!=.None { return .Undeclared_Access }
    scene.has_draws=has_draws
    if scene.plan.owner!=nil {
        gfx.compiled_graph_destroy(&scene.plan)
        scene.plan,graph_error=gfx.graph_compile(scene_graph_target(scene))
        if graph_error!=.None { return .Invalid_Plan }
    }
    return .None
}
/// Releases the graph plan and owned declarations after native submissions retain their resources.
scene_graph_destroy :: proc(scene:^Scene_Graph) { gfx.compiled_graph_destroy(&scene.plan); gfx.graph_destroy(&scene.graph); delete(scene.namespace,scene.allocator); scene^={} }
/// Returns the one authored graph receiving this view's declarations and packets.
scene_graph_target :: proc(scene:^Scene_Graph)->^gfx.Graph { return scene.target if scene.target!=nil else &scene.graph }
@(private="package")
scene_graph_pass :: proc(scene:^Scene_Graph,name:string,queue:gfx.Pass_Kind,accesses:[]gfx.Buffer_Access,side_effect:bool=false,images:[]gfx.Image_Access=nil)->(gfx.Pass_Id,gfx.Graph_Error) {
    if scene.namespace=="" { return gfx.graph_pass(scene_graph_target(scene),name,queue,accesses,side_effect,images) }
    combined:=fmt.aprintf("%s: %s",scene.namespace,name); defer delete(combined)
    return gfx.graph_pass(scene_graph_target(scene),combined,queue,accesses,side_effect,images)
}
/// Selects one acquired frame slot's actual allocations without rewriting earlier submissions.
scene_graph_inputs :: proc(scene:^Scene_Graph,frame,objects,geometry:gfx.Buffer_Handle)->[3]gfx.Buffer_Input {
    return {{scene.frame,frame},{scene.objects,objects},{scene.geometry,geometry}}
}
/// Selects the slot's independent color/depth allocations for the same graph image roles.
scene_graph_textures :: proc(scene:^Scene_Graph,color,depth,output:gfx.Texture_Handle)->[3]gfx.Texture_Input { return {{scene.color,color},{scene.depth,depth},{scene.output,output}} }
