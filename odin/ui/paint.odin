//! Draw commands own shaped-text inputs and preserve exact layered clip ordering.
package ui
import "core:strings"
import "core:fmt"

@(private="package")
node_visible :: proc(ctx:^Context,node:^Node)->bool {
    current:=node
    for current!=nil { if current.descriptor.hidden || !current.mounted { return false }; current=node_get(ctx,current.parent) }; return true
}
@(private="package")
node_layer :: proc(ctx:^Context,node:^Node)->Layer {
    layer:=node.descriptor.layer; current:=node
    for current!=nil {
        layer=max(layer,current.descriptor.layer)
        if current.descriptor.dock!=nil && current.descriptor.dock_root!=0 { if _,floating:=dock_floating_index(current.descriptor.dock,current.descriptor.dock_root); floating { layer=max(layer,Layer.Overlay) } }
        if current.descriptor.kind==.Modal { layer=max(layer,Layer.Modal) }
        if current.descriptor.kind==.Context_Menu { layer=max(layer,Layer.Popup) }
        if current.descriptor.kind==.Tooltip { layer=max(layer,Layer.Tooltip) }
        current=node_get(ctx,current.parent)
    }; return layer
}
@(private="package")
node_descends :: proc(ctx:^Context,node:^Node,parent:Node_Id)->bool {
    current:=node; for current!=nil { if current.id==parent { return true }; current=node_get(ctx,current.parent) }; return false
}
@(private="package")
paint_rect :: proc(ctx:^Context,bounds,clip:Rect,color:Color,radius:f32=0) { if bounds.width>0 && bounds.height>0 && clip.width>0 && clip.height>0 { append(&ctx.commands,Rect_Draw{bounds,clip,color,radius}) } }
@(private="package")
paint_text :: proc(ctx:^Context,node:^Node,text:string,position:Vec2,color:Color,clip:Rect,wrap:f32=0,runs:[]Text_Run=nil) {
    if len(text)==0 || clip.width<=0 || clip.height<=0 { return }; font,size:=node_font(ctx,node)
    owned_runs:=make([]Text_Run,len(runs),ctx.allocator); copy(owned_runs,runs)
    append(&ctx.commands,Text_Draw{font=font,text=strings.clone(text,ctx.allocator),position=position,size=size,wrap_width=wrap,clip=clip,color=color,runs=owned_runs})
}
@(private="package")
slider_track :: proc(ctx:^Context,node:^Node)->Rect {
    pad:=ctx.theme.padding; label:f32=0; if node.descriptor.text!="" { label=min(node.bounds.width*0.4,80) }
    _,size:=node_font(ctx,node); number_width:=size*4.75+pad
    return {node.bounds.x+label+pad,node.bounds.y+node.bounds.height/2-2,max(0,node.bounds.width-label-2*pad-number_width),4}
}
@(private="package")
node_number :: proc(ctx:^Context,node:^Node)->f32 { if value,ok:=state_get(ctx,node.descriptor.state,f32); ok { return value }; return node.descriptor.value }
@(private="package")
node_boolean :: proc(ctx:^Context,node:^Node)->bool { if value,ok:=state_get(ctx,node.descriptor.state,bool); ok { return value }; return node.descriptor.selected }
@(private="package")
paint_node :: proc(ctx:^Context,node:^Node) {
    if !node_visible(ctx,node) { return }
    d:=node.descriptor; b,clip:=node.bounds,node.clip; fg:=ctx.theme.text; bg:=ctx.theme.control
    if d.has_foreground { fg=d.foreground }; if node.input_disabled { fg=ctx.theme.disabled }
    hovered:=ctx.hovered==node.id; held:=ctx.captured==node.id
    if d.has_background { bg=d.background } else { if hovered { bg=ctx.theme.hover }; if held || d.selected { bg=ctx.theme.active } }
    _,label_size:=node_font(ctx,node); label_pos:=Vec2{b.x+ctx.theme.padding,b.y+(b.height-label_size)/2}
    #partial switch d.kind {
    case .Button,.Icon_Button,.Menu_Item,.Selectable,.Combo,.Drag_Value,.Text_Input,.Code_Editor,.Numeric_Input:
        paint_rect(ctx,b,clip,bg,ctx.theme.radius)
    case .Modal,.Context_Menu,.Tooltip: paint_rect(ctx,b,clip,d.background if d.has_background else ctx.theme.panel,ctx.theme.radius)
    case: if d.has_background { paint_rect(ctx,b,clip,d.background,ctx.theme.radius) }
    }
    #partial switch d.kind {
    case .Text:
        if d.text_max_width>0 {
            label,shortened:=text_ellipsize(ctx,node,min(d.text_max_width,b.width))
            paint_text(ctx,node,label,{b.x,b.y},fg,clip)
            if shortened { delete(label,ctx.allocator) }
        } else { paint_text(ctx,node,d.text,{b.x,b.y},fg,clip,b.width) }
    case .Button,.Icon_Button,.Menu_Item,.Selectable,.Tree_Row,.Section:
        if d.kind==.Tree_Row || d.kind==.Section {
            marker:="▸"; if d.expanded { marker="▾" }; if d.kind==.Section || d.has_children { paint_text(ctx,node,marker,label_pos,ctx.theme.muted,clip) }; label_pos.x+=14
            if d.selected && !d.has_background { paint_rect(ctx,b,clip,ctx.theme.active,0) }
        }
        paint_text(ctx,node,d.text,label_pos,fg,clip)
    case .Checkbox:
        box:=Rect{b.x+ctx.theme.padding,b.y+(b.height-14)/2,14,14}; paint_rect(ctx,box,clip,bg,3)
        if node_boolean(ctx,node) { paint_rect(ctx,rect_inset(box,3),clip,ctx.theme.accent,1) }; label_pos.x+=20; paint_text(ctx,node,d.text,label_pos,fg,clip)
    case .Slider:
        track:=slider_track(ctx,node); value:=clamp(node_number(ctx,node),d.minimum,d.maximum); ratio:=(value-d.minimum)/(d.maximum-d.minimum)
        label_clip:=rect_intersection(clip,{b.x,b.y,max(0,track.x-b.x-ctx.theme.padding),b.height})
        paint_text(ctx,node,d.text,label_pos,fg,label_clip)
        number:=fmt.aprintf("%.3g",node_number(ctx,node)); defer delete(number)
        font,size:=node_font(ctx,node); measured:=ctx.fonts.measure(ctx.fonts.state,font,number,size,0)
        number_start:=track.x+track.width+ctx.theme.padding
        number_clip:=rect_intersection(clip,{number_start,b.y,max(0,b.x+b.width-ctx.theme.padding-number_start),b.height})
        paint_text(ctx,node,number,{max(number_start,b.x+b.width-ctx.theme.padding-measured.x),label_pos.y},fg,number_clip)
        paint_rect(ctx,track,clip,ctx.theme.hover,2)
        paint_rect(ctx,{track.x,track.y,track.width*ratio,track.height},clip,ctx.theme.accent,2)
        paint_rect(ctx,{track.x+track.width*ratio-4,track.y-5,8,14},clip,ctx.theme.accent,3)
    case .Drag_Value:
        text:=fmt.aprintf("%s %g",d.text,node_number(ctx,node)); defer delete(text); paint_text(ctx,node,text,label_pos,fg,clip)
    case .Text_Input,.Code_Editor,.Numeric_Input:
        text:=node_text(ctx,node); inset:=text_inner(ctx,node); text_clip:=rect_intersection(clip,inset)
        position:=Vec2{inset.x-node.text_offset.x,inset.y-node.text_offset.y}; font,size:=node_font(ctx,node); wrap:=text_wrap(node,inset.width)
        if d.kind==.Code_Editor { paint_code_gutter(ctx,node,text) }
        if ctx.focused==node.id && node.cursor!=node.anchor {
            a:=ctx.fonts.caret(ctx.fonts.state,font,text,size,wrap,min(node.cursor,node.anchor)); z:=ctx.fonts.caret(ctx.fonts.state,font,text,size,wrap,max(node.cursor,node.anchor))
            if a.y==z.y { paint_rect(ctx,{position.x+a.x,position.y+a.y,max(1,z.x-a.x),size+2},text_clip,{0.35,0.78,0.98,0.35},0) }
            else { paint_rect(ctx,{position.x+a.x,position.y+a.y,max(1,inset.width-a.x),size+2},text_clip,{0.35,0.78,0.98,0.35}); paint_rect(ctx,{position.x,position.y+a.y+size+2,inset.width,max(0,z.y-a.y-size-2)},text_clip,{0.35,0.78,0.98,0.35}); paint_rect(ctx,{position.x,position.y+z.y,max(1,z.x),size+2},text_clip,{0.35,0.78,0.98,0.35}) }
        }
        if text=="" && ctx.focused!=node.id { paint_text(ctx,node,d.placeholder,position,ctx.theme.muted,text_clip,wrap) } else { paint_text(ctx,node,text,position,fg,text_clip,wrap,d.syntax) }
        if ctx.focused==node.id && ctx.window_focused {
            caret:=ctx.fonts.caret(ctx.fonts.state,font,text,size,wrap,node.cursor)
            paint_rect(ctx,{position.x+caret.x,position.y+caret.y,1,size+2},text_clip,ctx.theme.accent)
            if node.preedit!="" { paint_text(ctx,node,node.preedit,position+caret,fg,text_clip,wrap); measured:=ctx.fonts.measure(ctx.fonts.state,font,node.preedit,size,wrap); paint_rect(ctx,{position.x+caret.x,position.y+caret.y+size+2,measured.x,1},text_clip,ctx.theme.accent) }
        }
    case .Image:
        uv:=d.uv; if uv.width==0 && uv.height==0 { uv={0,0,1,1} }; append(&ctx.commands,Image_Draw{texture=d.texture,bounds=b,uv=uv,clip=clip,tint={1,1,1,1}})
    case .Separator: paint_rect(ctx,b,clip,ctx.theme.hover)
    case .Progress:
        paint_rect(ctx,b,clip,ctx.theme.hover,ctx.theme.radius); paint_rect(ctx,{b.x,b.y,b.width*clamp(d.value,0,1),b.height},clip,ctx.theme.accent,ctx.theme.radius)
    case .Splitter: paint_rect(ctx,b,clip,ctx.theme.accent if held else ctx.theme.hover)
    case .Combo:
        index:=int(node_number(ctx,node)); text:=d.text; if index>=0 && index<len(d.options) { text=d.options[index] }; paint_text(ctx,node,text,label_pos,fg,clip)
    case .Tabs:
        x:=b.x; selected:=int(node_number(ctx,node))
        for option,i in d.options { font,size:=node_font(ctx,node); width:=ctx.fonts.measure(ctx.fonts.state,font,option,size,0).x+ctx.theme.padding*2; rect:=Rect{x,b.y,width,b.height}; if i==selected { paint_rect(ctx,rect,clip,ctx.theme.active,ctx.theme.radius) }; paint_text(ctx,node,option,{x+ctx.theme.padding,label_pos.y},fg,clip); x+=width }
    case .Dock_Space: paint_dock(ctx,node)
    case .Timeline:
        paint_rect(ctx,b,clip,ctx.theme.panel)
        for i in 0..<11 { x:=b.x+b.width*f32(i)/10; paint_rect(ctx,{x,b.y,1,b.height},clip,ctx.theme.hover) }
        paint_rect(ctx,{b.x+b.width*clamp(d.value,0,1),b.y,2,b.height},clip,ctx.theme.accent)
    }
    if node.numeric_invalid { paint_rect(ctx,{b.x,b.y+b.height-2,b.width,2},clip,{1,0.25,0.2,1}) }
    if ctx.focused==node.id && d.kind!=.Text_Input { paint_rect(ctx,{b.x,b.y+b.height-1,b.width,1},clip,ctx.theme.accent) }
}
@(private="package")
combo_popup :: proc(ctx:^Context,node:^Node)->Rect { return {node.bounds.x,node.bounds.y+node.bounds.height,node.bounds.width,ctx.theme.row_height*f32(len(node.descriptor.options))} }
@(private="package")
paint_combo_popup :: proc(ctx:^Context,node:^Node,window:Rect) {
    popup:=combo_popup(ctx,node); clip:=rect_intersection(popup,window); paint_rect(ctx,popup,clip,ctx.theme.panel,ctx.theme.radius)
    for option,i in node.descriptor.options {
        row:=Rect{popup.x,popup.y+ctx.theme.row_height*f32(i),popup.width,ctx.theme.row_height}; if rect_contains(row,ctx.pointer) { paint_rect(ctx,row,clip,ctx.theme.hover) }
        paint_text(ctx,node,option,{row.x+ctx.theme.padding,row.y+ctx.theme.padding},ctx.theme.text,clip)
    }
}
@(private="package")
dock_label :: proc(node:^Node,tab:Tab_Id)->string { for item in node.descriptor.dock_tabs { if item.tab==tab { return item.label } }; return "Panel" }
@(private="package")
paint_dock :: proc(ctx:^Context,node:^Node) {
    tree:=node.descriptor.dock; if tree==nil { return }; regions:=dock_subtree_bounds(tree,node.descriptor.dock_root,node.bounds,ctx.theme.row_height,4,ctx.allocator); defer delete(regions,ctx.allocator)
    if _,floating:=dock_floating_index(tree,node.descriptor.dock_root); floating && !node.descriptor.has_background { paint_rect(ctx,node.bounds,node.clip,ctx.theme.panel,ctx.theme.radius) }
    for region in regions {
        dn:=tree.nodes[region.node]
        if dn.kind==.Split {
            split:=region.bounds
            if dn.direction==.Horizontal { split.x+=max(0,split.width-4)*dn.ratio; split.width=4 } else { split.y+=max(0,split.height-4)*dn.ratio; split.height=4 }
            paint_rect(ctx,split,node.clip,ctx.theme.hover); continue
        }
        paint_rect(ctx,region.tab_bar,node.clip,ctx.theme.panel)
        x:=region.tab_bar.x
        for tab in dn.tabs {
            label:=dock_label(node,tab); font,size:=node_font(ctx,node); width:=ctx.fonts.measure(ctx.fonts.state,font,label,size,0).x+2*ctx.theme.padding
            rect:=Rect{x,region.tab_bar.y,width,region.tab_bar.height}; if tab==region.active { paint_rect(ctx,rect,node.clip,ctx.theme.active,ctx.theme.radius) }
            paint_text(ctx,node,label,{x+ctx.theme.padding,rect.y+ctx.theme.padding},ctx.theme.text,node.clip); x+=width
        }
    }
}

