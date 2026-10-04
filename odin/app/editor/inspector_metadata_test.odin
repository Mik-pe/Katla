#+test
package editor_app

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import "core:testing"
import "core:encoding/json"

Metadata_Choice :: enum {First,Second}
Metadata_Settings :: struct {hidden:f32 `inspect:"skip"`,range:f32 `min:"2" max:"8" speed:"0.5" display_name:"Range"`,tint:[4]f32 `inspect:"color" min:"0" max:"1"`,choice:Metadata_Choice}
Metadata_Component :: struct {settings:Metadata_Settings}

@(test)
test_nested_inspector_tags_hidden_fields_color_and_enum_metadata :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner); testing.expect_value(t,app.authoring_services_init(&owner),editor.Scene_Error.None)
    editor.editor_register(&owner.world,&owner.registry,"Metadata",Metadata_Component{},spawn_default=false)
    entity:=ecs.spawn(&owner.world,struct {component:Metadata_Component}{{{7,3,{.25,.5,.75,1},.Second}}})
    state:State; state_init(&state,&owner); defer state_destroy(&state); selection_set(&state,entity)
    snapshot,error:=inspector_read(&state); defer inspector_destroy(&snapshot); testing.expect_value(t,error,editor.Scene_Error.None)
    range_found,tint_found,enum_found:=false,false,false
    for component in snapshot.components { if component.name!="Metadata" { continue }; testing.expect_value(t,len(component.fields),3)
        for field in component.fields {
            testing.expect(t,field.path!="/settings/hidden")
            if field.path=="/settings/range" { range_found=true; testing.expect(t,field.label=="Range" && field.kind==.Float && field.constraints.has_min && field.constraints.min==2 && field.constraints.has_max && field.constraints.max==8 && field.constraints.speed==.5) }
            if field.path=="/settings/tint" { tint_found=true; testing.expect(t,field.kind==.Color && field.constraints.has_min && field.constraints.has_max); value,parse_error:=json.parse(field.value,spec=.JSON); defer json.destroy_value(value); testing.expect(t,parse_error==nil && len(value.(json.Array))==4) }
            if field.path=="/settings/choice" { enum_found=true; testing.expect(t,field.kind==.Enum && len(field.variants)==2 && field.variants[1]=="Second") }
        }
    }
    testing.expect(t,range_found && tint_found && enum_found)
    encoded:string=`[0.1,0.2,0.3,0.4]`
    testing.expect_value(t,inspector_set(&state,entity,"Metadata","/settings/tint",transmute([]byte)encoded),editor.Scene_Error.None)
    edited,_:=ecs.get_component(&owner.world,entity,Metadata_Component); testing.expect_value(t,edited.settings.tint,[4]f32{.1,.2,.3,.4})
    testing.expect_value(t,history_apply(&state,false),editor.Scene_Error.None); restored,_:=ecs.get_component(&owner.world,entity,Metadata_Component); testing.expect_value(t,restored.settings.tint,[4]f32{.25,.5,.75,1})
}
