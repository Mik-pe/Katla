//! Shared image, raster and retained readback evidence runs on both native adapters.
package gfx_conformance

import gfx "../gfx"
import "core:fmt"
import "core:time"
import "core:mem"

/// Mandatory native operations keep acceptance on the public renderer contract.
Graphics_API :: struct($R:typeid) {
    create_texture:proc(^R,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error),
    destroy_texture:proc(^R,gfx.Texture_Handle)->gfx.Gpu_Error,
    create_pipeline:proc(^R,gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error),
    destroy_pipeline:proc(^R,gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error,
    create_buffer:proc(^R,gfx.Buffer_Desc)->(gfx.Buffer_Handle,gfx.Gpu_Error),
    destroy_buffer:proc(^R,gfx.Buffer_Handle)->gfx.Gpu_Error,
    read_buffer:proc(^R,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error,
    acquire:proc(^R)->(gfx.Frame_Token,gfx.Gpu_Error),
    abort:proc(^R,gfx.Frame_Token)->gfx.Gpu_Error,
    submit:proc(^R,gfx.Frame_Token,^gfx.Graph,^gfx.Compiled_Graph,[]gfx.Buffer_Input,[]gfx.Texture_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error),
    wait:proc(^R,gfx.Submission)->gfx.Gpu_Error,
    source:proc(^R,gfx.Submission,gfx.Image_Id)->(gfx.Texture_Source,gfx.Gpu_Error),
    queue_readback:proc(^R,gfx.Texture_Source,gfx.Image_Region)->(gfx.Readback_Ticket,gfx.Gpu_Error),
    poll_readback:proc(^R,gfx.Readback_Ticket)->(gfx.Readback_Data,bool,gfx.Gpu_Error),
    destroy_readback:proc(^R,gfx.Readback_Ticket)->gfx.Gpu_Error,
    release_exports:proc(^R,^gfx.Graph)->gfx.Gpu_Error,
}

@(private="package")
readback_complete :: proc(r:^$R,api:Graphics_API(R),ticket:gfx.Readback_Ticket)->gfx.Readback_Data {
    for _ in 0..<5000 {
        data,done,err:=api.poll_readback(r,ticket); assert(err==.None)
        if done { return data }
        time.sleep(time.Millisecond)
    }
    panic("native image readback did not complete within five seconds")
}

/// Proves actual color/depth pixels and source retention across later submissions.
run_graphics :: proc(r:^$R,api:Graphics_API(R),desc:gfx.Graphics_Desc) {
    pipeline,pipeline_error:=api.create_pipeline(r,desc); assert(pipeline_error==.None)
    color_desc:=gfx.Texture_Desc{64,8,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    depth_desc:=gfx.Texture_Desc{64,8,1,1,.D32_Float,{.Depth_Attachment,.Transfer_Source},1}
    color,color_error:=api.create_texture(r,color_desc); assert(color_error==.None)
    depth,depth_error:=api.create_texture(r,depth_desc); assert(depth_error==.None)
    output,buffer_error:=api.create_buffer(r,{size=2048,usage={.Transfer_Destination,.Readback}}); assert(buffer_error==.None)
    depth_output,depth_buffer_error:=api.create_buffer(r,{size=2048,usage={.Transfer_Destination,.Readback}}); assert(depth_buffer_error==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    image,_:=gfx.graph_image(&graph,color_desc,{},false,true)
    depth_image,_:=gfx.graph_image(&graph,depth_desc,{},false,true)
    bytes,_:=gfx.graph_buffer(&graph,{size=2048,usage={.Transfer_Destination,.Readback}},false,true)
    depth_bytes,_:=gfx.graph_buffer(&graph,{size=2048,usage={.Transfer_Destination,.Readback}},false,true)
    color_write:=gfx.Image_Access{image,gfx.image_full_range(color_desc),.Write,.Color_Attachment}
    depth_write:=gfx.Image_Access{depth_image,gfx.image_full_range(depth_desc),.Write,.Depth_Attachment}
    draw,_:=gfx.graph_pass(&graph,"raster",.Graphics,nil,images={color_write,depth_write})
    assert(gfx.graph_set_packet(&graph,draw,gfx.Render{colors={{color_write,.Clear,.Store,{0,0,0,1}}},depth={true,depth_write,.Clear,.Store,1,0},phases={{pipeline=pipeline,draws={gfx.Draw{3,1,0,0}}}}})==.None)
    color_copy,_:=gfx.graph_pass(&graph,"color-to-buffer",.Transfer,{{bytes,{0,2048},.Write,.Transfer_Destination}},images={{image,gfx.image_full_range(color_desc),.Read,.Transfer_Source}})
    depth_copy,_:=gfx.graph_pass(&graph,"depth-to-buffer",.Transfer,{{depth_bytes,{0,2048},.Write,.Transfer_Destination}},images={{depth_image,gfx.image_full_range(depth_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,color_copy,gfx.Copy_Image_Buffer{image,{0,0,0,0,64,8,.Color,0,1,0,0},bytes,0})==.None)
    assert(gfx.graph_set_packet(&graph,depth_copy,gfx.Copy_Image_Buffer{depth_image,{0,0,0,0,64,8,.Depth,0,1,0,0},depth_bytes,0})==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=api.acquire(r); assert(acquire_error==.None)
    submission,submit_error,packet_error:=api.submit(r,token,&graph,&plan,{{bytes,output},{depth_bytes,depth_output}},{{image,color},{depth_image,depth}})
    assert(submit_error==.None && packet_error==.None)
    source,source_error:=api.source(r,submission,image); assert(source_error==.None && source.submission==submission)
    ticket,ticket_error:=api.queue_readback(r,source,{0,0,0,0,64,8,.Color,0,1,0,0}); assert(ticket_error==.None)
    invalid_source:=source; invalid_source.generation+=1
    invalid_ticket,invalid_error:=api.queue_readback(r,invalid_source,{0,0,0,0,64,8,.Color,0,1,0,0}); assert(invalid_ticket.owner==nil && invalid_error==.Invalid_Resource)
    assert(api.wait(r,submission)==.None)
    raw:[2048]byte; assert(api.read_buffer(r,output,0,raw[:])==.None)
    for i in 0..<512 { assert(raw[i*4+0]==64 && raw[i*4+1]==128 && raw[i*4+2]==191 && raw[i*4+3]==255) }
    depths:[512]f32; assert(api.read_buffer(r,depth_output,0,mem.slice_to_bytes(depths[:]))==.None)
    for value in depths { assert(value==0.25) }
    // Replacement is ordinary graph work; the earlier queued copy owns its bytes.
    replacement:gfx.Graph; gfx.graph_init(&replacement); defer gfx.graph_destroy(&replacement)
    newer,_:=gfx.graph_image(&replacement,color_desc,{},false,true)
    clear_access:=gfx.Image_Access{newer,gfx.image_full_range(color_desc),.Write,.Color_Attachment}
    clear,_:=gfx.graph_pass(&replacement,"replace",.Graphics,nil,images={clear_access})
    assert(gfx.graph_set_packet(&replacement,clear,gfx.Render{colors={{clear_access,.Clear,.Store,{1,0,0,1}}}})==.None)
    replacement_plan,replacement_error:=gfx.graph_compile(&replacement); assert(replacement_error==.None); defer gfx.compiled_graph_destroy(&replacement_plan)
    for round in 0..<4 {
        replacement_token,replacement_acquire:=api.acquire(r); assert(replacement_acquire==.None)
        accepted,native_error,preflight:=api.submit(r,replacement_token,&replacement,&replacement_plan,nil,{{newer,color}})
        assert(native_error==.None && preflight==.None)
        assert(api.wait(r,accepted)==.None)
        if round==0 {
            stale_ticket,stale_error:=api.queue_readback(r,source,{0,0,0,0,64,8,.Color,0,1,0,0})
            assert(stale_ticket.owner==nil && stale_error==.Invalid_Resource)
        }
    }
    assert(api.destroy_texture(r,color)==.None)
    assert(api.destroy_texture(r,depth)==.None)
    assert(api.destroy_pipeline(r,pipeline)==.None)
    assert(api.release_exports(r,&graph)==.None && api.release_exports(r,&replacement)==.None)
    data:=readback_complete(r,api,ticket); defer gfx.readback_data_destroy(&data)
    assert(data.source.submission==submission && data.region.width==64 && data.region.height==8)
    for y in 0..<8 { for x in 0..<64 {
        i:=int(u64(y)*data.row_pitch+u64(x)*4)
        assert(data.bytes[i]==64 && data.bytes[i+1]==128 && data.bytes[i+2]==191 && data.bytes[i+3]==255)
    } }
    _,done,double_error:=api.poll_readback(r,ticket); assert(!done && double_error==.Invalid_Resource)
    assert(api.destroy_readback(r,ticket)==.Invalid_Resource)
    assert(api.destroy_buffer(r,output)==.None && api.destroy_buffer(r,depth_output)==.None)
    fmt.println("Raster: 512 RGBA pixels and 512 depth values verified; retained image ticket survived four submissions, slot reuse and resource destruction")
}
