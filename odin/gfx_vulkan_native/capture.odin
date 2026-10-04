//! Native diagnostics remain independent after exact fences, slot reuse and owner teardown.
package main

import gfx "../gfx"
import gpu "../gfx/vulkan"
import "core:fmt"
import "core:mem"
import vk "vendor:vulkan"

capture_assert_clean :: proc(snapshot:^gfx.Capture_Snapshot) {
    findings:=gfx.capture_compare(snapshot);defer delete(findings)
    for finding in findings { fmt.eprintln("Capture divergence",finding) }
    assert(len(findings)==0,"actual native capture diverged from independently translated requirements")
}
run_capture :: proc(loader:string,fill_desc:gfx.Compute_Desc,vertex_code,fragment_code:[]u32) {
    owner:gpu.Renderer;assert(gpu.renderer_init(&owner,validation=true,loader_path=loader)==.None)
    destroyed:=false;defer { if !destroyed { assert(gpu.renderer_destroy(&owner)==.None) } }
    assert(gpu.capture_enable(&owner,true)==.None)
    run_aliases(&owner)
    aliases,alias_ok:=gpu.capture_snapshot(&owner);assert(alias_ok);defer gfx.capture_snapshot_destroy(&aliases)
    capture_assert_clean(&aliases)
    heap:u64;alias_seen:=false
    for event in aliases.events { if event.kind==.Alias { assert(event.heap!=0);heap=event.heap;alias_seen=true } }
    assert(alias_seen)
    matches:=0;for event in aliases.events { if event.kind==.Allocation&&event.heap==heap { matches+=1;assert(event.offset==0&&event.size>0&&event.alignment>0&&event.memory_flags!=0) } };assert(matches>=2)
    run_sampling(&owner,vertex_code,fragment_code)
    sampled,sampled_ok:=gpu.capture_snapshot(&owner);assert(sampled_ok&&sampled.feedback==.Completed);defer gfx.capture_snapshot_destroy(&sampled)
    capture_assert_clean(&sampled)
    image_binding,sampler_binding,constant_binding,attachment,encoder_begin,encoder_end:int
    for event in sampled.events {
        #partial switch event.kind {
        case .Bind_Image: if event.binding_path==.Descriptor { image_binding+=1;assert(event.table!=0&&event.image_range.mip_count>0&&event.binding_stages!=0) }
        case .Bind_Sampler: sampler_binding+=1;assert(event.table!=0&&event.binding_stages!=0)
        case .Bind_Buffer: if event.binding_path==.Descriptor&&event.resource_kind==.Auxiliary { constant_binding+=1;assert(event.size==16&&event.binding_stages!=0&&event.table!=0) }
        case .Attachment: attachment+=1
        case .Encoder_Begin: encoder_begin+=1
        case .Encoder_End: encoder_end+=1
        }
    }
    assert(image_binding==14&&sampler_binding==14&&constant_binding==14&&attachment==1&&encoder_begin==1&&encoder_end==1)
    original_submission:=sampled.submission;original_generation:=sampled.generation
    run_indirect(&owner,fill_desc)
    compute,compute_ok:=gpu.capture_snapshot(&owner);assert(compute_ok&&compute.submission>original_submission);defer gfx.capture_snapshot_destroy(&compute)
    capture_assert_clean(&compute)
    actual_buffer_barrier:=false
    for event in compute.events { if event.kind==.Buffer_Barrier { assert(event.buffer_range.size>0&&event.source_access!=0&&event.destination_access!=0);actual_buffer_barrier=true } }
    assert(actual_buffer_barrier)
    // A rejected recording must not publish a replacement snapshot.
    prior_count:=len(owner.capture.accepted)
    native_begin:=owner.table.BeginCommandBuffer;owner.table.BeginCommandBuffer=capture_reject_begin
    run_capture_rejection(&owner)
    owner.table.BeginCommandBuffer=native_begin
    assert(len(owner.capture.accepted)==prior_count&&!owner.capture.recording)
    native_submit:=owner.table.QueueSubmit2;owner.table.QueueSubmit2=capture_reject_submit
    run_capture_rejection(&owner);owner.table.QueueSubmit2=native_submit
    assert(len(owner.capture.accepted)==prior_count&&!owner.capture.recording)
    pending,completed:=run_capture_feedback(&owner);defer gfx.capture_snapshot_destroy(&pending);defer gfx.capture_snapshot_destroy(&completed)
    assert(gpu.validation_error_count(&owner)==0);assert(gpu.renderer_destroy(&owner)==.None);destroyed=true
    assert(sampled.submission==original_submission&&sampled.generation==original_generation&&sampled.feedback==.Completed)
    capture_assert_clean(&sampled);capture_assert_clean(&aliases);capture_assert_clean(&compute);capture_assert_clean(&pending);capture_assert_clean(&completed)
    assert(pending.feedback==.Pending&&completed.feedback==.Completed&&pending.submission==completed.submission)
    fill_found:=false
    for &event in completed.events { if event.kind==.Bind_Buffer&&event.binding_path==.Transfer { assert(event.native_index==0&&event.transfer_value==29&&event.buffer_range==gfx.Buffer_Range{0,16});fill_found=true;saved:=event.transfer_value;event.transfer_value=17;findings:=gfx.capture_compare(&completed);assert(len(findings)>0);delete(findings);event.transfer_value=saved } }
    assert(fill_found);capture_assert_clean(&completed)
    for &event in sampled.events { if event.kind==.Image_Barrier { saved:=event.destination_access;event.destination_access=0;findings:=gfx.capture_compare(&sampled);assert(len(findings)>0);delete(findings);event.destination_access=saved;break } }
    capture_assert_clean(&sampled)
    fmt.println("Capture: actual Vulkan commandbuffer/pass spans, physical sharedheap memory facts, alias and exact image/buffer scopes, immutable phase descriptors; independent snapshots survived pending rejection, slotreuse, graph/resources and nativeowner teardown")
}

