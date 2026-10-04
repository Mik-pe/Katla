#+test
package gfx
import "core:testing"

capture_test_has :: proc(c:^Capture_Snapshot,kind:Capture_Divergence_Kind)->bool {
    findings:=capture_compare(c);defer delete(findings)
    for finding in findings { if finding.kind==kind { return true } };return false
}
capture_test_record :: proc(store:^Capture_Store,index:int,barrier:Capture_Event) {
    capture_record(store,{kind=.Pass_Begin,pass_index=index,resource_index= -1,emitted=true})
    capture_record(store,{kind=.Encoder_Begin,pass_index=index,resource_index= -1,encoder=u64(index+1),object=u64(index+1),emitted=true})
    observed:=barrier;observed.encoder=u64(index+1);capture_record(store,observed)
    capture_record(store,{kind=.Bind_Pipeline,pass_index=index,resource_index= -1,encoder=u64(index+1),pipeline=20,object=20,emitted=true})
    capture_record(store,{kind=.Argument_Table,pass_index=index,resource_index= -1,encoder=u64(index+1),pipeline=20,layout=30,table=40,object=40,emitted=true})
    capture_record(store,{kind=.Bind_Buffer,pass_index=index,resource_kind=.Buffer,resource_index=0,encoder=u64(index+1),pipeline=20,layout=30,table=40,object=50,buffer_range={16,16},emitted=true})
    capture_record(store,{kind=.Encoder_End,pass_index=index,resource_index= -1,encoder=u64(index+1),object=u64(index+1),emitted=true})
    capture_record(store,{kind=.Pass_End,pass_index=index,resource_index= -1,emitted=true})
}
capture_test_fixture :: proc(store:^Capture_Store,g:^Graph,backend:Capture_Backend)->Compiled_Graph {
    graph_init(g)
    buffer,error:=graph_buffer(g,{64,{.Storage},.CPU_Visible},true,false);assert(error==.None)
    _,error=graph_pass(g,"producer",.Compute,{{buffer,{16,16},.Write,.Storage}},side_effect=true);assert(error==.None)
    _,error=graph_pass(g,"consumer",.Compute,{{buffer,{16,16},.Read,.Storage}},side_effect=true);assert(error==.None)
    _,error=graph_pass(g,"culled",.Transfer,nil);assert(error==.None)
    plan,compile_error:=graph_compile(g);assert(compile_error==.None)
    capture_init(store);store.enabled=true;capture_begin(store,backend,{g,0,9},g,&plan)
    capture_record(store,{kind=.Allocation,pass_index= -1,resource_kind=.Buffer,resource_index=0,object=50,size=64,emitted=true})
    capture_record(store,{kind=.Residency,pass_index= -1,resource_kind=.Auxiliary,resource_index= -1,object=50,table=60,emitted=true})
    for index in 0..<2 {
        barrier:=Capture_Event{kind=.Buffer_Barrier,pass_index=index,resource_kind=.Buffer,resource_index=0,source_stages=2,destination_stages=4,source_access=8,destination_access=16,buffer_range={16,16},emitted=true}
        if backend==.Metal { barrier={kind=.Global_Barrier,pass_index=index,resource_index= -1,source_stages=u64(max(int)),destination_stages=u64(max(int)),native_visibility=3,emitted=true} }
        capture_expect(store,barrier);capture_test_record(store,index,barrier)
    }
    capture_record(store,{kind=.Submit,pass_index= -1,resource_index= -1,emitted=true})
    capture_accept(store,{id=1,token={g,0,9}});return plan
}
@(test)
test_capture_compares_live_pass_order_ranges_and_independent_native_scopes :: proc(t:^testing.T) {
    graph:Graph;store:Capture_Store;plan:=capture_test_fixture(&store,&graph,.Vulkan)
    defer graph_destroy(&graph);defer compiled_graph_destroy(&plan);defer capture_destroy(&store)
    snapshot,found:=capture_snapshot(&store);testing.expect(t,found);defer capture_snapshot_destroy(&snapshot)
    findings:=capture_compare(&snapshot);testing.expect_value(t,len(findings),0);delete(findings)
    barrier:=snapshot.events[4]
    snapshot.events[4].source_stages=7;testing.expect(t,capture_test_has(&snapshot,.Native_Scope));snapshot.events[4]=barrier
    snapshot.events[4].buffer_range={0,32};testing.expect(t,capture_test_has(&snapshot,.Native_Scope));snapshot.events[4]=barrier
    snapshot.events[4].emitted=false;testing.expect(t,capture_test_has(&snapshot,.Native_Scope));snapshot.events[4]=barrier
    saved:=snapshot.events[2];snapshot.events[2].pass_index=1;testing.expect(t,capture_test_has(&snapshot,.Pass_Order));snapshot.events[2]=saved
    saved=snapshot.events[7];snapshot.events[7].buffer_range={0,16};testing.expect(t,capture_test_has(&snapshot,.Undeclared_Resource));snapshot.events[7]=saved
    snapshot.events[7].buffer_range={60,16};testing.expect(t,capture_test_has(&snapshot,.Resource_Range));snapshot.events[7]=saved
    snapshot.events[7].resource_index=7;testing.expect(t,capture_test_has(&snapshot,.Resource_Ownership));snapshot.events[7]=saved
    snapshot.events[7].table=99;testing.expect(t,capture_test_has(&snapshot,.Table_Scope));snapshot.events[7]=saved
    saved=snapshot.events[5];snapshot.events[5].pipeline=0;testing.expect(t,capture_test_has(&snapshot,.Pipeline_Scope));snapshot.events[5]=saved
    saved=snapshot.events[1];snapshot.events[1].object=999;testing.expect(t,capture_test_has(&snapshot,.Residency_Scope));snapshot.events[1]=saved
    saved=snapshot.events[4];snapshot.events[4].kind=.Allocation;testing.expect(t,capture_test_has(&snapshot,.Missing_Event));snapshot.events[4]=saved
    append(&snapshot.events,capture_event_clone(barrier,context.allocator));testing.expect(t,capture_test_has(&snapshot,.Duplicate_Event))
    duplicate:=pop(&snapshot.events);delete(duplicate.label);delete(duplicate.reason)
    snapshot.events[4].pass_index=2;testing.expect(t,capture_test_has(&snapshot,.Invalid_Pass));testing.expect(t,capture_test_has(&snapshot,.Unexpected_Event));snapshot.events[4]=barrier
    snapshot.events[9].kind=.Allocation;testing.expect(t,capture_test_has(&snapshot,.Missing_Pass))
    snapshot.events[9].kind=.Pass_End
    clear(&snapshot.expected);snapshot.events[4].kind=.Allocation;snapshot.events[12].kind=.Allocation
    testing.expect(t,capture_test_has(&snapshot,.Dependency_Coverage))
}
@(test)
test_capture_preserves_global_metal_scope_and_explicit_no_call_translation :: proc(t:^testing.T) {
    graph:Graph;store:Capture_Store;plan:=capture_test_fixture(&store,&graph,.Metal)
    defer graph_destroy(&graph);defer compiled_graph_destroy(&plan);defer capture_destroy(&store)
    snapshot,found:=capture_snapshot(&store);testing.expect(t,found);defer capture_snapshot_destroy(&snapshot)
    findings:=capture_compare(&snapshot);testing.expect_value(t,len(findings),0);delete(findings)
    original:=snapshot.events[4];snapshot.events[4].native_visibility=1;testing.expect(t,capture_test_has(&snapshot,.Native_Scope));snapshot.events[4]=original
    snapshot.events[4].kind=.Buffer_Barrier;testing.expect(t,capture_test_has(&snapshot,.Missing_Event));testing.expect(t,capture_test_has(&snapshot,.Unexpected_Event));snapshot.events[4]=original
    snapshot.expected[0].emitted=false;snapshot.events[4].emitted=false
    findings=capture_compare(&snapshot);testing.expect_value(t,len(findings),0);delete(findings)
    snapshot.events[4].emitted=true;testing.expect(t,capture_test_has(&snapshot,.Native_Scope))
}
@(test)
test_capture_snapshot_owns_names_ranges_feedback_and_survives_rejection_slot_reuse :: proc(t:^testing.T) {
    graph:Graph;store:Capture_Store;plan:=capture_test_fixture(&store,&graph,.Vulkan)
    defer graph_destroy(&graph);defer compiled_graph_destroy(&plan);defer capture_destroy(&store)
    independent,found:=capture_snapshot(&store);testing.expect(t,found);defer capture_snapshot_destroy(&independent)
    name_bytes:=transmute([]byte)graph.passes[0].name;name_bytes[0]='X'
    graph.passes[0].accesses[0].range={0,1}
    testing.expect_value(t,independent.passes[0].name,"producer");testing.expect_value(t,independent.passes[0].buffers[0].range,(Buffer_Range{16,16}))
    graph.revision+=1
    capture_begin(&store,.Vulkan,{&graph,1,10},&graph,&plan);testing.expect(t,!store.recording)
    selected,retained:=capture_snapshot(&store,1);testing.expect(t,retained);capture_snapshot_destroy(&selected)
    graph.revision=plan.revision
    capture_begin(&store,.Vulkan,{&graph,1,11},&graph,&plan)
    capture_record(&store,{kind=.Pass_Begin,pass_index=0,resource_index= -1,label="failed encode"});capture_abandon(&store)
    testing.expect_value(t,len(store.accepted),1)
    for id in u64(2)..<20 {
        capture_begin(&store,.Vulkan,{&graph,int(id%3),id+11},&graph,&plan)
        capture_accept(&store,{id=id,token={&graph,int(id%3),id+11}})
    }
    testing.expect_value(t,len(store.accepted),16);_,retained=capture_snapshot(&store,1);testing.expect(t,!retained)
    capture_feedback(&store,19,.Completed);testing.expect_value(t,store.accepted[len(store.accepted)-1].feedback,Capture_Feedback.Completed)
    testing.expect_value(t,independent.feedback,Capture_Feedback.Pending);testing.expect_value(t,independent.generation,u64(9))
    findings:=capture_compare(&independent);defer delete(findings);testing.expect_value(t,len(findings),0)
}

