#+build darwin, arm64
//! Actual tier-two arrays prove high indices, immutable recordings and removed resource owners.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:fmt"
import "core:mem"

@(test)
test_native_bindless_4096_immutable_argument_buffers :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer assert(renderer_destroy(&r)==.None)
    shader:=`#include <metal_stdlib>
using namespace metal;
template<typename T> struct ImageTable { T inner; };
kernel void array_sample(constant ImageTable<texture2d<float>>* images [[buffer(9)]], constant uint& index [[buffer(0)]], texture2d<float,access::write> output [[texture(0)]], sampler nearest [[sampler(0)]], uint2 p [[thread_position_in_grid]]) {
    output.write(images[min(index,4095u)].inner.sample(nearest,float2(0.5)),p);
}`
    descriptor:=gfx.Compute_Desc{entry="array_sample",metal_entry="array_sample",metal_source=shader,local_size={1,1,1},runtime_sizes_index=-1,
        buffers={{group=0,slot=0,metal_index=0,size_index=-1,usage=.Uniform,mode=.Read,minimum_size=4}},
        images={{group=1,slot=0,metal_index=9,usage=.Sampled,mode=.Read,array_count=4096,metal_kind=.Argument_Buffer},{group=0,slot=1,metal_index=0,usage=.Storage,mode=.Write,storage_format=.RGBA8_Unorm,array_count=1}},samplers={{group=1,slot=1,metal_index=0}}}
    pipeline,error:=create_pipeline(&r,descriptor); testing.expect_value(t,error,gfx.Gpu_Error.None); if error!=.None { return }
    sampler,sampler_error:=create_sampler(&r,{max_anisotropy=1}); assert(sampler_error==.None)
    source_desc:=gfx.Texture_Desc{1,1,1,1,.RGBA8_Unorm,{.Sampled,.Transfer_Destination},1}
    output_desc:=gfx.Texture_Desc{4,4,1,1,.RGBA8_Unorm,{.Storage,.Transfer_Source},1}
    colors:=[3][4]byte{{255,0,0,255},{0,255,0,255},{0,0,255,255}}
    textures:[3]gfx.Texture_Handle
    for &color,i in colors { handle,err:=create_texture_with_data(&r,source_desc,color[:]); assert(err==.None); textures[i]=handle }
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    images:[3]gfx.Image_Id
    accesses:=make([]gfx.Image_Access,4096); defer delete(accesses)
    for &image in images { image,_=gfx.graph_image(&graph,source_desc,{.Transfer_Destination,.Shader_Read,true},true,false) }
    for &access in accesses { access={images[0],gfx.image_full_range(source_desc),.Read,.Sampled} }
    accesses[2046].resource=images[1]; accesses[2047].resource=images[1]; accesses[4094].resource=images[2]; accesses[4095].resource=images[2]
    output,_:=gfx.graph_image(&graph,output_desc,{},false,true)
    output_access:=gfx.Image_Access{output,gfx.image_full_range(output_desc),.Write,.Storage}
    params_desc:=gfx.Buffer_Desc{4,{.Uniform},.CPU_Visible}
    params,_:=gfx.graph_buffer(&graph,params_desc,true,false)
    param_access:=gfx.Buffer_Access{params,{0,4},.Read,.Uniform}
    used:=[4]gfx.Image_Access{accesses[0],accesses[2047],accesses[4095],output_access}
    pass,_:=gfx.graph_pass(&graph,"immutable-4096-ids",.Compute,{param_access},images=used[:])
    packet:=gfx.Dispatch{pipeline=pipeline,groups={4,4,1},bindings={{group=0,slot=0,access=param_access}},images={{1,0,{.Compute},accesses},{0,1,{.Compute},{output_access}}},samplers={{1,1,{.Compute},sampler}}}
    assert(gfx.graph_set_packet(&graph,pass,packet)==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    tickets:[3]gfx.Readback_Ticket
    submissions:[3]gfx.Submission
    outputs:[3]gfx.Texture_Handle
    parameters:[3]gfx.Buffer_Handle
    indices:=[3]u32{4095,2047,99}
    inputs:=[3]gfx.Texture_Input{{images[0],textures[0]},{images[1],textures[1]},{images[2],textures[2]}}
    for &index,i in indices {
        param,err:=create_buffer_with_data(&r,params_desc,mem.slice_to_bytes(mem.slice_ptr(&index,1))); assert(err==.None); parameters[i]=param
        target,target_error:=create_texture(&r,output_desc); assert(target_error==.None); outputs[i]=target
        token,acquire_error:=acquire(&r); assert(acquire_error==.None)
        mappings:=[4]gfx.Texture_Input{inputs[0],inputs[1],inputs[2],{output,target}}
        submitted,native_error,packet_error:=submit(&r,token,&graph,&plan,{{params,param}},mappings[:]); assert(native_error==.None && packet_error==.None); submissions[i]=submitted
        source,source_error:=graph_texture_source(&r,submitted,output); assert(source_error==.None)
        ticket,ticket_error:=queue_texture_readback(&r,source,{0,0,0,0,4,4,.Color,0,1,0,0}); assert(ticket_error==.None); tickets[i]=ticket
        // Replacing the CPU packet after recording must never mutate an accepted ID table.
        accesses[indices[i]].resource=images[0]
        assert(gfx.graph_set_packet(&graph,pass,packet)==.None)
    }
    for submission in submissions { assert(wait(&r,submission)==.None) }
    for param,i in parameters {
        token,acquire_error:=acquire(&r); assert(acquire_error==.None)
        mappings:=[4]gfx.Texture_Input{inputs[0],inputs[1],inputs[2],{output,outputs[i]}}
        submitted,native_error,packet_error:=submit(&r,token,&graph,&plan,{{params,param}},mappings[:]); assert(native_error==.None && packet_error==.None); submissions[i]=submitted
    }
    for texture in textures { assert(destroy_texture(&r,texture)==.None) }
    for target in outputs { assert(destroy_texture(&r,target)==.None) }
    for param in parameters { assert(destroy_buffer(&r,param)==.None) }
    assert(destroy_pipeline(&r,pipeline)==.None && destroy_sampler(&r,sampler)==.None)
    assert(release_graph_exports(&r,&graph)==.None)
    for submission in submissions { assert(wait(&r,submission)==.None) }
    expected:=[3]int{2,1,0}
    for ticket,i in tickets {
        data,ready,err:=poll_texture_readback(&r,ticket); assert(ready && err==.None)
        for pixel in 0..<16 { for channel in 0..<4 { assert(data.bytes[pixel*4+channel]==colors[expected[i]][channel]) } }
        gfx.readback_data_destroy(&data); assert(destroy_readback(&r,ticket)==.Invalid_Resource)
    }
    fmt.println("Metal Tier2 4096 texture IDs: indices4095/2047/fallback99, three immutable pending tables, replaced CPU snapshots, three overwritten/reused slots, removed texture/pipeline/sampler owners and retained readback exact48texels PASS")
}
