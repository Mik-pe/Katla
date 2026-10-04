#+build darwin, arm64
//! Compiler-owned mixed placement groups prove native alias transitions through rendered bytes.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:fmt"

@(test)
test_native_compiled_mixed_placement_allocations :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init(); defer pool->drain()
    r:Renderer; assert(renderer_init(&r)==.None); defer { assert(renderer_destroy(&r)==.None) }
    desc:=gfx.Texture_Desc{16,8,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    bytes:[512]byte; for i in 0..<128 { bytes[i*4]=41; bytes[i*4+1]=137; bytes[i*4+2]=229; bytes[i*4+3]=255 }
    imported_desc:=gfx.Buffer_Desc{512,{.Transfer_Source},.GPU_Private}
    immutable,immutable_error:=create_buffer_with_data(&r,imported_desc,bytes[:]); assert(immutable_error==.None); defer assert(destroy_buffer(&r,immutable)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    input,_:=gfx.graph_buffer(&graph,imported_desc,true,false)
    transient,_:=gfx.graph_buffer(&graph,{512,{.Transfer_Source,.Transfer_Destination},.GPU_Private},false,false)
    output_desc:=gfx.Buffer_Desc{1024,{.Transfer_Destination,.Readback},.CPU_Visible}
    output,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    image,_:=gfx.graph_image(&graph,desc,{},false,false)
    copied,_:=gfx.graph_pass(&graph,"initialize-transient-private-buffer",.Transfer,{{input,{0,512},.Read,.Transfer_Source},{transient,{0,512},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,copied,gfx.Copy_Buffer{input,transient,0,0,512})==.None)
    saved,_:=gfx.graph_pass(&graph,"save-before-handoff",.Transfer,{{transient,{0,512},.Read,.Transfer_Source},{output,{0,512},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,saved,gfx.Copy_Buffer{transient,output,0,0,512})==.None)
    color:=gfx.Image_Access{image,gfx.image_full_range(desc),.Write,.Color_Attachment}
    cleared,_:=gfx.graph_pass(&graph,"clear-after-buffer-alias",.Graphics,nil,images={color})
    assert(gfx.graph_set_packet(&graph,cleared,gfx.Render{colors={{color,.Clear,.Store,{0,1,0,1}}}})==.None)
    captured,_:=gfx.graph_pass(&graph,"save-rendered-alias",.Transfer,{{output,{512,512},.Write,.Transfer_Destination}},images={{image,gfx.image_full_range(desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,captured,gfx.Copy_Image_Buffer{image,{0,0,0,0,16,8,.Color,0,1,0,0},output,512})==.None)
    plan,plan_error:=gfx.graph_compile(&graph); assert(plan_error==.None); defer gfx.compiled_graph_destroy(&plan)
    allocation,allocation_error:=gfx.graph_allocation_plan(&graph,&plan,allocation_query(&r)); assert(allocation_error==.None); defer gfx.allocation_plan_destroy(&allocation)
    assert(len(allocation.groups)==2)
    for _ in 0..<4 {
        owned,owner_error:=gfx.graph_allocate(&allocation,allocation_api(&r)); assert(owner_error==.None)
        temporary:gfx.Buffer_Handle; rendered:gfx.Texture_Handle; readback:gfx.Buffer_Handle
        for binding in owned.buffers { if binding.resource==transient { temporary=binding.handle }; if binding.resource==output { readback=binding.handle } }
        for binding in owned.textures { if binding.resource==image { rendered=binding.handle } }
        b,_:=gfx.storage_get(&r.buffers,temporary); x,_:=gfx.storage_get(&r.textures,rendered)
        assert(b^.heap!=nil && b^.heap==x^.heap)
        assert(send(NS.UInteger,b^.object,"heapOffset")==0 && send(NS.UInteger,x^.object,"heapOffset")==0)
        append(&owned.buffers,gfx.Buffer_Input{input,immutable})
        token,acquire_error:=acquire(&r); assert(acquire_error==.None)
        submission,native_error,packet_error:=submit(&r,token,&graph,&plan,owned.buffers[:],owned.textures[:]); assert(native_error==.None && packet_error==.None)
        assert(wait(&r,submission)==.None)
        result:[1024]byte; assert(read_buffer(&r,readback,0,result[:])==.None)
        for i in 0..<512 { assert(result[i]==bytes[i]) }
        for i in 0..<128 { assert(result[512+i*4]==0 && result[513+i*4]==255 && result[514+i*4]==0 && result[515+i*4]==255) }
        assert(gfx.graph_allocations_destroy(&owned)==.None)
    }
    fmt.println("Metal 4 automatic allocations: four cycles, mixed private buffer/image same heap+offset0, exact pre-alias bytes and post-alias rendered pixels verified")
}
