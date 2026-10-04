//! Co-Creator connects to one explicitly selected existing conversation and waits for committed viewport capture.
package editor_app
import host "../../agent/host"
import ui "../../ui"
import "core:mem"
import "core:strings"
import "core:fmt"

Host_Message :: struct { turn,item,text:string }
Host_Panel :: struct {
    bridge:host.Bridge,allocator:mem.Allocator,messages:[dynamic]Host_Message,
    status,name,turn,pending_prompt:string,capturing:bool,connected:bool,
    capture_state:rawptr,capture_request:proc(rawptr)->bool,
    completed_turns:[dynamic]string,retained_bytes:int,
}
/// Keep the panel stationary until its bridge worker has been joined.
host_panel_init :: proc(panel:^Host_Panel,state:rawptr,capture_request:proc(rawptr)->bool,allocator:=context.allocator) { panel^={allocator=allocator,messages=make([dynamic]Host_Message,allocator),completed_turns=make([dynamic]string,allocator),capture_state=state,capture_request=capture_request}; host_text(panel,&panel.status,"Disconnected") }
@(private="package")
host_text :: proc(panel:^Host_Panel,target:^string,text:string) { next:=strings.clone(text,panel.allocator); delete(target^,panel.allocator); target^=next }
/// Explicit connection never creates a thread or changes the host model or approval policy.
host_panel_connect :: proc(panel:^Host_Panel,config:host.Config)->host.Error {
    host_panel_disconnect(panel)
    error:=host.bridge_connect(&panel.bridge,config,panel.allocator)
    status:=fmt.aprintf("Connection: %v",error,allocator=panel.allocator); defer delete(status,panel.allocator); host_text(panel,&panel.status,"Connecting…" if error==.None else status)
    return error
}
/// Drops only the owned connection and unsent capture; accepted external turns keep running.
host_panel_disconnect :: proc(panel:^Host_Panel) {
    host.bridge_destroy(&panel.bridge); panel.connected=false; panel.capturing=false
    host_text(panel,&panel.turn,""); host_text(panel,&panel.pending_prompt,""); host_text(panel,&panel.status,"Disconnected")
}
/// Captures prompt ownership before requesting the next native accepted color/ID snapshot.
host_panel_request :: proc(panel:^Host_Panel,prompt:string)->bool {
    if panel.capturing || !host.bridge_connected(&panel.bridge) || strings.trim_space(prompt)=="" || len(prompt)>host.MAX_TEXT_BYTES || panel.capture_request==nil { return false }
    host_text(panel,&panel.pending_prompt,prompt); panel.capturing=true
    if !panel.capture_request(panel.capture_state) { panel.capturing=false; host_text(panel,&panel.pending_prompt,""); host_text(panel,&panel.status,"Viewport capture could not be queued"); return false }
    host_text(panel,&panel.status,"Capturing viewport…"); return true
}
/// Only the completed paired snapshot owner may admit a question with its metadata and PNG.
host_panel_committed :: proc(panel:^Host_Panel,metadata,png:string)->host.Error {
    if !panel.capturing { return .Invalid_Config }
    error:=host.bridge_submit(&panel.bridge,panel.pending_prompt,metadata,png)
    if error==.None {
        append(&panel.messages,Host_Message{text=strings.clone(panel.pending_prompt,panel.allocator)}); panel.retained_bytes+=len(panel.pending_prompt); host_panel_trim(panel)
        host_text(panel,&panel.status,"Question sent")
    } else { value:=fmt.aprintf("Question failed: %v",error,allocator=panel.allocator); defer delete(value,panel.allocator); host_text(panel,&panel.status,value) }
    panel.capturing=false; host_text(panel,&panel.pending_prompt,""); return error
}
host_panel_capture_failed :: proc(panel:^Host_Panel) { panel.capturing=false; host_text(panel,&panel.pending_prompt,""); host_text(panel,&panel.status,"Viewport capture failed; question was not sent") }
/// Owner polling retains exact turn/item identity across streamed deltas and terminal notifications.
host_panel_poll :: proc(panel:^Host_Panel) {
    for {
        event,present:=host.bridge_poll(&panel.bridge); if !present { break }; defer host.event_destroy(&event)
        host_panel_event(panel,event)
    }
}
/// Applies wire-ordered events without resurrecting a turn completed before its RPC acceptance.
host_panel_event :: proc(panel:^Host_Panel,event:host.Event) {
        switch event.kind {
        case .Connected: panel.connected=true; host_text(panel,&panel.name,event.name); host_text(panel,&panel.status,"Connected")
        case .Accepted:
            terminal:=false; for turn in panel.completed_turns { if turn==event.turn_id { terminal=true; break } }
            if !terminal { host_text(panel,&panel.turn,event.turn_id); host_text(panel,&panel.status,"Working") }
        case .Text:
            index:=-1; for item,i in panel.messages { if item.turn==event.turn_id && item.item==event.item_id { index=i; break } }
            if index<0 { append(&panel.messages,Host_Message{strings.clone(event.turn_id,panel.allocator),strings.clone(event.item_id,panel.allocator),""}); index=len(panel.messages)-1 }
            item:=&panel.messages[index]
            if len(item.text)+len(event.text)<=host.MAX_TEXT_BYTES { next:=strings.concatenate({item.text,event.text},panel.allocator); delete(item.text,panel.allocator); item.text=next; panel.retained_bytes+=len(event.text) }
        case .Finished:
            known:=false; for turn in panel.completed_turns { if turn==event.turn_id { known=true; break } }
            if !known { append(&panel.completed_turns,strings.clone(event.turn_id,panel.allocator)) }
            for len(panel.completed_turns)>256 { delete(panel.completed_turns[0],panel.allocator); ordered_remove(&panel.completed_turns,0) }
            if panel.turn==event.turn_id || panel.turn=="" { host_text(panel,&panel.status,event.status); host_text(panel,&panel.turn,"") }
        case .Attention: host_text(panel,&panel.status,"Attention required in the connected conversation")
        case .Error: host_text(panel,&panel.status,event.text)
        case .Disconnected: panel.connected=false; host_text(panel,&panel.turn,""); host_text(panel,&panel.pending_prompt,""); host_text(panel,&panel.status,"Disconnected"); panel.capturing=false
        }
        host_panel_trim(panel)
}
@(private="package")
host_panel_trim :: proc(panel:^Host_Panel) {
    for len(panel.messages)>256 || panel.retained_bytes>4*1024*1024 {
        index:=0
        for item,i in panel.messages { if item.turn!=panel.turn || panel.turn=="" { index=i; break } }
        item:=panel.messages[index]; panel.retained_bytes-=len(item.text); delete(item.turn,panel.allocator); delete(item.item,panel.allocator); delete(item.text,panel.allocator); ordered_remove(&panel.messages,index)
    }
}
host_panel_destroy :: proc(panel:^Host_Panel) {
    host.bridge_destroy(&panel.bridge)
    for item in panel.messages { delete(item.turn,panel.allocator); delete(item.item,panel.allocator); delete(item.text,panel.allocator) }; delete(panel.messages)
    for turn in panel.completed_turns { delete(turn,panel.allocator) }; delete(panel.completed_turns)
    for text in ([4]string{panel.status,panel.name,panel.turn,panel.pending_prompt}) { delete(text,panel.allocator) }; panel^={}
}
@(private="package")
shell_host :: proc(shell:^Shell)->ui.Descriptor {
    children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children)
    panel:=shell.host
    if panel==nil { append(&children,text(90,"External conversation service is unavailable")) }
    else {
        host_panel_poll(panel)
        append(&children,text(90,panel.name if panel.name!="" else "Existing conversation"),text(91,panel.status))
        controls:=ui.Descriptor{key=key(90,"connection"),kind=.Row,layout={gap={6,0}},children=nodes(shell,{button("Connect",.Host_Connect),button("Disconnect",.Host_Disconnect,!panel.connected),button("Interrupt turn",.Host_Interrupt,!panel.connected || panel.turn=="")})}; append(&children,controls)
        messages:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(messages)
        for item,i in panel.messages { line:=text(92,item.text); line.key=key(92,"message",u64(i)); line.layout.height={}; append(&messages,line) }
        append(&children,ui.Descriptor{key=key(90,"history"),kind=.Scroll_Area,layout={grow=1,width=ui.percent(1)},children=nodes(shell,{ui.Descriptor{key=key(90,"messages"),kind=.Column,layout={gap={0,10},width=ui.percent(1)},children=nodes(shell,messages[:])}})})
        prompt_key:=key(90,"prompt"); append(&children,ui.Descriptor{key=prompt_key,kind=.Text_Input,multiline=true,placeholder="Ask about this viewport",state=ui.state(shell.ctx,prompt_key,0,""),action=u64(Action.Host_Prompt),layout={height=ui.pixels(100),width=ui.percent(1)}})
        append(&children,button("Send viewport",.Host_Send,!panel.connected || panel.capturing || panel.capture_request==nil))
    }
    return {key=key(90,"panel"),kind=.Column,layout={padding={12,12,12,12},gap={0,8}},children=nodes(shell,children[:])}
}
@(private="package")
shell_host_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    panel:=shell.host
    #partial switch Action(event.action) {
    case .Host_Connect: if panel!=nil && shell.preferences!=nil { host_panel_connect(panel,{socket=shell.preferences.external_chat.socket,thread_id=shell.preferences.external_chat.thread_id}) }; return true
    case .Host_Disconnect: if panel!=nil { host_panel_disconnect(panel) }; return true
    case .Host_Interrupt: if panel!=nil && panel.connected && panel.turn!="" { host.bridge_cancel(&panel.bridge,panel.turn) }; return true
    case .Host_Send:
        if panel!=nil { state:=ui.state(shell.ctx,key(90,"prompt"),0,""); prompt,valid:=ui.state_get(shell.ctx,state,string); if valid && host_panel_request(panel,prompt) { ui.state_set(shell.ctx,state,string("")) } }; return true
    }; return false
}
