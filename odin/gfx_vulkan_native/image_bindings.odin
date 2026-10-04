//! Uploaded and shader-written texels exercise grouped image and sampler descriptors.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:fmt"

run_image_bindings :: proc(r:^gpu.Renderer,vertex_code,sample_code,image_code:[]u32) {
    sampled_desc:=gfx.Texture_Desc{4,4,1,1,.RGBA8_Unorm,{.Sampled,.Transfer_Destination},1}
    pixels:[64]byte
    for i in 0..<16 { pixels[i*4]=255; pixels[i*4+3]=255 }
    sampled,sampled_error:=gpu.create_texture_with_data(r,sampled_desc,pixels[:]); assert(sampled_error==.None)
    defer assert(gpu.destroy_texture(r,sampled)==.None)
    patch:=[16]byte{0,255,0,255,0,255,0,255,0,255,0,255,0,255,0,255}
    assert(gpu.upload_texture(r,sampled,{0,0,1,1,2,2,.Color,0,1,0,0},patch[:])==.None)
    sampler,sampler_error:=gpu.create_sampler(r,{address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_lod=0,max_anisotropy=1}); assert(sampler_error==.None)
    defer assert(gpu.destroy_sampler(r,sampler)==.None)
    graphics_desc:=gfx.Graphics_Desc{vertex_entry="main",fragment_entry="main",vertex_spirv=vertex_code,fragment_spirv=sample_code,images={{group=2,slot=1,stages={.Fragment},usage=.Sampled,arrayed=true,sample_type=.Float,mode=.Read}},samplers={{group=2,slot=2,stages={.Fragment}}},colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}}
    graphics,graphics_error:=gpu.create_graphics_pipeline(r,graphics_desc); assert(graphics_error==.None)
    defer assert(gpu.destroy_graphics_pipeline(r,graphics)==.None)
    compute,compute_error:=gpu.create_pipeline(r,{entry="main",spirv=image_code,local_size={4,4,1},images={{group=3,slot=0,usage=.Storage,sample_type=.Float,storage_format=.RGBA8_Unorm,mode=.Write}}}); assert(compute_error==.None)
    defer assert(gpu.destroy_pipeline(r,compute)==.None)
    target_desc:=gfx.Texture_Desc{16,16,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    target,target_error:=gpu.create_texture(r,target_desc); assert(target_error==.None)
    defer assert(gpu.destroy_texture(r,target)==.None)
    storage_desc:=gfx.Texture_Desc{8,8,1,1,.RGBA8_Unorm,{.Storage,.Transfer_Source},1}
    storage,storage_error:=gpu.create_texture(r,storage_desc); assert(storage_error==.None)
    defer assert(gpu.destroy_texture(r,storage)==.None)
    output_desc:=gfx.Buffer_Desc{size=1280,usage={.Readback,.Transfer_Destination}}
    output,output_error:=gpu.create_buffer(r,output_desc); assert(output_error==.None)
    defer assert(gpu.destroy_buffer(r,output)==.None)
    blue:=[4]byte{0,0,255,255}
    staging,staging_error:=gpu.create_buffer_with_data(r,{size=4,usage={.Transfer_Source},memory=.GPU_Private},blue[:]); assert(staging_error==.None)
    defer assert(gpu.destroy_buffer(r,staging)==.None)
    untouched,untouched_error:=gpu.create_texture(r,sampled_desc); assert(untouched_error==.None)
    assert(gpu.upload_texture(r,untouched,{0,0,0,0,1,1,.Color,0,1,0,0},blue[:])==.Invalid_Graph)
    assert(gpu.destroy_texture(r,untouched)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer { assert(gpu.release_graph_exports(r,&graph)==.None); gfx.graph_destroy(&graph) }
    source,_:=gfx.graph_image(&graph,sampled_desc,{initial=.Shader_Read,final=.Shader_Read,initialized=true},true,false)
    color,_:=gfx.graph_image(&graph,target_desc,{},false,true)
    written,_:=gfx.graph_image(&graph,storage_desc,{},false,true)
    destination,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    upload,_:=gfx.graph_buffer(&graph,{size=4,usage={.Transfer_Source},memory=.GPU_Private},true,false)
    partial:=gfx.Image_Access{source,gfx.image_full_range(sampled_desc),.Read_Write,.Transfer_Destination}
    upload_pass,_:=gfx.graph_pass(&graph,"partial-private-upload",.Transfer,{{upload,{0,4},.Read,.Transfer_Source}},images={partial})
    assert(gfx.graph_set_packet(&graph,upload_pass,gfx.Copy_Buffer_Image{upload,0,source,{0,0,0,0,1,1,.Color,0,1,0,0}})==.None)
    sample_access:=gfx.Image_Access{source,gfx.image_full_range(sampled_desc),.Read,.Sampled}
    color_access:=gfx.Image_Access{color,gfx.image_full_range(target_desc),.Write,.Color_Attachment}
    render,_:=gfx.graph_pass(&graph,"array-view-sampling",.Graphics,nil,images={sample_access,color_access})
    assert(gfx.graph_set_packet(&graph,render,gfx.Render{colors={{color_access,.Clear,.Store,{0,0,0,1}}},images={{2,1,{.Fragment},sample_access}},samplers={{2,2,{.Fragment},sampler}},phases={{pipeline=graphics,draws={gfx.Draw{3,1,0,0}}}}})==.None)
    storage_access:=gfx.Image_Access{written,gfx.image_full_range(storage_desc),.Write,.Storage}
    dispatch,_:=gfx.graph_pass(&graph,"group3-image-write",.Compute,nil,images={storage_access})
    assert(gfx.graph_set_packet(&graph,dispatch,gfx.Dispatch{pipeline=compute,groups={2,2,1},images={{3,0,{.Compute},storage_access}}})==.None)
    copy_color,_:=gfx.graph_pass(&graph,"sampled-pixels",.Transfer,{{destination,{0,1024},.Write,.Transfer_Destination}},images={{color,gfx.image_full_range(target_desc),.Read,.Transfer_Source}})
    copy_storage,_:=gfx.graph_pass(&graph,"storage-pixels",.Transfer,{{destination,{1024,256},.Write,.Transfer_Destination}},images={{written,gfx.image_full_range(storage_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy_color,gfx.Copy_Image_Buffer{color,{0,0,0,0,16,16,.Color,0,1,0,0},destination,0})==.None)
    assert(gfx.graph_set_packet(&graph,copy_storage,gfx.Copy_Image_Buffer{written,{0,0,0,0,8,8,.Color,0,1,0,0},destination,1024})==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    for _ in 0..<4 {
        token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
        private_read:[4]byte; assert(gpu.read_buffer(r,staging,0,private_read[:])==.Unsupported)
        assert(gpu.write_buffer(r,token,staging,0,blue[:])==.Unsupported)
        submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{destination,output},{upload,staging}},{{source,sampled},{color,target},{written,storage}})
        assert(native_error==.None && packet_error==.None)
        assert(gpu.wait(r,submission)==.None)
        data:[1280]byte; assert(gpu.read_buffer(r,output,0,data[:])==.None)
        for y in 0..<16 { for x in 0..<16 {
            texel_x,texel_y:=x/4,y/4
            expected:=[4]byte{255,0,0,255}
            if texel_x>=1 && texel_x<=2 && texel_y>=1 && texel_y<=2 { expected={0,255,0,255} }
            if texel_x==0 && texel_y==0 { expected=blue }
            for channel in 0..<4 { assert(data[(y*16+x)*4+channel]==expected[channel]) }
        } }
        for pixel in 0..<64 { for channel in 0..<4 { assert(data[1024+pixel*4+channel]==blue[channel]) } }
    }
    fmt.println("Images: constructor/API partial upload, GPU-private buffer-to-image copy, single-layer array sampling and group3 storage writes verified across four frames")
}
