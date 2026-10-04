#+test
package agent

import "core:testing"

@(test)
test_spawn_optional_null_defaults_and_required_position_type :: proc(t:^testing.T) {
    for text in ([]string{`{"position":[1,2,3],"name":null,"rotation":null,"scale":null,"shape":null}`,`{"position":[1,2,3]}`}) {
        decoded,error:=decode_call({"spawn","spawn_entity",transmute([]byte)text})
        testing.expect(t,error==.None && decoded.operation.position==([3]f32{1,2,3}) && decoded.operation.rotation==([3]f32{}) && decoded.operation.scale==([3]f32{1,1,1}) && decoded.operation.name=="")
        if error==.None { decoded_call_destroy(&decoded) }
    }
    for text in ([]string{`{"position":null}`,`{"position":[0,0,0],"rotation":false}`,`{"position":[0,0,0],"scale":false}`,`{"position":[0,0,0],"name":false}`}) {
        decoded,error:=decode_call({"spawn","spawn_entity",transmute([]byte)text}); testing.expect(t,error==.Invalid_Arguments); if error==.None { decoded_call_destroy(&decoded) }
    }
    for text in ([]string{`{"path":"models/Box.glb","position":null,"default_animation":null}`,`{"path":"models/Box.glb"}`}) {
        decoded,error:=decode_call({"model","spawn_model",transmute([]byte)text})
        testing.expect(t,error==.None && decoded.operation.position==([3]f32{}) && decoded.operation.default_animation=="")
        if error==.None { decoded_call_destroy(&decoded) }
    }
}
