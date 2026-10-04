//! UTF-8 text editing preserves byte boundaries, selection and uncommitted IME composition.
package ui
import "core:strings"

@(private="package")
text_boundary :: proc(text:string,offset:int)->int { at:=clamp(offset,0,len(text)); for at>0 && at<len(text) && (u8(text[at])&0xc0)==0x80 { at-=1 }; return at }
@(private="package")
text_previous :: proc(text:string,offset:int)->int { if offset<=0 { return 0 }; return text_boundary(text,offset-1) }
@(private="package")
text_next :: proc(text:string,offset:int)->int { at:=clamp(offset+1,0,len(text)); for at<len(text) && (u8(text[at])&0xc0)==0x80 { at+=1 }; return at }
@(private="package")
text_word :: proc(text:string,offset:int,direction:int)->int {
    at:=text_boundary(text,offset)
    if direction<0 { for at>0 && (text[at-1]==' ' || text[at-1]=='\n' || text[at-1]=='\t') { at=text_previous(text,at) }; for at>0 && text[at-1]!=' ' && text[at-1]!='\n' && text[at-1]!='\t' { at=text_previous(text,at) } }
    else { for at<len(text) && text[at]!=' ' && text[at]!='\n' && text[at]!='\t' { at=text_next(text,at) }; for at<len(text) && (text[at]==' ' || text[at]=='\n' || text[at]=='\t') { at=text_next(text,at) } }; return at
}
@(private="package")
text_replace :: proc(ctx:^Context,node:^Node,insert:string)->bool {
    text:=node_text(ctx,node); if node.descriptor.kind!=.Numeric_Input { if _,ok:=state_get(ctx,node.descriptor.state,string); !ok { return false } }
    low:=text_boundary(text,min(node.cursor,node.anchor)); high:=text_boundary(text,max(node.cursor,node.anchor))
    builder:strings.Builder; strings.builder_init(&builder,ctx.allocator)
    strings.write_string(&builder,text[:low]); strings.write_string(&builder,insert); strings.write_string(&builder,text[high:]); next:=strings.to_string(builder); defer delete(next,ctx.allocator)
    if next==text { node.cursor=low+len(insert); node.anchor=node.cursor; return false }
    text_history_push(ctx,node,&node.undo); text_history_clear(ctx,&node.redo)
    if !node_text_set(ctx,node,next) { return false }; node.cursor=low+len(insert); node.anchor=node.cursor
    delete(node.preedit,ctx.allocator); node.preedit=""
    node.text_dirty=true; if node.descriptor.kind!=.Numeric_Input { emit_text(ctx,node,false) }; return true
}
@(private="package")
text_hit :: proc(ctx:^Context,node:^Node,position:Vec2)->int {
    text:=node_text(ctx,node); font,size:=node_font(ctx,node); inset:=text_inner(ctx,node); wrap:=text_wrap(node,inset.width)
    return text_boundary(text,ctx.fonts.hit_test(ctx.fonts.state,font,text,size,wrap,position-Vec2{inset.x,inset.y}+node.text_offset))
}
@(private="package")
text_caret_visible :: proc(ctx:^Context,node:^Node) {
    text:=node_text(ctx,node); node.cursor=text_boundary(text,node.cursor); node.anchor=text_boundary(text,node.anchor)
    font,size:=node_font(ctx,node); inset:=text_inner(ctx,node); wrap:=text_wrap(node,inset.width)
    caret:=ctx.fonts.caret(ctx.fonts.state,font,text,size,wrap,node.cursor)
    if caret.x<node.text_offset.x { node.text_offset.x=caret.x }; if caret.x>node.text_offset.x+inset.width-2 { node.text_offset.x=max(0,caret.x-inset.width+2) }
    if caret.y<node.text_offset.y { node.text_offset.y=caret.y }; if caret.y+size+2>node.text_offset.y+inset.height { node.text_offset.y=max(0,caret.y+size+2-inset.height) }
    if node.descriptor.multiline { node.text_offset.x=0 }
}
@(private="package")
clipboard_write :: proc(ctx:^Context,text:string) { next:=strings.clone(text,ctx.allocator); delete(ctx.clipboard,ctx.allocator); ctx.clipboard=next; if ctx.clipboard_provider.write!=nil { ctx.clipboard_provider.write(ctx.clipboard_provider.state,next) } }
@(private="package")
text_key :: proc(ctx:^Context,node:^Node,event:Key_Down)->bool {
    text:=node_text(ctx,node); d:=node.descriptor; shift:=.Shift in event.modifiers; command:=.Control in event.modifiers || .Super in event.modifiers
    destination:=node.cursor; moving:=false
    #partial switch event.key {
    case .Z: if command { text_undo(ctx,node,shift); return true }; return false
    case .Y: if command { text_undo(ctx,node,true); return true }; return false
    case .A: if command { node.anchor=0; node.cursor=len(text); return true }; return false
    case .C,.X:
        if !command { return false }; clipboard_write(ctx,text[min(node.cursor,node.anchor):max(node.cursor,node.anchor)]); if event.key==.X { text_replace(ctx,node,"") }; return true
    case .V:
        if !command { return false }; pasted:=ctx.clipboard; if ctx.clipboard_provider.read!=nil { pasted=ctx.clipboard_provider.read(ctx.clipboard_provider.state) }; text_replace(ctx,node,pasted); return true
    case .Left:
        moving=true; if !shift && node.cursor!=node.anchor { destination=text_selection_edge(ctx,node,false) }
        else if command || .Alt in event.modifiers { destination=text_word(text,node.cursor,-1) } else { font,size:=node_font(ctx,node); destination=ctx.fonts.navigate(ctx.fonts.state,font,text,size,text_wrap(node,text_inner(ctx,node).width),node.cursor,-1) }
    case .Right:
        moving=true; if !shift && node.cursor!=node.anchor { destination=text_selection_edge(ctx,node,true) }
        else if command || .Alt in event.modifiers { destination=text_word(text,node.cursor,1) } else { font,size:=node_font(ctx,node); destination=ctx.fonts.navigate(ctx.fonts.state,font,text,size,text_wrap(node,text_inner(ctx,node).width),node.cursor,1) }
    case .Home: moving=true; destination=0; if !command { destination=node.cursor; for destination>0 && text[destination-1]!='\n' { destination-=1 } }
    case .End: moving=true; destination=len(text); if !command { destination=node.cursor; for destination<len(text) && text[destination]!='\n' { destination+=1 } }
    case .Up,.Down:
        if !text_multiline(node) { return true }; font,size:=node_font(ctx,node); inset:=text_inner(ctx,node); caret:=ctx.fonts.caret(ctx.fonts.state,font,text,size,text_wrap(node,inset.width),node.cursor)
        caret.y+=-size if event.key==.Up else size; destination=text_boundary(text,ctx.fonts.hit_test(ctx.fonts.state,font,text,size,text_wrap(node,inset.width),caret)); moving=true
    case .Backspace:
        if node.cursor==node.anchor { node.anchor=text_word(text,node.cursor,-1) if command else ctx.fonts.grapheme(ctx.fonts.state,text,node.cursor,-1) }; text_replace(ctx,node,""); return true
    case .Delete:
        if node.cursor==node.anchor { node.anchor=text_word(text,node.cursor,1) if command else ctx.fonts.grapheme(ctx.fonts.state,text,node.cursor,1) }; text_replace(ctx,node,""); return true
    case .Enter:
        if d.kind==.Numeric_Input { numeric_commit(ctx,node); return true }
        if node.preedit!="" { return true }
        if text_multiline(node) && !command { text_replace(ctx,node,"\n") } else { emit_text(ctx,node,true) }; return true
    case .Escape: if d.kind==.Numeric_Input { numeric_sync(ctx,node); ctx.focused={} }; delete(node.preedit,ctx.allocator); node.preedit=""; return true
    case: return false
    }
    if moving { node.cursor=text_boundary(text,destination); if !shift { node.anchor=node.cursor }; text_caret_visible(ctx,node); return true }; return false
}
/// Native clipboard callbacks keep OS transport in the application while editing remains retained UI state.
clipboard_provider :: proc(ctx:^Context,provider:Clipboard_Provider) { ctx.clipboard_provider=provider }

