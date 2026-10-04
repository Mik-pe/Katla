#+test
package ron

import "core:testing"
import "core:encoding/json"
import "core:os"

@(test)
test_current_mesh_and_prefab_fixtures_preserve_tuple_and_variant_syntax :: proc(t:^testing.T) {
    for path in ([2]string{"resources/meshes/chair-frame.katmesh","resources/prefabs/chair.katprefab"}) {
        bytes,read_error:=os.read_entire_file(path,context.allocator); testing.expect(t,read_error==nil); if read_error!=nil { continue }; defer delete(bytes)
        value,err:=parse(string(bytes)); defer json.destroy_value(value)
        testing.expect_value(t,err.kind,Error_Kind.None)
        if err.kind!=.None { continue }
        object,ok:=value.(json.Object); testing.expect(t,ok && object["version"].(json.Integer)==1)
        if path=="resources/prefabs/chair.katprefab" {
            scene:=object["scene"].(json.Object); entities:=scene["entities"].(json.Array)
            source:=entities[1].(json.Object)["source"].(json.Object)
            name,payload,_,valid:=variant_read(source); testing.expect(t,valid && name=="MeshAsset")
            fields:=payload.(json.Object); reference:=fields["path"]
            reference_name,_,_,reference_valid:=variant_read(reference); testing.expect(t,reference_valid && reference_name=="Resource")
        }
    }
}

@(test)
test_duplicate_fields_truncated_input_and_unclosed_comments_fail_without_leaks :: proc(t:^testing.T) {
    for text in ([5]string{`(version:1,version:2)`,`(a:[1,2)`,`/* never closed`,`(a:"unterminated)`,`#![enable(unknown)] (a:1)`}) {
        value,err:=parse(text); defer json.destroy_value(value); testing.expect_value(t,err.kind,Error_Kind.Syntax)
    }
    value,err:=parse(`/* outer /* nested */ comment */ (value:Some("quoted\ntext"),items:[1,2,3,],)`); defer json.destroy_value(value)
    testing.expect_value(t,err.kind,Error_Kind.None)
}

@(test)
test_unsigned_integer_roundtrip_preserves_full_document_range :: proc(t:^testing.T) {
    for text in ([2]string{"9223372036854775808","18446744073709551615"}) {
        tree,err:=parse(text); defer json.destroy_value(tree); testing.expect_value(t,err.kind,Error_Kind.None)
        number,valid:=uint_read(tree); testing.expect(t,valid && number>u64(max(i64)))
        bytes,write_error:=write(tree); defer delete(bytes); testing.expect(t,write_error.kind==.None && string(bytes)==text)
        json_bytes,json_error:=write_json(tree); defer delete(json_bytes); testing.expect(t,json_error.kind==.None && string(json_bytes)==text)
        again,parse_error:=parse_json(json_bytes); defer json.destroy_value(again); second,second_valid:=uint_read(again); testing.expect(t,parse_error.kind==.None && second_valid && number==second)
    }
    for text in ([3]string{"18446744073709551616","-9223372036854775809","01"}) { tree,err:=parse_json(transmute([]byte)text); defer json.destroy_value(tree); testing.expect_value(t,err.kind,Error_Kind.Syntax) }
}

@(test)
test_authored_objects_cannot_impersonate_internal_integer_or_variant_values :: proc(t:^testing.T) {
    source:string=`{"unsigned":{"__uint":"18446744073709551615"},"variant":{"__payload":"owned","__variant":"ordinary"}}`
    tree,parse_error:=parse_json(transmute([]byte)source); defer json.destroy_value(tree); testing.expect_value(t,parse_error.kind,Error_Kind.None)
    object:=tree.(json.Object); _,is_uint:=uint_read(object["unsigned"]); testing.expect(t,!is_uint)
    bytes,write_error:=write_json(tree); defer delete(bytes); testing.expect(t,write_error.kind==.None && string(bytes)==source)
    ron_bytes,ron_error:=write(tree); defer delete(ron_bytes); testing.expect_value(t,ron_error.kind,Error_Kind.None)
    again,again_error:=parse(string(ron_bytes)); defer json.destroy_value(again); testing.expect_value(t,again_error.kind,Error_Kind.None)
    final_bytes,final_error:=write_json(again); defer delete(final_bytes); testing.expect(t,final_error.kind==.None && string(final_bytes)==source)
}

@(test)
test_internal_tags_cannot_be_forged_by_valid_utf8_strings :: proc(t:^testing.T) {
    for source in ([2]string{"{\"\xffkatla.uint\":\"18446744073709551615\"}","(\"\xffkatla.uint\":\"18446744073709551615\")"}) {
        value,error:=parse(source); defer json.destroy_value(value); testing.expect_value(t,error.kind,Error_Kind.Syntax)
    }
    for source in ([2]string{`{"\u00ffkatla.uint":"18446744073709551615"}`,`{"\u0000katla.uint":"18446744073709551615"}`}) {
        value,error:=parse_json(transmute([]byte)source); defer json.destroy_value(value); testing.expect_value(t,error.kind,Error_Kind.None)
        _,is_uint:=uint_read(value); testing.expect(t,!is_uint)
        cloned,clone_error:=clone_value(value); defer json.destroy_value(cloned); testing.expect_value(t,clone_error.kind,Error_Kind.None)
        encoded,encode_error:=write_json(cloned); defer delete(encoded); testing.expect(t,encode_error.kind==.None && len(encoded)>0)
    }
    invalid:string="{\"\xffkatla.uint\":\"18446744073709551615\"}"
    invalid_tree,invalid_error:=parse_json(transmute([]byte)invalid); defer json.destroy_value(invalid_tree); testing.expect_value(t,invalid_error.kind,Error_Kind.Syntax)
}
