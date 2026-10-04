#+test
package box3d

import "core:testing"
import "core:thread"

NATIVE_LIBRARY :: #config(BOX3D_LIBRARY,"")

@(private="package")
test_body :: proc(id:u64,kind:Body_Type,sensor:=false,position:=[3]f32{})->Body {
    return {id=id,body_type=kind,shape_kind=.Sphere,position=position,rotation={0,0,0,1},radius=0.5,density=1,gravity_scale=1,friction=0.5,layers=max(u32),mask=max(u32),sensor=u32(sensor)}
}
@(private="package")
test_native_init :: proc(t:^testing.T,b:^Backend)->bool {
    err:=backend_init(b,NATIVE_LIBRARY); testing.expect_value(t,err,Error.None); return err==.None
}
when NATIVE_LIBRARY!="" {
@(test)
test_native_concurrent_world_lifecycles_remain_independent :: proc(t:^testing.T) {
    states:[8]struct { failures:int }
    workers:[8]^thread.Thread
    for &state,i in states {
        workers[i]=thread.create_and_start_with_poly_data(&state,proc(p:^struct { failures:int }) {
            for iteration in 0..<8 {
                b:Backend
                if backend_init(&b,NATIVE_LIBRARY)!=.None { p.failures+=1; continue }
                body:=test_body(u64(iteration),.Dynamic,position={0,3,0})
                if backend_sync(&b,{body})!=.None { p.failures+=1 }
                step:=backend_step(&b,1.0/60)
                if step.error!=.None || len(step.poses)!=1 || step.poses[0].id!=u64(iteration) || step.poses[0].position[1]>=3 { p.failures+=1 }
                step_destroy(&step)
                if backend_destroy(&b)!=.None { p.failures+=1 }
            }
        })
    }
    for worker in workers { thread.join(worker); thread.destroy(worker) }
    for state in states { testing.expect_value(t,state.failures,0) }
}
@(test)
test_native_fall_contact_full_id_and_unchanged_sync :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    ground:=test_body(1,.Fixed,position={0,-0.5,0}); ground.shape_kind=.Box; ground.half_extents={10,0.5,10}
    ball:=test_body(max(u64),.Dynamic,position={0,3,0})
    bodies:=[2]Body{ground,ball}
    testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None)
    for _ in 0..<30 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
    before:=backend_step(&b,1.0/60); defer step_destroy(&before)
    testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None)
    after:=backend_step(&b,1.0/60); defer step_destroy(&after)
    testing.expect(t,len(after.poses)==2 && after.poses[1].id==max(u64) && after.poses[1].position[1]<before.poses[1].position[1])
    for _ in 0..<180 { step:=backend_step(&b,1.0/60); testing.expect_value(t,step.error,Error.None); step_destroy(&step) }
    settled:=backend_step(&b,1.0/60); defer step_destroy(&settled)
    testing.expect(t,abs(settled.poses[1].position[1]-0.5)<0.03)
}
@(test)
test_native_sensor_directions_filters_deletion_and_atomic_rejection :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    bodies:=[2]Body{test_body(2,.Fixed,true),test_body(3,.Fixed,true,{0.5,0,0})}
    testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None)
    first:=backend_step(&b,1.0/60); defer step_destroy(&first)
    testing.expect(t,first.error==.None && len(first.overlaps)==2 && len(first.events)==2)
    repeat:=backend_step(&b,1.0/60); defer step_destroy(&repeat); testing.expect_value(t,len(repeat.events),0)
    invalid:=bodies; invalid[1].rotation={0,0,0,0}
    testing.expect_value(t,backend_sync(&b,invalid[:]),Error.Invalid)
    testing.expect_value(t,len(b.entries),2)
    bodies[0].mask=0
    testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None)
    filtered:=backend_step(&b,1.0/60); defer step_destroy(&filtered)
    testing.expect(t,filtered.error==.None && len(filtered.overlaps)==0 && len(filtered.events)==2)
    bodies[0].mask=max(u32)
    testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None)
    restored:=backend_step(&b,1.0/60); defer step_destroy(&restored); testing.expect_value(t,len(restored.events),2)
    testing.expect_value(t,backend_sync(&b,bodies[:1]),Error.None)
    removed:=backend_step(&b,1.0/60); defer step_destroy(&removed)
    testing.expect(t,len(removed.overlaps)==0 && len(removed.events)==2 && removed.events[0].phase==.Exit)
    testing.expect_value(t,backend_reset(&b),Error.None)
    empty:=backend_step(&b,1.0/60); defer step_destroy(&empty); testing.expect_value(t,len(empty.poses),0)
}
@(test)
test_native_rotated_thin_box_capsule_and_thread_affinity :: proc(t:^testing.T) {
    b:Backend; if !test_native_init(t,&b) { return }; defer testing.expect_value(t,backend_destroy(&b),Error.None)
    bodies:=[2]Body{test_body(1,.Fixed,true),test_body(2,.Fixed,false,{1.3,0.0,1.3})}
    bodies[0].shape_kind=.Box; bodies[0].half_extents={2,0.1,0.1}; bodies[0].rotation={0,0.38268343,0,0.92387953}
    bodies[1].shape_kind=.Capsule; bodies[1].radius=0.1; bodies[1].half_height=0.2
    testing.expect_value(t,backend_sync(&b,bodies[:]),Error.None)
    step:=backend_step(&b,1.0/60); defer step_destroy(&step)
    testing.expect(t,step.error==.None && len(step.overlaps)==0)
    state:=struct { b:^Backend, error:Error }{b=&b}
    worker:=thread.create_and_start_with_poly_data(&state,proc(p:^struct { b:^Backend,error:Error }) {
        step:=backend_step(p.b,1.0/60); p.error=step.error; step_destroy(&step)
    })
    thread.join(worker); thread.destroy(worker)
    testing.expect_value(t,state.error,Error.Wrong_Thread)
}
}
@(test)
test_body_preflight_rejects_invalid_values_without_dependency :: proc(t:^testing.T) {
    body:=test_body(0,.Dynamic); testing.expect(t,body_valid(body))
    body.body_type=cast(Body_Type)3; testing.expect(t,!body_valid(body)); body.body_type=.Dynamic
    body.sensor=2; testing.expect(t,!body_valid(body)); body.sensor=0
    body.shape_kind=.Box; body.half_extents={1,0,1}; testing.expect(t,!body_valid(body))
    body.half_extents={1,1,1}; body.rotation={0,0,0,0}; testing.expect(t,!body_valid(body))
}
@(test)
test_uninitialized_owner_rejects_calls_without_native_allocation :: proc(t:^testing.T) {
    state:=struct { backend:Backend,error:Error }{}
    worker:=thread.create_and_start_with_poly_data(&state,proc(p:^struct { backend:Backend,error:Error }) {
        step:=backend_step(&p.backend,1.0/60); p.error=step.error; step_destroy(&step)
    })
    thread.join(worker); thread.destroy(worker)
    testing.expect_value(t,state.error,Error.Uninitialized)
    testing.expect_value(t,backend_destroy(&state.backend),Error.Uninitialized)
}
