//! Code editors share the retained IME engine, local edit history and exact full-text shaping.
package ui
import "core:fmt"
import "core:strings"

@(private="package")
text_multiline :: proc(node:^Node)->bool { return node.descriptor.multiline || node.descriptor.kind==.Code_Editor }
@(private="package")
text_wrap :: proc(node:^Node,width:f32)->f32 { if node.descriptor.kind!=.Code_Editor && node.descriptor.multiline { return width }; return 0 }
@(private="package")
text_inner :: proc(ctx:^Context,node:^Node)->Rect {
    rect:=rect_inset(node.bounds,ctx.theme.padding)
    if node.descriptor.kind==.Code_Editor { gutter:=code_gutter_width(ctx,node); rect.x+=gutter; rect.width=max(0,rect.width-gutter) }; return rect
}
@(private="package")
code_gutter_width :: proc(ctx:^Context,node:^Node)->f32 {
    lines:=1; for r in node_text(ctx,node) { if r=='\n' { lines+=1 } }; text:=fmt.aprintf("%d",lines); defer delete(text)
    font,size:=node_font(ctx,node); return ctx.fonts.measure(ctx.fonts.state,font,text,size,0).x+ctx.theme.padding*3
}
@(private="package")
paint_code_gutter :: proc(ctx:^Context,node:^Node,text:string) {
    width:=code_gutter_width(ctx,node); rect:=Rect{node.bounds.x,node.bounds.y,width+ctx.theme.padding,node.bounds.height}; clip:=rect_intersection(node.clip,rect)
    paint_rect(ctx,rect,clip,ctx.theme.panel)
    font,size:=node_font(ctx,node); line:=1; start:=0
    for at:=0;at<=len(text);at+=1 {
        if at<len(text) && text[at]!='\n' { continue }
        position:=ctx.fonts.caret(ctx.fonts.state,font,text,size,0,start); label:=fmt.aprintf("%d",line)
        measured:=ctx.fonts.measure(ctx.fonts.state,font,label,size,0)
        paint_text(ctx,node,label,{rect.x+width-measured.x-ctx.theme.padding,node.bounds.y+ctx.theme.padding+position.y-node.text_offset.y},ctx.theme.muted,clip)
        delete(label); line+=1; start=at+1
    }
}
@(private="package")
code_indent :: proc(ctx:^Context,node:^Node,remove:bool) {
    text:=node_text(ctx,node); low:=min(node.cursor,node.anchor); high:=max(node.cursor,node.anchor)
    for low>0 && text[low-1]!='\n' { low-=1 }
    if high>low && high<len(text) && text[high-1]=='\n' { high-=1 }
    for high<len(text) && text[high]!='\n' { high+=1 }
    builder:strings.Builder; strings.builder_init(&builder,ctx.allocator); at:=low
    for at<=high {
        end:=at; for end<high && text[end]!='\n' { end+=1 }
        if remove { if at<end && text[at]=='\t' { at+=1 } else { count:=0; for at<end && text[at]==' ' && count<4 { at+=1; count+=1 } } }
        else { strings.write_string(&builder,"\t") }
        strings.write_string(&builder,text[at:end]); if end<high { strings.write_string(&builder,"\n") }; at=end+1
    }
    insert:=strings.to_string(builder); defer delete(insert,ctx.allocator); node.anchor=low; node.cursor=high; text_replace(ctx,node,insert)
}
/// Replacing an editor document explicitly clears local undo and uncommitted composition.
text_document :: proc(ctx:^Context,id:Node_Id,text:string)->bool {
    node:=node_get(ctx,id); if node==nil || !text_editable(node) { return false }
    if !state_set(ctx,node.descriptor.state,text) { return false }; text_history_clear(ctx,&node.undo); text_history_clear(ctx,&node.redo)
    node.cursor=0; node.anchor=0; node.text_offset={}; delete(node.preedit,ctx.allocator); node.preedit=""; return true
}
