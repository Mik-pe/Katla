#+build darwin, arm64
//! Canonical WGSL material arrays execute through selected Naga reflection and Metal resource IDs.
package main

import gfx "../gfx"
import gpu "../gfx/metal"
import shader "../gfx/shader"
import adapter "../gfx/shader_adapter"
import "core:time"
import "core:fmt"

array_expected :: proc(x,round:int)->[4]byte {
    if x==0 { return [4]byte{255,0,0,255} if round==0 else [4]byte{0,0,255,255} }
    if x==2048 { return {0,255,0,255} }
    if x>=4094 { return [4]byte{0,0,255,255} if round==0 else [4]byte{255,0,0,255} }
    return {32,64,96,255}
}
array_source :: `
@vertex fn vertex_main(@builtin(vertex_index) index:u32)->@builtin(position) vec4f {
    let positions=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));
    return vec4f(positions[index],0.25,1);
}
` + string(#load("../gfx_shader_native/shaders/array.wgsl"))

run_arrays :: proc(r:^gpu.Renderer,compiler_path:string) {
    compiler:shader.Compiler; assert(shader.compiler_init(&compiler,compiler_path)==.None); defer assert(shader.compiler_destroy(&compiler)==.None)
    compiled,compiler_error:=shader.compile(&compiler,array_source,{{"vertex_main",.Vertex},{"main",.Fragment}})
    assert(compiler_error==.None); defer shader.compiled_destroy(&compiled)
    adapted,adapter_error:=adapter.graphics(&compiled,"vertex_main","main",{colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}},front_counter_clockwise=true})
    assert(adapter_error==.None); defer adapter.graphics_destroy(&adapted)
    assert(len(adapted.descriptor.images)==1 && adapted.descriptor.images[0].array_count==4096 && adapted.descriptor.images[0].metal_kind==.Argument_Buffer && adapted.descriptor.images[0].fragment_index==9)
    pipeline,pipeline_error:=gpu.create_graphics_pipeline(r,adapted.descriptor); assert(pipeline_error==.None)
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
        data:gfx.Readback_Data; complete:bool
        for _ in 0..<1000 {
            error:gfx.Gpu_Error; data,complete,error=gpu.poll_texture_readback(r,ticket); assert(error==.None)
            if complete { break }
            time.sleep(time.Millisecond)
        }
        assert(complete)
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
    run_single_array(r,&compiler); run_storage_array(r,&compiler)
    fmt.println("Arrays: 4096 populated material descriptors, nonuniform pixels, clamped indices4096/4097, rejected array length/retry and two immutable pending replacements verified after handle removal")
}

single_array_source :: `
@group(1) @binding(0) var images:binding_array<texture_2d<f32>,1>;
@group(1) @binding(1) var image_sampler:sampler;
@vertex fn vertex_main(@builtin(vertex_index) index:u32)->@builtin(position) vec4f {
    let positions=array<vec2f,3>(vec2f(-1,-1),vec2f(3,-1),vec2f(-1,3));
    return vec4f(positions[index],0.25,1);
}
@fragment fn fragment_main(@builtin(position) p:vec4f)->@location(0) vec4f {
    return textureSample(images[u32(p.x)],image_sampler,p.xy);
}
`

array_poll :: proc(r:^gpu.Renderer,ticket:gfx.Readback_Ticket)->gfx.Readback_Data {
    for _ in 0..<1000 {
        data,complete,error:=gpu.poll_texture_readback(r,ticket); assert(error==.None)
        if complete { return data }
        time.sleep(time.Millisecond)
    }
    panic("Native image transfer did not complete")
}

