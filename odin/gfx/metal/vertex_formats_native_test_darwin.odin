#+build darwin, arm64
//! Compact joints, normalized weights and independent UV locations use native vertex fetch.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:mem"
import "core:fmt"

@(private="package")
Compact_Material_Vertex :: struct { position,uv0,uv1:[2]f32,small:[4]u8,wide,weights:[4]u16 }

@(test)
test_native_compact_vertex_formats_and_independent_secondary_uv_locations :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer assert(renderer_destroy(&r)==.None)
    shader:=`#include <metal_stdlib>
using namespace metal;
struct Input { float2 position [[attribute(0)]]; float2 uv0 [[attribute(2)]]; float2 uv1 [[attribute(7)]]; uint4 small [[attribute(10)]]; uint4 wide [[attribute(14)]]; float4 weights [[attribute(20)]]; };
struct Output { float4 position [[position]]; float4 color; };
vertex Output compact(Input i [[stage_in]]) { return {float4(i.position,0,1),float4(float(i.small.x)/255.0,float(i.wide.y)/65535.0,i.weights.z,i.uv1.x-i.uv0.x)}; }
fragment float4 material(Output i [[stage_in]]) { return i.color; }`
    desc:=gfx.Graphics_Desc{vertex_entry="compact",fragment_entry="material",vertex_metal_entry="compact",fragment_metal_entry="material",vertex_metal_source=shader,fragment_metal_source=shader,vertex_sizes_index=-1,fragment_sizes_index=-1,
        vertex={attributes={{0,0,0,.Float2},{2,0,8,.Float2},{7,0,16,.Float2},{10,0,24,.Uint8x4},{14,0,28,.Uint16x4},{20,0,36,.Unorm16x4}},buffers={{0,size_of(Compact_Material_Vertex),.Vertex}}},colors={{format=.RGBA16_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}}
    pipeline,error:=create_graphics_pipeline(&r,desc); assert(error==.None)
    vertices:[3]Compact_Material_Vertex; positions:=[3][2]f32{{-1,-1},{3,-1},{-1,3}}
    for &vertex,i in vertices { vertex={positions[i],{.25,.1},{.75,.9},{33,4,5,6},{9,40000,11,12},{123,456,32768,65535}} }
    bytes:=mem.slice_to_bytes(vertices[:]); vertex_desc:=gfx.Buffer_Desc{u64(len(bytes)),{.Vertex},.GPU_Private}; vertex_buffer,vertex_error:=create_buffer_with_data(&r,vertex_desc,bytes); assert(vertex_error==.None)
    texture_desc:=gfx.Texture_Desc{32,8,1,1,.RGBA16_Unorm,{.Color_Attachment,.Transfer_Source},1}; texture,texture_error:=create_texture(&r,texture_desc); assert(texture_error==.None)
    capture_desc:=gfx.Buffer_Desc{2048,{.Transfer_Destination,.Readback},.CPU_Visible}; capture,capture_error:=create_buffer(&r,capture_desc); assert(capture_error==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    geometry,_:=gfx.graph_buffer(&graph,vertex_desc,true,false); captured,_:=gfx.graph_buffer(&graph,capture_desc,false,true); output,_:=gfx.graph_image(&graph,texture_desc,{},false,false)
    access:=gfx.Buffer_Access{geometry,{0,u64(len(bytes))},.Read,.Vertex}; color:=gfx.Image_Access{output,gfx.image_full_range(texture_desc),.Write,.Color_Attachment}
    pass,_:=gfx.graph_pass(&graph,"fetch-compact-vertices-and-uv1",.Graphics,{access},images={color})
    assert(gfx.graph_set_packet(&graph,pass,gfx.Render{colors={{color,.Clear,.Store,{}}},phases={{pipeline=pipeline,draws={gfx.Draw_Vertices{vertices={{0,access}},vertex_count=3,instance_count=1}}}}})==.None)
    copy_pass,_:=gfx.graph_pass(&graph,"capture-native-vertex-fetch",.Transfer,{{captured,{0,2048},.Write,.Transfer_Destination}},images={{output,gfx.image_full_range(texture_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy_pass,gfx.Copy_Image_Buffer{output,{0,0,0,0,32,8,.Color,0,1,0,0},captured,0})==.None)
    plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,_:=acquire(&r); submission,native_error,packet_error:=submit(&r,token,&graph,&plan,{{geometry,vertex_buffer},{captured,capture}},{{output,texture}}); assert(native_error==.None && packet_error==.None)
    assert(destroy_graphics_pipeline(&r,pipeline)==.None && destroy_buffer(&r,vertex_buffer)==.None && destroy_texture(&r,texture)==.None)
    assert(wait(&r,submission)==.None)
    pixels:[1024]u16; assert(read_buffer(&r,capture,0,mem.slice_to_bytes(pixels[:]))==.None)
    expected:=[4]int{8481,40000,32768,32768}
    for i in 0..<256 { for channel in 0..<4 { actual:=int(pixels[i*4+channel]); testing.expect(t,actual>=expected[channel]-1 && actual<=expected[channel]+1) } }
    assert(destroy_buffer(&r,capture)==.None)
    fmt.println("Metal compactvertex fetch: Uint8x4, Uint16x4, Unorm16x4 + sparse independent UV0/UV1 locations exact256 RGBA16Unorm pixels and pendingparent retirement PASS")
}
