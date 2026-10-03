package main
import "core:fmt"
import ecs "../ecs"
import editor "../editor"
import workload "../workload"
Component :: struct { value:f32 }
main :: proc() {
    checksum:=workload.run()
    w:ecs.World; ecs.world_init(&w); defer ecs.world_destroy(&w)
    reg:editor.Component_Registry; editor.editor_registry_init(&reg); defer editor.editor_registry_destroy(&reg)
    editor.editor_register(&w,&reg,"C1",Component{9})
    id:=ecs.spawn(&w,struct{c:Component}{Component{9}})
    data:=transmute([]byte)string("25.0")
    result,group:=editor.scene_execute(&w,&reg,editor.Scene_Op{kind=.Set_Field,entity=id,component="C1",field="value",value=data})
    defer editor.tool_result_destroy(&result); defer editor.undo_group_destroy(&group)
    assert(result.error==.None)
    value,ok:=ecs.get_component(&w,id,Component); assert(ok)
    checksum+=f64(value.value)
    assert(editor.undo_group(&w,&reg,&group)==.None)
    fmt.printf("%.0f\n",checksum)
}
