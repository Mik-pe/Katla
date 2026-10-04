//! Ordered native input routes through the same layered rectangles used for drawing.
package ui
import "core:math"
import "core:strings"

@(private="package")
interactive :: proc(node:^Node)->bool {
    if node.input_disabled { return false }
    #partial switch node.descriptor.kind {
    case .Button,.Icon_Button,.Menu_Item,.Text_Input,.Code_Editor,.Numeric_Input,.Slider,.Drag_Value,.Checkbox,.Combo,.Tree_Row,.Selectable,.Tabs,.Dock_Space,.Scroll_Area,.Image,.Splitter,.Section,.Timeline: return true
    }
    return node.descriptor.focusable
}
@(private="package")
focusable :: proc(node:^Node)->bool {
    if node.input_disabled { return false }; if node.descriptor.focusable { return true }
    #partial switch node.descriptor.kind {
    case .Button,.Icon_Button,.Menu_Item,.Text_Input,.Code_Editor,.Numeric_Input,.Slider,.Drag_Value,.Checkbox,.Combo,.Selectable,.Tree_Row: return true
    }; return false
}
@(private="package")
hit_node :: proc(ctx:^Context,position:Vec2)->^Node {
    for layer_index:=int(Layer.Tooltip);layer_index>=0;layer_index-=1 {
        layer:=Layer(layer_index); if layer==.Tooltip { continue }
        for i:=len(ctx.order)-1;i>=0;i-=1 {
            node:=node_get(ctx,ctx.order[i]); if node_layer(ctx,node)!=layer || !node_visible(ctx,node) || node.input_disabled || (ctx.modal.key!=0 && !node_descends(ctx,node,ctx.modal)) { continue }
            if node.descriptor.kind==.Scroll_Area { for horizontal in ([]bool{false,true}) { track,_,present:=scrollbar(ctx,node,horizontal); if present && rect_contains(track,position) && rect_contains(node.clip,position) { return node } } }
        }
        for i:=len(ctx.order)-1;i>=0;i-=1 {
            node:=node_get(ctx,ctx.order[i]); if node_layer(ctx,node)!=layer || !node_visible(ctx,node) || (!interactive(node) && node.descriptor.kind!=.Context_Menu) || (ctx.modal.key!=0 && !node_descends(ctx,node,ctx.modal)) { continue }
            if rect_contains(node.bounds,position) && rect_contains(node.clip,position) { return node }
        }
    }; return nil
}
@(private="package")
focus_next :: proc(ctx:^Context,backward:bool) {
    chain:=make([dynamic]Node_Id,ctx.allocator); defer delete(chain)
    popup:=node_get(ctx,ctx.popup); popup_scope:=popup!=nil && popup.descriptor.kind==.Context_Menu
    for id in ctx.order { node:=node_get(ctx,id); if node_visible(ctx,node) && node_layer(ctx,node)!=.Tooltip && focusable(node) && (ctx.modal.key==0 || node_descends(ctx,node,ctx.modal)) && (!popup_scope || node_descends(ctx,node,ctx.popup)) { append(&chain,id) } }
    if len(chain)==0 { focus_set(ctx,{}); return }; index:=-1; for id,i in chain { if id==ctx.focused { index=i; break } }
    index+=-1 if backward else 1; if index<0 { index=len(chain)-1 }; if index>=len(chain) { index=0 }; focus_set(ctx,chain[index]); if node:=node_get(ctx,ctx.focused); node!=nil && (node.descriptor.kind==.Numeric_Input || (node.descriptor.kind==.Text_Input && !node.descriptor.multiline)) { node.anchor=0; node.cursor=len(node_text(ctx,node)); text_caret_visible(ctx,node) }
}
@(private="package")
set_number :: proc(ctx:^Context,node:^Node,value:f32,started,finished:bool) {
    d:=node.descriptor; bounded:=clamp(value,d.minimum,d.maximum)
    if d.step>0 {
        step:=f64(d.step); anchor:=f64(d.minimum)
        if math.abs(anchor)>step*16_777_216 { anchor=0 }
        quantized:=anchor+math.round((f64(bounded)-anchor)/step)*step
        bounded=f32(clamp(quantized,f64(d.minimum),f64(d.maximum)))
    }
    state_set(ctx,d.state,bounded); append(&ctx.actions,Number_Action{node.id,d.action,d.payload,d.state,bounded,started,finished})
}
@(private="package")
number_drag :: proc(ctx:^Context,node:^Node,position:Vec2,started,finished:bool) {
    d:=node.descriptor; value:=ctx.capture_value
    if d.kind==.Slider { track:=slider_track(ctx,node); if track.width>0 { value=d.minimum+clamp((position.x-track.x)/track.width,0,1)*(d.maximum-d.minimum) } }
    else { increment:=d.step; if increment<=0 { increment=(d.maximum-d.minimum)/200 }; value+=(position.x-ctx.capture_start.x)*increment }
    set_number(ctx,node,value,started,finished)
}
@(private="package")
dismiss :: proc(ctx:^Context,id:Node_Id,reason:Dismiss_Reason) { node:=node_get(ctx,id); if node!=nil { append(&ctx.actions,Dismiss_Action{node.id,node.descriptor.action,node.descriptor.payload,reason}) }; if ctx.popup==id { ctx.popup={}; if ctx.focus_before_popup.key!=0 { focus_set(ctx,ctx.focus_before_popup); ctx.focus_before_popup={} } } }
@(private="package")
dock_pointer_down :: proc(ctx:^Context,node:^Node,position:Vec2,button:Pointer_Button)->bool {
    tree:=node.descriptor.dock; if tree==nil { return false }; regions:=dock_subtree_bounds(tree,node.descriptor.dock_root,node.bounds,ctx.theme.row_height,4,ctx.allocator); defer delete(regions,ctx.allocator)
    if dock_float_pointer_down(ctx,node,position,button) { return true }
    for region in regions {
        dn:=tree.nodes[region.node]
        if dn.kind==.Split {
            bar:=region.bounds; if dn.direction==.Horizontal { bar.x+=(bar.width-4)*dn.ratio; bar.width=4 } else { bar.y+=(bar.height-4)*dn.ratio; bar.height=4 }
            if rect_contains(bar,position) { ctx.dock_split=dn.id; ctx.dock_bounds=region.bounds; return true }; continue
        }
        if !rect_contains(region.tab_bar,position) { continue }; x:=region.tab_bar.x
        for tab in dn.tabs {
            font,size:=node_font(ctx,node); width:=ctx.fonts.measure(ctx.fonts.state,font,dock_label(node,tab),size,0).x+ctx.theme.padding*2
            if position.x>=x && position.x<x+width {
                if button==.Middle { append(&ctx.actions,Dock_Action{kind=.Close,source=dn.id,tab=tab}); return false }
                ctx.dock_source=dn.id; ctx.dock_tab=tab; ctx.dock_bounds=region.bounds; append(&ctx.actions,Dock_Action{kind=.Activate,source=dn.id,tab=tab}); return true
            }; x+=width
        }
        if region.floating_root!=0 && button==.Left { ctx.dock_float=region.floating_root; ctx.dock_bounds=tree.floating[dock_float_index(tree,region.floating_root)].bounds; return true }
    }; return false
}
@(private="package")
dock_pointer_move :: proc(ctx:^Context,node:^Node,position:Vec2,finished:bool) {
    tree:=node.descriptor.dock; if tree==nil { return }
    if ctx.dock_float!=0 { dock_float_pointer_move(ctx,position); return }
    if ctx.dock_split!=0 {
        dn:=tree.nodes[ctx.dock_split]; if dn==nil { return }; ratio:f32=0.5
        if dn.direction==.Horizontal && ctx.dock_bounds.width>4 { ratio=(position.x-ctx.dock_bounds.x)/(ctx.dock_bounds.width-4) }
        else if dn.direction==.Vertical && ctx.dock_bounds.height>4 { ratio=(position.y-ctx.dock_bounds.y)/(ctx.dock_bounds.height-4) }
        append(&ctx.actions,Dock_Action{kind=.Resize,target=ctx.dock_split,ratio=clamp(ratio,0.05,0.95)}); return
    }
    delta:=position-ctx.capture_start; if delta.x*delta.x+delta.y*delta.y>16 { ctx.dock_dragging=true }
    if !finished || !ctx.dock_dragging || ctx.dock_tab==0 { return }
    regions:=dock_bounds(tree,dock_host_bounds(ctx,tree),ctx.theme.row_height,4,ctx.allocator); defer delete(regions,ctx.allocator)
    for i:=len(regions)-1;i>=0;i-=1 {
        region:=regions[i]
        if tree.nodes[region.node].kind==.Split || !rect_contains(region.bounds,position) { continue }
        zone:=Dock_Zone.Center; at:=len(tree.nodes[region.node].tabs)
        if !rect_contains(region.tab_bar,position) {
            relative:=position-Vec2{region.content.x,region.content.y}
            if relative.x<region.content.width*0.2 { zone=.Left } else if relative.x>region.content.width*0.8 { zone=.Right }
            else if relative.y<region.content.height*0.2 { zone=.Top } else if relative.y>region.content.height*0.8 { zone=.Bottom }
        } else {
            at=0; x:=region.tab_bar.x; font,size:=node_font(ctx,node)
            for tab in tree.nodes[region.node].tabs { width:=ctx.fonts.measure(ctx.fonts.state,font,dock_label(node,tab),size,0).x+ctx.theme.padding*2; if position.x<x+width/2 { break }; at+=1; x+=width }
        }
        append(&ctx.actions,Dock_Action{kind=.Move,source=ctx.dock_source,target=region.node,tab=ctx.dock_tab,zone=zone,index=at}); return
    }
    append(&ctx.actions,Dock_Action{kind=.Undock,source=ctx.dock_source,tab=ctx.dock_tab,bounds={position.x-32,position.y-16,clamp(ctx.dock_bounds.width,240,640),clamp(ctx.dock_bounds.height,160,480)}})
}
@(private="package")
activate_node :: proc(ctx:^Context,node:^Node) {
    d:=node.descriptor
    #partial switch d.kind {
    case .Checkbox: value:=!node_boolean(ctx,node); state_set(ctx,d.state,value); append(&ctx.actions,Toggle_Action{node.id,d.action,d.payload,d.state,value})
    case .Combo: if ctx.popup==node.id { ctx.popup={} } else { ctx.popup=node.id }
    case .Tabs:
        x:=node.bounds.x; font,size:=node_font(ctx,node)
        for option,i in d.options { width:=ctx.fonts.measure(ctx.fonts.state,font,option,size,0).x+ctx.theme.padding*2; if ctx.pointer.x>=x && ctx.pointer.x<x+width { state_set(ctx,d.state,f32(i)); append(&ctx.actions,Selection_Action{node.id,d.action,d.payload,i}); break }; x+=width }
    case .Tree_Row,.Section:
        if d.kind==.Section || (d.has_children && ctx.pointer.x<node.bounds.x+ctx.theme.padding+14) { append(&ctx.actions,Expand_Action{node.id,d.action,d.payload,!d.expanded}) }
        else { append(&ctx.actions,Click_Action{node=node.id,action=d.action,payload=d.payload,modifiers=ctx.capture_modifiers,clicks=ctx.capture_clicks,button=ctx.capture_button}) }
    case: append(&ctx.actions,Click_Action{node=node.id,action=d.action,payload=d.payload,modifiers=ctx.capture_modifiers,clicks=ctx.capture_clicks,button=ctx.capture_button})
    }
}
@(private="package")
route_input :: proc(ctx:^Context,input:Input)->Frame_Result {
    result:Frame_Result
    for event in input.events {
        switch e in event {
        case Pointer_Move:
            delta:=e.position-ctx.pointer; ctx.pointer=e.position
            if held:=node_get(ctx,ctx.captured); held!=nil {
                result.consumed_pointer=true
                if ctx.capture_draggable { moved:=e.position-ctx.capture_start; if moved.x*moved.x+moved.y*moved.y>16 { ctx.capture_dragged=true }; append(&ctx.actions,Pointer_Action{held.id,held.descriptor.action,held.descriptor.payload,e.position,delta,ctx.capture_button,false,false}) }
                #partial switch held.descriptor.kind {
                case .Slider,.Drag_Value: number_drag(ctx,held,e.position,false,false)
                case .Text_Input,.Code_Editor,.Numeric_Input:
                    if ctx.capture_text_lines { low,high:=text_line_range(node_text(ctx,held),text_hit(ctx,held,e.position)); if low<ctx.capture_line_start { held.cursor=low; held.anchor=ctx.capture_line_end } else { held.cursor=high; held.anchor=ctx.capture_line_start } } else { held.cursor=text_hit(ctx,held,e.position) }; text_caret_visible(ctx,held)
                case .Scroll_Area: scroll_pointer_move(ctx,held,e.position)
                case .Dock_Space: dock_pointer_move(ctx,held,e.position,false)
                case .Image,.Splitter,.Timeline: append(&ctx.actions,Pointer_Action{held.id,held.descriptor.action,held.descriptor.payload,e.position,delta,ctx.capture_button,false,false})
                }
            }
        case Pointer_Down:
            ctx.pointer=e.position
            if popup:=node_get(ctx,ctx.popup); popup!=nil && popup.descriptor.kind==.Combo {
                rect:=combo_popup(ctx,popup)
                if rect_contains(rect,e.position) { index:=clamp(int((e.position.y-rect.y)/ctx.theme.row_height),0,len(popup.descriptor.options)-1); state_set(ctx,popup.descriptor.state,f32(index)); append(&ctx.actions,Selection_Action{popup.id,popup.descriptor.action,popup.descriptor.payload,index}); ctx.popup={}; result.consumed_pointer=true; continue }
                if !rect_contains(popup.bounds,e.position) { dismiss(ctx,popup.id,.Outside); result.consumed_pointer=true; continue }
            } else if popup!=nil && !popup_contains(ctx,popup,e.position) { dismiss(ctx,popup.id,.Outside); result.consumed_pointer=true; continue }
            if modal:=node_get(ctx,ctx.modal); modal!=nil && !rect_contains(modal.bounds,e.position) { result.consumed_pointer=true; continue }
            node:=hit_node(ctx,e.position); if node==nil { focus_set(ctx,{}); if ctx.modal.key!=0 { result.consumed_pointer=true }; continue }; result.consumed_pointer=true
            dock_raise_node(ctx,node)
            if focusable(node) { focus_set(ctx,node.id) }; ctx.capture_start=e.position; ctx.capture_value=node_number(ctx,node); ctx.capture_button=e.button; ctx.capture_modifiers=e.modifiers; ctx.capture_clicks=e.clicks
            ctx.dock_source=0; ctx.dock_tab=0; ctx.dock_split=0; ctx.dock_float=0; ctx.dock_resize_edges=0; ctx.dock_dragging=false; ctx.scroll_drag=false; ctx.capture_draggable=node.descriptor.draggable && (node.descriptor.kind==.Selectable || node.descriptor.kind==.Tree_Row) && e.button==.Left; ctx.capture_dragged=false; ctx.capture_text_lines=false
            capture:=true
            #partial switch node.descriptor.kind {
            case .Context_Menu: capture=false
            case .Scroll_Area: capture=scroll_pointer_down(ctx,node,e.position)
            case .Dock_Space: capture=dock_pointer_down(ctx,node,e.position,e.button)
            case .Text_Input,.Code_Editor,.Numeric_Input:
                at:=text_hit(ctx,node,e.position); node.cursor=at; if .Shift not_in e.modifiers { node.anchor=at }
                if e.clicks>=3 || (node.descriptor.kind==.Code_Editor && e.position.x<text_inner(ctx,node).x) { low,high:=text_line_range(node_text(ctx,node),at); node.anchor=low; node.cursor=high; ctx.capture_text_lines=true; ctx.capture_line_start=low; ctx.capture_line_end=high }
                else if e.clicks==2 { text:=node_text(ctx,node); node.anchor=text_word(text,at,-1); node.cursor=text_word(text,at,1) }; text_caret_visible(ctx,node)
            case .Slider,.Drag_Value: number_drag(ctx,node,e.position,true,false)
            case .Image,.Splitter,.Timeline: append(&ctx.actions,Pointer_Action{node.id,node.descriptor.action,node.descriptor.payload,e.position,{},e.button,true,false})
            }
            if capture { ctx.captured=node.id; if ctx.capture_draggable { append(&ctx.actions,Pointer_Action{node.id,node.descriptor.action,node.descriptor.payload,e.position,{},e.button,true,false}) } }
        case Pointer_Up:
            ctx.pointer=e.position; node:=node_get(ctx,ctx.captured); if node==nil || e.button!=ctx.capture_button { continue }; result.consumed_pointer=true
            if ctx.capture_draggable { moved:=e.position-ctx.capture_start; if moved.x*moved.x+moved.y*moved.y>16 { ctx.capture_dragged=true }; append(&ctx.actions,Pointer_Action{node.id,node.descriptor.action,node.descriptor.payload,e.position,{},e.button,false,true}) }
            #partial switch node.descriptor.kind {
            case .Slider,.Drag_Value: number_drag(ctx,node,e.position,false,true)
            case .Dock_Space: dock_pointer_move(ctx,node,e.position,true)
            case .Image,.Splitter,.Timeline: append(&ctx.actions,Pointer_Action{node.id,node.descriptor.action,node.descriptor.payload,e.position,{},e.button,false,true})
            case .Text_Input,.Code_Editor,.Numeric_Input,.Scroll_Area:
            case: if !ctx.capture_dragged && rect_contains(node.bounds,e.position) && rect_contains(node.clip,e.position) { activate_node(ctx,node) }
            }; ctx.captured={}; ctx.dock_split=0; ctx.dock_tab=0; ctx.dock_source=0; ctx.dock_float=0; ctx.dock_resize_edges=0
        case Scroll:
            ctx.pointer=e.position; node:=hit_node(ctx,e.position)
            if node==nil { for i:=len(ctx.order)-1;i>=0;i-=1 { n:=node_get(ctx,ctx.order[i]); if node_visible(ctx,n) && rect_contains(n.clip,e.position) && n.descriptor.kind==.Scroll_Area && (ctx.modal.key==0 || node_descends(ctx,n,ctx.modal)) { node=n; break } } }
            for node!=nil {
                if node.descriptor.kind==.Scroll_Area {
                    node.scroll+=e.delta; node.scroll.x=clamp(node.scroll.x,0,max(0,node.content.width-node.bounds.width)); node.scroll.y=clamp(node.scroll.y,0,max(0,node.content.height-node.bounds.height))
                    append(&ctx.actions,Scroll_Action{node.id,node.descriptor.action,node.descriptor.payload,node.scroll}); result.consumed_pointer=true
                    layout_node(ctx,node_get(ctx,ctx.root),root_bounds(ctx,ctx.logical_size),{0,0,ctx.logical_size.x,ctx.logical_size.y}); break
                }; node=node_get(ctx,node.parent)
            }
        case Key_Down:
            if e.key==.Escape && ctx.popup.key!=0 { dismiss(ctx,ctx.popup,.Escape); result.consumed_keyboard=true; continue }
            if e.key==.Escape && ctx.modal.key!=0 { dismiss(ctx,ctx.modal,.Escape); result.consumed_keyboard=true; continue }
            if popup:=node_get(ctx,ctx.popup); popup!=nil && popup.descriptor.kind==.Context_Menu && (e.key==.Down || e.key==.Up) { focus_next(ctx,e.key==.Up); result.consumed_keyboard=true; continue }
            if e.key==.Tab { if node:=node_get(ctx,ctx.focused); node!=nil && node.descriptor.kind==.Code_Editor { code_indent(ctx,node,.Shift in e.modifiers); result.consumed_keyboard=true; continue }; focus_next(ctx,.Shift in e.modifiers); result.consumed_keyboard=true; continue }
            node:=node_get(ctx,ctx.focused)
            if node!=nil && text_editable(node) {
                handled:=text_key(ctx,node,e); if !handled && node.descriptor.kind==.Code_Editor { append(&ctx.actions,Key_Action{node=node.id,action=node.descriptor.action,payload=node.descriptor.payload,key=e.key,modifiers=e.modifiers,repeat=e.repeat}) }; result.consumed_keyboard=true; continue
            }
            if node!=nil && !node.input_disabled {
                if e.key==.Enter || e.key==.Space { if !e.repeat { ctx.capture_modifiers=e.modifiers; ctx.capture_clicks=1; ctx.capture_button=.Left; if node.descriptor.kind==.Tree_Row { append(&ctx.actions,Click_Action{node=node.id,action=node.descriptor.action,payload=node.descriptor.payload,modifiers=e.modifiers,clicks=1,button=.Left}) } else { activate_node(ctx,node) } }; result.consumed_keyboard=true; continue }
                if node.descriptor.kind==.Combo && (e.key==.Down || e.key==.Up) && len(node.descriptor.options)>0 { index:=clamp(int(node_number(ctx,node))+(-1 if e.key==.Up else 1),0,len(node.descriptor.options)-1); state_set(ctx,node.descriptor.state,f32(index)); append(&ctx.actions,Selection_Action{node.id,node.descriptor.action,node.descriptor.payload,index}); ctx.popup=node.id; result.consumed_keyboard=true; continue }
                if node.descriptor.kind==.Slider || node.descriptor.kind==.Drag_Value {
                    increment:=node.descriptor.step; if increment<=0 { increment=(node.descriptor.maximum-node.descriptor.minimum)/100 }
                    if e.key==.Left || e.key==.Down || e.key==.Right || e.key==.Up { set_number(ctx,node,node_number(ctx,node)+(-increment if e.key==.Left || e.key==.Down else increment),true,true); result.consumed_keyboard=true; continue }
                }
            }
            if ctx.modal.key!=0 { result.consumed_keyboard=true; continue }
            append(&ctx.actions,Key_Action{node=ctx.root,key=e.key,modifiers=e.modifiers,repeat=e.repeat})
        case Key_Up: if ctx.focused.key!=0 || ctx.modal.key!=0 { result.consumed_keyboard=true }
        case Text_Commit:
            node:=node_get(ctx,ctx.focused); if node!=nil && text_editable(node) && !node.input_disabled { text_replace(ctx,node,e.text); text_caret_visible(ctx,node); result.consumed_keyboard=true }
        case IME_Preedit:
            node:=node_get(ctx,ctx.focused); if node!=nil && text_editable(node) && !node.input_disabled { next:=strings.clone(e.text,ctx.allocator); delete(node.preedit,ctx.allocator); node.preedit=next; node.preedit_cursor=text_boundary(next,e.cursor); node.preedit_end=text_boundary(next,e.selection_end); result.consumed_keyboard=true }
        case Window_Focus:
            ctx.window_focused=e.focused; if !e.focused { cancel_capture(ctx); if node:=node_get(ctx,ctx.focused); node!=nil { delete(node.preedit,ctx.allocator); node.preedit="" } }
        }
    }
    ctx.hovered={}; if node:=hit_node(ctx,ctx.pointer); node!=nil { ctx.hovered=node.id
        #partial switch node.descriptor.kind {
        case .Text_Input,.Code_Editor,.Numeric_Input: result.cursor=.Text
        case .Button,.Icon_Button,.Checkbox,.Combo,.Menu_Item,.Tree_Row,.Selectable: result.cursor=.Hand
        case .Slider,.Drag_Value,.Splitter: result.cursor=.Horizontal_Resize
        case .Image: result.cursor=.Grab
        case .Dock_Space:
            if node.descriptor.dock_root!=0 { result.cursor=.Grab }
        }
    }
    if ctx.captured.key!=0 { node:=node_get(ctx,ctx.captured); if node!=nil && node.descriptor.kind==.Image { result.cursor=.Grabbing } }
    if ctx.dock_float!=0 { result.cursor=.Grabbing; if ctx.dock_resize_edges&3!=0 { result.cursor=.Horizontal_Resize } else if ctx.dock_resize_edges&12!=0 { result.cursor=.Vertical_Resize } }
    return result
}

