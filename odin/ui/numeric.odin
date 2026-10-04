//! Numeric entry publishes finite validated values as one typed edit gesture.
package ui
import "core:fmt"
import "core:strings"
import "core:strconv"

@(private="package")
node_text_set :: proc(ctx:^Context,node:^Node,text:string)->bool {
    if node.descriptor.kind!=.Numeric_Input { return state_set(ctx,node.descriptor.state,text) }
    next:=strings.clone(text,ctx.allocator); delete(node.field_text,ctx.allocator); node.field_text=next; node.numeric_invalid=false; return true
}
@(private="package")
numeric_sync :: proc(ctx:^Context,node:^Node) {
    text:=fmt.aprintf("%g",node_number(ctx,node)); defer delete(text)
    if text!=node.field_text { node_text_set(ctx,node,text); text_history_clear(ctx,&node.undo); text_history_clear(ctx,&node.redo) }; node.numeric_invalid=false
}
@(private="package")
numeric_commit :: proc(ctx:^Context,node:^Node)->bool {
    value,parsed:=strconv.parse_f32(node.field_text)
    if !parsed || !finite(value) || value<node.descriptor.minimum || value>node.descriptor.maximum { node.numeric_invalid=true; return false }
    if value==node_number(ctx,node) { numeric_sync(ctx,node); return true }
    set_number(ctx,node,value,true,true); numeric_sync(ctx,node); return true
}
