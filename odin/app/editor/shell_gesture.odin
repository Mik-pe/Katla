//! Generic captured fields preview atomically and retain one first-before/last-after history command.
package editor_app
import app ".."
import ecs "../../ecs"
import editor "../../editor"
import ui "../../ui"
import "core:encoding/json"

@(private="package")
shell_field_number :: proc(shell:^Shell,event:ui.Number_Action) {
    item:=binding(shell,event.payload); if item==nil || !shell.inspector.has_entity { return }
    owner:=shell.state.owner
    defer {
        if event.finished && shell.state.last_error!=.None && shell.field_gesture.active {
            cancel_error:=app.scene_gesture_cancel(owner,&shell.field_gesture)
            if cancel_error==.None { shell.field_gesture_node=0 } else { shell.state.last_error=cancel_error }
        }
    }
    if event.started {
        if shell.field_gesture.active { shell.state.last_error=.Invalid_Operation; return }
        ids:=make([]ecs.Entity_Id,len(shell.state.selection.entries),shell.allocator); defer delete(ids,shell.allocator)
        for selected,i in shell.state.selection.entries { ids[i]=selected.entity }
        shell.state.last_error=app.scene_gesture_begin(owner,&shell.field_gesture,ids)
        if shell.state.last_error!=.None { return }
        shell.field_gesture_node=event.node.key
    }
    if !shell.field_gesture.active || shell.field_gesture_node!=event.node.key { return }
    value:json.Value=json.Float(event.value); if item.field.kind==.Int { value=json.Integer(event.value) }
    color_value:json.Value
    if item.is_color { parsed,parse_error:=json.parse(item.field.value,spec=.JSON,allocator=shell.allocator); if parse_error!=nil { shell.state.last_error=.Decode_Failed; return }; color_value=parsed; defer json.destroy_value(color_value); array,valid:=color_value.(json.Array); if !valid || item.color_channel>=len(array) { shell.state.last_error=.Invalid_Field_Value; return }; array[item.color_channel]=json.Float(event.value); value=color_value }
    encoded,error:=json.marshal(value,allocator=shell.allocator); if error!=nil { shell.state.last_error=.Decode_Failed; return }; defer delete(encoded,shell.allocator)
    operations:=make([dynamic]editor.Scene_Op,shell.allocator)
    defer { for &operation in operations { inspector_operation_destroy(&operation,shell.allocator) }; delete(operations) }
    for selected in shell.state.selection.entries {
        operation,operation_error:=inspector_operation(shell.state,selected.entity,item.component,item.field.path,encoded)
        if operation_error!=.None { shell.state.last_error=operation_error; return }; append(&operations,operation)
    }
    shell.state.last_error=app.scene_gesture_preview_values(owner,&shell.field_gesture,operations[:])
    if shell.state.last_error==.None && event.finished {
        shell.state.last_error=app.scene_gesture_finish(owner,&shell.field_gesture)
        if shell.state.last_error==.None { shell.field_gesture_node=0; shell.state.revision+=1 }
    }
}