@(test)
test_capture_image_ranges_layouts_and_vulkan_encoder_span_are_independent :: proc(t:^testing.T) {
    image_range:=Image_Range{1,1,0,1,{.Depth}}
    snapshot:=Capture_Snapshot{schema_version=1,native_captured=true,backend=.Vulkan,
        passes=make([dynamic]Capture_Pass),images=make([dynamic]Capture_Image),events=make([dynamic]Capture_Event),expected=make([dynamic]Capture_Event)}
    defer capture_snapshot_destroy(&snapshot)
    accesses:=make([]Capture_Image_Access,1);accesses[0]={3,image_range,.Read_Write,.Depth_Attachment}
    append(&snapshot.passes,Capture_Pass{index=5,order=0,live=true,images=accesses})
    append(&snapshot.images,Capture_Image{index=3,desc={width=8,height=8,mip_levels=3,layers=2,depth=1,format=.D32_Float_S8_Uint,usage={.Depth_Attachment,.Sampled}}})
    expected:=Capture_Event{kind=.Image_Barrier,pass_index=5,resource_kind=.Image,resource_index=3,image_range=image_range,source_stages=2,destination_stages=4,source_access=8,destination_access=16,old_layout=3,new_layout=7,emitted=true}
    append(&snapshot.expected,expected)
    append(&snapshot.events,Capture_Event{kind=.Encoder_Begin,pass_index= -1,resource_index= -1,encoder=1,object=1})
    append(&snapshot.events,Capture_Event{kind=.Pass_Begin,pass_index=5,resource_index= -1,encoder=1})
    observed:=expected;observed.encoder=1;append(&snapshot.events,observed)
    append(&snapshot.events,Capture_Event{kind=.Attachment,pass_index=5,resource_kind=.Image,resource_index=3,image_range=image_range,encoder=1})
    append(&snapshot.events,Capture_Event{kind=.Pass_End,pass_index=5,resource_index= -1,encoder=1})
    append(&snapshot.events,Capture_Event{kind=.Encoder_End,pass_index= -1,resource_index= -1,encoder=1,object=1})
    findings:=capture_compare(&snapshot);testing.expect_value(t,len(findings),0);delete(findings)
    snapshot.events[2].new_layout=9;testing.expect(t,capture_test_has(&snapshot,.Native_Scope));snapshot.events[2]=observed
    snapshot.events[2].image_range.layer_count=2;testing.expect(t,capture_test_has(&snapshot,.Native_Scope));snapshot.events[2]=observed
    snapshot.events[3].image_range.aspects={.Stencil};testing.expect(t,capture_test_has(&snapshot,.Undeclared_Resource))
    snapshot.events[3].image_range.base_mip=3;testing.expect(t,capture_test_has(&snapshot,.Resource_Range))
    snapshot.native_captured=false
    findings=capture_compare(&snapshot);defer delete(findings);testing.expect_value(t,len(findings),0)
}

