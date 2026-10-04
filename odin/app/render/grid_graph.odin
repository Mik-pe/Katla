//! Forty-two real floor line instances use two depth-tested geometry draws.
package render

import gfx "../../gfx"
import km "../../math"
import "core:mem"

Grid_Constants :: struct { parameters,color:km.Vec4 }
feature_grid_packet :: proc(scene:^Scene_Graph,settings:Feature_Settings)->Native_Error {
    f:=&scene.features
    color:=gfx.Color_Attachment{{scene.color,gfx.image_full_range(scene.color_desc),.Read_Write,.Color_Attachment},.Load,.Store,{}}
    depth:=gfx.Depth_Attachment{enabled=true,access={scene.depth,gfx.image_full_range(scene.depth_desc),.Read_Write,.Depth_Attachment},load=.Load,store=.Store}
    packet:=gfx.Render{colors={color},depth=depth}
    constants:=[2]Grid_Constants{{{settings.ground_height,0.008,0,0},{0.10,0.11,0.13,1}},{{settings.ground_height,0.008,1,0},{0.16,0.17,0.20,1}}}
    phases:[2]gfx.Render_Phase
    bindings:[2]gfx.Constant_Binding
    draws:=[2]gfx.Draw_Op{gfx.Draw{6,36,0,0},gfx.Draw{6,6,0,0}}
    binding:=gfx.Stage_Buffer_Binding{0,0,{.Vertex},{scene.frame,{0,128},.Read,.Uniform}}
    if settings.grid {
        for &phase,i in phases {
            bindings[i]={group=5,slot=0,stages={.Vertex,.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(constants[i:i+1])}
            phase={pipeline=f.pipelines.grid,constants=bindings[i:i+1],draws=draws[i:i+1]}
        }
        packet.buffers={binding}; packet.phases=phases[:]
    }
    accesses:[]gfx.Buffer_Access; if settings.grid { accesses={binding.access} }
    packet_error,graph_error:=gfx.graph_set_commands(scene_graph_target(scene),f.grid,packet,accesses,{color.access,depth.access})
    if graph_error!=.None { return {gpu=.Invalid_Graph} }; return {packet=packet_error}
}
