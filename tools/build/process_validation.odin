#+feature dynamic-literals
//! Actual HTTP/SSE clients and stdio processes exercise the application boundaries.
package katla_build
import "../providers"
import "../wire"
import "core:fmt"
import "core:strings"
import "core:os"
import "core:time"
import "core:sync"

json_output :: proc(command:[]string)->wire.Value {
    child:=wire.start(command); defer wire.abort(&child); text:=wire.line(&child,30*time.Second)
    require(!strings.contains(text,"local-transport-test"),"Secret appeared in process output"); result:=wire.parse(text); wire.finish(&child); return result
}
provider_config :: proc(port:int,scenario,api:string,timeout:=3000,maximum:=20)->string {
    path:=join(output,"llm.toml")
    write(path,fmt.aprintf("provider=\"open_ai_compatible\"\napi=\"%s\"\napi_key=\"local-transport-test\"\nbase_url=\"http://127.0.0.1:%d/%s/v1\"\nmodel=\"explicit-test-model\"\nrate_limit_min_interval_ms=0\nrate_limit_max_calls_per_minute=%d\ntimeout_ms=%d\n",api,port,scenario,maximum,timeout))
    require(os.chmod(path,{.Read_User,.Write_User})==nil,"Cannot protect fixture credentials"); return path
}
LLM_Case :: struct { scenario,api,error,mode:string,entities,actions:int }
validation_http :: proc() {
    server:providers.Server; providers.start(&server); defer providers.stop(&server)
    llm:=validation_build("odin/examples/llm_authoring","llm-authoring")
    assistant:=validation_build("odin/examples/assistant_authoring","assistant-authoring")
    cases:=[]LLM_Case{{"responses","responses","None","",1,2},{"chat","chat_completions","None","",1,2},{"unknown","responses","None","",0,0},{"truncated","responses","Truncated","",0,0},{"malformed","chat_completions","Protocol","",0,0},{"http429","responses","Rate_Limited","",0,0},{"redirect","responses","HTTP","",0,0},{"oversize","responses","Limit","",0,0},{"timeout","responses","Timeout","",0,0},{"cancel","responses","Cancelled","cancel",0,0},{"paused","responses","Cancelled","paused",0,0},{"known_blocked","responses","None","readonly",0,0},{"rate","responses","Rate_Limited","",1,1},{"cancel_after_tool","responses","Cancelled","cancel_after_tool",1,1},{"backpressure","responses","Limit","backpressure",0,0},{"parallel","responses","None","parallel",2,4},{"partial_cancel","responses","Cancelled","cancel",0,0}}
    for executable in ([]string{llm,assistant}) {
        for test in cases {
            if executable==llm && test.scenario=="partial_cancel" { continue }
            if executable==assistant && (test.scenario=="malformed" || test.scenario=="known_blocked" || test.scenario=="rate" || test.scenario=="backpressure" || test.scenario=="parallel") { continue }
            path:=provider_config(server.port,test.scenario,test.api,150 if test.scenario=="timeout" else 3000,1 if test.scenario=="rate" else 20)
            command:=make([dynamic]string); append(&command,executable,path); if test.mode!="" { append(&command,test.mode) }
            cpu_leak_environment(); started:=time.tick_now(); result:=json_output(command[:])
            require(wire.s(result,"error")==test.error && int(wire.number(wire.get(result,"entities")))==test.entities && int(wire.number(wire.get(result,"actions")))==test.actions,cat("HTTP journey failed: ",test.scenario," ",wire.encode(result)))
            if test.mode!="" { require(time.tick_since(started)<4*time.Second,"Cancellation shutdown deadline") }
            if executable==assistant { require(wire.s(result,"state")==("Completed" if test.error=="None" else "Failed"),"Assistant completion state mismatch") }
            if test.scenario=="responses" || test.scenario=="chat" { require(wire.s(result,"text")=="Fox created. 🦊","Assistant text mismatch"); if executable==llm { require(wire.s(result,"streamed")==wire.s(result,"text"),"Stream differs from final text") } }
            if test.scenario=="parallel" { require(wire.number(wire.get(result,"jobs"))==2 && len(wire.a(result,"texts"))==2,"Concurrent jobs missing") }
            if test.scenario=="partial_cancel" { require(wire.b(result,"progress") && wire.s(result,"text")=="Working on the scene…","Partial cancellation discarded progress") }
            fmt.println("PASS",test.scenario,"actual",executable)
        }
    }
    scene:=validation_build("odin/examples/assistant_scene","assistant-scene")
    for api in ([]string{"responses","chat_completions"}) {
        project:=join(output,cat("scene-",api)); remove(project); mkdir(join(project,"resources"))
        result:=json_output({scene,provider_config(server.port,"scene",api),project,join(project,"resources")})
        require(wire.s(result,"text")=="Scene restored with fresh IDs." && wire.number(wire.get(result,"ticks"))==7 && wire.number(wire.get(result,"entities"))==1 && wire.number(wire.get(result,"edits"))==0 && wire.b(result,"loaded") && wire.b(result,"failed_load_preserved") && wire.b(result,"reset"),"Assistant atomic scene journey failed")
        paths:=files(project,".katla"); require(len(paths)==1 && strings.contains(string(read(paths[0])),`name:"fox"`) && strings.contains(string(read(paths[0])),"metallic:0"),"Scene file publication mismatch")
    }
    sync.mutex_lock(&server.mutex); require(server.calls["forbidden"]==0 && server.calls["scene"]==16,"Redirect or scene request count mismatch"); sync.mutex_unlock(&server.mutex)
    validation_tls({llm,assistant})
}
validation_tls :: proc(executables:[]string) {
    certificate:=join(output,"local-cert.pem"); key:=join(output,"local-key.pem")
    run({"openssl","req","-x509","-newkey","rsa:2048","-nodes","-days","1","-subj","/CN=localhost","-keyout",key,"-out",certificate})
    reserve:providers.Server; providers.start(&reserve); port:=reserve.port; providers.stop(&reserve)
    process,error:=os.process_start({command={"openssl","s_server","-accept",fmt.aprintf("127.0.0.1:%d",port),"-cert",certificate,"-key",key,"-quiet","-www"},stderr=os.stderr})
    require(error==nil,"Cannot start untrusted TLS fixture"); defer { _=os.process_kill(process); _,_=os.process_wait(process,os.TIMEOUT_INFINITE) }
    time.sleep(100*time.Millisecond)
    path:=provider_config(port,"untrusted_tls","responses"); write(path,replace(string(read(path)),"http://","https://"))
    for executable in executables { result:=json_output({executable,path}); require(wire.s(result,"error")=="Network" && wire.number(wire.get(result,"entities"))==0 && wire.number(wire.get(result,"actions"))==0,"Untrusted TLS certificate accepted") }
}
validation_mcp :: proc() {
    binary:=join(filepath_dir(validation_manifest.executable),cat("katla-mcp-stdio",executable_suffix()))
    client:=wire.client({binary}); defer wire.abort(&client.child)
    discovery:=wire.rpc(&client,"server/discover",{}); require(wire.text(wire.a(discovery,"supportedVersions")[0])=="2026-07-28","MCP discovery version mismatch")
    listed:=wire.a(wire.rpc(&client,"tools/list",{}),"tools"); expected:=wire.array(wire.parse(string(read(join(root,"odin/agent/tools.json")))))
    require(len(listed)==len(expected),"MCP tool schemas missing")
    for item,i in listed { require(wire.s(item,"name")==wire.s(expected[i],"name"),"MCP tool inventory differs from canonical schemas") }
    args:=wire.Object{"name"="Study / stol 🪑","position"=wire.Array{i64(1),i64(2),i64(3)}}
    entity:=wire.text(wire.a(wire.content(&client,"spawn_entity",args),"entity_ids")[0])
    require(wire.s(wire.data(&client,"get_component_attributes",{"entity_id"=entity,"component"="SceneName"}),"name")=="Study / stol 🪑","Unicode scene name mismatch")
    _=wire.content(&client,"material",{"action"="set","entity_ids"=wire.Array{entity},"preset"="oak","roughness"=f64(0.27)})
    require(abs(wire.number(wire.get(wire.get(wire.data(&client,"material",{"action"="inspect","entity_id"=entity}),"values"),"roughness"))-0.27)<1e-6,"Actual material edit missing")
    for name in ([]string{"unknown_shape",""}) { require(wire.b(wire.tool(&client,"spawn_entity",{"shape"=name},true),"isError"),"Invalid shape accepted") }
    _=wire.content(&client,"destroy_entity",{"entity_id"=entity}); require(len(wire.a(wire.content(&client,"query_entities",{}),"entity_ids"))==0,"Scene deletion failed")
    wire.finish(&client.child)
    validation_mcp_framing(binary)
}
filepath_dir :: proc(value:Maybe(string))->string { path,ok:=value.?; require(ok,"Missing MCP build executable"); return os.dir(path) }
validation_mcp_framing :: proc(binary:string) {
    child:=wire.start({binary}); defer wire.abort(&child)
    meta:=`"_meta":{"io.modelcontextprotocol/protocolVersion":"2026-07-28","io.modelcontextprotocol/clientCapabilities":{}}`
    fragmented:=cat(`{"jsonrpc":"2.0","id":9007199254740993,"method":"tools/call","params":{"name":"spawn_entity","arguments":{"name":"stol 🪑"},`,meta,"}}\n")
    for i:=0;i<len(fragmented);i+=7 { wire.send_raw(&child,transmute([]byte)fragmented[i:min(i+7,len(fragmented))]) }
    response:=wire.reply(&child); exact_id,exact_ok:=wire.get(response,"id").(i64); require(exact_ok && exact_id==9007199254740993,"Integer RPC ID rounded")
    for i:=0;i<24;i+=1 { wire.send_raw(&child,transmute([]byte)cat(`{"jsonrpc":"2.0","id":`,fmt.aprintf("%d",i),`,"method":"ping","params":{`,meta,"}}\n")) }
    ids:=make(map[i64]bool); for i:=0;i<24;i+=1 { message:=wire.reply(&child); id,ok:=wire.get(message,"id").(i64); require(ok && !ids[id] && id>=0 && id<24,cat("Pipelined ID missing or duplicate: ",wire.encode(message))); ids[id]=true }
    for malformed in ([]string{"{} {}\n",cat(strings.repeat("x",(1<<20)+1),"\n")}) { wire.send_raw(&child,transmute([]byte)malformed); require(wire.number(wire.get(wire.get(wire.reply(&child),"error"),"code"))== -32700,"Malformed input recovery failed") }
    wire.send_raw(&child,transmute([]byte)cat(`{"jsonrpc":"2.0","id":"after-errors","method":"ping","params":{`,meta,"}}\n")); require(wire.s(wire.reply(&child),"id")=="after-errors","MCP did not recover")
    wire.close_input(&child); wire.finish(&child)
    eof:=wire.start({binary}); defer wire.abort(&eof); for i:=0;i<40;i+=1 { wire.send_raw(&eof,transmute([]byte)cat(`{"jsonrpc":"2.0","id":"eof-`,fmt.aprintf("%d",i),`","method":"tools/call","params":{"name":"spawn_entity","arguments":{},`,meta,"}}\n")) }; wire.close_input(&eof)
    for i:=0;i<40;i+=1 { _=wire.reply(&eof) }; wire.finish(&eof)
    broken:=wire.start({binary}); os.close(broken.output); broken.output=nil
    wire.send_raw(&broken,transmute([]byte)cat(`{"jsonrpc":"2.0","id":"broken","method":"ping","params":{`,meta,"}}\n")); wire.finish(&broken,-1)
}
validation_processes :: proc() { cpu_leak_environment(); validation_http(); validation_mcp(); validation_socket(); validation_host(); validation_proxy() }