capture_reject_begin :: proc "system" (_:vk.CommandBuffer,_:^vk.CommandBufferBeginInfo)->vk.Result { return .ERROR_OUT_OF_HOST_MEMORY }
run_capture_rejection :: proc(r:^gpu.Renderer) {
    graph:gfx.Graph;gfx.graph_init(&graph);defer gfx.graph_destroy(&graph)
    desc:=gfx.Buffer_Desc{size=16,usage={.Transfer_Destination,.Readback}}
    handle,error:=gpu.create_buffer(r,desc);assert(error==.None);defer assert(gpu.destroy_buffer(r,handle)==.None)
    resource,_:=gfx.graph_buffer(&graph,desc,false,true)
    pass,_:=gfx.graph_pass(&graph,"rejected-native-encoding",.Transfer,{{resource,{0,16},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,pass,gfx.Fill_Buffer{resource,0,16,17})==.None)
    plan,compile_error:=gfx.graph_compile(&graph);assert(compile_error==.None);defer gfx.compiled_graph_destroy(&plan)
    token,acquired:=gpu.acquire(r);assert(acquired==.None)
    submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{resource,handle}})
    assert(submission.owner==nil&&native_error==.Native_Failure&&packet_error==.None)
    assert(gpu.abort(r,token)==.None)
}

capture_reject_submit :: proc "system" (_:vk.Queue,_:u32,_:[^]vk.SubmitInfo2,_:vk.Fence)->vk.Result { return .ERROR_OUT_OF_HOST_MEMORY }
run_capture_feedback :: proc(r:^gpu.Renderer)->(gfx.Capture_Snapshot,gfx.Capture_Snapshot) {
    graph:gfx.Graph;gfx.graph_init(&graph);defer gfx.graph_destroy(&graph)
    desc:=gfx.Buffer_Desc{size=16,usage={.Transfer_Destination,.Readback}}
    handle,error:=gpu.create_buffer(r,desc);assert(error==.None);defer assert(gpu.destroy_buffer(r,handle)==.None)
    resource,_:=gfx.graph_buffer(&graph,desc,false,true)
    pass,_:=gfx.graph_pass(&graph,"feedback-owner",.Transfer,{{resource,{0,16},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,pass,gfx.Fill_Buffer{resource,0,16,29})==.None)
    plan,compile_error:=gfx.graph_compile(&graph);assert(compile_error==.None);defer gfx.compiled_graph_destroy(&plan)
    pending,completed:gfx.Capture_Snapshot
    for round in 0..<4 {
        token,acquired:=gpu.acquire(r);assert(acquired==.None)
        submission,native_error,packet_error:=gpu.submit(r,token,&graph,&plan,{{resource,handle}});assert(native_error==.None&&packet_error==.None)
        if round==0 { found:bool;pending,found=gpu.capture_snapshot(r,submission.id);assert(found&&pending.feedback==.Pending);capture_assert_clean(&pending) }
        assert(gpu.wait(r,submission)==.None)
        if round==0 { found:bool;completed,found=gpu.capture_snapshot(r,submission.id);assert(found&&completed.feedback==.Completed);capture_assert_clean(&completed) }
        else if round==3 { assert(token.slot==pending.slot&&token.generation>pending.generation) }
        values:[4]u32;assert(gpu.read_buffer(r,handle,0,mem.slice_to_bytes(values[:]))==.None);for value in values { assert(value==29) }
    }
    return pending,completed
}

capture_assert_store :: proc(r:^gpu.Renderer) { for &snapshot in r.capture.accepted { capture_assert_clean(&snapshot) } }
