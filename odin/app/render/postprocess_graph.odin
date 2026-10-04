//! Final display packets consume the newest complete HDR image.
package render

import gfx "../../gfx"
import "core:mem"

Display_Settings :: struct { exposure:f32, mode:u32, padding:[2]u32 }
/// Removes only the final pass so feature owners can append before the display transform.
scene_graph_extend :: proc(scene:^Scene_Graph)->Native_Error {
    if scene.display_pass.owner==nil { return {} }
    gfx.compiled_graph_destroy(&scene.plan)
    error:=gfx.graph_remove_last_pass(scene_graph_target(scene),scene.display_pass)
    if error!=.None { return {gpu=.Invalid_Graph} }
    scene.display_pass={}; return {}
}
/// Freezes the authored transform and appends it after every linear scene writer.
scene_graph_finalize :: proc(scene:^Scene_Graph)->Native_Error {
    if !postprocess_valid(scene.postprocess) || scene.display_pipeline.owner==nil { return {scene=.Invalid_Material} }
    source:=gfx.Image_Access{scene.color,gfx.image_full_range(scene.color_desc),.Read,.Sampled}
    target:=gfx.Color_Attachment{{scene.output,gfx.image_full_range(scene.output_desc),.Write,.Color_Attachment},.Clear,.Store,{}}
    values:=[1]Display_Settings{{scene.postprocess.exposure,u32(scene.postprocess.operator),{u32(int(scene.wallhack)),0}}}
    packet:=gfx.Render{colors={target},images={{group=0,slot=0,stages={.Fragment},accesses={source}},{group=0,slot=2,stages={.Fragment},accesses={{scene.features.indicator,gfx.image_full_range(scene.features.indicator_desc),.Read,.Sampled}}}},constants={{group=0,slot=1,stages={.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(values[:])}},phases={{pipeline=scene.display_pipeline,draws={gfx.Draw{3,1,0,0}}}}}
    if scene.display_pass.owner==nil {
        pass,error:=scene_graph_pass(scene,"Scene display transform",.Graphics,nil,images={source,target.access,{scene.features.indicator,gfx.image_full_range(scene.features.indicator_desc),.Read,.Sampled}})
        if error!=.None { return {gpu=.Invalid_Graph} }; scene.display_pass=pass
    }
    packet_error:=gfx.graph_set_packet(scene_graph_target(scene),scene.display_pass,packet)
    if packet_error!=.None { return {packet=packet_error} }
    if scene.target==nil && (scene.plan.owner==nil || scene.plan.revision!=scene_graph_target(scene).revision) {
        plan,error:=gfx.graph_compile(scene_graph_target(scene)); if error!=.None { return {gpu=.Invalid_Graph} }
        gfx.compiled_graph_destroy(&scene.plan); scene.plan=plan
    }
    return {}
}
