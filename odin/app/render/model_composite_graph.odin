//! Existing display pixels are decoded before linear model blending and encoded once afterward.
package render

import gfx "../../gfx"
Composite_Graph_Error :: gfx.Graph_Error

model_composite_slots_destroy :: proc(cache:^Native_Model($R)) {
    for &slot in cache.slots {
        if slot.color_staging.owner!=nil { consumer_cleanup_error(cache.operations.gpu.destroy_buffer(cache.renderer,slot.color_staging)); slot.color_staging={} }
        if slot.copied_color.owner!=nil { consumer_cleanup_error(cache.operations.gpu.destroy_texture(cache.renderer,slot.copied_color)); slot.copied_color={} }
        if slot.linear_color.owner!=nil { consumer_cleanup_error(cache.operations.gpu.destroy_texture(cache.renderer,slot.linear_color)); slot.linear_color={} }
    }
}
model_composite_slots_prepare :: proc(cache:^Native_Model($R),scene:^Scene_Graph)->Native_Error {
    if len(cache.batch.entries)==0 { return {} }
    cache.copied_desc=scene.color_desc; cache.copied_desc.usage={.Sampled,.Transfer_Destination}
    cache.linear_desc=scene.color_desc; cache.linear_desc.format=.RGBA16_Float; cache.linear_desc.usage={.Sampled,.Color_Attachment}
    cache.staging_desc={size=u64(scene.color_desc.width)*u64(scene.color_desc.height)*4,usage={.Transfer_Source,.Transfer_Destination},memory=.GPU_Private}
    zero:=make([]byte,int(cache.staging_desc.size),cache.allocator); defer delete(zero,cache.allocator)
    for &slot in cache.slots {
        error:gfx.Gpu_Error
        slot.copied_color,error=cache.operations.gpu.create_texture(cache.renderer,cache.copied_desc); if error!=.None { return {gpu=error} }
        slot.linear_color,error=cache.operations.gpu.create_texture(cache.renderer,cache.linear_desc); if error!=.None { return {gpu=error} }
        slot.color_staging,error=cache.operations.gpu.create_buffer(cache.renderer,cache.staging_desc,zero); if error!=.None { return {gpu=error} }
    }
    return {}
}
model_composite_pass :: proc(cache:^Native_Model($R),scene:^Scene_Graph,encode:bool)->Native_Error {
    source:=cache.linear_color if encode else cache.copied_color
    target:=scene.color if encode else cache.linear_color
    source_desc:=cache.linear_desc if encode else cache.copied_desc
    target_desc:=scene.color_desc if encode else cache.linear_desc
    access:=gfx.Image_Access{source,gfx.image_full_range(source_desc),.Read,.Sampled}
    color:=[1]gfx.Color_Attachment{{{target,gfx.image_full_range(target_desc),.Write,.Color_Attachment},.Clear,.Store,{}}}
    images:=[2]gfx.Image_Access{access,color[0].access}
    pass,error:=gfx.graph_pass(&scene.graph,"Encode model composition" if encode else "Decode existing scene",.Graphics,nil,images=images[:]); if error!=.None { return {gpu=.Invalid_Graph} }
    bindings:=[1]gfx.Image_Binding{{group=0,slot=0,stages={.Fragment},accesses=images[:1]}}
    draws:=[1]gfx.Draw_Op{gfx.Draw{3,1,0,0}}
    phases:=[1]gfx.Render_Phase{{pipeline=cache.compositors[int(encode)],draws=draws[:]}}
    packet_error:=gfx.graph_set_packet(&scene.graph,pass,gfx.Render{colors=color[:],images=bindings[:],phases=phases[:]})
    return {packet=packet_error}
}
model_composite_begin :: proc(cache:^Native_Model($R),scene:^Scene_Graph)->Native_Error {
    error:Composite_Graph_Error
    cache.copied_color,error=gfx.graph_image(&scene.graph,cache.copied_desc,{},false,false); if error!=.None { return {gpu=.Invalid_Graph} }
    cache.linear_color,error=gfx.graph_image(&scene.graph,cache.linear_desc,{},false,false); if error!=.None { return {gpu=.Invalid_Graph} }
    cache.color_staging,error=gfx.graph_buffer(&scene.graph,cache.staging_desc,true,false); if error!=.None { return {gpu=.Invalid_Graph} }
    buffer:=gfx.Buffer_Access{cache.color_staging,{0,cache.staging_desc.size},.Write,.Transfer_Destination}
    image:=gfx.Image_Access{scene.color,gfx.image_full_range(scene.color_desc),.Read,.Transfer_Source}
    read:gfx.Pass_Id
    read,error=gfx.graph_pass(&scene.graph,"Stage encoded scene",.Transfer,{buffer},images={image}); if error!=.None { return {gpu=.Invalid_Graph} }
    region:=gfx.Image_Region{width=scene.color_desc.width,height=scene.color_desc.height,depth=1,aspect=.Color}
    packet:=gfx.graph_set_packet(&scene.graph,read,gfx.Copy_Image_Buffer{source=scene.color,region=region,destination=cache.color_staging}); if packet!=.None { return {packet=packet} }
    buffer.mode=.Read; buffer.usage=.Transfer_Source
    image={cache.copied_color,gfx.image_full_range(cache.copied_desc),.Write,.Transfer_Destination}
    write:gfx.Pass_Id
    write,error=gfx.graph_pass(&scene.graph,"Copy encoded scene",.Transfer,{buffer},images={image}); if error!=.None { return {gpu=.Invalid_Graph} }
    packet=gfx.graph_set_packet(&scene.graph,write,gfx.Copy_Buffer_Image{source=cache.color_staging,destination=cache.copied_color,region=region}); if packet!=.None { return {packet=packet} }
    return model_composite_pass(cache,scene,false)
}
