#+test
package ecs

import "core:testing"
import "core:mem"
import "core:sync"
import "base:intrinsics"

Test_Position :: struct { x,y,z:f32 }
Test_Velocity :: struct { x,y,z:f32 }
Test_Tag :: struct {}
Test_Health :: struct { value:i32 }
Test_Stats :: struct { visits:int }
Test_Event :: struct { value:int }
Test_Row :: struct { position:Write(Test_Position), velocity:Read(Test_Velocity) }
Test_Read_Row :: struct { position:Read(Test_Position) }
Test_Params :: struct { query:Query(Test_Row,No_Filter) }

@(test)
test_entity_generations :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    a:=create_entity(&w); b:=create_entity(&w)
    testing.expect(t,a!=b && w.live_count==2)
    testing.expect(t,destroy_entity(&w,a))
    c:=create_entity(&w)
    testing.expect(t,entity_index(a)==entity_index(c) && a!=c)
    testing.expect(t,!entity_exists(&w,a) && entity_exists(&w,c))
    testing.expect(t,!destroy_entity(&w,a) && validate(&w))
}
@(test)
test_generation_exhaustion :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    a:=create_entity(&w)
    w.slots[entity_index(a)].generation=max(u32)
    last:=entity_id(u32(entity_index(a)),max(u32))
    destroy_entity(&w,last)
    clear_entities(&w)
    b:=create_entity(&w)
    testing.expect(t,entity_index(a)!=entity_index(b) && validate(&w))
}
@(test)
test_clear_invalidates_ids :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    a:=spawn(&w,struct{p:Test_Position}{Test_Position{1,2,3}})
    clear_entities(&w)
    b:=create_entity(&w)
    testing.expect(t,a!=b && !entity_exists(&w,a) && w.live_count==1 && validate(&w))
}
@(test)
test_sparse_pages_and_swap_remove :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    ids:=make([dynamic]Entity_Id); defer delete(ids)
    for i in 0..<3100 { append(&ids,spawn(&w,struct{h:Test_Health}{Test_Health{i32(i)}})) }
    for i in 0..<3100 { if i%3==0 { destroy_entity(&w,ids[i]) } }
    for i in 0..<3100 {
        v,ok:=get_component(&w,ids[i],Test_Health)
        testing.expect(t,ok==(i%3!=0))
        if ok { testing.expect_value(t,v.value,i32(i)) }
    }
    testing.expect(t,validate(&w))
}
@(test)
test_lifecycle_and_replacement :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    a:=spawn(&w,struct{p:Test_Position}{Test_Position{1,2,3}})
    add_component(&w,a,Test_Position{4,5,6})
    testing.expect_value(t,len(w.component_events),2)
    destroy_entity(&w,a)
    testing.expect_value(t,len(w.component_events),3)
    testing.expect_value(t,w.component_events[2].kind,Component_Event_Kind.Removed)
    testing.expect_value(t,w.entity_events[1].kind,Entity_Event_Kind.Destroyed)
    testing.expect(t,!add_component(&w,a,Test_Position{}) && !remove_component(&w,a,Test_Position))
    testing.expect_value(t,world_update(&w,0),System_Error.None)
    testing.expect_value(t,len(w.entity_events)+len(w.component_events),0)
}
@(test)
test_query_join_filters_and_changed :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    a:=spawn(&w,struct{p:Test_Position,v:Test_Velocity}{Test_Position{1,0,0},Test_Velocity{2,0,0}})
    b:=spawn(&w,struct{p:Test_Position,v:Test_Velocity,tag:Test_Tag}{Test_Position{9,0,0},Test_Velocity{3,0,0},{}})
    clear_changed(&w)
    Filter::struct{tag:Without(Test_Tag)}
    q:=query_begin(&w,Test_Row,Filter)
    testing.expect_value(t,len(query_entities(&q)),1)
    for id in query_entities(&q) {
        row,ok:=query_row(&q,id); testing.expect(t,ok)
        write(row.position).x+=read(row.velocity).x
    }
    query_end(&w,&q)
    testing.expect(t,component_changed(&w,a,Test_Position) && !component_changed(&w,b,Test_Position))
    changed:=query_begin(&w,Test_Read_Row,No_Filter,changed_only=true)
    testing.expect_value(t,len(query_entities(&changed)),1)
    query_end(&w,&changed)
    v,ok:=get_component(&w,a,Test_Position); testing.expect(t,ok && v.x==3)
}
@(test)
test_direct_query_marks_entire_column :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    a:=spawn(&w,struct{p:Test_Position,v:Test_Velocity}{})
    b:=spawn(&w,struct{p:Test_Position}{})
    clear_changed(&w)
    q:=query_begin(&w,Test_Row,No_Filter,direct=true)
    query_end(&w,&q)
    testing.expect(t,component_changed(&w,a,Test_Position) && component_changed(&w,b,Test_Position))
}
@(test)
test_resources_replace_and_remove :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    insert_resource(&w,Test_Stats{1})
    get_resource_mut(&w,Test_Stats).visits+=2
    stats,ok:=get_resource(&w,Test_Stats); testing.expect(t,ok && stats.visits==3)
    insert_resource(&w,Test_Stats{7})
    testing.expect(t,remove_resource(&w,Test_Stats) && !remove_resource(&w,Test_Stats))
}

