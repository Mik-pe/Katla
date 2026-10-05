#+build darwin, linux
#+feature dynamic-literals
//! Owner-private Unix host/proxy fixtures exercise real process boundaries.
package unix_fixtures
import socket "../../odin/agent/socket"
import "../wire"
import "core:thread"
import "core:time"
import "core:strings"
import "base:runtime"
import "core:mem/virtual"

Fixture :: struct { listener:socket.Listener, worker:^thread.Thread, mode,scenario,thread_id,receipt:string, methods:[dynamic]string, hold_second:bool, active:string, questions:int }
start :: proc(fixture:^Fixture,path,mode,scenario:string) {
    listener,error:=socket.listen(path); wire.require(error==.None,"Cannot bind private fixture")
    fixture.listener=listener; fixture.mode=mode; fixture.scenario=scenario; fixture.thread_id="fixture-existing-thread"
    fixture.worker=thread.create(serve); fixture.worker.data=fixture; thread.start(fixture.worker)
}
stop :: proc(fixture:^Fixture) { thread.join(fixture.worker); thread.destroy(fixture.worker); socket.listener_destroy(&fixture.listener); for method in fixture.methods { delete(method) }; delete(fixture.methods) }
write_raw :: proc(channel:^socket.Channel,data:[]byte)->bool {
    offset:=0; started:=time.tick_now()
    for offset<len(data) {
        ready,error:=socket.wait(channel,true,100); if error!=.None || time.tick_since(started)>20*time.Second { return false }; if !ready { continue }
        n,write_error:=socket.write(channel,data[offset:]); if write_error!=.None { return false }; offset+=n
    }; return true
}
send :: proc(channel:^socket.Channel,value:any)->bool { encoded:=strings.concatenate({wire.encode(value),"\n"}); return write_raw(channel,transmute([]byte)encoded) }
line :: proc(channel:^socket.Channel)->(string,bool) {
    buffer:=make([dynamic]byte); defer delete(buffer); started:=time.tick_now()
    for len(buffer)<32<<20 && time.tick_since(started)<20*time.Second {
        ready,error:=socket.wait(channel,false,100); if error!=.None { return "",false }; if !ready { continue }
        byte:[1]u8; n,read_error:=socket.read(channel,byte[:]); if read_error!=.None { return "",false }; if n==0 { continue }
        if byte[0]=='\n' { return strings.clone(string(buffer[:])),true }; append(&buffer,byte[0])
    }; return "",false
}
serve :: proc(worker:^thread.Thread) {
    context=runtime.default_context(); heap:=context.allocator; fixture:=cast(^Fixture)worker.data
    started:=time.tick_now(); channel:socket.Channel
    for { accepted,error,ready:=socket.accept(&fixture.listener); wire.require(error==.None && time.tick_since(started)<25*time.Second,"Fixture accept deadline"); if ready { channel=accepted; break }; time.sleep(time.Millisecond) }
    defer socket.close(&channel)
    arena:virtual.Arena; wire.require(virtual.arena_init_growing(&arena)==nil,"Cannot allocate fixture arena"); defer virtual.arena_destroy(&arena)
    switch fixture.mode {
    case "echo":
        buffer:[8192]byte
        for { ready,error:=socket.wait(&channel,false,100); if error==.Closed { break }; wire.require(error==.None,"Echo read failed"); if !ready { continue }; n,read_error:=socket.read(&channel,buffer[:]); if read_error==.Closed { break }; wire.require(read_error==.None,"Echo read failed"); if n>0 && !write_raw(&channel,buffer[:n]) { break } }
    case "reply": _=write_raw(&channel,transmute([]byte)string(`{"jsonrpc":"2.0","id":"closed","result":{}}`)); _=write_raw(&channel,{'\n'})
    case "broken","slow": _=write_raw(&channel,transmute([]byte)strings.repeat("x",8<<20))
    case "host":
        context.allocator=virtual.arena_allocator(&arena)
        for {
            raw,ok:=line(&channel); if !ok { break }; request:=wire.parse(raw); method:=wire.s(request,"method")
            { context.allocator=heap; append(&fixture.methods,strings.clone(method)) }
            if fixture.scenario=="eof" { return }
            if fixture.scenario=="malformed" { _=write_raw(&channel,transmute([]byte)string("{\"id\":18446744073709551617,\"result\":{}}\n")); continue }
            if fixture.scenario=="cancel_wait" { _,_=line(&channel); return }
            if method=="initialized" { continue }
            params:=wire.get(request,"params"); result:wire.Object
            switch method {
            case "initialize": wire.require(wire.s(wire.get(params,"clientInfo"),"name")=="katla_odin_editor","Host initialize client mismatch"); result={"userAgent"="local-api-fixture"}
            case "thread/loaded/list":
                if fixture.scenario=="missing_thread" { result={"data"=wire.Array{"other-thread"},"nextCursor"=nil} }
                else if wire.is_null(wire.get(params,"cursor")) { result={"data"=wire.Array{"other-thread"},"nextCursor"="page2"} }
                else { result={"data"=wire.Array{fixture.thread_id},"nextCursor"=nil} }
            case "thread/resume": wire.require(wire.s(params,"threadId")==fixture.thread_id,"Host resumed wrong conversation"); result={"thread"=wire.Object{"id"="wrong-thread" if fixture.scenario=="wrong_resume" else fixture.thread_id,"name"="Existing fixture conversation"}}
            case "thread/read":
                wire.require(wire.s(params,"threadId")==fixture.thread_id && wire.b(params,"includeTurns"),"Host read contract mismatch")
                turns:=wire.Array{}; if fixture.active!="" { append(&turns,wire.Object{"id"="existing-active-turn","status"="inProgress"}) }
                result={"thread"=wire.Object{"id"=fixture.thread_id,"status"=wire.Object{"type"="active" if fixture.active!="" else "idle"},"turns"=turns}}
            case "turn/start","turn/steer":
                wire.require(wire.s(params,"threadId")==fixture.thread_id,"Host changed conversation")
                inputs:=wire.a(params,"input"); wire.require(len(inputs)==2 && strings.contains(wire.s(inputs[0],"text"),"Geometry candidates are not proof") && strings.has_prefix(wire.s(inputs[1],"url"),"data:image/png;base64,"),"Host viewport text/image pairing missing")
                if method=="turn/start" {
                    _=send(&channel,wire.Object{"method"="item/agentMessage/delta","params"=wire.Object{"threadId"="foreign-thread","turnId"="wrong","itemId"="wrong","delta"="Must be ignored"}})
                    _=send(&channel,wire.Object{"method"="item/agentMessage/delta","params"=wire.Object{"threadId"=fixture.thread_id,"turnId"="new-turn","itemId"="agent-item","delta"="Viewport fixture reply"}})
                    _=send(&channel,wire.Object{"id"="approval-owned-by-host","method"="item/commandExecution/requestApproval","params"=wire.Object{}})
                    _=send(&channel,wire.Object{"method"="turn/completed","params"=wire.Object{"threadId"=fixture.thread_id,"turn"=wire.Object{"id"="new-turn","status"="completed"}}})
                    result={"turn"=wire.Object{"id"="new-turn"}}; fixture.active="existing-active-turn"
                } else { wire.require(wire.s(params,"expectedTurnId")=="existing-active-turn","Steer must target existing turn"); result={"turnId"="existing-active-turn"} }
            case "turn/interrupt":
                wire.require(wire.s(params,"threadId")==fixture.thread_id && wire.s(params,"turnId")=="existing-active-turn","Interrupt changed identity")
                _=send(&channel,wire.Object{"id"=wire.get(request,"id"),"result"=wire.Object{}})
                _=send(&channel,wire.Object{"method"="turn/completed","params"=wire.Object{"threadId"=fixture.thread_id,"turn"=wire.Object{"id"="existing-active-turn","status"="interrupted"}}}); return
            case: wire.require(false,strings.concatenate({"Unexpected host method: ",method}))
            }
            if !send(&channel,wire.Object{"id"=wire.get(request,"id"),"result"=result}) { break }
        }
    case: wire.require(false,"Unknown Unix fixture mode")
    }
}
