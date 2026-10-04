package scene

import "core:testing"
import "core:fmt"
import "core:mem"

@(test)
test_scene_entity_overflow_rejects_top_level_and_nested_references :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    for id in ([]string{"18446744073709551616","18446744073709551617","36893488147419103232","000000000000000000000"}) {
        text:=fmt.aprintf(`{{"action":"inspect","entity_id":"%s"}}`,id)
        animation,animation_error:=decode_animation(transmute([]byte)text); testing.expect_value(t,animation_error,Decode_Error.Invalid_Arguments); if animation_error==.None { decoded_animation_destroy(&animation) }
        behavior,behavior_error:=decode_behavior(transmute([]byte)text); testing.expect_value(t,behavior_error,Decode_Error.Invalid_Arguments); if behavior_error==.None { decoded_behavior_destroy(&behavior) }
        trigger,trigger_error:=decode_trigger(transmute([]byte)text); testing.expect_value(t,trigger_error,Decode_Error.Invalid_Arguments); if trigger_error==.None { decoded_trigger_destroy(&trigger) }; delete(text)
        for target in ([2]bool{false,true}) {
            nested:string
            if target { nested=fmt.aprintf(`{{"action":"set_rules","entity_id":"0","rules":[{{"event":"enter","actions":[{{"action":"burst_particles","target":{{"kind":"entity","entity":"%s"}},"count":1}}]}}]}}`,id) }
            else { nested=fmt.aprintf(`{{"action":"set_rules","entity_id":"0","rules":[{{"event":"enter","other_entity":"%s","actions":[{{"action":"emit","name":"enter"}}]}}]}}`,id) }
            decoded,error:=decode_trigger(transmute([]byte)nested); testing.expect_value(t,error,Decode_Error.Invalid_Arguments); if error==.None { decoded_trigger_destroy(&decoded) }; delete(nested)
        }
    }
    for id in ([]string{"0","18446744073709551615"}) {
        text:=fmt.aprintf(`{{"action":"inspect","entity_id":"%s"}}`,id)
        animation,animation_error:=decode_animation(transmute([]byte)text); testing.expect_value(t,animation_error,Decode_Error.None); decoded_animation_destroy(&animation)
        behavior,behavior_error:=decode_behavior(transmute([]byte)text); testing.expect_value(t,behavior_error,Decode_Error.None); decoded_behavior_destroy(&behavior)
        trigger,trigger_error:=decode_trigger(transmute([]byte)text); testing.expect_value(t,trigger_error,Decode_Error.None); decoded_trigger_destroy(&trigger); delete(text)
    }
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
