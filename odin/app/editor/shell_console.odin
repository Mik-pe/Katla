//! The console retains real completed script logs and command diagnostics without owning scene history.
package editor_app
import app ".."
import editor "../../editor"
import ui "../../ui"
import "core:fmt"
import "core:strings"
import "core:mem"

Console_Level :: enum { Error,Warn,Info,Debug,Trace }
Console_Row :: struct { id:u64,level:Console_Level,message:string }
Console_State :: struct { rows:[dynamic]Console_Row,levels:[5]bool,search:string,bytes:int,next_id,last_action:u64,has_action:bool,allocator:mem.Allocator }
console_init :: proc(console:^Console_State,allocator:mem.Allocator) { console^={rows=make([dynamic]Console_Row,allocator),levels={true,true,true,true,true},allocator=allocator} }
console_clear :: proc(console:^Console_State) { for row in console.rows { delete(row.message,console.allocator) }; clear(&console.rows); console.bytes=0 }
console_destroy :: proc(console:^Console_State) { console_clear(console); delete(console.rows); delete(console.search,console.allocator); console^={} }
/// Records a changed runtime failure once, preserving the last accepted native owner.
shell_runtime_diagnostic :: proc(shell:^Shell,source:string,error:editor.Scene_Error) {
    value:=fmt.aprintf("%s: %v",source,error,allocator=shell.allocator); defer delete(value,shell.allocator)
    if value==shell.last_runtime_diagnostic { return }; delete(shell.last_runtime_diagnostic,shell.allocator); shell.last_runtime_diagnostic=strings.clone(value,shell.allocator); console_append(&shell.console,.Error,value)
}
/// Keeps the newest bounded diagnostics while preserving exact owned UTF8 messages.
console_append :: proc(console:^Console_State,level:Console_Level,value:string) {
    cut:=min(len(value),1<<20)
    for cut>0 && cut<len(value) && value[cut]&0xc0==0x80 { cut-=1 }
    console.next_id+=1; message:=strings.clone(value[:cut],console.allocator); append(&console.rows,Console_Row{console.next_id,level,message}); console.bytes+=len(message)
    for len(console.rows)>4096 || console.bytes>4<<20 { row:=console.rows[0]; console.bytes-=len(row.message); delete(row.message,console.allocator); ordered_remove(&console.rows,0) }
}
/// Drains the runtime even when the panel is inactive; clearing display never clears Undo commands.
shell_console_poll :: proc(shell:^Shell) {
    owner:=shell.state.owner
    logs:=app.script_logs_drain(owner); defer app.script_logs_destroy(logs,owner.world.allocator)
    for log in logs { value:=fmt.aprintf("Script %d · %s",log.entity,log.message,allocator=shell.allocator); console_append(&shell.console,.Warn if log.level==.Warn else .Info,value); delete(value,shell.allocator) }
    animation:=app.animation_feedback_drain(owner); defer app.animation_feedback_destroy(&animation,owner.world.allocator)
    for notice in animation.events { value:=fmt.aprintf("Animation %d · %v · %s · loop %d",u64(notice.entity),notice.event.kind,notice.event.clip,notice.event.loop_count,allocator=shell.allocator); console_append(&shell.console,.Info,value); delete(value,shell.allocator) }
    if animation.retired>0 { value:=fmt.aprintf("Retired %d older animation console events",animation.retired,allocator=shell.allocator); console_append(&shell.console,.Warn,value); delete(value,shell.allocator) }
    for action in owner.agent.session.actions {
        if shell.console.has_action && action.id<=shell.console.last_action { continue }
        value:=fmt.aprintf("#%d %v · %v",action.id,action.operation.kind,action.result.error,allocator=shell.allocator)
        console_append(&shell.console,.Info if action.result.error==.None else .Error,value); delete(value,shell.allocator); shell.console.last_action=max(shell.console.last_action,action.id); shell.console.has_action=true
    }
}
@(private="package")
shell_console_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    if Action(event.action)==.Console_Clear { console_clear(&shell.console); return true }
    if Action(event.action)==.Console_Level { if event.payload<5 { shell.console.levels[event.payload]=!shell.console.levels[event.payload] }; return true }; return false
}
@(private="package")
shell_console_text :: proc(shell:^Shell,event:ui.Text_Action,value:string)->bool {
    if Action(event.action)!=.Console_Search { return false }; next:=strings.clone(value,shell.allocator); delete(shell.console.search,shell.allocator); shell.console.search=next; return true
}
@(private="package")
shell_console :: proc(shell:^Shell)->ui.Descriptor {
    console:=&shell.console; controls:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(controls)
    for name,index in ([5]string{"Error","Warn","Info","Debug","Trace"}) { control:=button(name,.Console_Level); control.key=key(80,name); control.payload=u64(index); control.has_background=true; control.background=shell.ctx.theme.active if console.levels[index] else shell.ctx.theme.control; append(&controls,control) }
    width:=max(1,shell_panel_width(shell)-16)
    search_key:=key(80,"search"); search_control:=ui.Descriptor{key=search_key,kind=.Text_Input,placeholder="Filter logs…",state=ui.state(shell.ctx,search_key,0,console.search),action=u64(Action.Console_Search),layout={grow=1,height=ui.pixels(30),min_width=ui.pixels(1)}}
    rows:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(rows)
    search:=strings.to_lower(console.search,shell.allocator); defer delete(search,shell.allocator)
    for row in console.rows {
        if !console.levels[int(row.level)] { continue }
        if search!="" { lower:=strings.to_lower(row.message,shell.allocator); matches:=strings.contains(lower,search); delete(lower,shell.allocator); if !matches { continue } }
        label:=text(81,row.message); label.key=key(81,"log",row.id); label.layout.height={}; label.layout.no_shrink=true; label.has_foreground=true; label.foreground=ui.Color{1,.43,.35,1} if row.level==.Error else ui.Color{1,.75,.35,1} if row.level==.Warn else shell.ctx.theme.text; append(&rows,label)
    }
    if len(rows)==0 { append(&rows,text(80,"No log entries")) }
    toolbar:=ui.Descriptor{key=key(80,"toolbar"),kind=.Row,layout={width=ui.percent(1),gap={4,0},no_shrink=true}}
    if width<620 {
        columns:=clamp(int((width+4)/76),1,5);cell:=(width-4*f32(columns-1))/f32(columns)
        for &control in controls { control.layout.width=ui.pixels(cell) }
        levels:=ui.Descriptor{key=key(80,"levels"),kind=.Grid,layout={width=ui.percent(1),columns=u32(columns),cell_size={cell,30},gap={4,4},no_shrink=true},children=nodes(shell,controls[:])}
        clear_button:=button("Clear",.Console_Clear);clear_button.layout.width=ui.pixels(min(56,width*.35))
        toolbar.kind=.Column;toolbar.layout.gap={0,6};toolbar.children=nodes(shell,{levels,ui.Descriptor{key=key(80,"search-row"),kind=.Row,layout={width=ui.percent(1),gap={4,0},no_shrink=true},children=nodes(shell,{search_control,clear_button})}})
    } else { append(&controls,search_control,button("Clear",.Console_Clear));toolbar.children=nodes(shell,controls[:]) }
    content:=ui.Descriptor{key=key(80,"scroll"),kind=.Scroll_Area,layout={grow=1},children=nodes(shell,{ui.Descriptor{key=key(80,"logs"),kind=.Column,layout={width=ui.percent(1),gap={0,4}},children=nodes(shell,rows[:])}})}
    return {key=key(80,"panel"),kind=.Column,layout={padding={8,8,8,8},gap={0,6}},children=nodes(shell,{toolbar,content})}
}