test_move :: proc(_:^Test_Stats,p:^Test_Params,dt:f32)->System_Error {
    for id in query_entities(&p.query) {
        row,ok:=query_row(&p.query,id); assert(ok)
        write(row.position).x+=read(row.velocity).x*dt
    }
    return .None
}
@(test)
test_typed_system_cache_churn :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    a:=spawn(&w,struct{p:Test_Position,v:Test_Velocity}{Test_Position{},Test_Velocity{2,0,0}})
    _,err:=register_typed_system(&w,Test_Stats{},Test_Params,test_move)
    testing.expect_value(t,err,System_Error.None)
    world_update(&w,1)
    destroy_entity(&w,a)
    b:=spawn(&w,struct{p:Test_Position,v:Test_Velocity}{Test_Position{10,0,0},Test_Velocity{3,0,0}})
    for _ in 0..<100 { spawn(&w,struct{p:Test_Position,v:Test_Velocity}{}) }
    world_update(&w,2)
    value,ok:=get_component(&w,b,Test_Position)
    testing.expect(t,ok && value.x==16 && !entity_exists(&w,a) && validate(&w))
}
Bad_Params :: struct { a:Query(Test_Row,No_Filter), b:Query(Test_Read_Row,No_Filter) }
@(test)
test_registration_rejects_aliases :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    callback:=proc(_:^Test_Stats,_:^Bad_Params,_:f32)->System_Error { return .None }
    handle,err:=register_typed_system(&w,Test_Stats{},Bad_Params,callback)
    testing.expect(t,handle==nil && err==.Invalid_Access && len(w.systems)==0)
}
Resource_Params :: struct { stats:Res_Mut(Test_Stats), local:Local(int), optional:Optional_Res(Test_Health) }
test_resource_tick :: proc(_:^Test_Tag,p:^Resource_Params,_:f32)->System_Error {
    local(p.local)^+=1
    resource_write(p.stats).visits+=local(p.local)^
    _,ok:=optional_resource_read(p.optional); assert(!ok)
    return .None
}
@(test)
test_resource_params_local_and_missing :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    _,err:=register_typed_system(&w,Test_Tag{},Resource_Params,test_resource_tick)
    testing.expect_value(t,err,System_Error.None)
    testing.expect_value(t,world_update(&w,0),System_Error.Missing_Resource)
    insert_resource(&w,Test_Stats{})
    world_update(&w,0); world_update(&w,0)
    stats,_:=get_resource(&w,Test_Stats)
    testing.expect_value(t,stats.visits,3)
}
Command_Params :: struct { commands:Commands }
test_command_tick :: proc(_:^Test_Tag,p:^Command_Params,_:f32)->System_Error {
    command_spawn(p.commands,struct{p:Test_Position,v:Test_Velocity}{Test_Position{1,0,0},Test_Velocity{4,0,0}})
    return .None
}
@(test)
test_commands_visible_next_batch :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    register_typed_system(&w,Test_Tag{},Command_Params,test_command_tick,EARLY)
    register_typed_system(&w,Test_Stats{},Test_Params,test_move,LATE)
    world_update(&w,1)
    ids:=entity_ids(&w); defer delete(ids)
    value,ok:=get_component(&w,ids[0],Test_Position)
    testing.expect(t,ok && value.x==5)
}
@(test)
test_command_fifo_and_stale_targets :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    a:=create_entity(&w)
    queue:Command_Queue; commands_init(&queue); defer commands_destroy(&queue)
    c:=Commands{queue=&queue}
    command_insert(c,a,Test_Health{1}); command_remove(c,a,Test_Health); command_insert(c,a,Test_Health{2})
    commands_apply(&queue,&w)
    value,ok:=get_component(&w,a,Test_Health); testing.expect(t,ok && value.value==2)
    command_destroy(c,a); command_insert(c,a,Test_Health{3}); commands_apply(&queue,&w)
    testing.expect(t,!entity_exists(&w,a) && validate(&w))
}
@(test)
test_system_failure_discards_batch_commands :: proc(t:^testing.T) {
    w:World; world_init(&w,2); defer world_destroy(&w)
    fail:=proc(_:^Test_Tag,p:^Command_Params,_:f32)->System_Error {
        command_spawn(p.commands,struct{p:Test_Position}{})
        return .Failed
    }
    register_typed_system(&w,Test_Tag{},Command_Params,fail)
    testing.expect_value(t,world_update(&w,0,true),System_Error.Failed)
    testing.expect_value(t,w.live_count,0)
    testing.expect(t,!w.frozen && !w.execution_active)
}
@(test)
test_event_cursors_retention_and_replacement :: proc(t:^testing.T) {
    events:Events(Test_Event); events_init(&events); defer events_destroy(&events)
    a,b:Event_Cursor
    event_send(&events,Test_Event{1})
    testing.expect_value(t,len(events_read(&events,&a)),1)
    testing.expect_value(t,len(events_read(&events,&a)),0)
    testing.expect_value(t,len(events_read(&events,&b)),1)
    events_clear(&events); event_send(&events,Test_Event{2})
    testing.expect_value(t,events_read(&events,&a)[0].value,2)
    events_destroy(&events); events_init(&events); event_send(&events,Test_Event{3})
    testing.expect_value(t,events_read(&events,&a)[0].value,3)
}
Writer_Params :: struct { writer:Event_Writer(Test_Event) }
Reader_Params :: struct { reader:Event_Reader(Test_Event), stats:Res_Mut(Test_Stats) }
test_writer :: proc(_:^Test_Tag,p:^Writer_Params,_:f32)->System_Error { writer_send(p.writer,Test_Event{7}); return .None }
test_reader :: proc(_:^Test_Tag,p:^Reader_Params,_:f32)->System_Error {
    for e in reader_read(p.reader) { resource_write(p.stats).visits+=e.value }
    return .None
}
@(test)
test_event_system_ordering :: proc(t:^testing.T) {
    w:World; world_init(&w,2); defer world_destroy(&w)
    insert_resource(&w,Test_Stats{})
    register_typed_system(&w,Test_Tag{},Writer_Params,test_writer)
    register_typed_system(&w,Test_Tag{},Reader_Params,test_reader)
    w.parallel_work_threshold=0
    world_update(&w,0,true); clear_events(&w,Test_Event); world_update(&w,0,true)
    stats,_:=get_resource(&w,Test_Stats); testing.expect_value(t,stats.visits,14)
    replace_events(&w,Test_Event); world_update(&w,0,true)
    stats,_=get_resource(&w,Test_Stats); testing.expect_value(t,stats.visits,21)
}
Overlap_State :: struct { entered:^i32, barrier:^sync.Barrier, thread_id:^int }
Overlap_A :: struct { resource:Res_Mut(Test_Stats) }
Overlap_B :: struct { resource:Res_Mut(Test_Health) }
test_overlap_a :: proc(s:^Overlap_State,p:^Overlap_A,_:f32)->System_Error {
    intrinsics.atomic_add(s.entered,1); s.thread_id^=sync.current_thread_id()
    sync.barrier_wait(s.barrier); resource_write(p.resource).visits+=1; return .None
}
test_overlap_b :: proc(s:^Overlap_State,p:^Overlap_B,_:f32)->System_Error {
    intrinsics.atomic_add(s.entered,1); s.thread_id^=sync.current_thread_id()
    sync.barrier_wait(s.barrier); resource_write(p.resource).value+=1; return .None
}
@(test)
test_parallel_workers_really_overlap :: proc(t:^testing.T) {
    w:World; world_init(&w,2); defer world_destroy(&w)
    insert_resource(&w,Test_Stats{}); insert_resource(&w,Test_Health{})
    barrier:sync.Barrier; sync.barrier_init(&barrier,2)
    entered:i32; a,b:int
    register_typed_system(&w,Overlap_State{&entered,&barrier,&a},Overlap_A,test_overlap_a)
    register_typed_system(&w,Overlap_State{&entered,&barrier,&b},Overlap_B,test_overlap_b)
    w.parallel_work_threshold=0
    for _ in 0..<3 { testing.expect_value(t,world_update(&w,0,true),System_Error.None) }
    testing.expect(t,entered==6 && a!=b && (a!=sync.current_thread_id() || b!=sync.current_thread_id()))
}
@(test)
test_parallel_query_chunks :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    for _ in 0..<1000 { spawn(&w,struct{p:Test_Position,v:Test_Velocity}{Test_Position{1,0,0},Test_Velocity{2,0,0}}) }
    q:=query_begin(&w,Test_Row,No_Filter)
    callback:=proc(_:Entity_Id,row:Test_Row) { write(row.position).x+=read(row.velocity).x }
    query_parallel_each(&q,64,callback,4)
    for id in query_entities(&q) { row,_:=query_row(&q,id); testing.expect_value(t,write(row.position).x,f32(3)) }
    query_end(&w,&q)
    testing.expect(t,validate(&w))
}
Shutdown_State :: struct { shutdowns:^int, caller:int }
test_exclusive_clear :: proc(s:^Shutdown_State,w:^World,_:f32)->System_Error {
    assert(sync.current_thread_id()==s.caller)
    create_entity(w); clear_systems(w); return .None
}
test_shutdown :: proc(s:^Shutdown_State) { s.shutdowns^+=1 }
@(test)
test_exclusive_thread_affinity_and_shutdown :: proc(t:^testing.T) {
    w:World; world_init(&w,2); defer world_destroy(&w)
    count:int
    register_exclusive_system(&w,Shutdown_State{&count,sync.current_thread_id()},test_exclusive_clear,shutdown=test_shutdown)
    register_typed_system(&w,Test_Tag{},Command_Params,test_command_tick,LATE)
    world_update(&w,0,true)
    testing.expect(t,count==1 && len(w.systems)==0 && w.live_count==1)
}
@(test)
test_allocations_are_released :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator)
    defer mem.tracking_allocator_destroy(&tracker)
    w:World; world_init(&w,2,mem.tracking_allocator(&tracker))
    spawn(&w,struct{p:Test_Position,v:Test_Velocity}{})
    insert_resource(&w,Test_Stats{})
    register_typed_system(&w,Test_Tag{},Resource_Params,test_resource_tick)
    register_typed_system(&w,Test_Tag{},Writer_Params,test_writer)
    world_update(&w,0,true)
    world_destroy(&w)
    testing.expect_value(t,len(tracker.allocation_map),0)
}