@(test)
test_capture_dependency_coverage_requires_every_image_cell_even_if_both_traces_omit_it :: proc(t:^testing.T) {
    snapshot:=Capture_Snapshot{schema_version=1,native_captured=true,backend=.Vulkan,
        passes=make([dynamic]Capture_Pass),images=make([dynamic]Capture_Image),dependencies=make([dynamic]Capture_Dependency),events=make([dynamic]Capture_Event),expected=make([dynamic]Capture_Event)}
    defer capture_snapshot_destroy(&snapshot)
    append(&snapshot.passes,Capture_Pass{index=0,order=0,live=true},Capture_Pass{index=1,order=1,live=true})
    desc:=Texture_Desc{width=8,height=8,mip_levels=2,layers=2,depth=1,format=.D32_Float_S8_Uint,usage={.Depth_Attachment}}
    append(&snapshot.images,Capture_Image{index=0,desc=desc})
    append(&snapshot.dependencies,Capture_Dependency{before=0,after=1,resource_index=0,resource_kind=.Image,source_image=image_full_range(desc),destination_image=image_full_range(desc),source_mode=.Write,destination_mode=.Read})
    append(&snapshot.events,Capture_Event{kind=.Encoder_Begin,pass_index= -1,resource_index= -1,object=1,encoder=1},Capture_Event{kind=.Pass_Begin,pass_index=0,resource_index= -1,encoder=1},Capture_Event{kind=.Pass_End,pass_index=0,resource_index= -1,encoder=1},Capture_Event{kind=.Pass_Begin,pass_index=1,resource_index= -1,encoder=1})
    for mip in u32(0)..<2 { for layer in u32(0)..<2 { for aspect in (Image_Aspects{.Depth,.Stencil}) {
        required:=Capture_Event{kind=.Image_Barrier,pass_index=1,resource_kind=.Image,resource_index=0,source_stages=2,destination_stages=4,source_access=8,destination_access=16,image_range={mip,1,layer,1,{aspect}},old_layout=3,new_layout=7,emitted=true}
        append(&snapshot.expected,required);required.encoder=1;append(&snapshot.events,required)
    } } }
    append(&snapshot.events,Capture_Event{kind=.Pass_End,pass_index=1,resource_index= -1,encoder=1},Capture_Event{kind=.Encoder_End,pass_index= -1,resource_index= -1,object=1,encoder=1})
    findings:=capture_compare(&snapshot);testing.expect_value(t,len(findings),0);delete(findings)
    ordered_remove(&snapshot.expected,3);ordered_remove(&snapshot.events,7)
    findings=capture_compare(&snapshot);defer delete(findings)
    testing.expect_value(t,len(findings),1);testing.expect_value(t,findings[0].kind,Capture_Divergence_Kind.Dependency_Coverage);testing.expect_value(t,findings[0].dependency_index,0)
}

