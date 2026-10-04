//! Code widgets edit owned source tabs; publication and VM reload remain explicit backend actions.
package editor_app
import ui "../../ui"
import document "../document"
import "core:fmt"

@(private="package")
shell_code :: proc(shell:^Shell)->ui.Descriptor {
    documents:=&shell.code; children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children)
    if len(documents.tabs)==0 { append(&children,text(120,"Open a Luau script from Assets to edit its source.")) }
    else {
        options:=make([]string,len(documents.tabs),shell.allocator); append(&shell.option_lists,options)
        for tab,index in documents.tabs { name:=fmt.aprintf("%s%s",tab.path," *" if code_document_dirty(tab) else "",allocator=shell.allocator); append(&shell.texts,name); options[index]=name }
        tab_key:=key(120,"tabs"); tabs:=ui.Descriptor{key=tab_key,kind=.Combo,state=ui.state(shell.ctx,tab_key,0,f32(documents.active)),options=options,action=u64(Action.Code_Select),layout={grow=1,height=ui.pixels(30)}}
        if shell.ctx.captured.key!=tab_key { ui.state_set(shell.ctx,tabs.state,f32(documents.active)) }
        append(&children,ui.Descriptor{key=key(120,"toolbar"),kind=.Row,layout={gap={6,0},no_shrink=true},children=nodes(shell,{tabs,button("Save source",.Code_Save,!documents.has_active),button("Close tab",.Code_Close,!documents.has_active)})})
        if documents.has_active {
            tab:=documents.tabs[documents.active]; edit_key:=key(120,tab.identity); hook:=ui.state(shell.ctx,edit_key,0,tab.text)
            current,valid:=ui.state_get(shell.ctx,hook,string); if valid && current!=tab.text { ui.state_set(shell.ctx,hook,tab.text) }
            syntax:=code_syntax(tab.text,shell.allocator); append(&shell.syntax_lists,syntax)
            append(&children,ui.Descriptor{key=edit_key,kind=.Code_Editor,state=hook,syntax=syntax,action=u64(Action.Code_Text),layout={grow=1,width=ui.percent(1)},multiline=true})
        }
        if documents.message!="" { append(&children,text(121,documents.message)) }
        if documents.last_error!=.None { value:=fmt.aprintf("Source operation failed: %v",documents.last_error,allocator=shell.allocator); append(&shell.texts,value); label:=text(121,value); label.has_foreground=true; label.foreground={1,.43,.35,1}; append(&children,label) }
    }
    return {key=key(120,"panel"),kind=.Column,layout={padding={8,8,8,8},gap={0,8}},children=nodes(shell,children[:])}
}
@(private="package")
shell_code_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    #partial switch Action(event.action) {
    case .Code_Save:
        result:=code_document_save(&shell.code); message(shell,"Source saved and attached instances reloaded" if result.reloaded else "Source saved" if result.published && result.error==.None else "Source save failed; check the code diagnostics")
    case .Code_Close: if shell.code.has_active { code_document_close(&shell.code,shell.code.active) }
    case .Code_Confirm_Save: code_document_respond(&shell.code,.Save)
    case .Code_Discard: code_document_respond(&shell.code,.Discard)
    case .Code_Cancel: code_document_respond(&shell.code,.Cancel); shell.code_quit_requested=false
    case: return false
    }
    if shell.code_quit_requested && shell.code.leave_ready { shell.code_quit_requested=false; if shell.document!=nil { document.request(shell.document,{kind=.Quit}) } }
    return true
}
@(private="package")
shell_code_choice :: proc(shell:^Shell,event:ui.Selection_Action)->bool { if Action(event.action)!=.Code_Select { return false }; code_document_select(&shell.code,event.index); return true }
@(private="package")
shell_code_text :: proc(shell:^Shell,event:ui.Text_Action,value:string)->bool {
    if Action(event.action)!=.Code_Text { return false }
    if shell.code.has_active && shell.code.tabs[shell.code.active].text!=value { code_document_edit(&shell.code,value) }; return true
}
@(private="package")
shell_code_shortcut :: proc(shell:^Shell,event:ui.Key_Action)->bool {
    node:=shell.ctx.nodes[shell.ctx.focused.key]; if node==nil || node.descriptor.kind!=.Code_Editor { return false }
    primary:=.Super in event.modifiers when ODIN_OS==.Darwin else .Control in event.modifiers
    if primary && event.key==.S { shell_code_click(shell,{action=u64(Action.Code_Save)}); return true }
    if primary && event.key==.W { shell_code_click(shell,{action=u64(Action.Code_Close)}); return true }
    return !primary
}
@(private="package")
shell_code_dialog :: proc(shell:^Shell,size:ui.Vec2)->ui.Descriptor {
    content:=ui.Descriptor{key=key(122,"content"),kind=.Column,layout={padding={16,16,16,16},gap={0,14}},children=nodes(shell,{text(122,"Save source changes?"),text(122,"The edited source has not been published to disk."),ui.Descriptor{key=key(122,"actions"),kind=.Row,layout={gap={8,0}},children=nodes(shell,{button("Save source",.Code_Confirm_Save),button("Discard source",.Code_Discard),button("Cancel",.Code_Cancel)})}})}
    width:=min(520,max(0,size[0]-32)); height:f32=190
    return {key=key(122,"dialog"),kind=.Modal,action=u64(Action.Code_Cancel),has_fixed_bounds=true,fixed_bounds={(size[0]-width)/2,(size[1]-height)/2,width,height},children=nodes(shell,{content})}
}
