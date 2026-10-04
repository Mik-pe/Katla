package editor_app
import app ".."
import ui "../../ui"
import "core:strings"
import "core:testing"

@(test)
test_code_editor_local_text_undo_dirty_close_and_quit_cancel_preserve_source :: proc(t:^testing.T) {
    owner:app.Authoring; app.authoring_init(&owner); defer app.authoring_destroy(&owner)
    state:State; state_init(&state,&owner); defer state_destroy(&state)
    ctx:ui.Context; ui.context_init(&ctx,{measure=fixture_measure,caret=fixture_caret,hit_test=fixture_hit,navigate=fixture_navigate,grapheme=fixture_grapheme}); defer ui.context_destroy(&ctx)
    shell:Shell; shell_init(&shell,&state,&ctx,nil); defer shell_destroy(&shell)
    append(&shell.code.tabs,Code_Tab{path=strings.clone("scripts/edit.luau"),identity=strings.clone("/owned/edit.luau"),text=strings.clone("speed=1"),saved=strings.clone("speed=1")}); shell.code.has_active=true
    descriptor:=shell_code(&shell); _,result:=ui.frame(&ctx,descriptor,{}, {600,400}); testing.expect(t,result.error==.None); shell_actions(&shell)
    node:=ctx.nodes[key(120,"/owned/edit.luau")]; testing.expect(t,node!=nil); if node==nil { return }
    point:=ui.Vec2{node.bounds.x+80,node.bounds.y+12}
    shell_frame_destroy(&shell); descriptor=shell_code(&shell)
    _,result=ui.frame(&ctx,descriptor,{events={ui.Pointer_Down{position=point},ui.Pointer_Up{position=point},ui.Key_Down{key=.End},ui.Text_Commit{text=" -- å"}}},{600,400}); shell_actions(&shell)
    testing.expect(t,result.error==.None && strings.contains(shell.code.tabs[0].text,"å") && code_document_dirty(shell.code.tabs[0]))
    shell_frame_destroy(&shell); descriptor=shell_code(&shell)
    modifiers:ui.Modifiers={.Super} when ODIN_OS==.Darwin else {.Control}
    _,result=ui.frame(&ctx,descriptor,{events={ui.Key_Down{key=.Z,modifiers=modifiers}}},{600,400}); shell_actions(&shell)
    testing.expect(t,result.error==.None && shell.code.tabs[0].text=="speed=1" && len(owner.agent.session.actions)==0)
    code_document_edit(&shell.code,"speed=2"); shell_request_close(&shell)
    testing.expect(t,shell.code_quit_requested && shell.code.dialog==.Unsaved && len(shell.code.tabs)==1)
    shell_code_click(&shell,{action=u64(Action.Code_Cancel)})
    testing.expect(t,!shell.code_quit_requested && shell.code.dialog==.None && shell.code.tabs[0].text=="speed=2")
    shell_code_click(&shell,{action=u64(Action.Code_Close)}); testing.expect(t,shell.code.dialog==.Unsaved && len(shell.code.tabs)==1)
    shell_code_click(&shell,{action=u64(Action.Code_Discard)}); testing.expect(t,len(shell.code.tabs)==0)
}

@(test)
test_luau_syntax_utf8_and_long_comment_delimiters :: proc(t:^testing.T) {
    source:=string("local å = 'str' --[=[\nfunction ignored()\n]=]\nreturn 12")
    runs:=code_syntax(source,context.allocator); defer delete(runs)
    testing.expect_value(t,len(runs),5)
    expected:=[5]string{"local","'str'","--[=[\nfunction ignored()\n]=]","return","12"}
    for run,index in runs { testing.expect_value(t,source[run.start:run.end],expected[index]); if index>0 { testing.expect(t,run.start>=runs[index-1].end) } }
}
