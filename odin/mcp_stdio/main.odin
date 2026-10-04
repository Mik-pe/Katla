//! Optional headless MCP consumer: stdin frames cross to the existing application scene owner.
package main

import app "../app"
import editor "../editor"
import mcp "../agent/mcp"
import "core:os"
import "core:io"
import "core:thread"
import "core:sync"
import "core:time"
import "core:mem"
import "core:fmt"

Input :: struct { reader:mcp.Line_Reader, frames:[dynamic]mcp.Frame, mutex:sync.Mutex, changed:sync.Cond, done:bool, allocator:mem.Allocator }
read_input :: proc(th:^thread.Thread) {
    input:=cast(^Input)th.data
    for {
        frame:=mcp.line_reader_next(&input.reader)
        sync.mutex_lock(&input.mutex)
        for len(input.frames)>=4 { sync.cond_wait(&input.changed,&input.mutex) }
        if frame.error==.EOF { input.done=true; sync.mutex_unlock(&input.mutex); break }
        append(&input.frames,frame)
        sync.mutex_unlock(&input.mutex)
    }
}
take_input :: proc(input:^Input)->(mcp.Frame,bool,bool) {
    sync.mutex_lock(&input.mutex); defer sync.mutex_unlock(&input.mutex)
    if len(input.frames)==0 { return {},false,input.done }
    frame:=input.frames[0]; ordered_remove(&input.frames,0); sync.cond_signal(&input.changed)
    return frame,true,false
}
write_output :: proc(output:string)->bool {
    if output=="" { return true }
    remaining:=transmute([]byte)output
    for len(remaining)>0 {
        count,err:=os.write(os.stdout,remaining)
        if err!=nil || count==0 { return false }
        remaining=remaining[count:]
    }
    count,err:=os.write(os.stdout,transmute([]byte)string("\n"))
    return err==nil && count==1
}
main :: proc() {
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    defer { context.allocator=backing; assert(len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker) }
    context.allocator=mem.tracking_allocator(&tracker)
    scene:app.Authoring; app.authoring_init(&scene,agent_capacity=64); defer app.authoring_destroy(&scene)
    assert(app.authoring_services_init(&scene)==.None)
    project_path,resource_path:=".","resources"
    if len(os.args)==3 { project_path=os.args[1]; resource_path=os.args[2] }
    else if len(os.args)!=1 { fmt.eprintln("usage: katla-odin-mcp [project-root resource-root]"); os.exit(2) }
    if app.asset_resources_init(&scene,project_path,resource_path)!=.None { fmt.eprintln("could not initialize confined asset roots"); os.exit(2) }
    server:mcp.Server; mcp.server_init(&server,&scene.agent); defer mcp.server_destroy(&server)
    input:=Input{allocator=context.allocator,frames=make([dynamic]mcp.Frame)}
    defer { for &frame in input.frames { mcp.frame_destroy(&frame) }; delete(input.frames) }
    mcp.line_reader_init(&input.reader,io.Stream{procedure=stdin_read},input.allocator)
    worker:=thread.create(read_input); worker.data=&input; thread.start(worker)
    started:=time.tick_now()
    closed:=false
    for {
        activity:=false
        for _ in 0..<10 {
            frame,ready,done:=take_input(&input)
            if done && !closed { closed=true; mcp.server_finish(&server); editor.agent_finish(&scene.agent) }
            if !ready { break }; defer mcp.frame_destroy(&frame)
            activity=true
            output:=""
            switch frame.error {
            case .None: output=mcp.server_receive(&server,string(frame.data),time.tick_since(started))
            case .Oversized: output=`{"jsonrpc":"2.0","error":{"code":-32700,"message":"Input frame exceeds one MiB"}}`
            case .Unterminated: output=`{"jsonrpc":"2.0","error":{"code":-32700,"message":"Input ended before newline"}}`
            case .Read_Failed:
                fmt.eprintln("MCP input failed")
                os.exit(1)
            case .EOF: unreachable()
            }
            if !write_output(output) { fmt.eprintln("MCP output failed"); os.exit(1) }
            if frame.error==.None { delete(output,server.allocator) }
        }
        if app.authoring_tick(&scene)>0 { activity=true }
        for {
            output:=mcp.server_poll(&server,time.tick_since(started))
            if output=="" { break }; defer delete(output,server.allocator)
            if !write_output(output) { fmt.eprintln("MCP output failed"); os.exit(1) }
            activity=true
        }
        if closed && scene.agent.session.finished && len(server.pending)==0 { break }
        if !activity { time.sleep(time.Millisecond) }
    }
    thread.join(worker); thread.destroy(worker)
}
stdin_read :: proc(data:rawptr,mode:io.Stream_Mode,p:[]byte,offset:i64,whence:io.Seek_From)->(i64,io.Error) {
    if mode!=.Read { return 0,.Unsupported }
    n,err:=os.read(os.stdin,p)
    if err==nil { return i64(n),nil }
    if stream_error,ok:=err.(io.Error); ok { return i64(n),stream_error }
    return i64(n),.Unknown
}
