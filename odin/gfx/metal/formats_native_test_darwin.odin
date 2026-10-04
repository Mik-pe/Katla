#+build darwin, arm64
//! Actual sampler decoding verifies sRGB, scalar, paired and compressed native format mappings.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:fmt"

@(test)
test_native_texture_format_sampling :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer { assert(renderer_destroy(&r)==.None) }
    shader:=`#include <metal_stdlib>
using namespace metal;
kernel void decode(texture2d<float> src [[texture(0)]], texture2d<float,access::write> dst [[texture(1)]], sampler nearest [[sampler(0)]], uint2 p [[thread_position_in_grid]]) { dst.write(src.sample(nearest,(float2(p)+.5)/4.0),p); }`
    pipeline,pipeline_error:=create_pipeline(&r,{entry="decode",metal_entry="decode",metal_source=shader,local_size={1,1,1},runtime_sizes_index=-1,images={{group=0,slot=0,metal_index=0,usage=.Sampled,mode=.Read},{group=0,slot=1,metal_index=1,usage=.Storage,mode=.Write,storage_format=.RGBA8_Unorm}},samplers={{group=0,slot=2,metal_index=0}}}); assert(pipeline_error==.None); defer assert(destroy_pipeline(&r,pipeline)==.None)
    sampler,sampler_error:=create_sampler(&r,{min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.Nearest,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1}); assert(sampler_error==.None); defer assert(destroy_sampler(&r,sampler)==.None)
    formats:=[7]gfx.Texture_Format{.RGBA8_Srgb,.BGRA8_Srgb,.R8_Unorm,.RG8_Unorm,.R32_Float,.BC1_RGBA_Unorm,.BC3_RGBA_Unorm}
    expected:=[7][4]byte{{55,13,4,255},{55,13,4,255},{113,0,0,255},{113,47,0,255},{128,0,0,255},{255,0,0,255},{255,0,0,255}}
    verified:=0
    for format,f in formats {
        desc:=gfx.Texture_Desc{4,4,1,1,format,{.Sampled,.Transfer_Destination,.Transfer_Source},1}
        if !texture_supported(&r,desc) { fmt.println("Native unsupported texture format",format); continue }
        size:=int(gfx.texture_pixel_size(format))*16
        if format==.BC1_RGBA_Unorm { size=8 }; if format==.BC3_RGBA_Unorm { size=16 }
        bytes:=make([]byte,size)
        #partial switch format {
        case .RGBA8_Srgb,.BGRA8_Srgb:
            for i in 0..<16 { bytes[i*4]=128 if format==.RGBA8_Srgb else 32; bytes[i*4+1]=64; bytes[i*4+2]=32 if format==.RGBA8_Srgb else 128; bytes[i*4+3]=255 }
        case .R8_Unorm: for &value in bytes { value=113 }
        case .RG8_Unorm: for i in 0..<16 { bytes[i*2]=113; bytes[i*2+1]=47 }
        case .R32_Float: for i in 0..<16 { bytes[i*4+3]=0x3f }
        case .BC1_RGBA_Unorm: bytes[1]=0xf8; bytes[2]=0xe0; bytes[3]=7
        case .BC3_RGBA_Unorm: bytes[0]=255; bytes[9]=0xf8; bytes[10]=0xe0; bytes[11]=7
        }
        source,source_error:=create_texture_with_data(&r,desc,bytes); assert(source_error==.None)
        output_desc:=gfx.Texture_Desc{4,4,1,1,.RGBA8_Unorm,{.Storage,.Transfer_Source},1}; output,_:=create_texture(&r,output_desc)
        buffer_desc:=gfx.Buffer_Desc{u64(64+size),{.Transfer_Destination,.Readback},.CPU_Visible}; buffer,_:=create_buffer(&r,buffer_desc)
        graph:gfx.Graph; gfx.graph_init(&graph)
        src,_:=gfx.graph_image(&graph,desc,{.Transfer_Destination,.Shader_Read,true},true,false); dst,_:=gfx.graph_image(&graph,output_desc,{},false,false); captured,_:=gfx.graph_buffer(&graph,buffer_desc,false,true)
        read:=gfx.Image_Access{src,gfx.image_full_range(desc),.Read,.Sampled}; write:=gfx.Image_Access{dst,gfx.image_full_range(output_desc),.Write,.Storage}
        pass,_:=gfx.graph_pass(&graph,"native-format-decode",.Compute,nil,images={read,write}); assert(gfx.graph_set_packet(&graph,pass,gfx.Dispatch{pipeline=pipeline,groups={4,4,1},images={{0,0,{.Compute},read},{0,1,{.Compute},write}},samplers={{0,2,{.Compute},sampler}}})==.None)
        copy_pixels,_:=gfx.graph_pass(&graph,"capture-decoded-color",.Transfer,{{captured,{0,64},.Write,.Transfer_Destination}},images={{dst,gfx.image_full_range(output_desc),.Read,.Transfer_Source}}); assert(gfx.graph_set_packet(&graph,copy_pixels,gfx.Copy_Image_Buffer{dst,{0,0,0,0,4,4,.Color,0,1,0,0},captured,0})==.None)
        copy_raw,_:=gfx.graph_pass(&graph,"capture-native-format-bytes",.Transfer,{{captured,{64,u64(size)},.Write,.Transfer_Destination}},images={{src,gfx.image_full_range(desc),.Read,.Transfer_Source}}); assert(gfx.graph_set_packet(&graph,copy_raw,gfx.Copy_Image_Buffer{src,{0,0,0,0,4,4,.Color,0,1,0,0},captured,64})==.None)
        plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None)
        token,_:=acquire(&r); submitted,native_error,packet_error:=submit(&r,token,&graph,&plan,{{captured,buffer}},{{src,source},{dst,output}}); assert(native_error==.None && packet_error==.None); assert(wait(&r,submitted)==.None)
        copied:=make([]byte,64+size); assert(read_buffer(&r,buffer,0,copied)==.None)
        for i in 0..<16 { for channel in 0..<4 { actual:=int(copied[i*4+channel]); target:=int(expected[f][channel]); assert(actual>=target-1 && actual<=target+1) } }
        for byte,i in bytes { assert(copied[64+i]==byte) }
        delete(copied); delete(bytes); gfx.compiled_graph_destroy(&plan); gfx.graph_destroy(&graph)
        assert(destroy_texture(&r,source)==.None && destroy_texture(&r,output)==.None && destroy_buffer(&r,buffer)==.None); verified+=1
    }
    fmt.println("Metal 4 formats:",verified,"sRGB/scalar/paired/float/BC mappings verified by decoded GPU texels and exact native bytes")
}