@(test)
test_capture_alias_predecessor_identity_and_source_pass_are_exact_native_scope :: proc(t:^testing.T) {
    graph:Graph;store:Capture_Store;plan:=capture_test_fixture(&store,&graph,.Metal)
    defer graph_destroy(&graph);defer compiled_graph_destroy(&plan);defer capture_destroy(&store)
    snapshot,found:=capture_snapshot(&store);testing.expect(t,found);defer capture_snapshot_destroy(&snapshot)
    alias:=Capture_Event{kind=.Alias,pass_index=1,resource_kind=.Buffer,resource_index=0,alias_previous_kind=.Image,alias_previous_index=2,previous_pass_index=0,native_visibility=2,emitted=false}
    append(&snapshot.expected,alias);append(&snapshot.events,alias)
    findings:=capture_compare(&snapshot);testing.expect_value(t,len(findings),0);delete(findings)
    observed:=&snapshot.events[len(snapshot.events)-1]
    observed.alias_previous_kind=.Buffer;testing.expect(t,capture_test_has(&snapshot,.Native_Scope));observed^=alias
    observed.alias_previous_index=3;testing.expect(t,capture_test_has(&snapshot,.Native_Scope));observed^=alias
    observed.previous_pass_index=1;testing.expect(t,capture_test_has(&snapshot,.Native_Scope))
}

