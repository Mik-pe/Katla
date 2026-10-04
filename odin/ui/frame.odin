//! Reconcile, solve, route input and publish one immutable ordered draw list.
package ui

@(private="package")
order_layers :: proc(ctx:^Context) {
    ordered:=make([dynamic]Node_Id,ctx.allocator); defer delete(ordered)
    for layer in Layer { for id in ctx.order { if node_layer(ctx,node_get(ctx,id))==layer { append(&ordered,id) } } }
    copy(ctx.order[:],ordered[:])
}
@(private="package")
frame_layers :: proc(ctx:^Context) {
    previous:=ctx.modal; previous_popup:=ctx.popup; ctx.modal={}
    for id in ctx.order {
        node:=node_get(ctx,id); if !node_visible(ctx,node) { continue }
        if node.descriptor.kind==.Modal { ctx.modal=id }
        if node.descriptor.kind==.Context_Menu { ctx.popup=id }
    }
    if previous!=ctx.modal {
        if ctx.modal.key!=0 { ctx.focus_before_modal=ctx.focused; focus_set(ctx,{}); focus_next(ctx,false) }
        else { focus_set(ctx,ctx.focus_before_modal); ctx.focus_before_modal={} }
    }
    if node:=node_get(ctx,ctx.popup); node==nil || (ctx.modal.key!=0 && !node_descends(ctx,node,ctx.modal)) || (node!=nil && !node_visible(ctx,node)) { ctx.popup={} }
    if popup:=node_get(ctx,ctx.popup); popup!=nil && popup.descriptor.kind==.Context_Menu && ctx.popup!=previous_popup { ctx.focus_before_popup=ctx.focused; focus_set(ctx,{}); focus_next(ctx,false) }
    if previous_popup.key!=0 && ctx.popup.key==0 && ctx.focus_before_popup.key!=0 && ctx.modal.key==0 { focus_set(ctx,ctx.focus_before_popup); ctx.focus_before_popup={} }
    if node:=node_get(ctx,ctx.focused); node!=nil && (!node_visible(ctx,node) || node.input_disabled) { focus_set(ctx,{}) }
    if node:=node_get(ctx,ctx.captured); node!=nil && (!node_visible(ctx,node) || node.input_disabled) { cancel_capture(ctx) }
}
/// The returned commands and text remain immutable until the next successful frame or context_destroy.
/// Invalid descriptors leave the previous retained tree and draw list available for recovery.
frame :: proc(ctx:^Context,descriptor:Descriptor,input:Input,size:Vec2)->(Draw_List,Frame_Result) {
    scale:=input.pixel_scale; if scale==0 { scale=1 }
    draw:=Draw_List{commands=ctx.commands[:],logical_size=size,pixel_scale=scale}
    if !ctx.initialized || ctx.closed { return draw,{error=.Closed} }
    if !finite(size.x) || !finite(size.y) || size.x<0 || size.y<0 || !finite(scale) || scale<=0 { return draw,{error=.Invalid_Layout} }
    if ctx.frame_index==max(u64) { return draw,{error=.Invalid_State} }
    error:=reconcile(ctx,descriptor); if error!=.None { return draw,{error=error} }
    for id in ctx.order { node:=node_get(ctx,id); if node.descriptor.kind==.Numeric_Input && ctx.focused!=node.id { numeric_sync(ctx,node) } }
    ctx.logical_size=size
    order_layers(ctx); window:=Rect{0,0,size.x,size.y}; layout_node(ctx,node_get(ctx,ctx.root),root_bounds(ctx,size),window); frame_layers(ctx)
    result:=route_input(ctx,input)
    // Scroll and committed text may change layout during this input sequence.
    layout_node(ctx,node_get(ctx,ctx.root),root_bounds(ctx,size),window)
    if node:=node_get(ctx,ctx.focused); node!=nil && text_editable(node) { text_caret_visible(ctx,node) }
    draw_commands_clear(ctx)
    for layer in Layer {
        for id in ctx.order { node:=node_get(ctx,id); if node_layer(ctx,node)==layer { paint_node(ctx,node) } }
        for id in ctx.order { node:=node_get(ctx,id); if node_layer(ctx,node)==layer && node_visible(ctx,node) && node.descriptor.kind==.Scroll_Area { paint_scrollbars(ctx,node) } }
    }
    paint_dock_overlay(ctx,window)
    if node:=node_get(ctx,ctx.popup); node!=nil && node.descriptor.kind==.Combo && node_visible(ctx,node) { paint_combo_popup(ctx,node,window) }
    if node:=node_get(ctx,ctx.focused); node!=nil && text_editable(node) && ctx.window_focused {
        text_caret_visible(ctx,node); font,font_size:=node_font(ctx,node); inset:=text_inner(ctx,node); wrap:=text_wrap(node,inset.width)
        caret:=ctx.fonts.caret(ctx.fonts.state,font,node_text(ctx,node),font_size,wrap,node.cursor)
        if node.preedit!="" { caret+=ctx.fonts.caret(ctx.fonts.state,font,node.preedit,font_size,wrap,node.preedit_cursor) }
        result.ime={active=true,cursor={inset.x+caret.x-node.text_offset.x,inset.y+caret.y-node.text_offset.y,1,font_size+2},selection_start=min(node.cursor,node.anchor),selection_end=max(node.cursor,node.anchor)}
    }
    result.captured_pointer=ctx.captured.key!=0 || ctx.modal.key!=0 || ctx.popup.key!=0
    result.captured_keyboard=ctx.modal.key!=0 || ctx.popup.key!=0; if focused:=node_get(ctx,ctx.focused); focused!=nil && text_editable(focused) { result.captured_keyboard=true }
    draw.commands=ctx.commands[:]; return draw,result
}
/// Explicit focus changes validate exact generational identity and modal scope.
focus :: proc(ctx:^Context,id:Node_Id)->bool { node:=node_get(ctx,id); if node==nil || !node_visible(ctx,node) || !focusable(node) || (ctx.modal.key!=0 && !node_descends(ctx,node,ctx.modal)) { return false }; focus_set(ctx,id); return true }
node_id :: proc(ctx:^Context,key:u64)->(Node_Id,bool) { node,present:=ctx.nodes[key]; if !present { return {},false }; return node.id,true }