@(private="package")
popup_contains :: proc(ctx:^Context,popup:^Node,position:Vec2)->bool {
    for id in ctx.order { node:=node_get(ctx,id); if node_visible(ctx,node) && node_descends(ctx,node,popup.id) && rect_contains(node.clip,position) { return true } }; return false
}

@(private="package")
focus_set :: proc(ctx:^Context,id:Node_Id) {
    if id==ctx.focused { return }
    if old:=node_get(ctx,ctx.focused); old!=nil {
        if text_editable(old) && old.text_dirty && old.descriptor.kind!=.Numeric_Input { emit_text(ctx,old,true) }
        if old.descriptor.kind==.Numeric_Input { if !numeric_commit(ctx,old) { numeric_sync(ctx,old) } }
        delete(old.preedit,ctx.allocator); old.preedit=""
    }; ctx.focused=id
}
@(private="package")
cancel_capture :: proc(ctx:^Context) {
    if node:=node_get(ctx,ctx.captured); node!=nil {
        if ctx.capture_draggable { append(&ctx.actions,Pointer_Action{node.id,node.descriptor.action,node.descriptor.payload,ctx.pointer,{},ctx.capture_button,false,true}) }
        #partial switch node.descriptor.kind {
        case .Slider,.Drag_Value: set_number(ctx,node,node_number(ctx,node),false,true)
        case .Image,.Splitter,.Timeline: append(&ctx.actions,Pointer_Action{node.id,node.descriptor.action,node.descriptor.payload,ctx.pointer,{},ctx.capture_button,false,true})
        }
    }; ctx.captured={}; ctx.dock_dragging=false; ctx.dock_split=0; ctx.dock_tab=0; ctx.dock_float=0; ctx.dock_resize_edges=0; ctx.scroll_drag=false; ctx.capture_draggable=false; ctx.capture_dragged=false
}
