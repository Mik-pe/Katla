#+build darwin, arm64
//! Real mixed placement, sampled rendering and readback exercise passive native facts across reuse.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"
import "core:testing"
import "core:fmt"
import "core:mem"

@(test)
test_native_passive_capture_retains_actual_tables_heap_barriers_and_feedback :: proc(t:^testing.T) {
    pool:=NS.AutoreleasePool.alloc()->init();defer pool->drain()
    r:Renderer;assert(renderer_init(&r)==.None)
    assert(capture_enable(&r,true)==.None)
    source_desc:=gfx.Texture_Desc{1,1,1,1,.RGBA8_Unorm,{.Sampled,.Transfer_Destination},1}
    source_bytes:=[4]byte{41,137,229,255};source,source_error:=create_texture_with_data(&r,source_desc,source_bytes[:]);assert(source_error==.None)
    sampler,sampler_error:=create_sampler(&r,{min_filter=.Nearest,mag_filter=.Nearest,mip_filter=.None,address_u=.Clamp_Edge,address_v=.Clamp_Edge,address_w=.Clamp_Edge,max_anisotropy=1});assert(sampler_error==.None)
    shader:=`#include <metal_stdlib>
using namespace metal;
vertex float4 fullscreen(uint i [[vertex_id]]) { const float2 p[3]={float2(-1,-1),float2(3,-1),float2(-1,3)}; return float4(p[i],0,1); }
fragment float4 sampled(texture2d<float> image [[texture(0)]],sampler policy [[sampler(0)]],constant float4& tint [[buffer(0)]]) { return image.sample(policy,float2(.5))*tint; }`
    pipeline,pipeline_error:=create_graphics_pipeline(&r,{vertex_entry="fullscreen",fragment_entry="sampled",vertex_metal_entry="fullscreen",fragment_metal_entry="sampled",vertex_metal_source=shader,fragment_metal_source=shader,vertex_sizes_index=-1,fragment_sizes_index=-1,
        buffers={{group=2,slot=0,stages={.Fragment},usage=.Uniform,vertex_index=-1,fragment_index=0,vertex_size_index=-1,fragment_size_index=-1,mode=.Read,minimum_size=16}},
        images={{group=2,slot=1,stages={.Fragment},usage=.Sampled,vertex_index=-1,fragment_index=0,array_count=1,mode=.Read}},samplers={{group=2,slot=2,stages={.Fragment},vertex_index=-1,fragment_index=0}},colors={{format=.RGBA8_Unorm,write_mask={.Red,.Green,.Blue,.Alpha}}}});assert(pipeline_error==.None)
    graph:gfx.Graph;gfx.graph_init(&graph)
    transient_desc:=gfx.Buffer_Desc{512,{.Transfer_Source,.Transfer_Destination},.GPU_Private};transient,_:=gfx.graph_buffer(&graph,transient_desc,false,false)
    output_desc:=gfx.Buffer_Desc{1024,{.Transfer_Destination,.Readback},.CPU_Visible};output,_:=gfx.graph_buffer(&graph,output_desc,false,true)
    target_desc:=gfx.Texture_Desc{16,8,1,1,.RGBA8_Unorm,{.Color_Attachment,.Transfer_Source},1};target,_:=gfx.graph_image(&graph,target_desc,{},false,false)
    image,_:=gfx.graph_image(&graph,source_desc,{.Shader_Read,.Shader_Read,true},true,false)
    filled,_:=gfx.graph_pass(&graph,"actual-private-word-fill",.Transfer,{{transient,{0,512},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,filled,gfx.Fill_Buffer{transient,0,512,0xe589292a})==.None)
    saved,_:=gfx.graph_pass(&graph,"save-before-mixed-alias",.Transfer,{{transient,{0,512},.Read,.Transfer_Source},{output,{0,512},.Write,.Transfer_Destination}})
    assert(gfx.graph_set_packet(&graph,saved,gfx.Copy_Buffer{transient,output,0,0,512})==.None)
    color:=gfx.Image_Access{target,gfx.image_full_range(target_desc),.Write,.Color_Attachment};sampled:=gfx.Image_Access{image,gfx.image_full_range(source_desc),.Read,.Sampled}
    rendered,_:=gfx.graph_pass(&graph,"sample-after-placement-handoff",.Graphics,nil,images={color,sampled})
    tint:=[4]f32{1,1,1,1}
    assert(gfx.graph_set_packet(&graph,rendered,gfx.Render{colors={{color,.Clear,.Store,{0,0,0,1}}},images={{2,1,{.Fragment},{sampled}}},samplers={{2,2,{.Fragment},sampler}},phases={{pipeline=pipeline,constants={{group=2,slot=0,stages={.Fragment},usage=.Uniform,bytes=mem.slice_to_bytes(tint[:])}},draws={gfx.Draw{3,1,0,0}}}}})==.None)
    copied,_:=gfx.graph_pass(&graph,"native-rendered-readback",.Transfer,{{output,{512,512},.Write,.Transfer_Destination}},images={{target,gfx.image_full_range(target_desc),.Read,.Transfer_Source}})
    assert(gfx.graph_set_packet(&graph,copied,gfx.Copy_Image_Buffer{target,{0,0,0,0,16,8,.Color,0,1,0,0},output,512})==.None)
    plan,plan_error:=gfx.graph_compile(&graph);assert(plan_error==.None)
    allocation,allocation_error:=gfx.graph_allocation_plan(&graph,&plan,allocation_query(&r));assert(allocation_error==.None)
    first:gfx.Capture_Snapshot
    first_submission:u64
    for round in 0..<19 {
        if round==18 { assert(capture_enable(&r,false)==.None) }
        owned,owner_error:=gfx.graph_allocate(&allocation,allocation_api(&r));assert(owner_error==.None)
        append(&owned.textures,gfx.Texture_Input{image,source})
        readback:gfx.Buffer_Handle;for binding in owned.buffers { if binding.resource==output { readback=binding.handle } }
        token,acquire_error:=acquire(&r);assert(acquire_error==.None)
        submission,native_error,packet_error:=submit(&r,token,&graph,&plan,owned.buffers[:],owned.textures[:]);assert(native_error==.None && packet_error==.None)
        if round<18 {
            pending,pending_exists:=capture_snapshot(&r,submission.id);assert(pending_exists)
            testing.expect(t,pending.submission==submission.id && pending.slot==token.slot && pending.generation==token.generation && pending.feedback!=.Failed)
            if round==0 { first_submission=submission.id;first=pending } else { gfx.capture_snapshot_destroy(&pending) }
        }
        assert(wait(&r,submission)==.None)
        result:[1024]byte;assert(read_buffer(&r,readback,0,result[:])==.None)
        for i in 0..<128 { assert(result[i*4]==42 && result[i*4+1]==41 && result[i*4+2]==137 && result[i*4+3]==229);for channel in 0..<4 { assert(result[512+i*4+channel]==source_bytes[channel]) } }
        if round<18 {
            completed,exists:=capture_snapshot(&r,submission.id);assert(exists && completed.feedback==.Completed)
            verify_native_capture(t,&completed)
            gfx.capture_snapshot_destroy(&completed)
        } else { _,exists:=capture_snapshot(&r,submission.id);testing.expect(t,!exists) }
        // The imported source belongs to the test rather than the allocation group.
        pop(&owned.textures);assert(gfx.graph_allocations_destroy(&owned)==.None)
    }
    testing.expect(t,len(r.capture.accepted)==16)
    _,evicted:=capture_snapshot(&r,first_submission);testing.expect(t,!evicted)
    assert(capture_enable(&r,true)==.None)
    latest,latest_exists:=capture_snapshot(&r);assert(latest_exists);latest_id:=latest.submission;gfx.capture_snapshot_destroy(&latest)
    assert(destroy_graphics_pipeline(&r,pipeline)==.None)
    rejected,rejected_owner_error:=gfx.graph_allocate(&allocation,allocation_api(&r));assert(rejected_owner_error==.None);append(&rejected.textures,gfx.Texture_Input{image,source})
    rejected_token,rejected_acquire:=acquire(&r);assert(rejected_acquire==.None)
    _,rejected_native,rejected_packet:=submit(&r,rejected_token,&graph,&plan,rejected.buffers[:],rejected.textures[:])
    testing.expect(t,rejected_native==.Invalid_Graph && rejected_packet==.Invalid_Pipeline && !r.capture.recording && len(r.capture.accepted)==16)
    assert(abort(&r,rejected_token)==.None);pop(&rejected.textures);assert(gfx.graph_allocations_destroy(&rejected)==.None)
    preserved,preserved_exists:=capture_snapshot(&r);assert(preserved_exists && preserved.submission==latest_id);gfx.capture_snapshot_destroy(&preserved)
    gfx.allocation_plan_destroy(&allocation);gfx.compiled_graph_destroy(&plan);gfx.graph_destroy(&graph)
    assert(destroy_texture(&r,source)==.None && destroy_sampler(&r,sampler)==.None && renderer_destroy(&r)==.None)
    verify_native_capture(t,&first);testing.expect(t,first.passes[rendered.index].name=="sample-after-placement-handoff")
    gfx.capture_snapshot_destroy(&first)
    fmt.println("Metal passive capture PASS: real wordfill+mixed placement alias+sampled pixels, native compute/render encoders, global scopes, immutable tables, residency, actual sizes and terminal feedback;16 bounded captures;clone survives19slotreuse and renderer/graph teardown;disabled frame unchanged")
}
@(private="package")
verify_native_capture :: proc(t:^testing.T,c:^gfx.Capture_Snapshot) {
    begins,ends,pass_begins,pass_ends,barriers,tables,residency,allocations,aliases,images,samplers,attachments,pipelines:int
    findings:=gfx.capture_compare(c);if len(findings)>0 { fmt.println("native Metal capture divergences",findings) };testing.expect(t,len(findings)==0);delete(findings)
    previous_pass:= -1
    for event in c.events {
        #partial switch event.kind {
        case .Pass_Begin:testing.expect(t,event.pass_index==previous_pass+1);previous_pass=event.pass_index;pass_begins+=1
        case .Pass_End:testing.expect(t,event.pass_index==previous_pass);pass_ends+=1
        case .Encoder_Begin:testing.expect(t,event.encoder!=0);begins+=1
        case .Encoder_End:testing.expect(t,event.encoder!=0);ends+=1
        case .Global_Barrier:testing.expect(t,event.emitted && event.source_stages==u64(max(int)) && event.destination_stages==u64(max(int)) && event.native_visibility==3);barriers+=1
        case .Argument_Table:testing.expect(t,event.table!=0 && event.pipeline!=0 && event.layout!=0);tables+=1
        case .Residency:testing.expect(t,event.emitted && event.object!=0 && event.table!=0);residency+=1
        case .Allocation:testing.expect(t,event.object!=0 && event.size>0);allocations+=1
        case .Alias:testing.expect(t,event.emitted && event.heap!=0 && event.resource_kind==.Image && event.alias_previous_kind==.Buffer && event.previous_pass_index==1 && event.pass_index==2);aliases+=1
        case .Bind_Image:
            if event.binding_path==.Descriptor { testing.expect(t,event.group==2 && event.binding==1 && event.table!=0 && event.pipeline!=0 && event.image_range.mip_count==1);images+=1 }
        case .Bind_Sampler:testing.expect(t,event.group==2 && event.binding==2 && event.table!=0 && event.pipeline!=0);samplers+=1
        case .Attachment:testing.expect(t,event.native_load==2 && event.native_store==1 && event.image_range.aspects=={.Color});attachments+=1
        case .Bind_Pipeline:testing.expect(t,event.pipeline!=0 && event.encoder!=0);pipelines+=1
        }
    }
    testing.expect(t,begins==4 && ends==4 && pass_begins==4 && pass_ends==4 && barriers==4 && aliases==1 && attachments==1)
    testing.expect(t,tables==3 && pipelines==2 && images==1 && samplers==1 && allocations>=6 && residency>=6 && len(c.expected)>=14)
}
