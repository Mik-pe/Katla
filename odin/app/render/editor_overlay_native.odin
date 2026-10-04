//! Immutable overlay uploads retain actual glyph textures and vertices through generic native recording lifetime.
package render

import gfx "../../gfx"
import font "../../deps/font_native"
import "core:mem"

/// Reset once when the host truncates/rebuilds its combined graph; all views share these imported icon roles.
Overlay_Graph :: struct { graph:^gfx.Graph,owner:rawptr,images:[2]gfx.Image_Id }
Overlay_Native :: struct($R:typeid) { renderer:^R,ops:UI_GPU_Ops(R),pipelines:[3]gfx.Graphics_Pipeline_Handle,glyphs:[2]gfx.Texture_Handle,sampler:gfx.Sampler_Handle,frames:int,allocator:mem.Allocator }
Overlay_Frame :: struct { owner:rawptr,vertices,identity:gfx.Buffer_Handle,resource:gfx.Resource_Id,desc:gfx.Buffer_Desc,counted:bool }
@(private="package")
overlay_glyph_pixels :: proc(fonts:^UI_Font_System,text:string,allocator:mem.Allocator)->([]byte,UI_Error) {
    if fonts==nil || fonts.engine==nil { return nil,.Invalid_Font }
    layout:=ui_shape(fonts,UI_FONT_ICONS,text,64,0); if layout==nil { return nil,.Invalid_Text }; defer fonts.api.layout_destroy(layout)
    count:uint; glyphs:=fonts.api.glyphs(layout,&count)
    if count!=1 || glyphs[0].glyph==0 || glyphs[0].font!=u32(UI_FONT_ICONS) { return nil,.Invalid_Font }
    bitmap:font.Bitmap
    size:=f32(64)
    for _ in 0..<4 {
        if fonts.api.raster(fonts.engine,u32(UI_FONT_ICONS),glyphs[0].glyph,size,&bitmap)==0 { return nil,.Invalid_Font }
        bound:=max(bitmap.width,bitmap.height); if bound<=56 { break }; size*=56/f32(bound)
    }
    if bitmap.width==0 || bitmap.height==0 || bitmap.width>56 || bitmap.height>56 || bitmap.pixels==nil { return nil,.Invalid_Font }
    pixels:=make([]byte,64*64*4,allocator); left,top:=(64-bitmap.width)/2,(64-bitmap.height)/2
    for y in 0..<bitmap.height { for x in 0..<bitmap.width {
        source:=int(y)*int(bitmap.pitch)+int(x); if bitmap.pitch<0 { source=int(bitmap.height-1-y)*(-int(bitmap.pitch))+int(x) }
        offset:=int(((top+y)*64+left+x)*4); pixels[offset]=255; pixels[offset+1]=255; pixels[offset+2]=255; pixels[offset+3]=bitmap.pixels[source]
    } }
    return pixels,.None
}
/// Rasterizes real ForkAwesome glyphs before frames and creates independent immutable native owners.
overlay_native_init :: proc(owner:^Overlay_Native($R),renderer:^R,ops:UI_GPU_Ops(R),compiled:^Overlay_Shader,fonts:^UI_Font_System,allocator:=context.allocator)->Native_Error {
    if renderer==nil || compiled==nil || ops.create_pipeline==nil || ops.destroy_pipeline==nil || ops.create_buffer==nil || ops.destroy_buffer==nil || ops.create_texture==nil || ops.destroy_texture==nil || ops.create_sampler==nil || ops.destroy_sampler==nil { return {gpu=.Unsupported} }
    owner^={renderer=renderer,ops=ops,allocator=allocator}; success:=false; defer { if !success { overlay_native_destroy(owner) } }
    for &handle,i in owner.pipelines[:2] { error:gfx.Gpu_Error; handle,error=ops.create_pipeline(renderer,compiled.mappings[i].descriptor); if error!=.None { return {gpu=error} } }
    error:gfx.Gpu_Error
    owner.pipelines[2],error=ops.create_pipeline(renderer,depth_descriptor(compiled.mappings[0].descriptor,.Reverse)); if error!=.None { return {gpu=error} }
    owner.sampler,error=ops.create_sampler(renderer,{min_filter=.Linear,mag_filter=.Linear,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1}); if error!=.None { return {gpu=error} }
    for text,i in ([2]string{"\uF0EB","\uF06D"}) {
        pixels,font_error:=overlay_glyph_pixels(fonts,text,allocator); if font_error!=.None { return {scene=.Invalid_Geometry} }; defer delete(pixels,allocator)
        owner.glyphs[i],error=ops.create_texture(renderer,overlay_glyph_desc(),pixels); if error!=.None { return {gpu=error} }
    }
    success=true; return {}
}
@(private="package")
overlay_glyph_desc :: proc()->gfx.Texture_Desc { return {width=64,height=64,depth=1,layers=1,mip_levels=1,format=.RGBA8_Unorm,usage={.Sampled,.Transfer_Destination}} }
/// Public parents can be removed after submit; native packets retain every accepted vertex owner.
overlay_frame_destroy :: proc(owner:^Overlay_Native($R),frame:^Overlay_Frame)->gfx.Gpu_Error {
    if frame.owner==nil { return .None }; if frame.owner!=owner { return .Invalid_Resource }
    if frame.identity.owner!=nil { error:=owner.ops.destroy_buffer(owner.renderer,frame.identity); if error!=.None { return error }; frame.identity={} }
    if frame.vertices.owner!=nil { error:=owner.ops.destroy_buffer(owner.renderer,frame.vertices); if error!=.None { return error } }
    if frame.counted { assert(owner.frames>0); owner.frames-=1 }; frame^={}; return .None
}
/// Drained or independently retained submissions govern actual native release.
overlay_native_destroy :: proc(owner:^Overlay_Native($R))->gfx.Gpu_Error {
    if owner.frames>0 { return .Busy }
    if owner.renderer==nil { return .None }
    for &handle in owner.glyphs { if handle.owner!=nil { error:=owner.ops.destroy_texture(owner.renderer,handle); if error!=.None { return error }; handle={} } }
    for &handle in owner.pipelines { if handle.owner!=nil { error:=owner.ops.destroy_pipeline(owner.renderer,handle); if error!=.None { return error }; handle={} } }
    if owner.sampler.owner!=nil { error:=owner.ops.destroy_sampler(owner.renderer,owner.sampler); if error!=.None { return error } }
    owner^={}; return .None
}
/// Inserts depth-tested debug/billboard triangles and always-visible gizmos before the final scene tone phase.
overlay_native_append :: proc(owner:^Overlay_Native($R),prepared:^Native_Prepared(R),mesh:^Overlay_Mesh,frame:Frame_Data,imports:^Overlay_Graph)->(Overlay_Frame,Native_Error) {
    if prepared.scene==nil || !prepared.scene.prepared || prepared.scene.renderer!=owner.renderer || mesh==nil || mesh.overflow || imports==nil || len(mesh.vertices)%3!=0 || len(mesh.vertices)!=len(mesh.triangles)*3 || mesh.gizmo_first<0 || mesh.gizmo_first>len(mesh.vertices) || mesh.gizmo_first%3!=0 { return {},{gpu=.Invalid_Resource} }
    if len(mesh.vertices)==0 { return {},{} }
    if mesh.view_projection!=frame.view_projection || mesh.clip_y!=frame.ambient[3] { return {},{gpu=.Invalid_Resource} }
    scene:=&prepared.scene.graph; g:=scene_graph_target(scene)
    if scene.display_pass.owner!=g || scene.display_pass.index!=len(g.passes)-1 { return {},{gpu=.Invalid_Graph} }
    if imports.graph!=nil && (imports.graph!=g || imports.owner!=owner) { return {},{gpu=.Invalid_Graph} }
    result:=Overlay_Frame{owner=owner}; success:=false; defer { if !success { overlay_frame_destroy(owner,&result) } }
    descriptor:=gfx.Buffer_Desc{size=u64(len(mesh.vertices))*48,usage={.Storage,.Transfer_Destination},memory=.GPU_Private}
    gpu_error:gfx.Gpu_Error
    result.vertices,gpu_error=owner.ops.create_buffer(owner.renderer,descriptor,mem.slice_to_bytes(mesh.vertices[:])); if gpu_error!=.None { return {},{gpu=gpu_error} }
    if imports.graph==g { for image in imports.images { if image.owner!=g || image.index<0 || image.index>=len(g.images) || g.images[image.index].desc!=overlay_glyph_desc() { return {},{gpu=.Invalid_Graph} } } }
    pass_count,buffer_count,image_count:=len(g.passes),len(g.buffers),len(g.images)
    old_imports:=imports^
    extend_error:=scene_graph_extend(scene); if extend_error!={} { return {},extend_error }
    defer { if !success { gfx.graph_truncate(g,pass_count-1,buffer_count,image_count); scene.display_pass={}; imports^=old_imports; restore_error:=scene_graph_finalize(scene); assert(restore_error=={}) } }
    vertex,graph_error:=gfx.graph_buffer(g,descriptor,true,false); if graph_error!=.None { return {},{gpu=.Invalid_Graph} }; result.resource=vertex; result.desc=descriptor
    new_imports:=imports.graph==nil
    if new_imports {
        imports.graph=g; imports.owner=owner
        for &image in imports.images { image,graph_error=gfx.graph_image(g,overlay_glyph_desc(),{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false); if graph_error!=.None { return {},{gpu=.Invalid_Graph} } }
    }
    colors:=[1]gfx.Color_Attachment{{{scene.color,gfx.image_full_range(scene.color_desc),.Read_Write,.Color_Attachment},.Load,.Store,{}}}
    depth:=gfx.Depth_Attachment{enabled=true,access={scene.depth,gfx.image_full_range(scene.depth_desc),.Read_Write,.Depth_Attachment},load=.Load,store=.Store}
    buffers:=[2]gfx.Stage_Buffer_Binding{{0,0,{.Vertex},{scene.frame,{0,128},.Read,.Uniform}},{6,0,{.Vertex},{vertex,{0,descriptor.size},.Read,.Storage}}}
    reads:=[2]gfx.Image_Access{{imports.images[0],gfx.image_full_range(overlay_glyph_desc()),.Read,.Sampled},{imports.images[1],gfx.image_full_range(overlay_glyph_desc()),.Read,.Sampled}}
    images:=[2]gfx.Image_Binding{{6,1,{.Fragment},reads[:1]},{6,2,{.Fragment},reads[1:]}}
    phases:[2]gfx.Render_Phase; draws:[2]gfx.Draw_Op; used:=0
    if mesh.gizmo_first>0 { draws[used]=gfx.Draw{u32(mesh.gizmo_first),1,0,0}; phases[used]={pipeline=owner.pipelines[2] if scene.depth_sense==.Reverse else owner.pipelines[0],draws=draws[used:used+1]}; used+=1 }
    if mesh.gizmo_first<len(mesh.vertices) { draws[used]=gfx.Draw{u32(len(mesh.vertices)-mesh.gizmo_first),1,u32(mesh.gizmo_first),0}; phases[used]={pipeline=owner.pipelines[1],draws=draws[used:used+1]}; used+=1 }
    accesses:=[2]gfx.Buffer_Access{buffers[0].access,buffers[1].access}; image_accesses:=[4]gfx.Image_Access{colors[0].access,depth.access,reads[0],reads[1]}
    pass,error:=scene_graph_pass(scene,"Editor overlays",.Graphics,accesses[:],images=image_accesses[:]); if error!=.None { return {},{gpu=.Invalid_Graph} }
    packet_error:=gfx.graph_set_packet(g,pass,gfx.Render{colors=colors[:],depth=depth,buffers=buffers[:],images=images[:],samplers={{6,3,{.Fragment},owner.sampler}},phases=phases[:used]}); if packet_error!=.None { return {},{packet=packet_error} }
    final_error:=scene_graph_finalize(scene); if final_error!={} { return {},final_error }
    append(&prepared.buffers,gfx.Buffer_Input{vertex,result.vertices})
    if new_imports { for image,i in imports.images { append(&prepared.textures,gfx.Texture_Input{image,owner.glyphs[i]}) } }
    owner.frames+=1; result.counted=true; success=true; return result,{}
}
