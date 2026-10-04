#+build darwin, arm64
//! Native transfer traces retain exact pitches, useful rows, mip scopes and byte-fill values.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:fmt"

@(test)
test_native_capture_transfer_operands_preserve_padded_volume_and_mip_arguments :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init();defer pool->drain()
    r:Renderer;assert(renderer_init(&r)==.None);assert(capture_enable(&r,true)==.None)
    pixels:[496]byte
    for z in 0..<3 { for y in 0..<4 { for x in 0..<8 { i:=z*172+y*40+x*4;pixels[i+z]=255;pixels[i+3]=255 } } }
    source_desc:=gfx.Buffer_Desc{496,{.Transfer_Source},.GPU_Private}
    texture_desc:=gfx.Texture_Desc{8,4,3,1,.RGBA8_Unorm,{.Sampled,.Transfer_Source,.Transfer_Destination},3}
    output_desc:=gfx.Buffer_Desc{48,{.Transfer_Destination,.Readback},.CPU_Visible}
    source,source_error:=create_buffer_with_data(&r,source_desc,pixels[:]);assert(source_error==.None)
    texture,texture_error:=create_texture(&r,texture_desc);assert(texture_error==.None)
    output,output_error:=create_buffer(&r,output_desc);assert(output_error==.None)
    graph:gfx.Graph;gfx.graph_init(&graph)
    src,_:=gfx.graph_buffer(&graph,source_desc,true,false);dst,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    gfx.graph_image(&graph,texture_desc,{},false,false)
    image,_:=gfx.graph_image(&graph,texture_desc,{},false,false)
    base:=gfx.Image_Range{0,1,0,1,{.Color}};lower:=gfx.Image_Range{1,2,0,1,{.Color}}
    upload,_:=gfx.graph_pass(&graph,"trace-padded-volume-upload",.Transfer,{{src,{0,496},.Read,.Transfer_Source}},images={{image,base,.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,upload,gfx.Copy_Buffer_Image{src,0,image,{0,0,0,0,8,4,.Color,0,3,40,172}})==.None)
    filtered,_:=gfx.graph_pass(&graph,"trace-native-volume-mips",.Transfer,nil,images={{image,base,.Read,.Transfer_Source},{image,lower,.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,filtered,gfx.Generate_Mips{image,gfx.image_full_range(texture_desc)})==.None)
    filled,_:=gfx.graph_pass(&graph,"trace-native-byte-fill",.Transfer,{{dst,{0,48},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,filled,gfx.Fill_Buffer{dst,0,48,0xa5a5a5a5})==.None)
    first,_:=gfx.graph_pass(&graph,"trace-padded-mip-readback",.Transfer,{{dst,{0,16},.Write,.Transfer_Destination},{dst,{24,16},.Write,.Transfer_Destination}},images={{image,{1,1,0,1,{.Color}},.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,first,gfx.Copy_Image_Buffer{image,{1,0,0,0,4,2,.Color,0,1,24,48},dst,0})==.None)
    second,_:=gfx.graph_pass(&graph,"trace-tight-mip-readback",.Transfer,{{dst,{40,8},.Write,.Transfer_Destination}},images={{image,{2,1,0,1,{.Color}},.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,second,gfx.Copy_Image_Buffer{image,{2,0,0,0,2,1,.Color,0,1,0,0},dst,40})==.None)
    plan,plan_error:=gfx.graph_compile(&graph);assert(plan_error==.None)
    token,token_error:=acquire(&r);assert(token_error==.None)
    accepted,native_error,packet_error:=submit(&r,token,&graph,&plan,{{src,source},{dst,output}},{{image,texture}});assert(native_error==.None && packet_error==.None)
    assert(destroy_buffer(&r,source)==.None && destroy_texture(&r,texture)==.None)
    assert(wait(&r,accepted)==.None)
    bytes:[48]byte;assert(read_buffer(&r,output,0,bytes[:])==.None)
    offsets:=[3]int{0,24,40};for offset in offsets { for x in 0..<(2 if offset==40 else 4) { i:=offset+x*4;testing.expect(t,bytes[i]==0 && bytes[i+1]==255 && bytes[i+2]==0 && bytes[i+3]==255) } }
    for i in 16..<24 { testing.expect(t,bytes[i]==0xa5) }
    snapshot,exists:=capture_snapshot(&r,accepted.id);assert(exists && snapshot.feedback==.Completed)
    gfx.compiled_graph_destroy(&plan);gfx.graph_destroy(&graph)
    assert(destroy_buffer(&r,output)==.None && renderer_destroy(&r)==.None)
    findings:=gfx.capture_compare(&snapshot);if len(findings)>0 { fmt.println("Metal transfer capture divergences",findings) };testing.expect(t,len(findings)==0);delete(findings)
    operands,upload_rows,readback_rows,mip_operands,byte_fills:int
    for event in snapshot.events { if event.binding_path==.Transfer {
        testing.expect(t,event.emitted && event.encoder!=0 && event.object!=0 && event.pipeline==0 && event.table==0 && event.layout==0 && event.binding_stages==0 && event.phase_index== -1)
        operands+=1
        if event.pass_index==upload.index && event.kind==.Bind_Buffer {
            testing.expect(t,event.native_index==0 && event.buffer_range==gfx.Buffer_Range{u64(event.array_index/4)*172+u64(event.array_index%4)*40,32} && event.transfer_region.bytes_per_row==40 && event.transfer_region.bytes_per_image==172 && event.transfer_region.depth==3);upload_rows+=1
        }
        if event.pass_index==first.index && event.kind==.Bind_Buffer {
            testing.expect(t,event.native_index==1 && event.buffer_range==gfx.Buffer_Range{u64(event.array_index)*24,16} && event.transfer_region.bytes_per_row==24 && event.transfer_region.bytes_per_image==48);readback_rows+=1
        }
        if event.pass_index==filtered.index { testing.expect(t,event.kind==.Bind_Image && event.image_range==(base if event.native_index==0 else lower));mip_operands+=1 }
        if event.pass_index==filled.index { testing.expect(t,event.kind==.Bind_Buffer && event.native_index==0 && event.transfer_value==0xa5a5a5a5 && event.buffer_range==gfx.Buffer_Range{0,48});byte_fills+=1 }
    } }
    testing.expect(t,operands==21 && upload_rows==12 && readback_rows==2 && mip_operands==2 && byte_fills==1)
    gfx.capture_snapshot_destroy(&snapshot)
    fmt.println("Metal transfer capture PASS: actual byte-fill value, padded depth3 upload, native filtered mips, useful readback rows preserve padding;21 descriptorless operands;zero comparisons after native teardown")
}
