//! Expanded native formats prove both source byte transfers and hardware sampling conversions.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:mem"
import "core:fmt"

format_payload :: proc(format:gfx.Texture_Format)->([]byte,[4]byte) {
    bw,bh,block_bytes:=gfx.texture_block_layout(format)
    payload:=make([]byte,int((8/bw)*(8/bh)*block_bytes))
    expected:=[4]byte{255,0,0,255}
    if format==.BC1_RGBA_Unorm || format==.BC3_RGBA_Unorm {
        for block in 0..<4 {
            offset:=block*int(block_bytes)
            if format==.BC3_RGBA_Unorm { payload[offset]=128; offset+=8; expected[3]=128 }
            payload[offset]=0; payload[offset+1]=0xf8
        }
        return payload,expected
    }
    for pixel in 0..<64 {
        offset:=pixel*int(block_bytes)
        #partial switch format {
        case .R8_Unorm: payload[offset]=128; expected={128,0,0,255}
        case .RG8_Unorm: payload[offset]=128; payload[offset+1]=64; expected={128,64,0,255}
        case .R32_Float:
            value:=[1]f32{0.25}; copy(payload[offset:offset+4],mem.slice_to_bytes(value[:])); expected[0]=64
        case .RGBA16_Float:
            payload[offset+1]=0x38; payload[offset+7]=0x3c; expected[0]=128
        case .RGBA16_Unorm:
            payload[offset]=0x80;payload[offset+1]=0x80;payload[offset+6]=255;payload[offset+7]=255;expected[0]=128
        case .RGBA8_Srgb: payload[offset]=128; payload[offset+3]=255; expected[0]=55
        case .BGRA8_Srgb: payload[offset+2]=128; payload[offset+3]=255; expected[0]=55
        case: panic("Unknown expanded format fixture")
        }
    }
    return payload,expected
}
run_formats :: proc(r:^gpu.Renderer,vertex_code,fragment_code:[]u32) {
    desc:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=fragment_code,images={{group=2,slot=1,stages={.Fragment},usage=.Sampled,arrayed=true,sample_type=.Float,mode=.Read,array_count=1}},samplers={{group=2,slot=2,stages={.Fragment}}},colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}}
    pipeline,pipeline_error:=gpu.create_graphics_pipeline(r,desc); assert(pipeline_error==.None)
    defer assert(gpu.destroy_graphics_pipeline(r,pipeline)==.None)
    sampler,sampler_error:=gpu.create_sampler(r,{address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1}); assert(sampler_error==.None)
    defer assert(gpu.destroy_sampler(r,sampler)==.None)
    formats:=[9]gfx.Texture_Format{.R8_Unorm,.RG8_Unorm,.R32_Float,.RGBA16_Float,.RGBA16_Unorm,.RGBA8_Srgb,.BGRA8_Srgb,.BC1_RGBA_Unorm,.BC3_RGBA_Unorm}
    for format in formats {
        payload,expected:=format_payload(format); defer delete(payload)
        texture_desc:=gfx.Texture_Desc{8,8,1,1,format,{.Sampled,.Transfer_Source,.Transfer_Destination},1}
        texture,texture_error:=gpu.create_texture_with_data(r,texture_desc,payload); assert(texture_error==.None)
        target_desc:=gfx.Texture_Desc{8,8,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
        target,target_error:=gpu.create_texture(r,target_desc); assert(target_error==.None)
        output_desc:=gfx.Buffer_Desc{size=256+u64(len(payload)),usage={.Readback,.Transfer_Destination}}
        output,output_error:=gpu.create_buffer(r,output_desc); assert(output_error==.None)
        defer assert(gpu.destroy_buffer(r,output)==.None)
        graph:gfx.Graph; gfx.graph_init(&graph)
        defer { assert(gpu.release_graph_exports(r,&graph)==.None); gfx.graph_destroy(&graph) }
        source,_:=gfx.graph_image(&graph,texture_desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
        color,_:=gfx.graph_image(&graph,target_desc,{},false,true)
        destination,_:=gfx.graph_buffer(&graph,output_desc,false,true)
        sample:=gfx.Image_Access{source,gfx.image_full_range(texture_desc),.Read,.Sampled}
        color_write:=gfx.Image_Access{color,gfx.image_full_range(target_desc),.Write,.Color_Attachment}
        draw,_:=gfx.graph_pass(&graph,"format-sample",.Graphics,nil,images={sample,color_write})
        assert(gfx.graph_set_packet(&graph,draw,gfx.Render{colors={{color_write,.Clear,.Store,{0,0,0,0}}},images={{2,1,{.Fragment},{sample}}},samplers={{2,2,{.Fragment},sampler}},phases={{pipeline=pipeline,draws={gfx.Draw{3,1,0,0}}}}})==.None)
        read_pixels,_:=gfx.graph_pass(&graph,"sample-pixels",.Transfer,{{destination,{0,256},.Write,.Transfer_Destination}},images={{color,gfx.image_full_range(target_desc),.Read,.Transfer_Source}})
        read_source,_:=gfx.graph_pass(&graph,"native-format-bytes",.Transfer,{{destination,{256,u64(len(payload))},.Write,.Transfer_Destination}},images={{source,gfx.image_full_range(texture_desc),.Read,.Transfer_Source}})
        assert(gfx.graph_set_packet(&graph,read_pixels,gfx.Copy_Image_Buffer{color,{0,0,0,0,8,8,.Color,0,1,0,0},destination,0})==.None)
        assert(gfx.graph_set_packet(&graph,read_source,gfx.Copy_Image_Buffer{source,{0,0,0,0,8,8,.Color,0,1,0,0},destination,256})==.None)
        plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
        token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
        submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{destination,output}},{{source,texture},{color,target}})
        assert(native_error==.None && packet_error==.None)
        assert(gpu.destroy_texture(r,texture)==.None); assert(gpu.destroy_texture(r,target)==.None)
        assert(gpu.wait(r,submission)==.None)
        actual:=make([]byte,int(output_desc.size)); defer delete(actual)
        assert(gpu.read_buffer(r,output,0,actual)==.None)
        for pixel in 0..<64 { for channel in 0..<4 { assert(actual[pixel*4+channel]==expected[channel]) } }
        for value,index in payload { assert(actual[256+index]==value) }
        fmt.println("Format:",format,"64 hardware-sampled pixels and exact native source bytes verified after pending handle removal")
    }
}
