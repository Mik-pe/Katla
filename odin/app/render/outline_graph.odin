//! Selected real geometry marks visible and occluded stencil regions before the final transform.
package render

import gfx "../../gfx"
import km "../../math"
import "core:mem"

Outline_Constants :: struct { parameters,color:km.Vec4 }
feature_selection_packet :: proc(scene:^Scene_Graph,kind,effect:int,frame,objects,geometry:gfx.Resource_Id,frame_desc,object_desc,geometry_desc:gfx.Buffer_Desc,draws:[]gfx.Draw_Op,settings:Feature_Settings)->Native_Error {
    f:=&scene.features
    target:=scene.color if effect!=3 else f.indicator
    target_desc:=scene.color_desc if effect!=3 else f.indicator_desc
    color:=gfx.Color_Attachment{{target,gfx.image_full_range(target_desc),.Read_Write,.Color_Attachment},.Load,.Store,{}}
    depth:=gfx.Depth_Attachment{enabled=true,access={scene.depth,gfx.image_full_range(scene.depth_desc),.Read_Write,.Depth_Attachment},load=.Load,store=.Store}
    packet:=gfx.Render{colors={color},depth=depth}
    bindings:=[3]gfx.Stage_Buffer_Binding{{0,0,{.Vertex},{frame,{0,frame_desc.size},.Read,.Uniform}},{0,1,{.Vertex},{objects,{0,object_desc.size},.Read,.Storage}},{0,2,{.Vertex},{geometry,{0,geometry_desc.size},.Read,.Storage}}}
    values:=[1]Outline_Constants{{{0.004*1080/f32(scene.color_desc.height),0,0,0},{1,0.55,0,1}}}
    phase:=[1]gfx.Render_Phase{{pipeline=f.pipelines.geometry[kind][effect+1],draws=draws}}
    enabled:=settings.outline && (effect!=1 && effect!=3 || settings.wallhack) && len(draws)>0
    accesses:[3]gfx.Buffer_Access
    if enabled {
        packet.buffers=bindings[:]; packet.phases=phase[:]
        for binding,i in bindings { accesses[i]=binding.access }
        if effect==2 { phase[0].constants={{group=5,slot=0,stages={.Vertex,.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(values[:])}} }
    }
    reads:=accesses[:] if enabled else nil
    packet_error,graph_error:=gfx.graph_set_commands(scene_graph_target(scene),f.selection[kind][effect],packet,reads,{color.access,depth.access})
    if graph_error!=.None { return {gpu=.Invalid_Graph} }; return {packet=packet_error}
}
native_features_finish :: proc(scene:^Native_Scene($R),token:gfx.Frame_Token,draws:[]gfx.Draw_Op)->Native_Error {
    f:=&scene.graph.features
    if !f.late_bound {
        error:=scene_graph_extend(&scene.graph); if error!={} { return error }
        pass,graph_error:=scene_graph_pass(&scene.graph,"Editor floor grid",.Graphics,nil); if graph_error!=.None { return {gpu=.Invalid_Graph} }; f.grid=pass
        names:=[2][4]string{{"Primitive selection stencil","Primitive selection occlusion","Primitive selection outline","Primitive selection mask"},{"Model selection stencil","Model selection occlusion","Model selection outline","Model selection mask"}}
        for effect in 0..<4 { for kind in 0..<2 { f.selection[kind][effect],graph_error=scene_graph_pass(&scene.graph,names[kind][effect],.Graphics,nil); if graph_error!=.None { return {gpu=.Invalid_Graph} } } }
        f.late_bound=true
    }
    error:=feature_grid_packet(&scene.graph,scene.feature_settings); if error!={} { return error }
    selected:=[2][dynamic]gfx.Draw_Op{}
    for &array in selected { array=make([dynamic]gfx.Draw_Op,scene.allocator) }
    defer { for array in selected { delete(array) } }
    if scene.batch!=nil {
        for entry,i in scene.batch.entries { for id in scene.selected { if entry.entity==id && i<len(draws) { append(&selected[0],draws[i]); break } } }
    }
    if scene.models!=nil {
        for entry in scene.models.batch.entries { for id in scene.selected { if entry.entity==id { append(&selected[1],gfx.Draw{entry.vertex_count,1,entry.first_vertex,entry.object_index}); break } } }
    }
    for effect in 0..<4 {
        error=feature_selection_packet(&scene.graph,0,effect,scene.graph.frame,scene.graph.objects,scene.graph.geometry,scene.graph.frame_desc,scene.graph.object_desc,scene.graph.geometry_desc,selected[0][:],scene.feature_settings); if error!={} { return error }
        if scene.models!=nil { error=feature_selection_packet(&scene.graph,1,effect,scene.models.frame,scene.models.objects,scene.models.geometry,scene.models.frame_desc,scene.models.object_desc,scene.models.geometry_desc,selected[1][:],scene.feature_settings) }
        else { error=feature_selection_packet(&scene.graph,1,effect,scene.graph.frame,scene.graph.objects,scene.graph.geometry,scene.graph.frame_desc,scene.graph.object_desc,scene.graph.geometry_desc,nil,scene.feature_settings) }
        if error!={} { return error }
    }
    return {}
}
