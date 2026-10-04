//! Volume pitches, filtered mips and shader image3D writes share exact source identity.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:fmt"

run_volume :: proc(r:^gpu.Renderer,code:[]u32) {
    desc:=gfx.Texture_Desc{8,4,3,1,.RGBA8_Unorm,{.Storage,.Transfer_Source,.Transfer_Destination},3}
    texture,error:=gpu.create_texture(r,desc); assert(error==.None)
    defer assert(gpu.destroy_texture(r,texture)==.None)
    colors:=[3][4]byte{{255,0,0,255},{0,255,0,255},{0,0,255,255}}
    pitched:[496]byte
    for z in 0..<3 { for y in 0..<4 { for x in 0..<8 { for c in 0..<4 { pitched[z*172+y*40+x*4+c]=colors[z][c] } } } }
    source_desc:=gfx.Buffer_Desc{size=496,usage={.Transfer_Source},memory=.GPU_Private}
    source,source_error:=gpu.create_buffer_with_data(r,source_desc,pitched[:]); assert(source_error==.None)
    defer assert(gpu.destroy_buffer(r,source)==.None)
    output_desc:=gfx.Buffer_Desc{size=424,usage={.Readback,.Transfer_Destination}}
    output,output_error:=gpu.create_buffer(r,output_desc); assert(output_error==.None)
    defer assert(gpu.destroy_buffer(r,output)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer { assert(gpu.release_graph_exports(r,&graph)==.None); gfx.graph_destroy(&graph) }
    image,_:=gfx.graph_image(&graph,desc,{final=.Storage},false,true)
    source_id,_:=gfx.graph_buffer(&graph,source_desc,true,false)
    destination,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    base:=gfx.Image_Range{0,1,0,1,{.Color}}
    upper:=gfx.Image_Range{1,2,0,1,{.Color}}
    upload,_:=gfx.graph_pass(&graph,"pitched-volume-upload",.Transfer,{{source_id,{0,496},.Read,.Transfer_Source}},images={{image,base,.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,upload,gfx.Copy_Buffer_Image{source_id,0,image,{0,0,0,0,8,4,.Color,0,3,40,172}})==.None)
    generate,_:=gfx.graph_pass(&graph,"volume-filtered-mips",.Transfer,nil,images={{image,base,.Read,.Transfer_Source},{image,upper,.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,generate,gfx.Generate_Mips{image,gfx.image_full_range(desc)})==.None)
    offsets:=[3]u64{0,384,416}
    sizes:=[3]u64{384,32,8}
    names:=[3]string{"volume-base","volume-mip1","volume-mip2"}
    for mip in 0..<3 {
        width,height,depth:=gfx.texture_mip_volume(desc,u32(mip))
        copy,_:=gfx.graph_pass(&graph,names[mip],.Transfer,{{destination,{offsets[mip],sizes[mip]},.Write,.Transfer_Destination}},images={{image,{u32(mip),1,0,1,{.Color}},.Read,.Transfer_Source}})
        assert(gfx.graph_set_packet(&graph,copy,gfx.Copy_Image_Buffer{image,{u32(mip),0,0,0,width,height,.Color,0,depth,0,0},destination,offsets[mip]})==.None)
    }
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=gpu.acquire(r); assert(acquire_error==.None)
    submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{source_id,source},{destination,output}},{{image,texture}})
    assert(native_error==.None && packet_error==.None)
    old_source,old_source_error:=gpu.graph_texture_source(r,submission,image); assert(old_source_error==.None)
    ticket,ticket_error:=gpu.queue_texture_readback(r,old_source,{0,0,0,0,8,4,.Color,0,3,36,148}); assert(ticket_error==.None)
    assert(gpu.wait(r,submission)==.None)
    data:[424]byte; assert(gpu.read_buffer(r,output,0,data[:])==.None)
    for z in 0..<3 { for pixel in 0..<32 { for c in 0..<4 { assert(data[z*128+pixel*4+c]==colors[z][c]) } } }
    for pixel in 0..<10 { for c in 0..<4 { assert(data[384+pixel*4+c]==colors[1][c]) } }
    pipeline,pipeline_error:=gpu.create_pipeline(r,{entry="main",spirv=code,local_size={4,4,1},images={{group=3,slot=0,usage=.Storage,dimension=.D3,sample_type=.Float,storage_format=.RGBA8_Unorm,mode=.Write}}}); assert(pipeline_error==.None)
    defer assert(gpu.destroy_pipeline(r,pipeline)==.None)
    replacement:gfx.Graph; gfx.graph_init(&replacement); defer { assert(gpu.release_graph_exports(r,&replacement)==.None); gfx.graph_destroy(&replacement) }
    newer,_:=gfx.graph_image(&replacement,desc,{initial=.Storage,final=.Storage,initialized=true},true,true)
    new_output,_:=gfx.graph_buffer(&replacement,output_desc,false,false)
    write:=gfx.Image_Access{newer,base,.Write,.Storage}
    dispatch,_:=gfx.graph_pass(&replacement,"write-image3d",.Compute,nil,images={write})
    assert(gfx.graph_set_packet(&replacement,dispatch,gfx.Dispatch{pipeline=pipeline,groups={2,1,3},images={{3,0,{.Compute},write}}})==.None)
    copy,_:=gfx.graph_pass(&replacement,"read-image3d",.Transfer,{{new_output,{0,384},.Write,.Transfer_Destination}},images={{newer,base,.Read,.Transfer_Source}},side_effect=true)
    assert(gfx.graph_set_packet(&replacement,copy,gfx.Copy_Image_Buffer{newer,{0,0,0,0,8,4,.Color,0,3,0,0},new_output,0})==.None)
    replacement_plan,replacement_compile:=gfx.graph_compile(&replacement); assert(replacement_compile==.None); defer gfx.compiled_graph_destroy(&replacement_plan)
    replacement_token,replacement_acquire:=gpu.acquire(r); assert(replacement_acquire==.None)
    accepted,replacement_error,replacement_packet:=gpu.submit(r,replacement_token,&replacement,&replacement_plan,{{new_output,output}},{{newer,texture}})
    assert(replacement_error==.None && replacement_packet==.None)
    invalid,invalid_error:=gpu.queue_texture_readback(r,old_source,{0,0,0,0,8,4,.Color,0,3,0,0}); assert(invalid.owner==nil && invalid_error==.Invalid_Resource)
    assert(gpu.wait(r,accepted)==.None)
    assert(gpu.read_buffer(r,output,0,data[:384])==.None)
    for z in 0..<3 { for pixel in 0..<32 { for c in 0..<4 { assert(data[z*128+pixel*4+c]==colors[(z+1)%3][c]) } } }
    complete:=false
    for !complete {
        retained,done,poll_error:=gpu.poll_texture_readback(r,ticket); assert(poll_error==.None)
        if done {
            assert(retained.row_pitch==36 && retained.image_pitch==148 && len(retained.bytes)==436)
            for z in 0..<3 { for y in 0..<4 { for x in 0..<8 { for c in 0..<4 { assert(retained.bytes[z*148+y*36+x*4+c]==colors[z][c]) } } } }
            gfx.readback_data_destroy(&retained); complete=true
        }
    }
    fmt.println("Volume: padded rows/non-row-multiple slice pitches, filtered3D mip chain, grouped image3D shader writes and retained old volume bytes verified")
}
