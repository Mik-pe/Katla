package scene
import "core:testing"
import "core:mem"
@(test)
test_animation_transport_defaults_lossless_ids_and_unknown_fields :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    decoded,err:=decode_animation(transmute([]byte)string(`{"action":"play","entity_id":"18446744073709551615","clip":"Run"}`))
    testing.expect(t,err==.None && decoded.operation.fade_seconds==0.25 && decoded.operation.speed==1 && decoded.operation.looping && u64(decoded.operation.entity)==max(u64)); decoded_animation_destroy(&decoded)
    for text in ([]string{`{"action":"play","entity_id":"1"}`,`{"action":"inspect","entity_id":1}`,`{"action":"play","entity_id":"1","clip":"Run","blend_weight":0.5}`,`{"action":"play","entity_id":"1","clip":"Run","speed":-1}`,`{"action":"play","entity_id":"1","clip":"Run","fade_seconds":1e200}`,`{"action":"inspect","entity_id":"18446744073709551616"}`}) {
        invalid,invalid_err:=decode_animation(transmute([]byte)text); testing.expect(t,invalid_err==.Invalid_Arguments); if invalid_err==.None { decoded_animation_destroy(&invalid) }
    }
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_simulation_actions_are_strict :: proc(t:^testing.T) {
    for text,i in ([5]string{`{"action":"inspect"}`,`{"action":"play"}`,`{"action":"pause"}`,`{"action":"resume"}`,`{"action":"stop"}`}) { op,err:=decode_simulation(transmute([]byte)text); testing.expect(t,err==.None && int(op)==i) }
    _,err:=decode_simulation(transmute([]byte)string(`{"action":"play","force":true}`)); testing.expect(t,err==.Invalid_Arguments)
}

@(test)
test_trigger_nested_lossless_references_defaults_and_bounds :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker); context.allocator=mem.tracking_allocator(&tracker)
    text:=`{"action":"set_rules","entity_id":"18446744073709551615","rules":[{"event":"enter","other_entity":"18446744073709551615","once":true,"actions":[{"action":"play_animation","target":{"kind":"entity","entity":"18446744073709551615"},"clip":"Run"},{"action":"emit","name":"run"}]}]}`
    decoded,err:=decode_trigger(transmute([]byte)text); testing.expect(t,err==.None)
    rule:=decoded.operation.rules[0]; testing.expect(t,rule.has_other && u64(rule.other)==max(u64) && rule.once); testing.expect(t,rule.actions[0].fade_seconds==0.25 && rule.actions[0].speed==1 && rule.actions[0].looping); decoded_trigger_destroy(&decoded)
    for invalid in ([]string{`{"action":"create_box","name":"T","position":[0,0,0],"half_extents":[0,1,1],"rules":[]}`,`{"action":"set_rules","entity_id":"1","rules":[{"event":"enter","actions":[]}]}`,`{"action":"inspect","entity_id":1}`,`{"action":"set_rules","entity_id":"1","rules":[{"event":"enter","actions":[{"action":"emit","name":"   "}]}]}`,`{"action":"set_rules","entity_id":"1","rules":[{"event":"enter","actions":[{"action":"burst_particles","target":{"kind":"other","entity":"1"},"count":1}]}]}`}) {
        result,result_error:=decode_trigger(transmute([]byte)invalid); testing.expect(t,result_error==.Invalid_Arguments); if result_error==.None { decoded_trigger_destroy(&result) }
    }
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
@(test)
test_behavior_explicit_detach_and_operation_fields :: proc(t:^testing.T) {
    for invalid in ([]string{`{"action":"set_script","entity_id":"1"}`,`{"action":"set_particles","entity_id":"1"}`,`{"action":"burst","entity_id":"1","count":0}`,`{"action":"inspect","entity_id":1}`,`{"action":"inspect","entity_id":"1","path":null}`}) {
        result,err:=decode_behavior(transmute([]byte)invalid); testing.expect(t,err==.Invalid_Arguments); if err==.None { decoded_behavior_destroy(&result) }
    }
    text:=`{"action":"set_script","entity_id":"18446744073709551615","path":null}`; result,err:=decode_behavior(transmute([]byte)text); testing.expect(t,err==.None && result.operation.detach && u64(result.operation.entity)==max(u64)); decoded_behavior_destroy(&result)
}
