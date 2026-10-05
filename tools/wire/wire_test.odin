//! Regressions for exact JSON identities and nullable metadata at process boundaries.
package wire
import "core:testing"
import "core:mem/virtual"
@(test)
test_integer_string_ids_and_explicit_null :: proc(t:^testing.T) {
    arena:virtual.Arena; testing.expect(t,virtual.arena_init_growing(&arena)==nil); defer virtual.arena_destroy(&arena); context.allocator=virtual.arena_allocator(&arena)
    integer:=parse(`{"id":9007199254740993,"selected_entity_id":null}`)
    string_id:=parse(`{"id":"9007199254740993"}`)
    exact,ok:=get(integer,"id").(i64); testing.expect(t,ok && exact==9007199254740993)
    testing.expect(t,text(get(string_id,"id"))=="9007199254740993")
    testing.expect(t,is_null(get(integer,"selected_entity_id")) && is_null(get(integer,"absent")))
    testing.expect(t,!is_null(get(integer,"id")))
    decoded:=parse(encode(integer)); roundtrip,exact_type:=get(decoded,"id").(i64); testing.expect(t,exact_type && roundtrip==exact)
}
