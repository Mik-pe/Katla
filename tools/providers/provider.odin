#+feature dynamic-literals
//! Deterministic loopback HTTP/SSE fixtures for real application process acceptance.
package providers
import "../wire"
import "core:net"
import "core:thread"
import "core:sync"
import "core:time"
import "core:strings"
import "core:fmt"
import "core:strconv"
import "base:runtime"
import "core:mem/virtual"

Server :: struct { socket:net.TCP_Socket, port:int, worker:^thread.Thread, mutex:sync.Mutex, stopping:bool, calls:map[string]int, failures:int, clients:[dynamic]^thread.Thread }
Connection :: struct { server:^Server, socket:net.TCP_Socket }
start :: proc(server:^Server,port:=0) {
    socket,error:=net.listen_tcp({address=net.IP4_Loopback,port=port}); wire.require(error==nil,"Cannot bind loopback provider")
    server.socket=socket; endpoint,info_error:=net.bound_endpoint(socket); wire.require(info_error==nil,"Cannot inspect provider port"); server.port=endpoint.port
    server.calls=make(map[string]int); server.worker=thread.create(accept_loop); server.worker.data=server; thread.start(server.worker)
}
stop :: proc(server:^Server,receipt:string="") {
    sync.mutex_lock(&server.mutex); server.stopping=true; sync.mutex_unlock(&server.mutex)
    wake,error:=net.dial_tcp(net.IP4_Loopback,int(server.port)); if error==nil { net.close(wake) }
    thread.join(server.worker); thread.destroy(server.worker); net.close(server.socket)
    for worker in server.clients { thread.join(worker); thread.destroy(worker) }; if receipt!="" { wire.write(receipt,wire.encode(struct { scope:string,calls:map[string]int,failures:int }{"Deterministic HTTP/SSE transport and actual editor; no paid model",server.calls,server.failures})) }; delete(server.clients); for key in server.calls { delete(key) }; delete(server.calls)
    wire.require(server.failures==0,"Fixture provider rejected a request")
}
accept_loop :: proc(worker:^thread.Thread) {
    context=runtime.default_context(); server:=cast(^Server)worker.data
    for {
        client,_,error:=net.accept_tcp(server.socket); if error!=nil { break }
        sync.mutex_lock(&server.mutex); stopping:=server.stopping; sync.mutex_unlock(&server.mutex)
        if stopping { net.close(client); break }
        wire.require(net.set_option(client,.Receive_Timeout,5*time.Second)==nil && net.set_option(client,.Send_Timeout,5*time.Second)==nil,"Cannot bound fixture socket I/O")
        connection:=new(Connection); connection^={server,client}; client_worker:=thread.create(serve); client_worker.data=connection; append(&server.clients,client_worker); thread.start(client_worker)
    }
}
put :: proc(socket:net.TCP_Socket,text:string)->bool {
    bytes:=transmute([]byte)text
    for len(bytes)>0 { n,error:=net.send(socket,bytes); if error!=nil || n<=0 { return false }; bytes=bytes[n:] }; return true
}
emit :: proc(socket:net.TCP_Socket,value:wire.Object)->bool {
    message:=strings.concatenate({"data: ",wire.encode(value),"\r\n\r\n"})
    for i:=0; i<len(message); i+=7 { if !put(socket,message[i:min(i+7,len(message))]) { return false } }; return true
}
response :: proc(socket:net.TCP_Socket,chat:bool,text,name:string,args:wire.Object,id:string,truncated:=false) {
    if chat {
        if name!="" {
            encoded:=wire.encode(args); midpoint:=len(encoded)/2
            if !emit(socket,{"choices"=wire.Array{wire.Object{"index"=i64(0),"delta"=wire.Object{"tool_calls"=wire.Array{wire.Object{"index"=i64(0),"id"=id,"type"="function","function"=wire.Object{"name"=name,"arguments"=encoded[:midpoint]}}}},"finish_reason"=nil}}}) { return }
            if !emit(socket,{"choices"=wire.Array{wire.Object{"index"=i64(0),"delta"=wire.Object{"tool_calls"=wire.Array{wire.Object{"index"=i64(0),"function"=wire.Object{"arguments"=encoded[midpoint:]}}}},"finish_reason"="tool_calls"}}}) { return }
        } else { if !emit(socket,{"choices"=wire.Array{wire.Object{"index"=i64(0),"delta"=wire.Object{"content"=text},"finish_reason"="stop"}}}) { return } }
        if !truncated { _=put(socket,"data: [DONE]\n\n") }
    } else {
        if !emit(socket,{"type"="response.created","response"=wire.Object{"id"="provider-response","status"="in_progress"}}) { return }
        result:=make(wire.Array)
        if text!="" {
            if !emit(socket,{"type"="response.output_text.delta","delta"=text}) { return }
            append(&result,wire.Object{"type"="message","role"="assistant","content"=wire.Array{wire.Object{"type"="output_text","text"=text}}})
        }
        if name!="" {
            encoded:=wire.encode(args)
            if !emit(socket,{"type"="response.function_call_arguments.delta","delta"=encoded[:1],"item_id"="item"}) { return }
            if !emit(socket,{"type"="response.function_call_arguments.delta","delta"=encoded[1:],"item_id"="item"}) { return }
            append(&result,wire.Object{"type"="function_call","call_id"=id,"name"=name,"arguments"=encoded})
        }
        if !truncated { _=emit(socket,{"type"="response.completed","response"=wire.Object{"status"="completed","output"=result}}) }
    }
}
results :: proc(request:wire.Value,chat:bool)->wire.Array {
    history:=wire.a(request,"messages" if chat else "input"); output:=make(wire.Array)
    for item in history { if string_is(wire.get(item,"type"),"function_call_output") || string_is(wire.get(item,"role"),"tool") { append(&output,item) } }; return output
}
string_is :: proc(value:wire.Value,expected:string)->bool { text,ok:=value.(string); return ok && text==expected }
result_payload :: proc(item:wire.Value,chat:bool)->wire.Value { return wire.parse(wire.s(item,"content" if chat else "output")) }
selected :: proc(request:wire.Value,chat:bool)->string {
    found:string
    for item in wire.a(request,"messages" if chat else "input") {
        if !string_is(wire.get(item,"role"),"system") { continue }
        text,ok:=wire.get(item,"content").(string); if !ok { continue }
        offset:=strings.index(text,"Selected entity_id="); if offset<0 { continue }; tail:=text[offset+len("Selected entity_id="):]
        end:=0; for end<len(tail) && tail[end]>='0' && tail[end]<='9' { end+=1 }; found=tail[:end]
    }; wire.require(found!="","Provider requires selected entity context"); return found
}
contains_entity :: proc(entities:wire.Array,entity:string)->bool { for value in entities { if wire.text(value)==entity { return true } }; return false }
selected_initial :: proc(request:wire.Value,chat:bool)->string {
    for item in wire.a(request,"messages" if chat else "input") { if string_is(wire.get(item,"role"),"system") { first:=wire.Object{"messages"=wire.Array{item},"input"=wire.Array{item}}; return selected(first,chat) } }; wire.require(false,"Provider requires initial selected context"); return ""
}
scene :: proc(socket:net.TCP_Socket,request:wire.Value,chat:bool,editor:bool) {
    history:=results(request,chat); payloads:=make(wire.Array)
    for item,index in history {
        prefix:="native-scene-" if editor else "scene-"; wire.require(wire.s(item,"tool_call_id" if chat else "call_id")==fmt.aprintf("%s%d",prefix,index),"Scene tool ID mismatch")
        payload:=result_payload(item,chat); expected:="None"
        if index==3 { expected="Invalid_Operation" }; if editor && index==5 { expected="Entity_Not_Found" }
        wire.require(wire.s(payload,"error")==expected,"Unexpected scene tool outcome"); append(&payloads,payload)
    }
    original:string; if len(payloads)>0 { original=wire.text(wire.a(payloads[0],"entities")[0]) }; if editor { original=selected_initial(request,chat) }
    fresh:=original; if len(payloads)>(4 if editor else 5) { fresh=wire.text(wire.a(payloads[4 if editor else 5],"entities")[0]); wire.require(fresh!=original,"Load must replace runtime IDs") }
    names:=[]string{"spawn_entity","save_scene","material","load_scene","query_entities","load_scene","query_entities"}
    args:=[]wire.Object{{"name"="fox"},{"path"="scene.katla"},{"action"="set","entity_ids"=wire.Array{original},"metallic"=f64(0.8)},{"path"="missing.katla"},{},{"path"="scene.katla"},{}}
    final:="Scene restored with fresh IDs."
    if editor {
        if len(payloads)>0 { entities:=wire.a(payloads[0],"entities"); wire.require(len(entities)==2 && contains_entity(entities,original),"Initial selected entity must belong to the two-entity scene") }
        if len(payloads)>1 { data:=wire.get(payloads[1],"data"); wire.require(wire.b(data,"published") && wire.number(wire.get(data,"entity_count"))==2,"Native scene must save two entities") }
        if len(payloads)>6 {
            entities:=wire.a(payloads[6],"entities"); wire.require(len(entities)==2,"Restored native scene must have two entities")
            for entity in entities { wire.require(!contains_entity(wire.a(payloads[0],"entities"),wire.text(entity)),"Load must replace all runtime IDs") }; fresh=wire.text(entities[0])
        }
        if len(payloads)>7 { key:=wire.number(wire.get(wire.get(payloads[7],"data"),"value")); wire.require(key==1 || key==2,"Restored SceneKey must identify an original entity"); if key==2 { fresh=wire.text(wire.a(payloads[6],"entities")[1]) } }
        if len(payloads)>8 { data:=wire.get(payloads[8],"data"); wire.require(wire.b(data,"published") && wire.number(wire.get(data,"entity_count"))==1,"Prefab must publish one entity") }
        inserted:string; if len(payloads)>9 { inserted=wire.s(wire.get(payloads[9],"data"),"root_entity"); wire.require(contains_entity(wire.a(payloads[9],"entities"),inserted) && !contains_entity(wire.a(payloads[6],"entities"),inserted),"Prefab must instantiate a fresh root") }
        if len(payloads)>10 { data:=wire.get(payloads[10],"data"); wire.require(wire.number(wire.get(data,"removed"))==1 && wire.s(data,"root_entity")==inserted,"Prefab must remove exactly its inserted root") }
        if len(payloads)>11 { wire.require(wire.encode(wire.a(payloads[11],"entities"))==wire.encode(wire.a(payloads[6],"entities")),"Removing prefab must restore scene entities") }
        names={"query_entities","save_scene","material","load_scene","load_scene","material","query_entities","get_component_attributes","prefab","prefab","prefab","query_entities"}
        args={{},{"path"="native.katla"},{"action"="set","entity_ids"=wire.Array{original},"base_color"=wire.Array{f64(0.15),f64(0.85),f64(0.3),i64(1)},"metallic"=f64(0.8),"roughness"=f64(0.2)},{"path"="missing.katla"},{"path"="native.katla"},{"action"="set","entity_ids"=wire.Array{original},"metallic"=i64(1)},{},{"entity_id"=fresh,"component"="SceneKey"},{"action"="capture","path"="resources/native.katprefab","root_entity"=fresh},{"action"="instantiate","path"="resources/native.katprefab","name"="Captured sphere","position"=wire.Array{i64(0),f64(0.8),i64(0)},"scale"=wire.Array{f64(0.65),f64(0.65),f64(0.65)}},{"action"="remove","root_entity"=inserted},{}}
        turns:=0; for item in wire.a(request,"messages" if chat else "input") { if string_is(wire.get(item,"role"),"user") { turns+=1 } }
        if turns>1 {
            entity:=selected(request,chat); wire.require(entity==fresh,"Next Send must refresh selection")
            new_names:=make([dynamic]string); append(&new_names,..names); append(&new_names,"material","get_component_attributes"); names=new_names[:]
            new_args:=make([dynamic]wire.Object); append(&new_args,..args); append(&new_args,wire.Object{"action"="set","entity_ids"=wire.Array{entity},"base_color"=wire.Array{f64(0.9),f64(0.65),f64(0.12),i64(1)},"metallic"=f64(0.8),"roughness"=f64(0.25)},wire.Object{"entity_id"=entity,"component"="SurfaceMaterial"}); args=new_args[:]
            if len(payloads)>13 { wire.require(abs(wire.number(wire.get(wire.get(payloads[13],"data"),"metallic"))-0.8)<0.001,"Second Send must edit current material") }
            final="Current selection edited. Earlier conversation preserved."
        } else { final="Scene restored. Stale ID rejected. Prefab captured, instantiated and removed; Undo is available." }
    }
    count:=len(history)
    if count<len(names) { response(socket,chat,"",names[count],args[count],fmt.aprintf("%s%d","native-scene-" if editor else "scene-",count)) }
    else { response(socket,chat,final,"",{},"") }
}
serve :: proc(worker:^thread.Thread) {
    context=runtime.default_context(); heap:=context.allocator; connection:=cast(^Connection)worker.data; defer { context.allocator=heap; free(connection) }
    socket:=connection.socket; defer net.close(socket)
    arena:virtual.Arena; wire.require(virtual.arena_init_growing(&arena)==nil,"Cannot allocate fixture arena"); defer virtual.arena_destroy(&arena); context.allocator=virtual.arena_allocator(&arena)
    buffer:=make([dynamic]byte); size:=0; header_end:=-1; started:=time.tick_now()
    for {
        if len(buffer)>8<<20 || time.tick_since(started)>5*time.Second { return }
        bytes:[4096]byte; n,error:=net.recv(socket,bytes[:]); if error!=nil || n<=0 { return }; append(&buffer,..bytes[:n])
        if header_end<0 {
            header_end=strings.index(string(buffer[:]),"\r\n\r\n"); if header_end<0 { continue }
            headers:=strings.split(string(buffer[:header_end]),"\r\n")
            for header in headers[1:] { if strings.has_prefix(strings.to_lower(header),"content-length:") { value,ok:=strconv.parse_int(strings.trim_space(header[len("content-length:"):])); if !ok || value<=0 || value>8<<20 { return }; size=value } }
        }
        if len(buffer)>=header_end+4+size { break }
    }
    header:=string(buffer[:header_end]); request_line:=strings.fields(strings.split(header,"\r\n")[0]); if len(request_line)!=3 || request_line[0]!="POST" { return }; path:=request_line[1]; segments:=strings.split(strings.trim(path,"/"),"/"); if len(segments)<3 { return }
    scenario:=segments[0]; chat:=strings.has_suffix(path,"chat/completions"); request:=wire.parse(string(buffer[header_end+4:header_end+4+size]))
    wire.require(strings.contains(header,"Bearer local-transport-test") && wire.s(request,"model")=="explicit-test-model" && wire.b(request,"stream") && len(wire.a(request,"tools"))>0,"Fixture request contract mismatch")
    sync.mutex_lock(&connection.server.mutex); count:=connection.server.calls[scenario]; if count==0 { context.allocator=heap; key:=strings.clone(scenario); connection.server.calls[key]=1; context.allocator=virtual.arena_allocator(&arena) } else { connection.server.calls[scenario]=count+1 }; sync.mutex_unlock(&connection.server.mutex)
    if scenario=="http429" { _=put(socket,"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"); return }
    if scenario=="redirect" { _=put(socket,"HTTP/1.1 307 Temporary Redirect\r\nLocation: /forbidden/v1/responses\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"); return }
    if !put(socket,"HTTP/1.1 200 OK\r\ncOnTeNt-TyPe: text/event-stream; charset=utf-8\r\nConnection: close\r\n\r\n") { return }
    history:=results(request,chat); step:=1+len(history)
    switch scenario {
    case "scene","editor_scene": scene(socket,request,chat,scenario=="editor_scene"); return
    case "timeout","cancel","paused": time.sleep(1500*time.Millisecond); return
    case "cancel_after_tool": if step>1 { time.sleep(1500*time.Millisecond); return }
    case "partial_cancel": _=emit(socket,{"type"="response.output_text.delta","delta"="Working on the scene…"}); time.sleep(1500*time.Millisecond); return
    case "oversize": _=put(socket,strings.concatenate({"data: ",strings.repeat("x",256*1024+1)})); return
    case "truncated": response(socket,chat,"partial","",{},"",true); return
    case "malformed":
        _=emit(socket,{"choices"=wire.Array{wire.Object{"index"=i64(0),"delta"=wire.Object{"tool_calls"=wire.Array{wire.Object{"index"=i64(0),"id"="bad","function"=wire.Object{"name"="spawn_entity","arguments"="[1]"}}}},"finish_reason"="tool_calls"}}}); _=put(socket,"data: [DONE]\n\n"); return
    case "backpressure":
        _=emit(socket,{"type"="response.output_text.delta","delta"="first"}); _=emit(socket,{"type"="response.output_text.delta","delta"="second"}); _=emit(socket,{"type"="response.completed","response"=wire.Object{"status"="completed","output"=wire.Array{wire.Object{"type"="message","role"="assistant","content"=wire.Array{wire.Object{"type"="output_text","text"="firstsecond"}}}}}}); return
    case "unknown","known_blocked":
        if step==1 { response(socket,chat,"","unknown_tool" if scenario=="unknown" else "spawn_entity",{},"unknown-id") }
        else { wire.require(wire.s(result_payload(history[len(history)-1],chat),"error")=="Tool_Not_Allowed","Blocked tool must be rejected"); response(socket,chat,"Tool rejected.","",{},"") }; return
    case "material":
        if step==1 { response(socket,chat,"","query_entities",{},"native-query-exact") }
        else if step==2 { response(socket,chat,"","material",{"action"="set","entity_ids"=wire.Array{selected(request,chat)},"base_color"=wire.Array{f64(0.15),f64(0.85),f64(0.3),i64(1)},"metallic"=f64(0.8),"roughness"=f64(0.2)},"native-material-exact") }
        else { response(socket,chat,"Material updated. Shared Undo is available.","",{},"") }; return
    }
    if step==1 { response(socket,chat,"","spawn_entity",{"name"="fox"},"spawn-id-exact"); return }
    previous:=history[len(history)-1]; payload:=result_payload(previous,chat)
    wire.require(wire.s(payload,"error")=="None" && len(wire.a(payload,"entities"))>0,"Scene owner must return actual entities")
    wire.require(wire.s(previous,"tool_call_id" if chat else "call_id")==("spawn-id-exact" if step==2 else "query-id-exact"),"Tool continuation ID mismatch")
    if step==2 { response(socket,chat,"","query_entities",{},"query-id-exact") } else { wire.require(step==3,"Unexpected provider turn"); response(socket,chat,"Fox created. 🦊","",{},"") }
}
