#+build darwin, arm64
//! Actual volume rows, word fills, GPU indirect arguments and filtered mip chains share native ownership.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:fmt"
import "core:time"

@(test)
test_native_volume_pitches_fill_indirect_and_mips :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer { assert(renderer_destroy(&r)==.None) }
    source_desc:=gfx.Texture_Desc{4,2,1,1,.RGBA8_Unorm,{.Storage,.Transfer_Destination},3}
    output_desc:=gfx.Texture_Desc{4,2,1,1,.RGBA8_Unorm,{.Storage,.Transfer_Source},3}
    volume_input,_:=create_texture(&r,source_desc); volume_output,_:=create_texture(&r,output_desc)
    padded:[168]byte
    for z in 0..<3 { for y in 0..<2 { for x in 0..<4 { i:=z*64+y*24+x*4; padded[i]=byte(40+z*20); padded[i+1]=byte(x+y*4); padded[i+2]=211; padded[i+3]=255 } } }
    assert(upload_texture(&r,volume_input,{0,0,0,0,4,2,.Color,0,3,24,64},padded[:])==.None)
    shader:=`#include <metal_stdlib>
using namespace metal;
kernel void volume(texture3d<float,access::read> src [[texture(0)]], texture3d<float,access::write> dst [[texture(1)]], uint3 p [[thread_position_in_grid]]) { dst.write(src.read(p),p); }
kernel void indirect_arguments(device uint *dst [[buffer(0)]], uint i [[thread_position_in_grid]]) { dst[i]=i==0?4:1; }
kernel void indirect_values(device uint *dst [[buffer(0)]], uint i [[thread_position_in_grid]]) { dst[i]=i+11; }`
    volume_pipeline,volume_error:=create_pipeline(&r,{entry="volume",metal_entry="volume",metal_source=shader,local_size={1,1,1},runtime_sizes_index=-1,images={{group=0,slot=0,metal_index=0,usage=.Storage,dimension=.D3,storage_format=.RGBA8_Unorm,mode=.Read},{group=0,slot=1,metal_index=1,usage=.Storage,dimension=.D3,storage_format=.RGBA8_Unorm,mode=.Write}}}); assert(volume_error==.None)
    indirect_pipeline,indirect_error:=create_pipeline(&r,{entry="indirect_values",metal_entry="indirect_values",metal_source=shader,local_size={1,1,1},runtime_sizes_index=-1,buffers={{group=0,slot=0,metal_index=0,size_index=-1,usage=.Storage,mode=.Write}}}); assert(indirect_error==.None)
    arguments_pipeline,arguments_error:=create_pipeline(&r,{entry="indirect_arguments",metal_entry="indirect_arguments",metal_source=shader,local_size={3,1,1},runtime_sizes_index=-1,buffers={{group=0,slot=0,metal_index=0,size_index=-1,usage=.Storage,mode=.Write}}}); assert(arguments_error==.None)
    command_desc:=gfx.Buffer_Desc{12,{.Indirect,.Storage},.GPU_Private}; command_buffer,_:=create_buffer(&r,command_desc)
    fill_desc:=gfx.Buffer_Desc{64,{.Transfer_Destination,.Transfer_Source},.GPU_Private}; fill_buffer,_:=create_buffer(&r,fill_desc)
    indirect_desc:=gfx.Buffer_Desc{16,{.Storage,.Transfer_Source},.GPU_Private}; indirect_buffer,_:=create_buffer(&r,indirect_desc)
    mip_desc:=gfx.Texture_Desc{4,4,3,1,.RGBA8_Unorm,{.Sampled,.Transfer_Source,.Transfer_Destination},1}; mip_texture,_:=create_texture(&r,mip_desc)
    mip_pixels:[64]byte; for i in 0..<16 { mip_pixels[i*4]=17; mip_pixels[i*4+1]=91; mip_pixels[i*4+2]=203; mip_pixels[i*4+3]=255 }
    assert(upload_texture(&r,mip_texture,{0,0,0,0,4,4,.Color,0,1,0,0},mip_pixels[:])==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    input,_:=gfx.graph_image(&graph,source_desc,{.Transfer_Destination,.Storage,true},true,false)
    result,_:=gfx.graph_image(&graph,output_desc,{},false,true)
    mip,_:=gfx.graph_image(&graph,mip_desc,{.Transfer_Destination,.Transfer_Source,false},true,false)
    fill,_:=gfx.graph_buffer(&graph,fill_desc,false,false); indirect,_:=gfx.graph_buffer(&graph,indirect_desc,false,false); command,_:=gfx.graph_buffer(&graph,command_desc,false,false)
    captured_desc:=gfx.Buffer_Desc{192,{.Transfer_Destination,.Readback},.CPU_Visible}; captured_buffer,_:=create_buffer(&r,captured_desc); captured,_:=gfx.graph_buffer(&graph,captured_desc,false,true)
    a:=gfx.Image_Access{input,gfx.image_full_range(source_desc),.Read,.Storage}; b:=gfx.Image_Access{result,gfx.image_full_range(output_desc),.Write,.Storage}
    compute,_:=gfx.graph_pass(&graph,"volume-storage-copy",.Compute,nil,images={a,b}); assert(gfx.graph_set_packet(&graph,compute,gfx.Dispatch{pipeline=volume_pipeline,groups={4,2,3},images={{0,0,{.Compute},a},{0,1,{.Compute},b}}})==.None)
    fill_pass,_:=gfx.graph_pass(&graph,"word-pattern-fill",.Transfer,{{fill,{0,64},.Write,.Transfer_Destination}}); assert(gfx.graph_set_packet(&graph,fill_pass,gfx.Fill_Buffer{fill,0,64,0x1234ab89})==.None)
    partial_fill,_:=gfx.graph_pass(&graph,"byte-pattern-fill",.Transfer,{{fill,{16,16},.Write,.Transfer_Destination}}); assert(gfx.graph_set_packet(&graph,partial_fill,gfx.Fill_Buffer{fill,16,16,0xc3c3c3c3})==.None)
    arguments_access:=gfx.Buffer_Access{command,{0,12},.Write,.Storage}
    arguments_pass,_:=gfx.graph_pass(&graph,"produce-indirect-arguments-on-gpu",.Compute,{arguments_access}); assert(gfx.graph_set_packet(&graph,arguments_pass,gfx.Dispatch{pipeline=arguments_pipeline,groups={1,1,1},bindings={{group=0,slot=0,access=arguments_access}}})==.None)
    indirect_write:=gfx.Buffer_Access{indirect,{0,16},.Write,.Storage}; indirect_read:=gfx.Buffer_Access{command,{0,12},.Read,.Indirect}
    indirect_pass,_:=gfx.graph_pass(&graph,"gpu-address-indirect-dispatch",.Compute,{indirect_write,indirect_read}); assert(gfx.graph_set_packet(&graph,indirect_pass,gfx.Dispatch{pipeline=indirect_pipeline,bindings={{group=0,slot=0,access=indirect_write}},indirect={true,indirect_read}})==.None)
    // Imported partial mip initialization is established by an ordinary declared full-level copy.
    mip_upload_desc:=gfx.Buffer_Desc{64,{.Transfer_Source},.GPU_Private}; mip_upload,_:=create_buffer_with_data(&r,mip_upload_desc,mip_pixels[:]); mip_source,_:=gfx.graph_buffer(&graph,mip_upload_desc,true,false)
    base:=gfx.Image_Range{0,1,0,1,{.Color}}
    mip_base,_:=gfx.graph_pass(&graph,"initialize-mip-base",.Transfer,{{mip_source,{0,64},.Read,.Transfer_Source}},images={{mip,base,.Write,.Transfer_Destination}}); assert(gfx.graph_set_packet(&graph,mip_base,gfx.Copy_Buffer_Image{mip_source,0,mip,{0,0,0,0,4,4,.Color,0,1,0,0}})==.None)
    lower:=gfx.Image_Range{1,2,0,1,{.Color}}; generated,_:=gfx.graph_pass(&graph,"filtered-mip-chain",.Transfer,nil,images={{mip,base,.Read,.Transfer_Source},{mip,lower,.Write,.Transfer_Destination}}); assert(gfx.graph_set_packet(&graph,generated,gfx.Generate_Mips{mip,gfx.image_full_range(mip_desc)})==.None)
    copy_fill,_:=gfx.graph_pass(&graph,"capture-fills",.Transfer,{{fill,{0,64},.Read,.Transfer_Source},{captured,{0,64},.Write,.Transfer_Destination}}); assert(gfx.graph_set_packet(&graph,copy_fill,gfx.Copy_Buffer{fill,captured,0,0,64})==.None)
    copy_indirect,_:=gfx.graph_pass(&graph,"capture-indirect",.Transfer,{{indirect,{0,16},.Read,.Transfer_Source},{captured,{64,16},.Write,.Transfer_Destination}}); assert(gfx.graph_set_packet(&graph,copy_indirect,gfx.Copy_Buffer{indirect,captured,0,64,16})==.None)
    copy_volume,_:=gfx.graph_pass(&graph,"capture-volume",.Transfer,{{captured,{80,96},.Write,.Transfer_Destination}},images={{result,gfx.image_full_range(output_desc),.Read,.Transfer_Source}}); assert(gfx.graph_set_packet(&graph,copy_volume,gfx.Copy_Image_Buffer{result,{0,0,0,0,4,2,.Color,0,3,0,0},captured,80})==.None)
    copy_mip,_:=gfx.graph_pass(&graph,"capture-generated-mip",.Transfer,{{captured,{176,16},.Write,.Transfer_Destination}},images={{mip,{1,1,0,1,{.Color}},.Read,.Transfer_Source}}); assert(gfx.graph_set_packet(&graph,copy_mip,gfx.Copy_Image_Buffer{mip,{1,0,0,0,2,2,.Color,0,1,0,0},captured,176})==.None)
    plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,_:=acquire(&r); submitted,native_error,packet_error:=submit(&r,token,&graph,&plan,{{fill,fill_buffer},{indirect,indirect_buffer},{command,command_buffer},{mip_source,mip_upload},{captured,captured_buffer}},{{input,volume_input},{result,volume_output},{mip,mip_texture}}); assert(native_error==.None && packet_error==.None)
    assert(wait(&r,submitted)==.None)
    bytes:[192]byte; assert(read_buffer(&r,captured_buffer,0,bytes[:])==.None)
    for i in 0..<16 { expected:u32=0x1234ab89; if i>=4 && i<8 { expected=0xc3c3c3c3 }; assert((cast(^u32)&bytes[i*4])^==expected) }
    for i in 0..<4 { assert((cast(^u32)&bytes[64+i*4])^==u32(i+11)) }
    for z in 0..<3 { for y in 0..<2 { for x in 0..<4 { i:=80+z*32+y*16+x*4; assert(bytes[i]==byte(40+z*20) && bytes[i+1]==byte(x+y*4) && bytes[i+2]==211 && bytes[i+3]==255) } } }
    for i in 0..<4 { assert(bytes[176+i*4]==17 && bytes[177+i*4]==91 && bytes[178+i*4]==203 && bytes[179+i*4]==255) }
    source_image,source_error:=graph_texture_source(&r,submitted,result); assert(source_error==.None)
    ticket,ticket_error:=queue_texture_readback(&r,source_image,{0,0,0,0,4,2,.Color,0,3,24,64}); assert(ticket_error==.None)
    assert(release_graph_exports(&r,&graph)==.None)
    complete:=false
    for _ in 0..<5000 {
        data,done,copy_error:=poll_texture_readback(&r,ticket); assert(copy_error==.None)
        if done {
            assert(data.row_pitch==24 && data.image_pitch==64 && len(data.bytes)==168)
            for z in 0..<3 { for y in 0..<2 { for x in 0..<4 { i:=z*64+y*24+x*4; assert(data.bytes[i]==byte(40+z*20) && data.bytes[i+1]==byte(x+y*4) && data.bytes[i+2]==211 && data.bytes[i+3]==255) } } }
            gfx.readback_data_destroy(&data); complete=true; break
        }
        time.sleep(time.Millisecond)
    }
    assert(complete)
    for texture in ([3]gfx.Texture_Handle{volume_input,volume_output,mip_texture}) { assert(destroy_texture(&r,texture)==.None) }
    for buffer in ([5]gfx.Buffer_Handle{fill_buffer,indirect_buffer,command_buffer,mip_upload,captured_buffer}) { assert(destroy_buffer(&r,buffer)==.None) }
    assert(destroy_pipeline(&r,volume_pipeline)==.None && destroy_pipeline(&r,indirect_pipeline)==.None && destroy_pipeline(&r,arguments_pipeline)==.None)
    fmt.println("Metal 4 volume/commands: padded three-slice upload, storage 3D texels, UInt32/byte fills, GPU-produced indirect compute, retained pitched volume readback and filtered generated mip pixels verified")
}
