//! Document decisions are real backend operations; modal views own no independent save state.
package editor_app

import document "../document"
import ui "../../ui"

@(private="package")
shell_document :: proc(shell:^Shell,size:ui.Vec2)->ui.Descriptor {
    state:=shell.document
    title,message:="",""
    actions:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(actions)
    children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children)
    switch state.dialog {
    case .Open,.Save_As:
        title="Open Scene" if state.dialog==.Open else "Save Scene As"
        message="Enter a scene path ending in .katla."
        path_key:=key(40,"path")
        field:=ui.Descriptor{key=path_key,kind=.Text_Input,placeholder="scene.katla",state=ui.state(shell.ctx,path_key,0,state.path),action=u64(Action.Document_Path),layout={height=ui.pixels(32),width=ui.percent(1)}}
        append(&children,field); append(&actions,button("Open" if state.dialog==.Open else "Save",.Document_Submit),button("Cancel",.Document_Cancel))
    case .Unsaved:
        title="Save changes?"; message="The current scene has unsaved changes."
        append(&actions,button("Save changes",.Document_Save),button("Discard changes",.Document_Discard),button("Cancel",.Document_Cancel))
    case .Overwrite:
        title="Replace existing scene?"; message=state.path
        append(&actions,button("Replace file",.Document_Overwrite),button("Cancel",.Document_Cancel))
    case .Error:
        title="Scene operation failed"
        message="Stop Play mode and check that the scene path and its referenced assets are valid. The current scene is preserved."
        append(&actions,button("Close",.Document_Cancel))
    case .None:
    }
    content:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(content)
    append(&content,text(40,title),text(41,message)); append(&content,..children[:])
    for &action in actions { action.key=key(42,action.text,u64(action.action)) }
    append(&content,ui.Descriptor{key=key(40,"actions"),kind=.Row,layout={gap={8,0},wrap=true},children=nodes(shell,actions[:])})
    width:=min(500,max(0,size[0]-32)); height:f32=240
    column:=ui.Descriptor{key=key(40,"content"),kind=.Column,layout={padding={16,16,16,16},gap={0,14},width=ui.percent(1),height=ui.percent(1)},children=nodes(shell,content[:])}
    return {key=key(40,"dialog"),kind=.Modal,action=u64(Action.Document_Cancel),has_fixed_bounds=true,fixed_bounds={max(0,(size[0]-width)/2),max(0,(size[1]-height)/2),width,height},children=nodes(shell,{column})}
}

/// The OS close button requests the same unsaved decision as File → Quit.
shell_request_close :: proc(shell:^Shell) {
    if shell.code.dialog!=.None { return }
    ready,error:=code_documents_request_leave(&shell.code)
    if error!=.None { shell.state.last_error=error; return }
    shell.code_quit_requested=!ready
    if ready && shell.document!=nil { document.request(shell.document,{kind=.Quit}) }
}
