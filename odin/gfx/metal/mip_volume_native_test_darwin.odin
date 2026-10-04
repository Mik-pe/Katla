#+build darwin, arm64
//! Three-slice mip filtering must sample the physical source volume's center plane.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:fmt"

@(test)
test_native_non_power_of_two_depth_mips :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer { assert(renderer_destroy(&r)==.None) }
    pixels:[496]byte
    for z in 0..<3 { for y in 0..<4 { for x in 0..<8 { i:=z*172+y*40+x*4; pixels[i+z]=255; pixels[i+3]=255 } } }
    source_desc:=gfx.Buffer_Desc{496,{.Transfer_Source},.GPU_Private}; source,_:=create_buffer_with_data(&r,source_desc,pixels[:])
    texture_desc:=gfx.Texture_Desc{8,4,3,1,.RGBA8_Unorm,{.Sampled,.Transfer_Source,.Transfer_Destination},3}; texture,_:=create_texture(&r,texture_desc)
    output_desc:=gfx.Buffer_Desc{40,{.Transfer_Destination,.Readback},.CPU_Visible}; output,_:=create_buffer(&r,output_desc)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    src,_:=gfx.graph_buffer(&graph,source_desc,true,false); dst,_:=gfx.graph_buffer(&graph,output_desc,false,true); image,_:=gfx.graph_image(&graph,texture_desc,{},false,false)
    base:=gfx.Image_Range{0,1,0,1,{.Color}}; lower:=gfx.Image_Range{1,2,0,1,{.Color}}
    upload,_:=gfx.graph_pass(&graph,"padded-colored-volume-upload",.Transfer,{{src,{0,496},.Read,.Transfer_Source}},images={{image,base,.Write,.Transfer_Destination}}); assert(gfx.graph_set_packet(&graph,upload,gfx.Copy_Buffer_Image{src,0,image,{0,0,0,0,8,4,.Color,0,3,40,172}})==.None)
    filtered,_:=gfx.graph_pass(&graph,"filter-volume-mips",.Transfer,nil,images={{image,base,.Read,.Transfer_Source},{image,lower,.Write,.Transfer_Destination}}); assert(gfx.graph_set_packet(&graph,filtered,gfx.Generate_Mips{image,gfx.image_full_range(texture_desc)})==.None)
    first,_:=gfx.graph_pass(&graph,"capture-volume-mip1",.Transfer,{{dst,{0,32},.Write,.Transfer_Destination}},images={{image,{1,1,0,1,{.Color}},.Read,.Transfer_Source}}); assert(gfx.graph_set_packet(&graph,first,gfx.Copy_Image_Buffer{image,{1,0,0,0,4,2,.Color,0,1,0,0},dst,0})==.None)
    second,_:=gfx.graph_pass(&graph,"capture-volume-mip2",.Transfer,{{dst,{32,8},.Write,.Transfer_Destination}},images={{image,{2,1,0,1,{.Color}},.Read,.Transfer_Source}}); assert(gfx.graph_set_packet(&graph,second,gfx.Copy_Image_Buffer{image,{2,0,0,0,2,1,.Color,0,1,0,0},dst,32})==.None)
    plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,_:=acquire(&r); submitted,native_error,packet_error:=submit(&r,token,&graph,&plan,{{src,source},{dst,output}},{{image,texture}}); assert(native_error==.None && packet_error==.None)
    assert(destroy_buffer(&r,source)==.None && destroy_texture(&r,texture)==.None)
    assert(wait(&r,submitted)==.None)
    bytes:[40]byte; assert(read_buffer(&r,output,0,bytes[:])==.None)
    for i in 0..<10 { assert(bytes[i*4]==0 && bytes[i*4+1]==255 && bytes[i*4+2]==0 && bytes[i*4+3]==255) }
    assert(destroy_buffer(&r,output)==.None)
    fmt.println("Metal 4 volume mips: red/green/blue depth3 filters exact green center plane in both lower mips; source owners removed pending")
}
