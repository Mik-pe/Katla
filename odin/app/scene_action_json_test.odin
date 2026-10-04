#+test
package app

import "core:testing"

@(test)
test_scene_action_json_nested_object_order_keeps_array_order_scalar_types_and_exact_u64 :: proc(t:^testing.T) {
    left:=transmute([]byte)string(`{"mesh":{"segments":32,"kind":"sphere"},"items":[{"id":18446744073709551615,"enabled":true},null,"32"]}`)
    reordered:=transmute([]byte)string(`{"items":[{"enabled":true,"id":18446744073709551615},null,"32"],"mesh":{"kind":"sphere","segments":32}}`)
    testing.expect(t,scene_action_json_equal(left,reordered,context.allocator))
    distinct_max:=transmute([]byte)string(`{"items":[{"enabled":true,"id":18446744073709551614},null,"32"],"mesh":{"kind":"sphere","segments":32}}`)
    testing.expect(t,!scene_action_json_equal(left,distinct_max,context.allocator))
    testing.expect(t,!scene_action_json_equal(transmute([]byte)string(`{"id":9007199254740992}`),transmute([]byte)string(`{"id":9007199254740993}`),context.allocator))
    testing.expect(t,!scene_action_json_equal(transmute([]byte)string(`{"items":[1,2]}`),transmute([]byte)string(`{"items":[2,1]}`),context.allocator))
    testing.expect(t,!scene_action_json_equal(transmute([]byte)string(`{"id":"32"}`),transmute([]byte)string(`{"id":32}`),context.allocator))
    testing.expect(t,scene_action_json_equal(transmute([]byte)string(`{"\u0061":"\u0062"}`),transmute([]byte)string(`{"a":"b"}`),context.allocator))
    testing.expect(t,!scene_action_json_equal(transmute([]byte)string(`{"id":1}`),transmute([]byte)string(`{"id":1,"extra":false}`),context.allocator))
}

@(test)
test_scene_action_json_invalid_snapshot_fails_and_releases_partially_owned_tree :: proc(t:^testing.T) {
    for malformed in ([]string{`{"a":["owned",}`,`{"a":"owned","a":2}`,`{"a":}`,`["owned",]`,`{"a":true} false`}) {
        testing.expect(t,!scene_action_json_equal(transmute([]byte)malformed,transmute([]byte)string(`{"a":true}`),context.allocator))
    }
}
