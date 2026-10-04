//! Reconciliation retains exact keyed nodes and validates the whole incoming tree first.
package ui
import "core:mem"
import "core:strings"

@(private="package")
descriptor_destroy :: proc(descriptor:^Descriptor,allocator:mem.Allocator) {
    delete(descriptor.text,allocator); delete(descriptor.placeholder,allocator)
    for option in descriptor.options { delete(option,allocator) }; delete(descriptor.options,allocator)
    for tab in descriptor.dock_tabs { delete(tab.label,allocator) }; delete(descriptor.dock_tabs,allocator)
    delete(descriptor.syntax,allocator)
    descriptor^={}
}
@(private="package")
descriptor_clone :: proc(source:Descriptor,allocator:mem.Allocator)->Descriptor {
    result:=source; result.children=nil
    result.text=strings.clone(source.text,allocator); result.placeholder=strings.clone(source.placeholder,allocator)
    result.options=make([]string,len(source.options),allocator)
    for option,i in source.options { result.options[i]=strings.clone(option,allocator) }
    result.syntax=make([]Text_Run,len(source.syntax),allocator); copy(result.syntax,source.syntax)
    result.dock_tabs=make([]Dock_Tab,len(source.dock_tabs),allocator)
    for tab,i in source.dock_tabs { result.dock_tabs[i]={tab.tab,strings.clone(tab.label,allocator)} }
    return result
}
@(private="package")
descriptor_validate :: proc(ctx:^Context,descriptor:Descriptor,keys:^map[u64]bool,depth:int)->Frame_Error {
    if int(descriptor.kind)<0 || int(descriptor.kind)>int(Widget_Kind.Timeline) || int(descriptor.layer)<0 || int(descriptor.layer)>int(Layer.Tooltip) { return .Invalid_Descriptor }
    for color in ([]Color{descriptor.background,descriptor.foreground}) { for value in color { if !finite(value) { return .Invalid_Descriptor } } }
    if depth>128 || len(keys^)>65536 || descriptor.key==0 { return .Invalid_Descriptor }
    if keys^[descriptor.key] { return .Duplicate_Key }; keys^[descriptor.key]=true
    if !layout_valid(descriptor.layout) || (descriptor.has_fixed_bounds && !rect_valid(descriptor.fixed_bounds)) { return .Invalid_Layout }
    if descriptor.kind==.Dock_Space && descriptor.dock==nil { return .Invalid_Descriptor }
    if descriptor.dock_root!=0 && (descriptor.dock==nil || descriptor.dock.nodes[descriptor.dock_root]==nil) { return .Invalid_Descriptor }
    if !finite(descriptor.text_max_width) || descriptor.text_max_width<0 { return .Invalid_Descriptor }
    if !finite(descriptor.font_size) || descriptor.font_size<0 || !finite(descriptor.minimum) || !finite(descriptor.maximum) || !finite(descriptor.step) || !finite(descriptor.value) { return .Invalid_Descriptor }
    if (descriptor.kind==.Slider || descriptor.kind==.Drag_Value || descriptor.kind==.Numeric_Input) && descriptor.maximum<=descriptor.minimum { return .Invalid_Descriptor }
    if descriptor.kind==.Text_Input || descriptor.kind==.Code_Editor { if _,valid:=state_get(ctx,descriptor.state,string); !valid { return .Invalid_State } }
    if descriptor.state.node.key!=0 {
        #partial switch descriptor.kind {
        case .Checkbox: if _,valid:=state_get(ctx,descriptor.state,bool); !valid { return .Invalid_State }
        case .Slider,.Drag_Value,.Numeric_Input,.Combo,.Tabs: if _,valid:=state_get(ctx,descriptor.state,f32); !valid { return .Invalid_State }
        }
    }
    for child in descriptor.children { error:=descriptor_validate(ctx,child,keys,depth+1); if error!=.None { return error } }
    return .None
}
@(private="package")
reconcile_node :: proc(ctx:^Context,descriptor:Descriptor,parent:Node_Id)->Node_Id {
    node:=node_ensure(ctx,descriptor.key)
    descriptor_destroy(&node.descriptor,ctx.allocator); node.descriptor=descriptor_clone(descriptor,ctx.allocator)
    node.input_disabled=descriptor.disabled; if p:=node_get(ctx,parent); p!=nil { node.input_disabled=node.input_disabled || p.input_disabled }
    node.parent=parent; node.seen=ctx.frame_index; node.mounted=true; clear(&node.children)
    append(&ctx.order,node.id)
    for child in descriptor.children { append(&node.children,reconcile_node(ctx,child,node.id)) }
    return node.id
}
@(private="package")
reconcile :: proc(ctx:^Context,descriptor:Descriptor)->Frame_Error {
    keys:=make(map[u64]bool,ctx.allocator); defer delete(keys)
    error:=descriptor_validate(ctx,descriptor,&keys,0); if error!=.None { return error }
    ctx.frame_index+=1; for _,node in ctx.nodes { node.mounted=false }; clear(&ctx.order); ctx.root=reconcile_node(ctx,descriptor,{})
    removed:=make([dynamic]u64,ctx.allocator); defer delete(removed)
    for key,node in ctx.nodes { if node.seen!=ctx.frame_index && node.retained_until<ctx.frame_index { append(&removed,key) } }
    for key in removed { node:=ctx.nodes[key]; if ctx.focused==node.id { focus_set(ctx,{}) }; if ctx.captured==node.id { cancel_capture(ctx) } }
    for key in removed { node:=ctx.nodes[key]; if ctx.captured==node.id { cancel_capture(ctx) }; delete_key(&ctx.nodes,key); node_destroy(ctx,node) }
    if node_get(ctx,ctx.focused)==nil { ctx.focused={} }
    if node_get(ctx,ctx.captured)==nil { ctx.captured={} }
    return .None
}