Conflict_Params :: struct { stats:Res_Mut(Test_Stats) }
test_conflict_tick :: proc(s:^int,p:^Conflict_Params,_:f32)->System_Error {
    resource_write(p.stats).visits=resource_write(p.stats).visits*10+s^; return .None
}
@(test)
test_conflicts_preserve_registration_and_enabled_state :: proc(t:^testing.T) {
    w:World; world_init(&w,3); defer world_destroy(&w)
    insert_resource(&w,Test_Stats{})
    register_typed_system(&w,1,Conflict_Params,test_conflict_tick)
    middle,_:=register_typed_system(&w,2,Conflict_Params,test_conflict_tick)
    register_typed_system(&w,3,Conflict_Params,test_conflict_tick)
    w.parallel_work_threshold=0
    world_update(&w,0,true)
    stats,_:=get_resource(&w,Test_Stats); testing.expect_value(t,stats.visits,123)
    get_resource_mut(&w,Test_Stats).visits=0; set_system_enabled(middle,false)
    world_update(&w,0,true)
    stats,_=get_resource(&w,Test_Stats); testing.expect_value(t,stats.visits,13)
}
Same_Batch_Params :: struct { query:Query(Test_Row,No_Filter), stats:Res_Mut(Test_Stats) }
@(test)
test_same_batch_structure_and_command_registration_order :: proc(t:^testing.T) {
    w:World; world_init(&w,3); defer world_destroy(&w)
    insert_resource(&w,Test_Stats{})
    spawn_tick:=proc(s:^int,p:^Command_Params,_:f32)->System_Error {
        command_spawn(p.commands,struct{p:Test_Position,v:Test_Velocity}{Test_Position{f32(s^),0,0},Test_Velocity{1,0,0}}); return .None
    }
    observe:=proc(_:^Test_Tag,p:^Same_Batch_Params,_:f32)->System_Error { resource_write(p.stats).visits=len(query_entities(&p.query)); return .None }
    register_typed_system(&w,10,Command_Params,spawn_tick)
    register_typed_system(&w,20,Command_Params,spawn_tick)
    register_typed_system(&w,Test_Tag{},Same_Batch_Params,observe)
    w.parallel_work_threshold=0; world_update(&w,0,true)
    stats,_:=get_resource(&w,Test_Stats); testing.expect_value(t,stats.visits,0)
    ids:=entity_ids(&w); defer delete(ids)
    a,_:=get_component(&w,ids[0],Test_Position); b,_:=get_component(&w,ids[1],Test_Position)
    testing.expect(t,a.x==10 && b.x==20 && validate(&w))
}
Test_Local_Buffer :: struct { values:[dynamic]int }
Test_Local_Params :: struct { buffer:Local(Test_Local_Buffer) }
test_local_destroy :: proc(p:rawptr) { buffer:=cast(^Test_Local_Buffer)p; delete(buffer.values); buffer.values=nil }
@(test)
test_owned_local_initialization_and_shutdown :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    run:=proc(_:^Test_Tag,p:^Test_Local_Params,_:f32)->System_Error { assert(local(p.buffer).values[0]==7); return .None }
    handle,err:=register_typed_system(&w,Test_Tag{},Test_Local_Params,run); testing.expect_value(t,err,System_Error.None)
    values:=make([dynamic]int); append(&values,7)
    testing.expect(t,initialize_local(&w,handle,"buffer",Test_Local_Buffer{values},Value_Ops{destroy=test_local_destroy}))
    world_update(&w,0); clear_systems(&w)
    testing.expect(t,!initialize_local(&w,handle,"buffer",Test_Local_Buffer{}))
}