run_single_array :: proc(r:^gpu.Renderer,compiler:^shader.Compiler) {
    compiled,error:=shader.compile(compiler,single_array_source,{{"vertex_main",.Vertex},{"fragment_main",.Fragment}}); assert(error==.None); defer shader.compiled_destroy(&compiled)
    adapted,adapter_error:=adapter.graphics(&compiled,"vertex_main","fragment_main",{colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}},front_counter_clockwise=true}); assert(adapter_error==.None); defer adapter.graphics_destroy(&adapted)
    assert(adapted.descriptor.images[0].array_count==1 && adapted.descriptor.images[0].metal_kind==.Argument_Buffer)
    pipeline,pipeline_error:=gpu.create_graphics_pipeline(r,adapted.descriptor); assert(pipeline_error==.None)
    source_desc:=gfx.Texture_Desc{1,1,1,1,.RGBA8_Unorm,{.Sampled,.Transfer_Destination},1}
    target_desc:=gfx.Texture_Desc{16,1,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    pixel:=[4]byte{11,73,219,255}
    source,source_error:=gpu.create_texture_with_data(r,source_desc,pixel[:]); assert(source_error==.None)
    target,target_error:=gpu.create_texture(r,target_desc); assert(target_error==.None)
    sampler,sampler_error:=gpu.create_sampler(r,{address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1}); assert(sampler_error==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    image,_:=gfx.graph_image(&graph,source_desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
    output,_:=gfx.graph_image(&graph,target_desc,{},false,true)
    read:=gfx.Image_Access{image,gfx.image_full_range(source_desc),.Read,.Sampled}
    write:=gfx.Image_Access{output,gfx.image_full_range(target_desc),.Write,.Color_Attachment}
    pass,_:=gfx.graph_pass(&graph,"count-one-argument-buffer",.Graphics,nil,images={read,write})
    assert(gfx.graph_set_packet(&graph,pass,gfx.Render{colors={{write,.Clear,.Store,{}}},images={{1,0,{.Fragment},{read}}},samplers={{1,1,{.Fragment},sampler}},phases={{pipeline=pipeline,draws={gfx.Draw{3,1,0,0}}}}})==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
    submitted,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,nil,{{image,source},{output,target}}); assert(native_error==.None && packet_error==.None)
    captured,capture_error:=gpu.graph_texture_source(r,submitted,output); assert(capture_error==.None)
    ticket,ticket_error:=gpu.queue_texture_readback(r,captured,{0,0,0,0,16,1,.Color,0,1,0,0}); assert(ticket_error==.None)
    assert(gpu.destroy_texture(r,source)==.None && gpu.destroy_texture(r,target)==.None && gpu.destroy_sampler(r,sampler)==.None && gpu.destroy_graphics_pipeline(r,pipeline)==.None)
    assert(gpu.release_graph_exports(r,&graph)==.None && gpu.wait(r,submitted)==.None)
    data:=array_poll(r,ticket); defer gfx.readback_data_destroy(&data)
    for i in 0..<16 { for channel in 0..<4 { assert(data.bytes[i*4+channel]==pixel[channel]) } }
    fmt.println("Arrays: canonical count1 remains native pointer-to-texture-ID ABI; all16 clamped nonzero-index pixels preserved after owner removal")
}

storage_array_source :: string(#load("../gfx_shader_native/shaders/storage_array.wgsl"))

run_storage_array :: proc(r:^gpu.Renderer,compiler:^shader.Compiler) {
    compiled,error:=shader.compile(compiler,storage_array_source,{{"main",.Compute}}); assert(error==.None); defer shader.compiled_destroy(&compiled)
    entry:=compiled.entries[0]; binding:=entry.bindings[0]
    assert(binding.kind==.Texture && binding.metal_kind==.Buffer && binding.array_count==2 && binding.metal_minimum_size==16 && binding.access==.Write && binding.storage_format=="Rgba8Unorm")
    adapted,adapter_error:=adapter.compute(&compiled,"main"); assert(adapter_error==.None); defer adapter.compute_destroy(&adapted)
    assert(adapted.descriptor.images[0].array_count==2 && adapted.descriptor.images[0].metal_kind==.Argument_Buffer && adapted.descriptor.images[0].metal_index==9)
    pipeline,pipeline_error:=gpu.create_pipeline(r,adapted.descriptor); assert(pipeline_error==.None)
    desc:=gfx.Texture_Desc{1,1,1,1,.RGBA8_Unorm,{.Storage,.Transfer_Source},1}
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    images:[2]gfx.Image_Id; accesses:[2]gfx.Image_Access; inputs:[2]gfx.Texture_Input
    for _,i in images {
        images[i],_=gfx.graph_image(&graph,desc,{},false,true); accesses[i]={images[i],gfx.image_full_range(desc),.Write,.Storage}
        texture,texture_error:=gpu.create_texture(r,desc); assert(texture_error==.None); inputs[i]={images[i],texture}
    }
    pass,_:=gfx.graph_pass(&graph,"storage-resource-id-array",.Compute,nil,images=accesses[:])
    assert(gfx.graph_set_packet(&graph,pass,gfx.Dispatch{pipeline=pipeline,groups={2,1,1},images={{binding.group,binding.binding,{.Compute},accesses[:]}}})==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
    submitted,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,nil,inputs[:]); assert(native_error==.None && packet_error==.None)
    tickets:[2]gfx.Readback_Ticket
    for image,i in images {
        source,source_error:=gpu.graph_texture_source(r,submitted,image); assert(source_error==.None)
        ticket,ticket_error:=gpu.queue_texture_readback(r,source,{0,0,0,0,1,1,.Color,0,1,0,0}); assert(ticket_error==.None); tickets[i]=ticket
    }
    for input in inputs { assert(gpu.destroy_texture(r,input.handle)==.None) }
    assert(gpu.destroy_pipeline(r,pipeline)==.None && gpu.release_graph_exports(r,&graph)==.None && gpu.wait(r,submitted)==.None)
    expected:=[2][4]byte{{255,0,0,255},{0,0,255,255}}
    for ticket,i in tickets { data:=array_poll(r,ticket); for channel in 0..<4 { assert(data.bytes[channel]==expected[i][channel]) }; gfx.readback_data_destroy(&data) }
    fmt.println("Arrays: canonical storage2 native argument buffer wrote both actual red/blue textures; independent copies survived CPU owner removal")
}
