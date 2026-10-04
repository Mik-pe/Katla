#+test
package assets
import "core:testing"
@(test)
test_asset_argument_primary_defaults_null_and_exact_unsigned_limit :: proc(t:^testing.T) {
    for text in ([4]string{`{}`,`{"path":null,"filter":null}`,`{"path":"","filter":""}`,`{"path":"."}`}) {
        value,error:=decode("list_resources",transmute([]byte)text); defer destroy(&value); testing.expect_value(t,error,Error.None)
        if text==`{}` || text==`{"path":null,"filter":null}` { testing.expect(t,value.request.path=="." && !value.request.has_filter) }
    }
    expected_limits:=[5]int{64,64,1,256,256}
    for text,index in ([5]string{`{}`,`{"limit":null}`,`{"limit":0}`,`{"limit":257}`,`{"limit":18446744073709551615}`}) {
        value,error:=decode("search_assets",transmute([]byte)text); defer destroy(&value); testing.expect_value(t,error,Error.None)
        expected:=expected_limits[index]; testing.expect_value(t,value.request.limit,expected)
    }
    for text in ([5]string{`{"limit":-1}`,`{"limit":1.5}`,`{"limit":18446744073709551616}`,`{"limit":"64"}`,`{"query":null}`}) {
        value,error:=decode("search_assets",transmute([]byte)text); defer destroy(&value); testing.expect(t,error!=.None)
    }
    for text in ([2]string{`{}`,`{"path":null}`}) {
        value,error:=scene_file_decode("save_scene",transmute([]byte)text); defer scene_file_destroy(&value); testing.expect(t,error==.None && !value.request.has_path)
    }
}
