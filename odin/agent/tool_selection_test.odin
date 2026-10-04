package agent

import "core:testing"
import "core:mem"
import "core:encoding/json"

@(test)
test_tool_selection_canonical_order_and_errors :: proc(t:^testing.T) {
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,context.allocator); defer mem.tracking_allocator_destroy(&tracker)
    context.allocator=mem.tracking_allocator(&tracker)
    names:=[3]string{"spawn_entity","material","query_entities"}
    data,error:=tools_select(names[:]); testing.expect(t,error==.None)
    tree,parse_error:=json.parse(data); testing.expect(t,parse_error==nil)
    entries:=tree.(json.Array); testing.expect(t,len(entries)==3)
    for expected,i in ([3]string{"material","query_entities","spawn_entity"}) { testing.expect(t,entries[i].(json.Object)["name"].(string)==expected) }
    json.destroy_value(tree); delete(data)
    repeated:=[2]string{"material","material"}; data,error=tools_select(repeated[:]); testing.expect(t,error==.Duplicate_Tool && data=="")
    missing:=[1]string{"missing-capability"}; data,error=tools_select(missing[:]); testing.expect(t,error==.Missing_Tool && data=="")
    data,error=tools_select(nil); testing.expect(t,error==.None && data=="[]"); delete(data)
    testing.expect(t,len(tracker.allocation_map)==0 && len(tracker.bad_free_array)==0)
}
