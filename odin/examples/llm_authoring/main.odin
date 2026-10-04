//! Native provider producers and an independent scene owner exercise tool/history rounds.
package main

import llm "../../agent/llm"
import agent "../../agent"
import app "../../app"
import editor "../../editor"
import "core:os"
import "core:mem"
import "core:time"
import "core:fmt"
import "core:encoding/json"

SCHEMAS :: agent.TOOLS_JSON
main :: proc() {
    if len(os.args)<2 || len(os.args)>3 { fmt.eprintln("usage: llm_authoring <llm.toml> [cancel|paused|readonly|parallel|backpressure|cancel_after_tool]"); os.exit(2) }
    backing:=context.allocator
    tracker:mem.Tracking_Allocator; mem.tracking_allocator_init(&tracker,backing)
    context.allocator=mem.tracking_allocator(&tracker)
    config,config_error:=llm.config_load(os.args[1]); if config_error!=.None { fmt.eprintln("invalid provider configuration"); os.exit(2) }
    transport:llm.Runtime; assert(llm.runtime_init(&transport,config)==.None)
    mode:=""; if len(os.args)==3 { mode=os.args[2] }
    count:=2 if mode=="parallel" else 1
    scene:app.Authoring; app.authoring_init(&scene,agent_capacity=count)
    assert(app.authoring_services_init(&scene)==.None)
    schemas:=SCHEMAS
    if mode=="readonly" { schemas=`[{"name":"query_entities","description":"Inspect CPU entities.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}]` }
    conversations:[2]llm.Conversation; jobs:[2]llm.Job; streamed:[2][dynamic]byte
    responses:[2]llm.Response; errors:[2]llm.Error; finished:[2]bool
    capacity:=1 if mode=="backpressure" else 64
    for i in 0..<count {
        assert(llm.conversation_init(&conversations[i],&transport,&config,&scene.agent,schemas,"Use only the supplied scene tools and actual tool results.")==.None)
        assert(llm.job_start(&jobs[i],&conversations[i],"Create one named fox, inspect the scene, then describe the result.",capacity)==.None)
        streamed[i]=make([dynamic]byte,context.allocator)
    }
    start:=time.tick_now(); completed:=0
    for completed<count {
        for i in 0..<count {
            if !finished[i] {
                responses[i],errors[i],finished[i]=llm.job_poll(&jobs[i])
                if finished[i] { completed+=1 }
            }
            if mode!="backpressure" {
                for {
                    chunk,has_chunk:=llm.job_poll_text(&jobs[i]); if !has_chunk { break }
                    append(&streamed[i],..transmute([]byte)chunk); delete(chunk,jobs[i].allocator)
                }
            }
        }
        if mode!="paused" { app.authoring_tick(&scene) }
        if (mode=="cancel" || mode=="paused") && time.tick_since(start)>100*time.Millisecond { llm.job_cancel(&jobs[0]) }
        if mode=="cancel_after_tool" && len(scene.agent.session.actions)>0 { llm.job_cancel(&jobs[0]) }
        time.sleep(time.Millisecond)
    }
    histories:=0; texts:[2]string
    for i in 0..<count {
        for { chunk,has_chunk:=llm.job_poll_text(&jobs[i]); if !has_chunk { break }; append(&streamed[i],..transmute([]byte)chunk); delete(chunk,jobs[i].allocator) }
        llm.job_destroy(&jobs[i]); texts[i]=string(responses[i].text[:]); histories+=len(conversations[i].history)
    }
    app.authoring_tick(&scene)
    for i in 0..<count { assert(llm.conversation_reap(&conversations[i])) }
    result:=struct { error,text,streamed:string,entities,actions,messages,jobs:int,texts:[2]string }{fmt.tprintf("%s",errors[0]),texts[0],string(streamed[0][:]),scene.world.live_count,len(scene.agent.session.actions),histories,count,texts}
    for i in 0..<count { assert(errors[i]==errors[0]) }
    bytes,err:=json.marshal(result); assert(err==nil); fmt.println(string(bytes)); delete(bytes)
    assert(editor.agent_undo_all(&scene.agent.session,&scene.world,&scene.registry)==.None && scene.world.live_count==0)
    for i in 0..<count { assert(llm.conversation_destroy(&conversations[i])==0); delete(streamed[i]); llm.response_destroy(&responses[i]) }
    app.authoring_destroy(&scene); llm.runtime_destroy(&transport); llm.config_destroy(&config)
    context.allocator=backing
    assert(len(tracker.allocation_map)==0); mem.tracking_allocator_destroy(&tracker)
}
