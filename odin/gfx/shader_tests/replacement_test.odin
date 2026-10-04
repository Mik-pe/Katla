#+test
package shader_tests
import shader "../shader"
import "core:testing"
import "core:mem"
import "core:thread"
import "core:time"


@(private="package")
Test_Publisher :: struct { allocator:mem.Allocator, created,destroyed:int, fail:bool, supersede:bool, service:^shader.Service }
@(private="package")
prepare_test :: proc(data:rawptr,compiled:^shader.Compiled)->(rawptr,bool) {
    state:=cast(^Test_Publisher)data
    state.created+=1
    pipeline:=new(u32,state.allocator); pipeline^=compiled.entries[0].workgroup_size[0]
    if state.supersede { state.supersede=false; shader.service_submit(state.service,1,SOURCE,{{"cs",.Compute}},{{"BLOCK",32}}) }
    return pipeline,!state.fail
}
@(private="package")
destroy_test :: proc(data,native:rawptr) {
    state:=cast(^Test_Publisher)data
    state.destroyed+=1; free(native,state.allocator)
}
@(private="package")
wait_publication :: proc(t:^testing.T,registry:^shader.Registry)->shader.Publication {
    started:=time.tick_now()
    for time.tick_since(started)<10*time.Second {
        outcome,got:=shader.registry_poll(registry)
        if got { return outcome }
        thread.yield()
    }
    testing.expect(t,false,"Shader worker did not return an accepted candidate")
    return {}
}
@(test)
test_replacement_failure_and_exact_pending_owner_lifetime :: proc(t:^testing.T) {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing); defer mem.tracking_allocator_destroy(&tracker)
    allocator:=mem.tracking_allocator(&tracker)
    testing.expect(t,len(SHADER_COMPILER)>0,"Run scripts/validate_odin_shader.py to select the compiler dependency")
    compiler:shader.Compiler; testing.expect_value(t,shader.compiler_init(&compiler,SHADER_COMPILER),shader.Error.None)
    service:shader.Service; testing.expect_value(t,shader.service_init(&service,&compiler,4,allocator),shader.Service_Error.None)
    state:=Test_Publisher{allocator=allocator,service=&service}
    registry:shader.Registry; testing.expect_value(t,shader.registry_init(&registry,&service,{&state,prepare_test,destroy_test},allocator),shader.Publication_Error.None)
    shader.service_submit(&service,1,SOURCE,{{"cs",.Compute}},{{"BLOCK",2}})
    first:=wait_publication(t,&registry); testing.expect_value(t,first.error,shader.Publication_Error.None); shader.publication_destroy(&first)
    old,old_error:=shader.registry_acquire(&registry,1); testing.expect_value(t,old_error,shader.Publication_Error.None)
    copied_old:=old
    shader.service_submit(&service,1,SOURCE,{{"cs",.Compute}},{{"BLOCK",4}})
    second:=wait_publication(t,&registry); testing.expect_value(t,second.error,shader.Publication_Error.None); shader.publication_destroy(&second)
    testing.expect_value(t,state.destroyed,0)
    testing.expect_value(t,(cast(^u32)old.pipeline)^,u32(2))
    current,current_error:=shader.registry_acquire(&registry,1); testing.expect(t,current_error==.None && (cast(^u32)current.pipeline)^==4)
    testing.expect_value(t,shader.registry_destroy(&registry),shader.Publication_Error.Busy)
    testing.expect_value(t,shader.registry_release(&registry,&old),shader.Publication_Error.None)
    testing.expect_value(t,shader.registry_release(&registry,&copied_old),shader.Publication_Error.Invalid_Snapshot)
    testing.expect_value(t,state.destroyed,1)
    shader.service_submit(&service,1,"not WGSL",{{"cs",.Compute}})
    failed:=wait_publication(t,&registry); testing.expect(t,failed.error==.Compile_Failed && failed.compiler_error==.Parse && len(failed.message)>0); shader.publication_destroy(&failed)
    state.fail=true
    shader.service_submit(&service,1,SOURCE,{{"cs",.Compute}},{{"BLOCK",8}})
    rejected:=wait_publication(t,&registry); testing.expect_value(t,rejected.error,shader.Publication_Error.Prepare_Failed); shader.publication_destroy(&rejected)
    testing.expect_value(t,state.destroyed,2)
    testing.expect_value(t,(cast(^u32)current.pipeline)^,u32(4))
    state.fail=false; state.supersede=true
    shader.service_submit(&service,1,SOURCE,{{"cs",.Compute}},{{"BLOCK",16}})
    superseded:=wait_publication(t,&registry); testing.expect_value(t,superseded.error,shader.Publication_Error.Superseded); shader.publication_destroy(&superseded)
    testing.expect_value(t,(cast(^u32)current.pipeline)^,u32(4))
    newest:=wait_publication(t,&registry); testing.expect_value(t,newest.error,shader.Publication_Error.None); shader.publication_destroy(&newest)
    last,last_error:=shader.registry_acquire(&registry,1); testing.expect(t,last_error==.None && (cast(^u32)last.pipeline)^==32)
    testing.expect_value(t,shader.registry_release(&registry,&current),shader.Publication_Error.None)
    testing.expect_value(t,shader.registry_release(&registry,&last),shader.Publication_Error.None)
    testing.expect_value(t,shader.registry_destroy(&registry),shader.Publication_Error.None)
    testing.expect_value(t,shader.service_destroy(&service),shader.Service_Error.None)
    testing.expect_value(t,shader.compiler_destroy(&compiler),shader.Error.None)
    testing.expect_value(t,state.created,state.destroyed)
    testing.expect_value(t,len(tracker.allocation_map),0)
}

