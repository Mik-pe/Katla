#+test
package scene

import "core:testing"
import "core:strings"
import "core:fmt"

@(test)
test_behavior_burst_wide_integer_never_aliases_bounded_count :: proc(t:^testing.T) {
    for count in ([]string{"18446744073709551617","36893488147419103233","-18446744073709551615","1.0","100001"}) {
        data:=fmt.aprintf(`{{"action":"burst","entity_id":"18446744073709551615","count":%s}}`,count); defer delete(data)
        decoded,error:=decode_behavior(transmute([]byte)data)
        testing.expect_value(t,error,Decode_Error.Invalid_Arguments)
        if error==.None { decoded_behavior_destroy(&decoded) }
    }
    data:string=`{"action":"burst","entity_id":"18446744073709551615","count":100000}`
    decoded,error:=decode_behavior(transmute([]byte)data); testing.expect(t,error==.None)
    if error==.None { testing.expect(t,decoded.operation.count==100000 && u64(decoded.operation.entity)==max(u64)); decoded_behavior_destroy(&decoded) }
}
@(test)
test_trigger_nested_count_overflow_and_wide_finite_coordinates :: proc(t:^testing.T) {
    for count in ([]string{"18446744073709551617","36893488147419103233","100001"}) {
        data:=fmt.aprintf(`{{"action":"set_rules","entity_id":"0","rules":[{{"event":"enter","actions":[{{"action":"burst_particles","target":{{"kind":"entity","entity":"18446744073709551615"}},"count":%s}}]}}]}}`,count); defer delete(data)
        decoded,error:=decode_trigger(transmute([]byte)data); testing.expect_value(t,error,Decode_Error.Invalid_Arguments)
        if error==.None { decoded_trigger_destroy(&decoded) }
    }
    data:string=`{"action":"create_box","name":"wide","position":[18446744073709551616,-18446744073709551616,0],"half_extents":[1,1,1],"rules":[{"event":"enter","actions":[{"action":"play_animation","target":{"kind":"other"},"clip":"Run","speed":18446744073709551616}]}]}`
    decoded,error:=decode_trigger(transmute([]byte)data); testing.expect(t,error==.None)
    if error==.None {
        testing.expect(t,decoded.operation.position[0]>1e19 && decoded.operation.position[1]<-1e19 && decoded.operation.rules[0].actions[0].speed>1e19)
        decoded_trigger_destroy(&decoded)
    }
}
@(test)
test_behavior_document_rejects_deep_and_nonfinite_json_before_parse :: proc(t:^testing.T) {
    opening:=strings.repeat("[",65); defer delete(opening); closing:=strings.repeat("]",65); defer delete(closing)
    data:=fmt.aprintf(`{{"action":"set_particles","entity_id":"0","document":%s0%s}}`,opening,closing); defer delete(data)
    decoded,error:=decode_behavior(transmute([]byte)data); testing.expect_value(t,error,Decode_Error.Invalid_JSON)
    if error==.None { decoded_behavior_destroy(&decoded) }
    infinite:string=`{"action":"set_particles","entity_id":"0","document":{"unknown":1e999}}`
    decoded,error=decode_behavior(transmute([]byte)infinite); testing.expect_value(t,error,Decode_Error.Invalid_JSON)
    if error==.None { decoded_behavior_destroy(&decoded) }
}
