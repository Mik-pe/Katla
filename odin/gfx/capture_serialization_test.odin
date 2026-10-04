#+test
//! Export regressions cover external JSON validity and graph-name escaping after declaration teardown.
package gfx
import "core:testing"
import "core:encoding/json"
import "core:strings"

@(test)
test_capture_exports_independent_compiler_projection_and_escaped_graph_names :: proc(t:^testing.T) {
    graph:Graph; graph_init(&graph)
    id,error:=graph_buffer(&graph,{32,{.Storage},.CPU_Visible},true,false); testing.expect_value(t,error,Graph_Error.None)
    _,error=graph_pass(&graph,"quoted \"pass\"\n\\ unicode å",.Compute,{{id,{0,32},.Read,.Storage}},side_effect=true); testing.expect_value(t,error,Graph_Error.None)
    _,error=graph_pass(&graph,"unreachable",.Transfer,nil); testing.expect_value(t,error,Graph_Error.None)
    plan,compiled:=graph_compile(&graph); testing.expect_value(t,compiled,Graph_Error.None)
    snapshot,found:=capture_graph_snapshot(&graph,&plan); testing.expect(t,found); defer capture_snapshot_destroy(&snapshot)
    compiled_graph_destroy(&plan); graph_destroy(&graph)
    testing.expect(t,!snapshot.native_captured); testing.expect_value(t,snapshot.backend,Capture_Backend.None)
    testing.expect_value(t,snapshot.feedback,Capture_Feedback.Not_Submitted)
    testing.expect_value(t,snapshot.passes[0].liveness,Capture_Liveness.Side_Effect_Root)
    testing.expect_value(t,snapshot.passes[1].liveness,Capture_Liveness.Not_Required)
    bytes,encoded:=capture_json(&snapshot); testing.expect(t,encoded); defer delete(bytes)
    tree,parse_error:=json.parse(bytes); testing.expect(t,parse_error==nil); defer json.destroy_value(tree)
    root,object:=tree.(json.Object); testing.expect(t,object)
    capture,has_capture:=root["capture"].(json.Object); testing.expect(t,has_capture)
    native,has_native:=capture["native_captured"].(bool); testing.expect(t,has_native && !native)
    comparison,has_comparison:=root["comparison"].(json.Array); testing.expect(t,has_comparison); testing.expect_value(t,len(comparison),0)
    passes,has_passes:=capture["passes"].(json.Array); testing.expect(t,has_passes)
    first,has_first:=passes[0].(json.Object); testing.expect(t,has_first); name,_:=first["name"].(string)
    testing.expect_value(t,name,"quoted \"pass\"\n\\ unicode å")
    dot:=capture_dot(&snapshot); defer delete(dot)
    testing.expect(t,strings.contains(dot,`quoted \"pass\"\n\\ unicode å`))
    testing.expect(t,strings.contains(dot,"style=dashed"))
    text:=capture_text(&snapshot); defer delete(text)
    testing.expect(t,strings.contains(text,"live=false")); testing.expect(t,!strings.contains(text,"divergence"))
}
