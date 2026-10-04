package editor_app
import host "../../agent/host"
import "core:testing"
import "core:strings"
import "core:fmt"

@(test)
test_host_panel_terminal_order_and_bound :: proc(t:^testing.T) {
    panel:Host_Panel; host_panel_init(&panel,nil,nil); defer host_panel_destroy(&panel)
    host_panel_event(&panel,host.Event{kind=.Finished,turn_id="old",status="completed"})
    host_panel_event(&panel,host.Event{kind=.Accepted,turn_id="old"})
    testing.expect(t,panel.turn=="" && panel.status=="completed")
    host_panel_event(&panel,host.Event{kind=.Accepted,turn_id="new"})
    host_panel_event(&panel,host.Event{kind=.Finished,turn_id="old",status="completed"})
    testing.expect(t,panel.turn=="new" && panel.status=="Working")
    chunk:=strings.repeat("x",host.MAX_TEXT_BYTES); defer delete(chunk)
    for i in 0..<7 { id:=fmt.aprintf("%d",i); defer delete(id); host_panel_event(&panel,host.Event{kind=.Text,turn_id=id,item_id=id,text=chunk}) }
    testing.expect(t,panel.retained_bytes<=4*1024*1024)
    testing.expect(t,len(panel.messages)==4)
    panel.connected=true; panel.capturing=true; host_text(&panel,&panel.turn,"new"); host_text(&panel,&panel.pending_prompt,"unsent prompt")
    host_panel_disconnect(&panel)
    testing.expect(t,!panel.connected && !panel.capturing && panel.turn=="" && panel.pending_prompt=="")
}
