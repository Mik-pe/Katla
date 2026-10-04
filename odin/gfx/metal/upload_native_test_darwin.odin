#+build darwin, arm64
//! Native uploads and sampled subresource views verify actual queue ownership and texels.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:fmt"

@(test)
test_native_uploaded_array_mip_and_storage_image :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer { assert(renderer_destroy(&r)==.None) }
    source_desc:=gfx.Texture_Desc{8,8,2,2,.RGBA8_Unorm,{.Sampled,.Transfer_Destination},1}
    output_desc:=gfx.Texture_Desc{4,4,1,1,.RGBA8_Unorm,{.Storage,.Transfer_Source,.Transfer_Destination},1}
    source,err:=create_texture(&r,source_desc); assert(err==.None)
    for layer in 0..<2 { for mip in 0..<2 {
        width:=8>>u32(mip)
        pixels:=make([]byte,width*width*4)
        for i in 0..<width*width { pixels[i*4]=byte(20+layer*80+mip*30); pixels[i*4+1]=byte(i); pixels[i*4+2]=211; pixels[i*4+3]=255 }
        assert(upload_texture(&r,source,{u32(mip),u32(layer),0,0,u32(width),u32(width),.Color,0,1,0,0},pixels)==.None)
        delete(pixels)
    } }
    output,output_error:=create_texture(&r,output_desc); assert(output_error==.None)
    sampler,sampler_error:=create_sampler(&r,{min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.Nearest,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=1,max_anisotropy=1}); assert(sampler_error==.None)
    shader:=`#include <metal_stdlib>
using namespace metal;
kernel void sampled(texture2d_array<float> src [[texture(0)]], texture2d<float,access::write> dst [[texture(1)]], sampler nearest [[sampler(0)]], uint2 p [[thread_position_in_grid]]) {
    dst.write(src.sample(nearest,(float2(p)+0.5)/4.0,0),p);
}`
    pipeline,pipeline_error:=create_pipeline(&r,{entry="sampled",metal_entry="sampled",metal_source=shader,local_size={1,1,1},runtime_sizes_index=-1,images={{group=0,slot=0,metal_index=0,usage=.Sampled,arrayed=true,mode=.Read},{group=0,slot=1,metal_index=1,usage=.Storage,storage_format=.RGBA8_Unorm,mode=.Write}},samplers={{group=0,slot=2,metal_index=0}}}); assert(pipeline_error==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    input,_:=gfx.graph_image(&graph,source_desc,{.Transfer_Destination,.Shader_Read,true},true,false)
    result,_:=gfx.graph_image(&graph,output_desc,{},false,false)
    buffer_desc:=gfx.Buffer_Desc{64,{.Transfer_Destination,.Readback},.CPU_Visible}
    captured,_:=gfx.graph_buffer(&graph,buffer_desc,false,true)
    input_access:=gfx.Image_Access{input,{1,1,1,1,{.Color}},.Read,.Sampled}
    output_access:=gfx.Image_Access{result,gfx.image_full_range(output_desc),.Write,.Storage}
    compute,_:=gfx.graph_pass(&graph,"sample-uploaded-array-mip",.Compute,nil,images={input_access,output_access})
    assert(gfx.graph_set_packet(&graph,compute,gfx.Dispatch{pipeline=pipeline,groups={4,4,1},images={{0,0,{.Compute},input_access},{0,1,{.Compute},output_access}},samplers={{0,2,{.Compute},sampler}}})==.None)
    patch_data:[16]byte; for &value in patch_data { value=255 }
    patch_desc:=gfx.Buffer_Desc{16,{.Transfer_Source},.GPU_Private}
    patch_buffer,patch_error:=create_buffer_with_data(&r,patch_desc,patch_data[:]); assert(patch_error==.None)
    assert(read_buffer(&r,patch_buffer,0,patch_data[:])==.Unsupported)
    patch_resource,_:=gfx.graph_buffer(&graph,patch_desc,true,false)
    patch_access:=gfx.Image_Access{result,gfx.image_full_range(output_desc),.Read_Write,.Transfer_Destination}
    patch_pass,_:=gfx.graph_pass(&graph,"partial-private-buffer-upload",.Transfer,{{patch_resource,{0,16},.Read,.Transfer_Source}},images={patch_access})
    assert(gfx.graph_set_packet(&graph,patch_pass,gfx.Copy_Buffer_Image{patch_resource,0,result,{0,0,0,0,2,2,.Color,0,1,0,0}})==.None)
    copy_pass,_:=gfx.graph_pass(&graph,"capture-storage-texels",.Transfer,{{captured,{0,64},.Write,.Transfer_Destination}},images={{result,gfx.image_full_range(output_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy_pass,gfx.Copy_Image_Buffer{result,{0,0,0,0,4,4,.Color,0,1,0,0},captured,0})==.None)
    plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
    buffer,buffer_error:=create_buffer(&r,buffer_desc); assert(buffer_error==.None)
    token,acquire_error:=acquire(&r); assert(acquire_error==.None)
    submitted,native_error,packet_error:=submit(&r,token,&graph,&plan,{{captured,buffer},{patch_resource,patch_buffer}},{{input,source},{result,output}})
    assert(native_error==.None && packet_error==.None)
    assert(destroy_texture(&r,source)==.None && destroy_texture(&r,output)==.None && destroy_sampler(&r,sampler)==.None && destroy_pipeline(&r,pipeline)==.None)
    assert(destroy_buffer(&r,patch_buffer)==.None)
    assert(wait(&r,submitted)==.None)
    pixels:[64]byte; assert(read_buffer(&r,buffer,0,pixels[:])==.None)
    for i in 0..<16 {
        if i%4<2 && i/4<2 { for value in pixels[i*4:i*4+4] { assert(value==255) } }
        else { assert(pixels[i*4]==130 && pixels[i*4+1]==byte(i) && pixels[i*4+2]==211 && pixels[i*4+3]==255) }
    }
    assert(destroy_buffer(&r,buffer)==.None)
    fmt.println("Metal 4 uploads: selected array layer1/mip1, nearest sampler, storage writes, partial private-buffer upload and removed handles retained through GPU completion verified")
}
