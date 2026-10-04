#+build darwin, arm64
//! Native placement aliasing evidence checks physical ownership and rendered bytes.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:mem"
import "core:fmt"

@(test)
test_native_placement_texture_aliases :: proc(t:^testing.T) {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    context.allocator=mem.tracking_allocator(&tracker)
    defer { context.allocator=backing; testing.expect(t,len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker) }
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer
    assert(renderer_init(&r)==.None)
    defer { assert(renderer_destroy(&r)==.None) }
    for rebuild in 0..<8 {
        desc:=gfx.Texture_Desc{32+u32(rebuild)*4,16,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
        size:=u64(desc.width)*u64(desc.height)*4
        graph:gfx.Graph; gfx.graph_init(&graph)
        a,_:=gfx.graph_image(&graph,desc,{},false,false)
        b,_:=gfx.graph_image(&graph,desc,{},false,false)
        output_desc:=gfx.Buffer_Desc{size,{.Transfer_Destination,.Readback},.CPU_Visible}
        first,_:=gfx.graph_buffer(&graph,output_desc,false,true)
        second,_:=gfx.graph_buffer(&graph,output_desc,false,true)
        a_write:=gfx.Image_Access{a,gfx.image_full_range(desc),.Write,.Color_Attachment}
        b_write:=gfx.Image_Access{b,gfx.image_full_range(desc),.Write,.Color_Attachment}
        clear_a,_:=gfx.graph_pass(&graph,"clear-earlier",.Graphics,nil,images={a_write})
        assert(gfx.graph_set_packet(&graph,clear_a,gfx.Render{colors={{a_write,.Clear,.Store,{1,0,0,1}}}})==.None)
        copy_a,_:=gfx.graph_pass(&graph,"copy-earlier",.Transfer,{{first,{0,size},.Write,.Transfer_Destination}},images={{a,gfx.image_full_range(desc),.Read,.Transfer_Source}})
        assert(gfx.graph_set_packet(&graph,copy_a,gfx.Copy_Image_Buffer{a,{0,0,0,0,desc.width,desc.height,.Color,0,1,0,0},first,0})==.None)
        clear_b,_:=gfx.graph_pass(&graph,"clear-later",.Graphics,nil,images={b_write})
        assert(gfx.graph_set_packet(&graph,clear_b,gfx.Render{colors={{b_write,.Clear,.Store,{0,1,0,1}}}})==.None)
        copy_b,_:=gfx.graph_pass(&graph,"copy-later",.Transfer,{{second,{0,size},.Write,.Transfer_Destination}},images={{b,gfx.image_full_range(desc),.Read,.Transfer_Source}})
        assert(gfx.graph_set_packet(&graph,copy_b,gfx.Copy_Image_Buffer{b,{0,0,0,0,desc.width,desc.height,.Color,0,1,0,0},second,0})==.None)
        plan,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None)
        allocation,allocation_error:=gfx.graph_allocation_plan(&graph,&plan,allocation_query(&r)); assert(allocation_error==.None)
        owners:[3]gfx.Graph_Allocations
        submissions:[3]gfx.Submission
        buffers:[3][2]gfx.Buffer_Handle
        for slot in 0..<3 {
            owners[slot],allocation_error=gfx.graph_allocate(&allocation,allocation_api(&r)); assert(allocation_error==.None)
            aliases:[2]gfx.Texture_Handle
            for input in owners[slot].textures { if input.resource==a { aliases[0]=input.handle }; if input.resource==b { aliases[1]=input.handle } }
            left,_:=gfx.storage_get(&r.textures,aliases[0]); right,_:=gfx.storage_get(&r.textures,aliases[1])
            assert(send(^NS.Object,left^.object,"heap")==send(^NS.Object,right^.object,"heap"))
            assert(send(NS.UInteger,left^.object,"heapOffset")==0 && send(NS.UInteger,right^.object,"heapOffset")==0)
            assert(left^.heap==right^.heap && left^.heap!=nil)
            for input in owners[slot].buffers { if input.resource==first { buffers[slot][0]=input.handle }; if input.resource==second { buffers[slot][1]=input.handle } }
            token,acquire_error:=acquire(&r); assert(acquire_error==.None)
            accepted,native_error,packet_error:=submit(&r,token,&graph,&plan,{{first,buffers[slot][0]},{second,buffers[slot][1]}},{{a,aliases[0]},{b,aliases[1]}})
            assert(native_error==.None && packet_error==.None)
            submissions[slot]=accepted
            assert(destroy_texture(&r,aliases[0])==.None && destroy_texture(&r,aliases[1])==.None)
            for &resource in owners[slot].resources { if _,image:=resource.handle.(gfx.Texture_Handle); image { resource.handle={} } }
        }
        for submission,slot in submissions {
            assert(wait(&r,submission)==.None)
            pixels:=make([]byte,int(size))
            for buffer,image in buffers[slot] {
                assert(read_buffer(&r,buffer,0,pixels)==.None)
                expected:=[4]byte{255,0,0,255} if image==0 else [4]byte{0,255,0,255}
                for i in 0..<len(pixels)/4 { for channel in 0..<4 { assert(pixels[i*4+channel]==expected[channel]) } }
            }
            delete(pixels)
            assert(gfx.graph_allocations_destroy(&owners[slot])==.None)
        }
        gfx.allocation_plan_destroy(&allocation)
        gfx.compiled_graph_destroy(&plan); gfx.graph_destroy(&graph)
    }
    fmt.println("Metal 4 placement aliases: shared native heap/offset, eight extents, three pending slots and both rendered colors verified")
}
