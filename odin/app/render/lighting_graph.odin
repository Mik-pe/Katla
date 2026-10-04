//! Ordinary declarations connect light culling and shadow depth to material stages.
package render

import gfx "../../gfx"
import "core:mem"

Feature_Graph :: struct {
    pipelines:Feature_Pipelines,
    buffers:[5]gfx.Resource_Id,
    buffer_descs:[5]gfx.Buffer_Desc,
    atlas,indicator:gfx.Image_Id,
    atlas_desc,indicator_desc:gfx.Texture_Desc,
    shadows:[2]gfx.Pass_Id,
    grid:gfx.Pass_Id,
    selection:[2][4]gfx.Pass_Id,
    sampler:gfx.Sampler_Handle,
    late_bound:bool,
}
feature_graph_begin :: proc(scene:^Scene_Graph,pipelines:Feature_Pipelines,sampler:gfx.Sampler_Handle,settings:Feature_Settings)->Native_Error {
    f:=&scene.features; f.pipelines=pipelines; f.sampler=sampler
    f.buffer_descs=feature_buffer_descs(scene.color_desc.width,scene.color_desc.height)
    f.atlas_desc,f.indicator_desc=feature_texture_descs(scene.color_desc.width,scene.color_desc.height,settings.shadow_size)[0],feature_texture_descs(scene.color_desc.width,scene.color_desc.height,settings.shadow_size)[1]
    for desc,i in f.buffer_descs { id,error:=gfx.graph_buffer(scene_graph_target(scene),desc,true,false); if error!=.None { return {gpu=.Invalid_Graph} }; f.buffers[i]=id }
    error:gfx.Graph_Error
    f.atlas,error=gfx.graph_image(scene_graph_target(scene),f.atlas_desc,{},false,false); if error!=.None { return {gpu=.Invalid_Graph} }
    f.indicator,error=gfx.graph_image(scene_graph_target(scene),f.indicator_desc,{},false,false); if error!=.None { return {gpu=.Invalid_Graph} }
    for index in ([2]int{2,3}) {
        access:=gfx.Buffer_Access{f.buffers[index],{0,f.buffer_descs[index].size},.Write,.Transfer_Destination}
        pass,pass_error:=scene_graph_pass(scene,"Clear tile light indices" if index==2 else "Clear tile light counts",.Transfer,{access}); if pass_error!=.None { return {gpu=.Invalid_Graph} }
        packet_error:=gfx.graph_set_packet(scene_graph_target(scene),pass,gfx.Fill_Buffer{destination=access.resource,size=access.range.size}); if packet_error!=.None { return {packet=packet_error} }
    }
    bindings:[4]gfx.Buffer_Binding
    accesses:[4]gfx.Buffer_Access
    for i in 0..<4 {
        usage:=gfx.Buffer_Usage.Uniform if i==0 else gfx.Buffer_Usage.Storage
        accesses[i]={f.buffers[i],{0,f.buffer_descs[i].size},.Read if i<2 else .Read_Write,usage}
        bindings[i]={3,u32(i),accesses[i]}
    }
    pass,pass_error:=scene_graph_pass(scene,"Forward tile light culling",.Compute,accesses[:]); if pass_error!=.None { return {gpu=.Invalid_Graph} }
    packet_error:=gfx.graph_set_packet(scene_graph_target(scene),pass,gfx.Dispatch{pipeline=pipelines.cull,groups={(scene.color_desc.width+15)/16,(scene.color_desc.height+15)/16,1},bindings=bindings[:]}); if packet_error!=.None { return {packet=packet_error} }
    for kind in 0..<2 {
        depth:=gfx.Depth_Attachment{enabled=true,access={f.atlas,gfx.image_full_range(f.atlas_desc),.Write if kind==0 else .Read_Write,.Depth_Attachment},load=.Clear if kind==0 else .Load,store=.Store,clear_depth=1}
        f.shadows[kind],error=scene_graph_pass(scene,"Primitive cascaded shadow depth" if kind==0 else "Model cascaded shadow depth",.Graphics,nil,images={depth.access}); if error!=.None { return {gpu=.Invalid_Graph} }
        packet_error=gfx.graph_set_packet(scene_graph_target(scene),f.shadows[kind],gfx.Render{depth=depth}); if packet_error!=.None { return {packet=packet_error} }
    }
    color:=gfx.Color_Attachment{{f.indicator,gfx.image_full_range(f.indicator_desc),.Write,.Color_Attachment},.Clear,.Store,{}}
    clear_pass,clear_error:=scene_graph_pass(scene,"Clear selected occlusion mask",.Graphics,nil,images={color.access}); if clear_error!=.None { return {gpu=.Invalid_Graph} }
    return {packet=gfx.graph_set_packet(scene_graph_target(scene),clear_pass,gfx.Render{colors={color}})}
}
feature_material_buffers :: proc(scene:^Scene_Graph)->[5]gfx.Stage_Buffer_Binding {
    f:=&scene.features
    result:[5]gfx.Stage_Buffer_Binding
    for i in 0..<4 { result[i]={3,u32(i),{.Fragment},{f.buffers[i],{0,f.buffer_descs[i].size},.Read,.Uniform if i==0 else .Storage}} }
    result[4]={4,0,{.Fragment},{f.buffers[4],{0,352},.Read,.Storage}}
    return result
}
feature_material_image :: proc(scene:^Scene_Graph)->gfx.Image_Access { return {scene.features.atlas,gfx.image_full_range(scene.features.atlas_desc),.Read,.Sampled} }
feature_shadow_packet :: proc(scene:^Scene_Graph,kind:int,objects,geometry:gfx.Resource_Id,object_desc,geometry_desc:gfx.Buffer_Desc,draws:[]gfx.Draw_Op,clip_sign:f32,enabled:bool)->Native_Error {
    f:=&scene.features
    depth:=gfx.Depth_Attachment{enabled=true,access={f.atlas,gfx.image_full_range(f.atlas_desc),.Write if kind==0 else .Read_Write,.Depth_Attachment},load=.Clear if kind==0 else .Load,store=.Store,clear_depth=1}
    packet:=gfx.Render{depth=depth}
    buffers:[3]gfx.Stage_Buffer_Binding
    phases:[4]gfx.Render_Phase
    values:[4]struct { index:u32,sign:f32,pad:[2]u32 }
    constants:[4]gfx.Constant_Binding
    accesses:[3]gfx.Buffer_Access
    if enabled && len(draws)>0 {
        buffers={{0,1,{.Vertex},{objects,{0,object_desc.size},.Read,.Storage}},{0,2,{.Vertex},{geometry,{0,geometry_desc.size},.Read,.Storage}},{4,0,{.Vertex},{f.buffers[4],{0,352},.Read,.Storage}}}
        for binding,i in buffers { accesses[i]=binding.access }
        size:=f64(f.atlas_desc.width/2)
        for &phase,i in phases {
            values[i]={u32(i),clip_sign,{}}
            constants[i]={group=4,slot=3,stages={.Vertex},usage=.Uniform,bytes=mem.slice_to_bytes(values[i:i+1])}
            phase={pipeline=f.pipelines.geometry[kind][0],constants=constants[i:i+1],viewport={true,f64(i%2)*size,f64(i/2)*size,size,size,0,1},draws=draws}
        }
        packet.buffers=buffers[:]; packet.phases=phases[:]
    }
    reads:=accesses[:] if len(packet.phases)>0 else nil
    packet_error,graph_error:=gfx.graph_set_commands(scene_graph_target(scene),f.shadows[kind],packet,reads,{depth.access})
    if graph_error!=.None { return {gpu=.Invalid_Graph} }; return {packet=packet_error}
}
