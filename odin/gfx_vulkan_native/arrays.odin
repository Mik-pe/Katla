//! All fixed material descriptors and immutable replacement lifetimes execute on the native GPU.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:fmt"

array_expected :: proc(x,round:int)->[4]byte {
    if x==0 { return [4]byte{255,0,0,255} if round==0 else [4]byte{0,0,255,255} }
    if x==2048 { return {0,255,0,255} }
    if x>=4094 { return [4]byte{0,0,255,255} if round==0 else [4]byte{255,0,0,255} }
    return {32,64,96,255}
}
run_arrays :: proc(r:^gpu.Renderer,vertex_code,array_code:[]u32) {
    desc:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=array_code,images={{group=1,slot=0,stages={.Fragment},usage=.Sampled,sample_type=.Float,mode=.Read,array_count=4096,metal_kind=.Argument_Buffer}},samplers={{group=1,slot=1,stages={.Fragment}}},colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}}
    pipeline,pipeline_error:=gpu.create_graphics_pipeline(r,desc); assert(pipeline_error==.None)
    desc.images[0].array_count=4095
    failed,failed_error:=gpu.create_graphics_pipeline(r,desc); assert(failed_error==.Invalid_Shader && failed.owner==nil)
    desc.images[0].array_count=4096
    sampler,sampler_error:=gpu.create_sampler(r,{address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=0,max_anisotropy=1}); assert(sampler_error==.None)
    source_desc:=gfx.Texture_Desc{1,1,1,1,.RGBA8_Unorm,{.Sampled,.Transfer_Destination},1}
    target_desc:=gfx.Texture_Desc{4098,2,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    target,target_error:=gpu.create_texture(r,target_desc); assert(target_error==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    sources:[4]gfx.Image_Id
    accesses:[4]gfx.Image_Access
    for _,i in sources {
        sources[i],_=gfx.graph_image(&graph,source_desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
        accesses[i]={sources[i],gfx.image_full_range(source_desc),.Read,.Sampled}
    }
    color,_:=gfx.graph_image(&graph,target_desc,{},false,true)
    color_access:=gfx.Image_Access{color,gfx.image_full_range(target_desc),.Write,.Color_Attachment}
    draw,_:=gfx.graph_pass(&graph,"material-4096",.Graphics,nil,images={accesses[0],accesses[1],accesses[2],accesses[3],color_access})
    elements:=make([]gfx.Image_Access,4096); defer delete(elements)
    for &element in elements { element=accesses[0] }
    elements[0]=accesses[1]; elements[2048]=accesses[2]; elements[4094]=accesses[3]; elements[4095]=accesses[3]
    packet:=gfx.Render{colors={{color_access,.Clear,.Store,{0,0,0,1}}},images={{1,0,{.Fragment},elements}},samplers={{1,1,{.Fragment},sampler}},phases={{pipeline=pipeline,draws={gfx.Draw{3,1,0,0}}}}}
    assert(gfx.graph_set_packet(&graph,draw,packet)==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    tickets:[2]gfx.Readback_Ticket
    submissions:[2]gfx.Submission
    old_source:gfx.Texture_Source
    region:=gfx.Image_Region{0,0,0,0,4098,2,.Color,0,1,0,0}
    for round in 0..<2 {
        handles:[4]gfx.Texture_Handle
        pixels:=[4][4]byte{{32,64,96,255},array_expected(0,round),array_expected(2048,round),array_expected(4095,round)}
        inputs:[5]gfx.Texture_Input
        inputs[4]={color,target}
        for &pixel,i in pixels {
            error:gfx.Gpu_Error
            handles[i],error=gpu.create_texture_with_data(r,source_desc,pixel[:]); assert(error==.None)
            inputs[i]={sources[i],handles[i]}
        }
        token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
        packet.images[0].accesses=elements[:4095]
        assert(gfx.graph_set_packet(&graph,draw,packet)==.None)
        rejected,native_failure,binding_failure:=gpu.submit(r,token,&graph,&plan,nil,inputs[:])
        assert(rejected.owner==nil && native_failure==.Invalid_Graph && binding_failure==.Invalid_Binding)
        assert(gfx.frame_is_acquired(&r.frames,token))
        packet.images[0].accesses=elements
        assert(gfx.graph_set_packet(&graph,draw,packet)==.None)
        saved:=elements[0]; elements[0]=accesses[0]
        submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,nil,inputs[:]); assert(native_error==.None && packet_error==.None)
        elements[0]=saved
        submissions[round]=submission
        for handle in handles { assert(gpu.destroy_texture(r,handle)==.None) }
        if round==1 {
            invalid,stale_error:=gpu.queue_texture_readback(r,old_source,region)
            assert(invalid.owner==nil && stale_error==.Invalid_Resource)
        }
        source,source_error:=gpu.graph_texture_source(r,submission,color); assert(source_error==.None)
        if round==0 { old_source=source }
        ticket,ticket_error:=gpu.queue_texture_readback(r,source,region); assert(ticket_error==.None); tickets[round]=ticket
    }
    assert(gpu.destroy_graphics_pipeline(r,pipeline)==.None)
    assert(gpu.destroy_sampler(r,sampler)==.None)
    assert(gpu.destroy_texture(r,target)==.None)
    assert(gpu.release_graph_exports(r,&graph)==.None)
    assert(gpu.wait(r,submissions[1])==.None); assert(gpu.wait(r,submissions[0])==.None)
    for ticket,round in tickets {
        data,complete,error:=gpu.poll_texture_readback(r,ticket); assert(complete && error==.None)
        assert(data.source.submission==submissions[round] && len(data.bytes)==4098*2*4)
        for y in 0..<2 { for x in 0..<4098 {
            expected:=array_expected(x,round)
            for channel in 0..<4 {
                actual:=data.bytes[(y*4098+x)*4+channel]
                if actual!=expected[channel] { fmt.eprintln("Array pixel mismatch",round,x,y,channel,actual,expected[channel]); panic("Fixed array GPU pixels differ") }
            }
        } }
        gfx.readback_data_destroy(&data)
    }
    fmt.println("Arrays: 4096 populated material descriptors, nonuniform pixels, clamped indices4096/4097, failed publication/retry and two immutable pending replacements verified after handle removal")
}

run_storage_arrays :: proc(r:^gpu.Renderer,code:[]u32) {
    pipeline,pipeline_error:=gpu.create_pipeline(r,{entry="main",spirv=code,local_size={1,1,1},images={{group=1,slot=0,usage=.Storage,sample_type=.Float,storage_format=.RGBA8_Unorm,mode=.Write,array_count=2,metal_kind=.Argument_Buffer}}}); assert(pipeline_error==.None)
    desc:=gfx.Texture_Desc{1,1,1,1,.RGBA8_Unorm,{.Storage,.Transfer_Source},1}
    handles:[2]gfx.Texture_Handle
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    accesses:[2]gfx.Image_Access
    inputs:[2]gfx.Texture_Input
    for _,i in handles {
        error:gfx.Gpu_Error
        handles[i],error=gpu.create_texture(r,desc); assert(error==.None)
        image,_:=gfx.graph_image(&graph,desc,{},false,true)
        accesses[i]={image,gfx.image_full_range(desc),.Write,.Storage}
        inputs[i]={image,handles[i]}
    }
    pass,_:=gfx.graph_pass(&graph,"storage-array-write",.Compute,nil,images=accesses[:])
    assert(gfx.graph_set_packet(&graph,pass,gfx.Dispatch{pipeline=pipeline,groups={2,1,1},images={{1,0,{.Compute},accesses[:]}}})==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
    first,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,nil,inputs[:]); assert(native_error==.None && packet_error==.None)
    tickets:[2]gfx.Readback_Ticket
    for input,i in inputs {
        source,source_error:=gpu.graph_texture_source(r,first,input.resource); assert(source_error==.None)
        error:gfx.Gpu_Error
        tickets[i],error=gpu.queue_texture_readback(r,source,{0,0,0,0,1,1,.Color,0,1,0,0}); assert(error==.None)
    }
    next_token,next_acquire:=gpu.acquire(r); assert(next_acquire==.None)
    second,next_native,next_packet:=gpu.submit(r,next_token,&graph,&plan,nil,inputs[:]); assert(next_native==.None && next_packet==.None)
    for handle in handles { assert(gpu.destroy_texture(r,handle)==.None) }
    assert(gpu.destroy_pipeline(r,pipeline)==.None); assert(gpu.release_graph_exports(r,&graph)==.None)
    assert(gpu.wait(r,second)==.None); assert(gpu.wait(r,first)==.None)
    for ticket,i in tickets {
        data,complete,error:=gpu.poll_texture_readback(r,ticket); assert(complete && error==.None && len(data.bytes)==4)
        expected:=[4]byte{255,0,0,255} if i==0 else [4]byte{0,0,255,255}
        for value,channel in data.bytes { assert(value==expected[channel]) }
        gfx.readback_data_destroy(&data)
    }
    fmt.println("Storage arrays: two GPU-selected image descriptors wrote exact red/blue texels; immutable readback copies survived two pending submissions and parent removal")
}