@(private="package")
text_editable :: proc(node:^Node)->bool { return node.descriptor.kind==.Text_Input || node.descriptor.kind==.Code_Editor || node.descriptor.kind==.Numeric_Input }
@(private="package")
text_history_clear :: proc(ctx:^Context,stack:^[dynamic]Text_Snapshot) { for item in stack^ { delete(item.text,ctx.allocator) }; clear(stack) }
@(private="package")
text_history_push :: proc(ctx:^Context,node:^Node,stack:^[dynamic]Text_Snapshot) {
    if len(stack^)>=256 { delete(stack^[0].text,ctx.allocator); ordered_remove(stack,0) }
    append(stack,Text_Snapshot{strings.clone(node_text(ctx,node),ctx.allocator),node.cursor,node.anchor})
}
@(private="package")
text_undo :: proc(ctx:^Context,node:^Node,redo:bool)->bool {
    source,destination:=&node.undo,&node.redo; if redo { source,destination=destination,source }
    if len(source^)==0 { return false }; text_history_push(ctx,node,destination)
    saved:=pop(source); defer delete(saved.text,ctx.allocator); node_text_set(ctx,node,saved.text); node.cursor=saved.cursor; node.anchor=saved.anchor
    delete(node.preedit,ctx.allocator); node.preedit=""; text_caret_visible(ctx,node)
    node.text_dirty=true; if node.descriptor.kind!=.Numeric_Input { emit_text(ctx,node,false) }; return true
}

@(private="package")
emit_text :: proc(ctx:^Context,node:^Node,submitted:bool) {
    text:=strings.clone(node_text(ctx,node),ctx.allocator); append(&ctx.action_snapshots,text)
    append(&ctx.actions,Text_Action{node=node.id,action=node.descriptor.action,payload=node.descriptor.payload,state=node.descriptor.state,submitted=submitted,text=text})
    if submitted { node.text_dirty=false }
}
/// Reads the latest live committed cell, or the immutable captured value when its subtree was removed.
/// Captured text remains borrowed until actions_clear or context_destroy, including after typed draining.
action_text :: proc(ctx:^Context,action:Text_Action)->string { if text,valid:=state_get(ctx,action.state,string); valid { return text }; return action.text }

@(private="package")
text_line_range :: proc(text:string,offset:int)->(int,int) {
    low:=text_boundary(text,offset); high:=low
    for low>0 && text[low-1]!='\n' { low-=1 }; for high<len(text) && text[high]!='\n' { high+=1 }; if high<len(text) { high+=1 }; return low,high
}
@(private="package")
text_selection_edge :: proc(ctx:^Context,node:^Node,right:bool)->int {
    low,high:=min(node.cursor,node.anchor),max(node.cursor,node.anchor); font,size:=node_font(ctx,node); text:=node_text(ctx,node); wrap:=text_wrap(node,text_inner(ctx,node).width)
    a:=ctx.fonts.caret(ctx.fonts.state,font,text,size,wrap,low); b:=ctx.fonts.caret(ctx.fonts.state,font,text,size,wrap,high)
    if a.y==b.y && a.x>b.x { return low if right else high }; return high if right else low
}
