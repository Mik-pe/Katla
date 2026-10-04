#+test
package app

import ecs "../ecs"
import editor "../editor"
import "core:testing"
import "core:strings"

@(test)
test_scene_query_unicode16_and17_names_and_contextual_final_sigma :: proc(t:^testing.T) {
    for pair in ([][2]string{{"\U00010D50","\U00010D70"},{"\U00016EA0","\U00016EBB"},{"\U00016EA0Σ","\U00016EBBς"},{"Σ\U00016EA0","σ\U00016EBB"}}) {
        lowered:=scene_query_lower(pair[0]); testing.expect_value(t,lowered,pair[1]); delete(lowered)
    }
    owner:Authoring; authoring_init(&owner); defer authoring_destroy(&owner); scene_components_register(&owner)
    named:=ecs.spawn(&owner.world,struct {name:Scene_Name}{ {strings.clone("\U00010D50 \U00016EA0Σ")} })
    ecs.spawn(&owner.world,struct {name:Scene_Name}{ {strings.clone("Unrelated")} })
    for filter in ([]string{"\U00010D70","\U00016EBBς"}) {
        result:=scene_action_query(&owner,{kind=.Query_Entities,name_filter=filter,has_name_filter=true})
        testing.expect(t,result.error==.None && len(result.entities)==1 && result.entities[0]==named)
        editor.tool_result_destroy(&result)
    }
}
