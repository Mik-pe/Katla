//! Real native placement groups prove mixed-resource aliasing and independent slot ownership.
package gfx_conformance

import gfx "../gfx"
import "core:fmt"
import "core:mem"

/// Fills a GPU-private buffer, copies its bytes, aliases its storage with an image and retains readback.
run_allocations :: proc(r:^$R,api:API(R),graphics:Graphics_API(R),query:gfx.Allocation_Query,native:gfx.Allocation_API,fill_desc:gfx.Compute_Desc) {
    pipeline,pipeline_error:=api.create_pipeline(r,fill_desc); assert(pipeline_error==.None)
    defer assert(api.destroy_pipeline(r,pipeline)==.None)
    graph:gfx.Graph; gfx.graph_init(&graph); defer gfx.graph_destroy(&graph)
    data,_:=gfx.graph_buffer(&graph,{size=4096,usage={.Storage,.Transfer_Source},memory=.GPU_Private},false,false)
    first,_:=gfx.graph_buffer(&graph,{size=4096,usage={.Transfer_Destination,.Readback}},false,true)
    second,_:=gfx.graph_buffer(&graph,{size=4096,usage={.Transfer_Destination,.Readback}},false,true)
    desc:=gfx.Texture_Desc{64,16,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1}
    image,_:=gfx.graph_image(&graph,desc,{},false,true)
    fill_access:=gfx.Buffer_Access{data,{0,4096},.Write,.Storage}
    fill,_:=gfx.graph_pass(&graph,"private-fill",.Compute,{fill_access})
    assert(gfx.graph_set_packet(&graph,fill,gfx.Dispatch{pipeline=pipeline,groups={16,1,1},bindings={{group=0,slot=0,access=fill_access}}})==.None)
    copy,_:=gfx.graph_pass(&graph,"private-buffer-copy",.Transfer,{{data,{0,4096},.Read,.Transfer_Source},{first,{0,4096},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,copy,gfx.Copy_Buffer{data,first,0,0,4096})==.None)
    write:=gfx.Image_Access{image,gfx.image_full_range(desc),.Write,.Color_Attachment}
    clear,_:=gfx.graph_pass(&graph,"alias-image-clear",.Graphics,nil,images={write})
    assert(gfx.graph_set_packet(&graph,clear,gfx.Render{colors={{write,.Clear,.Store,{1,0,0,1}}}})==.None)
    image_copy,_:=gfx.graph_pass(&graph,"alias-image-copy",.Transfer,{{second,{0,4096},.Write,.Transfer_Destination}},images={{image,gfx.image_full_range(desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,image_copy,gfx.Copy_Image_Buffer{image,{0,0,0,0,64,16,.Color,0,1,0,0},second,0})==.None)
    compiled,compile_error:=gfx.graph_compile(&graph); assert(compile_error==.None); defer gfx.compiled_graph_destroy(&compiled)
    plan,plan_error:=gfx.graph_allocation_plan(&graph,&compiled,query); assert(plan_error==.None); defer gfx.allocation_plan_destroy(&plan)
    assert(len(plan.groups)==3 && len(plan.resources)==4)
    mixed:=false
    for group in plan.groups { if group.domain==.GPU_Private { assert(len(group.resources)==2); mixed=true } }
    assert(mixed)
    owners:[3]gfx.Graph_Allocations
    for &owner in owners {
        owner_error:gfx.Gpu_Error; owner,owner_error=gfx.graph_allocate(&plan,native); assert(owner_error==.None)
        assert(len(owner.buffers)==3 && len(owner.textures)==1)
    }
    defer { for &owner in owners { assert(gfx.graph_allocations_destroy(&owner)==.None) } }
    pending:[3]gfx.Submission
    for _ in 0..<2 {
        for _ in 0..<3 {
            token,acquire_error:=api.acquire(r); assert(acquire_error==.None)
            owner:=&owners[token.slot]
            words:[4]u32
            assert(api.read_buffer(r,owner.buffers[0].handle,0,mem.slice_to_bytes(words[:]))==.Unsupported)
            error:gfx.Gpu_Error; preflight:gfx.Packet_Error
            pending[token.slot],error,preflight=api.submit(r,token,&graph,&compiled,owner.buffers[:],owner.textures[:]); assert(error==.None && preflight==.None)
        }
        for submission in pending {
            assert(api.wait(r,submission)==.None)
            owner:=&owners[submission.token.slot]
            words:[1024]u32
            assert(api.read_buffer(r,owner.buffers[1].handle,0,mem.slice_to_bytes(words[:]))==.None)
            for word,i in words { assert(word==u32(i)*3+7) }
            pixels:[4096]byte
            assert(api.read_buffer(r,owner.buffers[2].handle,0,pixels[:])==.None)
            for i in 0..<1024 { assert(pixels[i*4]==255 && pixels[i*4+1]==0 && pixels[i*4+2]==0 && pixels[i*4+3]==255) }
        }
    }
    tickets:[3]gfx.Readback_Ticket
    for _ in 0..<3 {
        token,acquire_error:=api.acquire(r); assert(acquire_error==.None)
        owner:=&owners[token.slot]
        submission,error,preflight:=api.submit(r,token,&graph,&compiled,owner.buffers[:],owner.textures[:]); assert(error==.None && preflight==.None)
        source,source_error:=graphics.source(r,submission,image); assert(source_error==.None)
        ticket,ticket_error:=graphics.queue_readback(r,source,{0,0,0,0,64,16,.Color,0,1,0,0}); assert(ticket_error==.None)
        tickets[token.slot]=ticket; pending[token.slot]=submission
        assert(gfx.graph_allocations_destroy(owner)==.None)
    }
    assert(graphics.release_exports(r,&graph)==.None)
    for submission in pending { assert(api.wait(r,submission)==.None) }
    for ticket in tickets {
        result:=readback_complete(r,graphics,ticket)
        for y in 0..<16 { for x in 0..<64 {
            offset:=int(u64(y)*result.row_pitch+u64(x)*4)
            assert(result.bytes[offset]==255 && result.bytes[offset+1]==0 && result.bytes[offset+2]==0 && result.bytes[offset+3]==255)
        } }
        gfx.readback_data_destroy(&result)
    }
    fmt.println("Allocation: three independent slot heaps, GPU-private buffer/image alias handoffs, 6144 compute values and 9216 image pixels verified; pending owners survived removal")
}