@(test)
test_resource_factory_and_ownership_transfer :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    factory:=proc()->Test_Stats { return Test_Stats{9} }
    p:=get_resource_or_insert(&w,Test_Stats,factory); p.visits=11
    testing.expect(t,contains_resource(&w,Test_Stats) && get_resource_or_insert(&w,Test_Stats,factory).visits==11)
    value,ok:=take_resource(&w,Test_Stats)
    testing.expect(t,ok && value.visits==11 && !contains_resource(&w,Test_Stats))
}
Test_Owned_Event :: struct { data:[dynamic]int }
test_event_destroy :: proc(p:rawptr) { event:=cast(^Test_Owned_Event)p; delete(event.data); event.data=nil }
@(test)
test_owned_event_replacement_retains_cleanup_hooks :: proc(t:^testing.T) {
    w:World; world_init(&w); defer world_destroy(&w)
    register_events(&w,Test_Owned_Event,Value_Ops{destroy=test_event_destroy})
    data:=make([dynamic]int); append(&data,1)
    event:=Test_Owned_Event{data}
    event_send_raw(w.event_logs[Test_Owned_Event],rawptr(&event))
    replace_events(&w,Test_Owned_Event)
    data=make([dynamic]int); append(&data,2); event.data=data
    event_send_raw(w.event_logs[Test_Owned_Event],rawptr(&event))
    clear_events(&w,Test_Owned_Event)
}
