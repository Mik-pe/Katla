#+test
package editor
import ecs "../ecs"
import "core:testing"

Unsigned_Editor_Component :: struct { reference:u64 `inspect:"skip"`,count:u64,enabled:bool }
@(test)
test_field_merge_preserves_full_unsigned_siblings_and_exact_unsigned_replacement :: proc(t:^testing.T) {
    world:ecs.World; ecs.world_init(&world); defer ecs.world_destroy(&world)
    registry:Component_Registry; editor_registry_init(&registry); defer editor_registry_destroy(&registry)
    editor_register(&world,&registry,"Unsigned",Unsigned_Editor_Component{},spawn_default=false)
    id:=ecs.spawn(&world,struct {component:Unsigned_Editor_Component}{{max(u64),max(u64)-1,false}})
    enabled:string="true"; testing.expect_value(t,editor_set_field(&world,&registry,id,"Unsigned","enabled",transmute([]byte)enabled),Scene_Error.None)
    value,_:=ecs.get_component(&world,id,Unsigned_Editor_Component); testing.expect(t,value.reference==max(u64) && value.count==max(u64)-1 && value.enabled)
    exact:string="18446744073709551615"; testing.expect_value(t,editor_set_field(&world,&registry,id,"Unsigned","count",transmute([]byte)exact),Scene_Error.None)
    updated,_:=ecs.get_component(&world,id,Unsigned_Editor_Component); testing.expect(t,updated.reference==max(u64) && updated.count==max(u64) && updated.enabled)
    overflow:string="18446744073709551616"; testing.expect_value(t,editor_set_field(&world,&registry,id,"Unsigned","count",transmute([]byte)overflow),Scene_Error.Invalid_Field_Value)
    retained,_:=ecs.get_component(&world,id,Unsigned_Editor_Component); testing.expect_value(t,retained.count,max(u64))
}
