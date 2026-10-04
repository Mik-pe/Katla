#+test
package gfx

import "core:testing"

@(test)
test_resource_identity_and_retirement :: proc(t:^testing.T) {
    a,b:Resource_Storage(i32,Buffer_Kind); storage_init(&a); storage_init(&b)
    defer storage_destroy(&a); defer storage_destroy(&b)
    first:=storage_insert(&a,12)
    _,other_owner:=storage_get(&b,first); testing.expect(t,!other_owner)
    value,ok:=storage_remove(&a,first); testing.expect(t,ok && value==12)
    second:=storage_insert(&a,34); testing.expect(t,second.index==first.index && second.generation!=first.generation)
    _,stale:=storage_get(&a,first); testing.expect(t,!stale)
    _,removed:=storage_remove(&a,first); testing.expect(t,!removed)
    storage_remove(&a,second)
    a.slots[second.index].generation=max(u32)
    last:=storage_insert(&a,56); storage_remove(&a,last)
    next:=storage_insert(&a,78); testing.expect(t,next.index!=last.index && a.slots[last.index].retired)
    storage_remove(&a,next)
}
@(test)
test_frame_exact_completion_and_abort :: proc(t:^testing.T) {
    f,other:Frames; frames_init(&f,2); frames_init(&other,1)
    defer frames_destroy(&f); defer frames_destroy(&other)
    token,err:=frame_acquire(&f,0); testing.expect_value(t,err,Frame_Error.None)
    _,busy:=frame_acquire(&f,0); testing.expect_value(t,busy,Frame_Error.Busy)
    testing.expect_value(t,frame_recorded(&other,token),Frame_Error.Invalid_Token)
    testing.expect_value(t,frame_recorded(&f,token),Frame_Error.None)
    submission,accepted:=frame_submitted(&f,token); testing.expect(t,accepted==.None && submission==1)
    testing.expect_value(t,frame_abort(&f,token),Frame_Error.Invalid_State)
    testing.expect_value(t,frame_completed(&f,token,submission+1),Frame_Error.Invalid_State)
    testing.expect_value(t,frame_completed(&f,token,submission),Frame_Error.None)
    replacement,_:=frame_acquire(&f,0)
    testing.expect_value(t,frame_abort(&f,token),Frame_Error.Invalid_Token)
    testing.expect_value(t,frame_abort(&f,replacement),Frame_Error.None)
    testing.expect_value(t,f.next_submission,u64(1))
}
@(test)
test_graph_live_hazards_and_disjoint_ranges :: proc(t:^testing.T) {
    g:Buffer_Graph; graph_init(&g); defer graph_destroy(&g)
    desc:=Buffer_Desc{64,{.Storage,.Transfer_Source,.Transfer_Destination,.Readback}}
    scratch,_:=graph_buffer(&g,desc,false,false)
    output,_:=graph_buffer(&g,desc,false,true)
    dead,_:=graph_buffer(&g,desc,false,false)
    p0,_:=graph_pass(&g,"write first",.Compute,{{scratch,{0,32},.Write,.Storage}})
    p1,_:=graph_pass(&g,"write second",.Compute,{{scratch,{32,32},.Write,.Storage}})
    graph_pass(&g,"dead",.Compute,{{dead,{0,64},.Write,.Storage}})
    p3,_:=graph_pass(&g,"copy",.Transfer,{{scratch,{0,64},.Read,.Transfer_Source},{output,{0,64},.Write,.Transfer_Destination}})
    plan,err:=graph_compile(&g); defer compiled_graph_destroy(&plan)
    testing.expect_value(t,err,Graph_Error.None); testing.expect_value(t,len(plan.order),3)
    testing.expect(t,plan.order[0]==p0 && plan.order[1]==p1 && plan.order[2]==p3)
    testing.expect_value(t,len(plan.hazards),2)
    testing.expect(t,plan.hazards[0].before==p0 && plan.hazards[1].before==p1)
}
@(test)
test_graph_rejection_and_initialization_gaps :: proc(t:^testing.T) {
    g,other:Buffer_Graph; graph_init(&g); graph_init(&other); defer graph_destroy(&g); defer graph_destroy(&other)
    buffer,_:=graph_buffer(&g,{64,{.Storage}},false,true)
    other_owner,_:=graph_buffer(&other,{64,{.Storage}},true,false)
    for access in ([]Buffer_Access{{other_owner,{0,16},.Read,.Storage},{buffer,{max(u64),16},.Write,.Storage},{buffer,{0,0},.Write,.Storage},{buffer,{0,16},.Read,.Transfer_Source}}) {
        _,err:=graph_pass(&g,"bad",.Compute,{access}); testing.expect(t,err!=.None)
    }
    testing.expect_value(t,len(g.passes),0)
    graph_pass(&g,"head",.Compute,{{buffer,{0,16},.Write,.Storage}})
    graph_pass(&g,"tail",.Compute,{{buffer,{32,32},.Write,.Storage}})
    graph_pass(&g,"gap",.Compute,{{buffer,{0,64},.Read_Write,.Storage}})
    plan,err:=graph_compile(&g); defer compiled_graph_destroy(&plan)
    testing.expect_value(t,err,Graph_Error.Uninitialized_Read)
    testing.expect_value(t,len(plan.order),0)
}

@(test)
test_export_initialization_and_read_only_hazards :: proc(t:^testing.T) {
    g:Buffer_Graph; graph_init(&g); defer graph_destroy(&g)
    buffer,_:=graph_buffer(&g,{64,{.Storage}},false,true)
    graph_pass(&g,"partial",.Compute,{{buffer,{0,32},.Write,.Storage}})
    empty,rejected:=graph_compile(&g); defer compiled_graph_destroy(&empty)
    testing.expect_value(t,rejected,Graph_Error.Uninitialized_Read)
    graph_pass(&g,"complete",.Compute,{{buffer,{32,32},.Write,.Storage}})
    read_a,_:=graph_pass(&g,"read a",.Compute,{{buffer,{0,64},.Read,.Storage}},side_effect=true)
    read_b,_:=graph_pass(&g,"read b",.Compute,{{buffer,{0,64},.Read,.Storage}},side_effect=true)
    plan,err:=graph_compile(&g); defer compiled_graph_destroy(&plan)
    testing.expect_value(t,err,Graph_Error.None)
    testing.expect_value(t,len(plan.order),4)
    testing.expect_value(t,len(plan.hazards),4)
    for hazard in plan.hazards { testing.expect(t,!(hazard.before==read_a && hazard.after==read_b)) }
    count:=len(g.passes)
    _,duplicate:=graph_pass(&g,"duplicate",.Compute,{{buffer,{0,32},.Read,.Storage},{buffer,{16,16},.Write,.Storage}})
    testing.expect_value(t,duplicate,Graph_Error.Duplicate_Access)
    testing.expect_value(t,len(g.passes),count)
}
