#+test
package render

import gfx "../../gfx"
import "core:testing"

Reset_Test_GPU :: struct { creates,releases:int }
reset_test_create :: proc(gpu:^Reset_Test_GPU,_:gfx.Buffer_Desc,_:[]byte)->(gfx.Buffer_Handle,gfx.Gpu_Error) {
    gpu.creates+=1
    if gpu.creates==2 { return {},.Native_Failure }
    return {gpu,1,1},.None
}
reset_test_release :: proc(gpu:^Reset_Test_GPU,handle:gfx.Buffer_Handle)->gfx.Gpu_Error { assert(handle.owner==gpu && handle.index==1); gpu.releases+=1; return .None }
@(test)
test_particle_reset_partial_native_allocation_failure_keeps_request_clock_and_accepted_pool :: proc(t:^testing.T) {
    gpu:Reset_Test_GPU
    consumer:=Particle_Consumer(Reset_Test_GPU){renderer=&gpu,operations={create_buffer=reset_test_create,destroy_buffer=reset_test_release},capacity=4,allocator=context.allocator,slots=make([]Particle_Slot,3),states=make([]Particle_Emitter_State,1),data={&gpu,5,1},dead={&gpu,6,1},sequence=9,alive_upper=3}
    defer { particle_abort(&consumer); delete(consumer.slots); delete(consumer.states) }
    consumer.states[0]={accumulator=.75,remaining_duration=.5,configured_duration=1,clock_initialized=true,active=true}
    accepted_data,accepted_dead,clock:=consumer.data,consumer.dead,consumer.states[0]
    testing.expect_value(t,particle_reset_all(&consumer),Particle_Error{})
    consumer.pending.ready=true
    testing.expect_value(t,particle_reset_all(&consumer).gpu,gfx.Gpu_Error.Busy)
    testing.expect(t,consumer.reset_requested)
    consumer.pending.ready=false
    failed,error:=particle_reset_prepare(&consumer)
    testing.expect_value(t,error.gpu,gfx.Gpu_Error.Native_Failure)
    testing.expect(t,!failed.ready && failed.reset_data.owner==nil && failed.reset_dead.owner==nil)
    testing.expect(t,gpu.creates==2 && gpu.releases==1)
    testing.expect(t,consumer.reset_requested && consumer.sequence==9 && consumer.alive_upper==3 && consumer.data==accepted_data && consumer.dead==accepted_dead && consumer.states[0]==clock)
}