@(private="package")
paint_dock_overlay :: proc(ctx:^Context,window:Rect) {
    if !ctx.dock_dragging || ctx.dock_tab==0 { return }; node:=node_get(ctx,ctx.captured)
    if node==nil || node.descriptor.kind!=.Dock_Space || node.descriptor.dock==nil { return }
    regions:=dock_bounds(node.descriptor.dock,dock_host_bounds(ctx,node.descriptor.dock),ctx.theme.row_height,4,ctx.allocator); defer delete(regions,ctx.allocator)
    for i:=len(regions)-1;i>=0;i-=1 {
        region:=regions[i]
        if node.descriptor.dock.nodes[region.node].kind==.Split || !rect_contains(region.bounds,ctx.pointer) { continue }
        preview:=region.content; local:=ctx.pointer-Vec2{preview.x,preview.y}
        if !rect_contains(region.tab_bar,ctx.pointer) {
            if local.x<preview.width*0.2 { preview.width/=2 } else if local.x>preview.width*0.8 { preview.x+=preview.width/2; preview.width/=2 }
            else if local.y<preview.height*0.2 { preview.height/=2 } else if local.y>preview.height*0.8 { preview.y+=preview.height/2; preview.height/=2 }
        }
        paint_rect(ctx,preview,window,{ctx.theme.selection.r,ctx.theme.selection.g,ctx.theme.selection.b,0.25},ctx.theme.radius); break
    }
    label:=dock_label(node,ctx.dock_tab); font,size:=node_font(ctx,node); measured:=ctx.fonts.measure(ctx.fonts.state,font,label,size,0)
    preview:=Rect{ctx.pointer.x+12,ctx.pointer.y+12,measured.x+ctx.theme.padding*2,ctx.theme.row_height}
    paint_rect(ctx,preview,window,ctx.theme.active,ctx.theme.radius); paint_text(ctx,node,label,{preview.x+ctx.theme.padding,preview.y+ctx.theme.padding},ctx.theme.text,window)
}
