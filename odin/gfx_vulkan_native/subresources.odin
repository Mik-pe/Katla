//! Actual disjoint mip/layer copies and graph allocation handoffs have deterministic pixel evidence.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:fmt"

run_subresources :: proc(renderer:^gpu.Renderer) {
    desc:=gfx.Texture_Desc{16,8,2,2,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    texture,texture_error:=gpu.create_texture(renderer,desc); assert(texture_error==.None)
    defer assert(gpu.destroy_texture(renderer,texture)==.None)
    output,buffer_error:=gpu.create_buffer(renderer,{size=1280,usage={.Transfer_Destination,.Readback}}); assert(buffer_error==.None)
    defer assert(gpu.destroy_buffer(renderer,output)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer { assert(gpu.release_graph_exports(renderer,&graph)==.None); gfx.graph_destroy(&graph) }
    image,_:=gfx.graph_image(&graph,desc,{},false,true)
    destination,_:=gfx.graph_buffer(&graph,{size=1280,usage={.Transfer_Destination,.Readback}},false,true)
    colors:=[4][4]f64{{1,0,0,1},{0,1,0,1},{0,0,1,1},{1,1,1,1}}
    offsets:=[4]u64{0,512,640,1152}
    names:=[4]string{"layer0-mip0","layer0-mip1","layer1-mip0","layer1-mip1"}
    copy_names:=[4]string{"copy00","copy01","copy10","copy11"}
    for i in 0..<4 {
        mip,layer:=u32(i%2),u32(i/2)
        width,height:=gfx.texture_mip_extent(desc,mip)
        range:=gfx.Image_Range{mip,1,layer,1,{.Color}}
        access:=gfx.Image_Access{image,range,.Write,.Color_Attachment}
        pass,error:=gfx.graph_pass(&graph,names[i],.Graphics,nil,images={access}); assert(error==.None)
        assert(gfx.graph_set_packet(&graph,pass,gfx.Render{colors={{access,.Clear,.Store,colors[i]}}})==.None)
        bytes:=u64(width)*u64(height)*4
        copy,error_copy:=gfx.graph_pass(&graph,copy_names[i],.Transfer,{{destination,{offsets[i],bytes},.Write,.Transfer_Destination}},images={{image,range,.Read,.Transfer_Source}}); assert(error_copy==.None)
        assert(gfx.graph_set_packet(&graph,copy,gfx.Copy_Image_Buffer{image,{mip,layer,0,0,width,height,.Color,0,1,0,0},destination,offsets[i]})==.None)
    }
    plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,acquired:=gpu.acquire(renderer); assert(acquired==.None)
    submission,native_error,preflight:=gpu.submit(renderer,token,&graph,&plan,{{destination,output}},{{image,texture}}); assert(native_error==.None && preflight==.None)
    assert(gpu.wait(renderer,submission)==.None)
    bytes:[1280]byte; assert(gpu.read_buffer(renderer,output,0,bytes[:])==.None)
    for i in 0..<4 {
        width,height:=gfx.texture_mip_extent(desc,u32(i%2))
        for pixel in 0..<int(width*height) {
            offset:=int(offsets[i])+pixel*4
            for channel in 0..<4 { assert(bytes[offset+channel]==byte(colors[i][channel]*255)) }
        }
    }
    fmt.println("Subresources: independent layouts and exact pixels verified across two mip levels and two array layers")
}
run_aliases :: proc(renderer:^gpu.Renderer) {
    desc:=gfx.Texture_Desc{8,8,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    texture,texture_error:=gpu.create_texture(renderer,desc); assert(texture_error==.None)
    defer assert(gpu.destroy_texture(renderer,texture)==.None)
    output,buffer_error:=gpu.create_buffer(renderer,{size=512,usage={.Transfer_Destination,.Readback}}); assert(buffer_error==.None)
    defer assert(gpu.destroy_buffer(renderer,output)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer { assert(gpu.release_graph_exports(renderer,&graph)==.None); gfx.graph_destroy(&graph) }
    first,_:=gfx.graph_image(&graph,desc,{},false,false)
    second,_:=gfx.graph_image(&graph,desc,{},false,true)
    destination,_:=gfx.graph_buffer(&graph,{size=512,usage={.Transfer_Destination,.Readback}},false,true)
    access1:=gfx.Image_Access{first,gfx.image_full_range(desc),.Write,.Color_Attachment}
    access2:=gfx.Image_Access{second,gfx.image_full_range(desc),.Write,.Color_Attachment}
    clear1,_:=gfx.graph_pass(&graph,"alias-first",.Graphics,nil,images={access1})
    copy1,_:=gfx.graph_pass(&graph,"alias-copy-first",.Transfer,{{destination,{0,256},.Write,.Transfer_Destination}},images={{first,gfx.image_full_range(desc),.Read,.Transfer_Source}})
    clear2,_:=gfx.graph_pass(&graph,"alias-second",.Graphics,nil,images={access2})
    copy2,_:=gfx.graph_pass(&graph,"alias-copy-second",.Transfer,{{destination,{256,256},.Write,.Transfer_Destination}},images={{second,gfx.image_full_range(desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,clear1,gfx.Render{colors={{access1,.Clear,.Store,{1,0,0,1}}}})==.None)
    assert(gfx.graph_set_packet(&graph,copy1,gfx.Copy_Image_Buffer{first,{0,0,0,0,8,8,.Color,0,1,0,0},destination,0})==.None)
    assert(gfx.graph_set_packet(&graph,clear2,gfx.Render{colors={{access2,.Clear,.Store,{0,1,0,1}}}})==.None)
    assert(gfx.graph_set_packet(&graph,copy2,gfx.Copy_Image_Buffer{second,{0,0,0,0,8,8,.Color,0,1,0,0},destination,256})==.None)
    plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
    for _ in 0..<4 {
        token,acquired:=gpu.acquire(renderer); assert(acquired==.None)
        submission,error,preflight:=gpu.submit(renderer,token,&graph,&plan,{{destination,output}},{{first,texture},{second,texture}}); assert(error==.None && preflight==.None)
        assert(gpu.wait(renderer,submission)==.None)
        bytes:[512]byte; assert(gpu.read_buffer(renderer,output,0,bytes[:])==.None)
        for pixel in 0..<64 { assert(bytes[pixel*4]==255 && bytes[pixel*4+1]==0 && bytes[256+pixel*4]==0 && bytes[256+pixel*4+1]==255) }
    }
    fmt.println("Aliasing: two logical image lifetimes reused one native allocation across four frame cycles without mixing pixels")
}
