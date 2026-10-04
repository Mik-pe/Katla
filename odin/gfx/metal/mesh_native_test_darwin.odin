#+build darwin, arm64
//! Native mesh phases prove vertex/index addressing, indirect commands and immutable constants.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:mem"
import "core:fmt"

@(private="package")
mesh_source :: `
#include <metal_stdlib>
using namespace metal;
struct Mesh_Input { float2 position [[attribute(0)]]; };
struct Mesh_Output { float4 position [[position]]; uint instance [[flat]]; };
vertex Mesh_Output mesh_vertex(Mesh_Input input [[stage_in]], uint instance [[instance_id]]) {
    Mesh_Output output; output.position = float4(input.position, 0.25, 1); output.instance = instance; return output;
}
fragment float4 mesh_fragment(Mesh_Output input [[stage_in]], constant float4 &tint [[buffer(0)]]) {
    return input.instance == 5 ? tint : float4(1,0,1,1);
}
`

@(test)
test_native_mesh_phases_and_indirect_arguments :: proc(t:^testing.T) {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing; testing.expect(t,len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker) }
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer { assert(renderer_destroy(&r)==.None) }
    descriptor:=gfx.Graphics_Desc{vertex_entry="mesh_vertex",fragment_entry="mesh_fragment",vertex_metal_entry="mesh_vertex",fragment_metal_entry="mesh_fragment",vertex_metal_source=mesh_source,fragment_metal_source=mesh_source,vertex_sizes_index=-1,fragment_sizes_index=-1,buffers={{group=0,slot=0,stages={.Fragment},usage=.Uniform,mode=.Read,minimum_size=16,vertex_index=-1,fragment_index=0,vertex_size_index=-1,fragment_size_index=-1}},vertex={attributes={{0,0,0,.Float2}},buffers={{0,8,.Vertex}}},colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}},front_counter_clockwise=true}
    pipeline,pipeline_error:=create_graphics_pipeline(&r,descriptor); assert(pipeline_error==.None)
    defer { assert(destroy_graphics_pipeline(&r,pipeline)==.None) }
    positions:=[4][2]f32{{0,0},{-1,-1},{3,-1},{-1,3}}
    indices:=[4]u16{99,2,3,4}
    indirect:=[4]u32{3,1,1,5}
    indexed_indirect:=[5]u32{3,1,1,~u32(0),5}
    position_desc:=gfx.Buffer_Desc{32,{.Vertex},.CPU_Visible}
    index_desc:=gfx.Buffer_Desc{8,{.Index},.CPU_Visible}
    indirect_desc:=gfx.Buffer_Desc{16,{.Indirect},.CPU_Visible}
    indexed_indirect_desc:=gfx.Buffer_Desc{20,{.Indirect},.CPU_Visible}
    vertex,vertex_error:=create_buffer_with_data(&r,position_desc,mem.slice_to_bytes(positions[:])); assert(vertex_error==.None); defer { assert(destroy_buffer(&r,vertex)==.None) }
    index,index_error:=create_buffer_with_data(&r,index_desc,mem.slice_to_bytes(indices[:])); assert(index_error==.None); defer { assert(destroy_buffer(&r,index)==.None) }
    indirect_buffer,indirect_error:=create_buffer_with_data(&r,indirect_desc,mem.slice_to_bytes(indirect[:])); assert(indirect_error==.None); defer { assert(destroy_buffer(&r,indirect_buffer)==.None) }
    indexed_indirect_buffer,indexed_indirect_error:=create_buffer_with_data(&r,indexed_indirect_desc,mem.slice_to_bytes(indexed_indirect[:])); assert(indexed_indirect_error==.None); defer { assert(destroy_buffer(&r,indexed_indirect_buffer)==.None) }
    image_desc:=gfx.Texture_Desc{32,16,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    image,image_error:=create_texture(&r,image_desc); assert(image_error==.None); defer { assert(destroy_texture(&r,image)==.None) }
    output_desc:=gfx.Buffer_Desc{2048,{.Transfer_Destination,.Readback},.CPU_Visible}
    output,output_error:=create_buffer(&r,output_desc); assert(output_error==.None); defer { assert(destroy_buffer(&r,output)==.None) }
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    vertices,_:=gfx.graph_buffer(&graph,position_desc,true,false)
    index_id,_:=gfx.graph_buffer(&graph,index_desc,true,false)
    indirect_id,_:=gfx.graph_buffer(&graph,indirect_desc,true,false)
    indexed_indirect_id,_:=gfx.graph_buffer(&graph,indexed_indirect_desc,true,false)
    target,_:=gfx.graph_image(&graph,image_desc,{},false,false)
    result,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    vertex_access:=gfx.Buffer_Access{vertices,{0,32},.Read,.Vertex}
    index_access:=gfx.Buffer_Access{index_id,{0,8},.Read,.Index}
    indirect_access:=gfx.Buffer_Access{indirect_id,{0,16},.Read,.Indirect}
    indexed_indirect_access:=gfx.Buffer_Access{indexed_indirect_id,{0,20},.Read,.Indirect}
    color_access:=gfx.Image_Access{target,gfx.image_full_range(image_desc),.Write,.Color_Attachment}
    pass,_:=gfx.graph_pass(&graph,"four-mesh-phases",.Graphics,{vertex_access,index_access,indirect_access,indexed_indirect_access},images={color_access})
    colors:=[4][4]f32{{1,0,0,1},{0,1,0,1},{0,0,1,1},{1,1,0,1}}
    commands:=[4]gfx.Draw_Op{
        gfx.Draw_Vertices{vertices={{0,vertex_access}},vertex_count=3,instance_count=1,first_vertex=1,first_instance=5},
        gfx.Draw_Indexed{vertices={{0,vertex_access}},index=index_access,index_format=.Uint16,index_count=3,instance_count=1,first_index=1,first_instance=5,vertex_offset=-1},
        gfx.Draw_Indirect{vertices={{0,vertex_access}},command=indirect_access,count=1,stride=16},
        gfx.Draw_Indexed_Indirect{vertices={{0,vertex_access}},index=index_access,index_format=.Uint16,command=indexed_indirect_access,count=1,stride=20},
    }
    phases:[4]gfx.Render_Phase
    constants:[4][1]gfx.Constant_Binding
    draws:[4][1]gfx.Draw_Op
    for &phase,i in phases {
        constants[i][0]={group=0,slot=0,stages={.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(colors[i][:])}
        draws[i][0]=commands[i]
        phase={pipeline=pipeline,constants=constants[i][:],scissor={true,u32(i)*8,0,8,16},draws=draws[i][:]}
    }
    assert(gfx.graph_set_packet(&graph,pass,gfx.Render{colors={{color_access,.Clear,.Store,{0,0,0,1}}},phases=phases[:]})==.None)
    copy,_:=gfx.graph_pass(&graph,"mesh-pixels",.Transfer,{{result,{0,2048},.Write,.Transfer_Destination}},images={{target,gfx.image_full_range(image_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copy,gfx.Copy_Image_Buffer{target,{0,0,0,0,32,16,.Color,0,1,0,0},result,0})==.None)
    plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&plan)
    token,acquire_error:=acquire(&r); assert(acquire_error==.None)
    accepted,native_error,packet_error:=submit(&r,token,&graph,&plan,{{vertices,vertex},{index_id,index},{indirect_id,indirect_buffer},{indexed_indirect_id,indexed_indirect_buffer},{result,output}},{{target,image}})
    assert(native_error==.None && packet_error==.None)
    assert(wait(&r,accepted)==.None)
    pixels:[2048]byte; assert(read_buffer(&r,output,0,pixels[:])==.None)
    for y in 0..<16 { for x in 0..<32 {
        expected:=colors[x/8]
        for channel in 0..<4 { assert(pixels[(y*32+x)*4+channel]==byte(expected[channel]*255)) }
    } }
    fmt.println("Metal 4 mesh phases: 512 pixels, direct vertices/index offsets, both indirect forms, first-instance=5 and four immutable inline tints verified")
}