@(test)
test_capture_transfer_operands_preserve_fill_pattern_and_pitched_image_region :: proc(t:^testing.T) {
    graph:Graph;store:Capture_Store;plan:=capture_test_fixture(&store,&graph,.Vulkan)
    defer graph_destroy(&graph);defer compiled_graph_destroy(&plan);defer capture_destroy(&store)
    snapshot,found:=capture_snapshot(&store);testing.expect(t,found);defer capture_snapshot_destroy(&snapshot)
    desc:=Texture_Desc{width=16,height=16,mip_levels=1,layers=1,depth=1,format=.RGBA8_Unorm,usage={.Transfer_Destination}}
    append(&snapshot.images,Capture_Image{index=2,desc=desc})
    snapshot.passes[0].images=make([]Capture_Image_Access,1)
    snapshot.passes[0].images[0]={2,image_full_range(desc),.Write,.Transfer_Destination}
    fill:=Capture_Event{kind=.Bind_Buffer,pass_index=0,resource_kind=.Buffer,resource_index=0,binding_path=.Transfer,object=50,encoder=1,buffer_range={16,16},transfer_value=0x12345678,emitted=true}
    image:=Capture_Event{kind=.Bind_Image,pass_index=0,resource_kind=.Image,resource_index=2,binding_path=.Transfer,object=51,encoder=1,image_range=image_full_range(desc),transfer_region={x=1,y=2,width=4,height=3,depth=1,aspect=.Color,bytes_per_row=20,bytes_per_image=100},emitted=true}
    append(&snapshot.expected,capture_event_clone(snapshot.events[7],context.allocator),capture_event_clone(snapshot.events[15],context.allocator),fill,image)
    append(&snapshot.events,fill,image)
    for i:=len(snapshot.events)-1;i>=6;i-=1 { snapshot.events[i]=snapshot.events[i-2] }
    snapshot.events[4]=fill;snapshot.events[5]=image
    findings:=capture_compare(&snapshot);testing.expect_value(t,len(findings),0);delete(findings)
    snapshot.events[4].transfer_value=0x12345679;testing.expect(t,capture_test_has(&snapshot,.Native_Scope));snapshot.events[4]=fill
    snapshot.events[5].transfer_region.bytes_per_row=24;testing.expect(t,capture_test_has(&snapshot,.Native_Scope));snapshot.events[5]=image
    snapshot.events[5].transfer_region.z=1;testing.expect(t,capture_test_has(&snapshot,.Native_Scope))
}
