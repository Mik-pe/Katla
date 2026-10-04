//! App-server JSONL lifecycle is pinned to an explicit loaded thread; unsolicited requests stay host-owned.
package host

import "core:encoding/json"
import "core:unicode/utf8"
import "core:strings"
import "core:fmt"
import "core:mem"
import "core:time"
import "core:thread"
import "core:encoding/base64"
import "core:strconv"
import ron "../../encoding/ron"

MAX_HOST_LINE :: 4<<20
RPC_TIMEOUT :: 15*time.Second
@(private="package")
Session :: struct { bridge:^Bridge, transport:Transport, next_id:i64, buffer:[dynamic]byte }
@(private="package")
json_string :: proc(text:string,a:mem.Allocator)->string {
    bytes,err:=json.marshal(text,allocator=a); assert(err==nil); return string(bytes)
}
@(private="package")
parse :: proc(text:string,a:mem.Allocator)->(json.Value,bool) {
    if !utf8.valid_string(text) || strings.contains(text,"\x00") { return {},false }
    tokenizer:=json.make_tokenizer(text,.JSON,true); depth:=0
    for {
        token,err:=json.get_token(&tokenizer)
        if err!=nil && err!=.EOF { return {},false }
        #partial switch token.kind {
        case .Open_Brace,.Open_Bracket: depth+=1; if depth>64 { return {},false }
        case .Close_Brace,.Close_Bracket: depth-=1; if depth<0 { return {},false }
        case .Integer:
            digits:=token.text; limit:=u64(max(i64))
            if len(digits)>0 && digits[0]=='-' { digits=digits[1:]; limit+=1 }
            value,valid:=ron.decimal_u64(digits); if !valid || value>limit { return {},false }
        case .Float:
            number,valid:=strconv.parse_f64(token.text); if !valid || !(number>=-max(f64) && number<=max(f64)) { return {},false }
        case .EOF: break
        }
        if token.kind==.EOF { break }
    }
    if depth!=0 { return {},false }
    parser:=json.make_parser(text,.JSON,true,a); value,err:=json.parse_value(&parser)
    if err!=nil { return {},false }
    if parser.curr_token.kind!=.EOF { json.destroy_value(value); return {},false }
    return value,true
}
@(private="package")
question_valid :: proc(text,metadata,png:string,a:mem.Allocator)->bool {
    context.allocator=a
    if !utf8.valid_string(text) || strings.contains(text,"\x00") || !strings.has_prefix(png,"iVBORw0KGgo") || len(png)%4!=0 { return false }
    for c in png { if !(c>='A' && c<='Z' || c>='a' && c<='z' || c>='0' && c<='9' || c=='+' || c=='/' || c=='=') { return false } }
    decoded,decode_error:=base64.decode(png,allocator=a); defer delete(decoded,a)
    if decode_error!=nil || len(decoded)<8 || string(decoded[:8])!="\x89PNG\r\n\x1a\n" { return false }
    tree,ok:=parse(metadata,a); if !ok { return false }; defer json.destroy_value(tree)
    _,object:=tree.(json.Object); return object
}
@(private="package")
write_line :: proc(s:^Session,line:string)->Error {
    b:=s.bridge; bytes:=transmute([]byte)line; offset:=0; started:=time.tick_now()
    for offset<len(bytes)+1 {
        if stopped(b) { return .Closed }; if time.tick_since(started)>RPC_TIMEOUT { return .Timeout }
        ready,wait_error:=transport_wait(&s.transport,true,20); if wait_error!=.None { return wait_error }; if !ready { continue }
        part:=bytes[offset:] if offset<len(bytes) else ([]byte{'\n'})
        n,err:=transport_write(&s.transport,part); if err!=.None { return err }; offset+=n
    }
    return .None
}
@(private="package")
read_line :: proc(s:^Session,wait_ms:i32)->(string,Error,bool) {
    for c,i in s.buffer {
        if c!='\n' { continue }
        end:=i; if end>0 && s.buffer[end-1]=='\r' { end-=1 }
        line:=strings.clone(string(s.buffer[:end]),s.bridge.allocator)
        copy(s.buffer[:],s.buffer[i+1:]); resize(&s.buffer,len(s.buffer)-i-1)
        return line,.None,true
    }
    ready,err:=transport_wait(&s.transport,false,wait_ms); if err!=.None { return "",err,false }; if !ready { return "",.None,false }
    buffer:[8192]byte; n,read_error:=transport_read(&s.transport,buffer[:])
    if read_error!=.None { return "",read_error,false }
    if len(s.buffer)+n>MAX_HOST_LINE { return "",.Limit,false }
    append(&s.buffer,..buffer[:n]); return "",.None,false
}
@(private="package")
notification :: proc(s:^Session,object:json.Object) {
    b:=s.bridge
    method,has_method:=object["method"].(string); if !has_method { return }
    if _,has_id:=object["id"]; has_id { emit(b,{kind=.Attention,text="The connected host requires attention; respond in its own UI."}); return }
    params,ok:=object["params"].(json.Object); if !ok { return }
    thread_id,_:=params["threadId"].(string); if thread_id!=b.config.thread_id { return }
    switch method {
    case "item/agentMessage/delta":
        turn,turn_ok:=params["turnId"].(string); item,item_ok:=params["itemId"].(string); delta,delta_ok:=params["delta"].(string)
        if turn_ok && item_ok && delta_ok { emit(b,{kind=.Text,turn_id=turn,item_id=item,text=delta}) }
    case "turn/completed":
        turn,turn_ok:=params["turn"].(json.Object); if !turn_ok { return }
        id,id_ok:=turn["id"].(string); status,status_ok:=turn["status"].(string)
        if id_ok && status_ok { emit(b,{kind=.Finished,turn_id=id,status=status}) }
    case "error": emit(b,{kind=.Error,text="The connected host reported an error; inspect its own UI."})
    }
}
@(private="package")
rpc :: proc(s:^Session,method,params:string)->(json.Value,Error) {
    if s.next_id==max(i64) { return {},.Limit }; s.next_id+=1
    line:=fmt.aprintf(`{{"id":%d,"method":"%s","params":%s}}`,s.next_id,method,params,allocator=s.bridge.allocator); defer delete(line,s.bridge.allocator)
    if err:=write_line(s,line); err!=.None { return {},err }
    started:=time.tick_now()
    for !stopped(s.bridge) {
        if time.tick_since(started)>RPC_TIMEOUT { return {},.Timeout }
        input,err,ready:=read_line(s,20); if err!=.None { return {},err }; if !ready { continue }
        tree,valid:=parse(input,s.bridge.allocator); delete(input,s.bridge.allocator)
        if !valid { return {},.Protocol }
        object,object_ok:=tree.(json.Object)
        if !object_ok { json.destroy_value(tree); return {},.Protocol }
        if _,request:=object["method"]; request { notification(s,object); json.destroy_value(tree); continue }
        id,id_ok:=object["id"].(json.Integer)
        if !id_ok || i64(id)!=s.next_id { json.destroy_value(tree); return {},.Protocol }
        if _,failure:=object["error"]; failure { json.destroy_value(tree); return {},.Protocol }
        if _,success:=object["result"]; !success { json.destroy_value(tree); return {},.Protocol }
        return tree,.None
    }
    return {},.Closed
}
@(private="package")
result_object :: proc(tree:json.Value)->(json.Object,bool) {
    root,ok:=tree.(json.Object); if !ok { return {},false }; return root["result"].(json.Object)
}
@(private="package")
handshake :: proc(s:^Session)->Error {
    tree,err:=rpc(s,"initialize",`{"clientInfo":{"name":"katla_odin_editor","title":"Katla editor","version":"0.1.0"}}`)
    if err!=.None { return err }; json.destroy_value(tree)
    if write_error:=write_line(s,`{"method":"initialized"}`); write_error!=.None { return write_error }
    cursor:=""; defer delete(cursor,s.bridge.allocator)
    found:=false
    for page:=0;page<4096;page+=1 {
        encoded:="null"; if cursor!="" { encoded=json_string(cursor,s.bridge.allocator) }
        params:=fmt.aprintf(`{{"cursor":%s}}`,encoded,allocator=s.bridge.allocator)
        if encoded!="null" { delete(encoded,s.bridge.allocator) }
        response,call_error:=rpc(s,"thread/loaded/list",params); delete(params,s.bridge.allocator)
        if call_error!=.None { return call_error }
        result,ok:=result_object(response); if !ok { json.destroy_value(response); return .Protocol }
        ids,array_ok:=result["data"].(json.Array); if !array_ok { json.destroy_value(response); return .Protocol }
        for id in ids { if name,valid:=id.(string); valid && name==s.bridge.config.thread_id { found=true; break } }
        next,_:=result["nextCursor"].(string)
        if found { json.destroy_value(response); break }
        if next=="" || next==cursor { json.destroy_value(response); return .Invalid_Config }
        replacement:=strings.clone(next,s.bridge.allocator); json.destroy_value(response); delete(cursor,s.bridge.allocator); cursor=replacement
    }
    if !found { return .Limit }
    id:=json_string(s.bridge.config.thread_id,s.bridge.allocator); defer delete(id,s.bridge.allocator)
    params:=fmt.aprintf(`{{"threadId":%s}}`,id,allocator=s.bridge.allocator); defer delete(params,s.bridge.allocator)
    response,resume_error:=rpc(s,"thread/resume",params); if resume_error!=.None { return resume_error }; defer json.destroy_value(response)
    result,result_ok:=result_object(response); if !result_ok { return .Protocol }
    thread_value,thread_ok:=result["thread"].(json.Object); if !thread_ok { return .Protocol }
    actual,_:=thread_value["id"].(string); if actual!=s.bridge.config.thread_id { return .Protocol }
    name,_:=thread_value["name"].(string); emit(s.bridge,{kind=.Connected,name=name}); return .None
}
@(private="package")
send_question :: proc(s:^Session,c:Command)->Error {
    a:=s.bridge.allocator
    id:=json_string(s.bridge.config.thread_id,a); defer delete(id,a)
    read_params:=fmt.aprintf(`{{"threadId":%s,"includeTurns":true}}`,id,allocator=a); defer delete(read_params,a)
    response,err:=rpc(s,"thread/read",read_params); if err!=.None { return err }; defer json.destroy_value(response)
    result,ok:=result_object(response); if !ok { return .Protocol }
    current,valid:=result["thread"].(json.Object); if !valid { return .Protocol }
    actual,_:=current["id"].(string); if actual!=s.bridge.config.thread_id { return .Protocol }
    status,status_ok:=current["status"].(json.Object); if !status_ok { return .Protocol }
    state,_:=status["type"].(string); if state!="idle" && state!="active" { return .Invalid_Config }
    turns,turns_ok:=current["turns"].(json.Array); if !turns_ok { return .Protocol }
    active:=""
    for index:=len(turns)-1;index>=0;index-=1 {
        turn,turn_ok:=turns[index].(json.Object); if !turn_ok { return .Protocol }
        turn_status,_:=turn["status"].(string)
        if turn_status=="inProgress" { active,_=turn["id"].(string); if active=="" { return .Protocol }; break }
    }
    if state=="active" && active=="" { return .Invalid_Config }
    message:=fmt.aprintf("%s\n\nKatla committed editor view. Geometry candidates are not proof of occlusion visibility or room membership. Use scene/spatial queries beyond the frustum when needed.\n%s",c.text,c.metadata,allocator=a)
    text:=json_string(message,a); delete(message,a); defer delete(text,a)
    image_url:=fmt.aprintf("data:image/png;base64,%s",c.png,allocator=a)
    image:=json_string(image_url,a); delete(image_url,a); defer delete(image,a)
    input:=fmt.aprintf(`[{{"type":"text","text":%s}},{{"type":"image","url":%s}}]`,text,image,allocator=a); defer delete(input,a)
    method:="turn/start"; params:string
    if active!="" {
        method="turn/steer"; active_id:=json_string(active,a)
        params=fmt.aprintf(`{{"threadId":%s,"expectedTurnId":%s,"input":%s}}`,id,active_id,input,allocator=a); delete(active_id,a)
    } else { params=fmt.aprintf(`{{"threadId":%s,"input":%s}}`,id,input,allocator=a) }
    accepted,accepted_error:=rpc(s,method,params); delete(params,a)
    if accepted_error!=.None { return accepted_error }; defer json.destroy_value(accepted)
    data,data_ok:=result_object(accepted); if !data_ok { return .Protocol }
    turn,has_turn:=data["turn"].(json.Object); turn_id:=""
    if has_turn { turn_id,_=turn["id"].(string) } else { turn_id,_=data["turnId"].(string) }
    if turn_id=="" { return .Protocol }
    if !emit(s.bridge,{kind=.Accepted,turn_id=turn_id}) { return .Limit }
    return .None
}
@(private="package")
interrupt_turn :: proc(s:^Session,c:Command)->Error {
    a:=s.bridge.allocator; id:=json_string(s.bridge.config.thread_id,a); defer delete(id,a)
    turn:=json_string(c.turn_id,a); defer delete(turn,a)
    params:=fmt.aprintf(`{{"threadId":%s,"turnId":%s}}`,id,turn,allocator=a); defer delete(params,a)
    response,err:=rpc(s,"turn/interrupt",params); if err==.None { json.destroy_value(response) }; return err
}
@(private="package")
bridge_worker :: proc(th:^thread.Thread) {
    b:=cast(^Bridge)th.data; context.allocator=b.allocator
    transport_worker_signals()
    s:=Session{bridge=b,buffer=make([dynamic]byte,b.allocator)}; defer delete(s.buffer)
    if err:=transport_open(&s.transport,b.config); err!=.None { terminal(b,"Could not connect the configured host socket."); return }
    defer transport_close(&s.transport)
    if err:=handshake(&s); err!=.None { terminal(b,"The configured conversation could not be verified as loaded on this host."); return }
    for !stopped(b) {
        if command,queued:=pop_command(b); queued {
            err:=interrupt_turn(&s,command) if command.interrupt else send_question(&s,command)
            command_destroy(&command,b.allocator)
            if err==.Invalid_Config { emit(b,{kind=.Error,text="The selected conversation is not ready; check its current turn in the host."}); continue }
            if err!=.None { terminal(b,"Host request failed; reconnect explicitly."); return }
            continue
        }
        line,err,ready:=read_line(&s,20)
        if err!=.None { terminal(b,"Host transport closed or exceeded its bound; reconnect explicitly."); return }
        if !ready { continue }
        tree,valid:=parse(line,b.allocator); delete(line,b.allocator)
        if !valid { terminal(b,"Host sent invalid JSON; reconnect explicitly."); return }
        object,object_ok:=tree.(json.Object)
        if object_ok { notification(&s,object) }
        json.destroy_value(tree)
        if !object_ok { terminal(b,"Host sent an invalid message; reconnect explicitly."); return }
    }
}
