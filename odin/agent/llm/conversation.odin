//! Conversation history and provider tool calls use only a correlated producer mailbox.
package llm

import agent ".."
import editor "../../editor"
import "core:encoding/json"
import "core:mem"
import "core:strings"
import "core:fmt"
import "core:time"
import "core:sync"

MAX_HISTORY_MESSAGES :: 512
MAX_ROUNDS :: 16
/// Owns provider history and schema serialization; one host thread owns each conversation.
Conversation :: struct {
    runtime:^Runtime,
    config:Config,
    mailbox:^editor.Agent_Harness,
    history:[dynamic]string,
    tools:string,
    allowed:map[string]bool,
    call_ids:map[string]bool,
    mutex:sync.Mutex,
    bytes:int,
    pending:u64,
    failed:bool,
    allocator:mem.Allocator,
}
@(private="package")
quote :: proc(text:string,allocator:mem.Allocator)->string {
    bytes,err:=json.marshal(text,allocator=allocator); assert(err==nil); return string(bytes)
}
@(private="package")
write_quote :: proc(b:^strings.Builder,text:string,allocator:mem.Allocator) {
    encoded:=quote(text,allocator); defer delete(encoded,allocator); strings.write_string(b,encoded)
}
@(private="package")
schema_tools :: proc(data:string,api:API,allocator:mem.Allocator)->(string,Error) {
    tree,ok:=parse_json(data,allocator); if !ok { return "",.Config }; defer json.destroy_value(tree,allocator)
    tools,array_ok:=tree.(json.Array); if !array_ok || len(tools)>MAX_TOOL_CALLS { return "",.Config }
    b:strings.Builder; strings.builder_init(&b,allocator); defer strings.builder_destroy(&b)
    names:=make(map[string]bool,allocator); defer delete(names)
    strings.write_string(&b,"[")
    for value,i in tools {
        object,object_ok:=value.(json.Object); if !object_ok { return "",.Config }
        name,name_ok:=get_string(object,"name"); description,description_ok:=get_string(object,"description")
        schema,schema_ok:=object["inputSchema"].(json.Object)
        if !name_ok || name=="" || len(name)>256 || names[name] || !description_ok || !schema_ok { return "",.Config }; names[name]=true
        if i>0 { strings.write_string(&b,",") }
        strings.write_string(&b,`{"type":"function",`)
        if api==.Chat_Completions { strings.write_string(&b,`"function":{`) }
        strings.write_string(&b,`"name":`); write_quote(&b,name,allocator)
        strings.write_string(&b,`,"description":`); write_quote(&b,description,allocator)
        strings.write_string(&b,`,"parameters":`)
        encoded,err:=json.marshal(schema,allocator=allocator); if err!=nil { return "",.Config }
        strings.write_bytes(&b,encoded); delete(encoded,allocator)
        strings.write_string(&b,`,"strict":false}`)
        if api==.Chat_Completions { strings.write_string(&b,"}") }
    }
    strings.write_string(&b,"]"); return strings.clone(strings.to_string(b),allocator),.None
}
@(private="package")
append_history :: proc(s:^Conversation,message:string)->Error {
    if len(s.history)>=MAX_HISTORY_MESSAGES || s.bytes+len(message)>MAX_RESPONSE_BYTES {
        delete(message,s.allocator); s.failed=true; return .Conversation_Limit
    }
    append(&s.history,message); s.bytes+=len(message); return .None
}
@(private="package")
text_message :: proc(role,text:string,allocator:mem.Allocator)->string {
    encoded:=quote(text,allocator); defer delete(encoded,allocator)
    return fmt.aprintf(`{{"role":"%s","content":%s}}`,role,encoded,allocator=allocator)
}
/// Initializes an explicit provider conversation with caller-selected tool schemas and no scene access.
conversation_init :: proc(s:^Conversation,r:^Runtime,c:^Config,mailbox:^editor.Agent_Harness,schemas,system_prompt:string,allocator:=context.allocator)->Error {
    if len(system_prompt)>MAX_TEXT_BYTES { return .Limit }
    if s.runtime!=nil || r==nil || c==nil || mailbox==nil || !r.initialized || config_validate(c^)!=.None { return .Config }
    tools,err:=schema_tools(schemas,c.api,allocator); if err!=.None { return err }
    s.runtime=r; s.config=config_clone(c^,allocator); s.mailbox=mailbox; s.tools=tools; s.allocator=allocator
    s.history=make([dynamic]string,allocator)
    s.allowed=make(map[string]bool,allocator); s.call_ids=make(map[string]bool,allocator)
    schema_tree,schema_valid:=parse_json(schemas,allocator); assert(schema_valid); defer json.destroy_value(schema_tree,allocator)
    for value in schema_tree.(json.Array) { definition:=value.(json.Object); name,_:=get_string(definition,"name"); s.allowed[strings.clone(name,allocator)]=true }
    init_error:=append_history(s,text_message("system",system_prompt,allocator))
    if init_error!=.None { conversation_destroy(s) }
    return init_error
}
/// Attempts to release an abandoned accepted reply; false means its owner is still executing.
conversation_reap :: proc(s:^Conversation)->bool {
    if s.pending==0 { return true }
    if editor.agent_cancel(s.mailbox,s.pending) { s.pending=0; return true }
    reply,ok:=editor.agent_take_result_for(s.mailbox,s.pending); if !ok { return false }; defer editor.agent_response_destroy(&reply)
    assert(reply.ticket==s.pending)
    s.pending=0; return true
}
/// Releases conversation memory and returns an executing ticket still owned by the mailbox.
conversation_destroy :: proc(s:^Conversation)->u64 {
    conversation_reap(s)
    ticket:=s.pending
    for message in s.history { delete(message,s.allocator) }
    delete(s.history); delete(s.tools,s.allocator)
    for name in s.allowed { delete(name,s.allocator) }; delete(s.allowed)
    for id in s.call_ids { delete(id,s.allocator) }; delete(s.call_ids)
    config_destroy(&s.config)
    s^={}; return ticket
}
/// Builds the complete bounded request and preserves every prior tool-call/result correlation.
conversation_request :: proc(s:^Conversation)->(string,Error) {
    b:strings.Builder; strings.builder_init(&b,s.allocator); defer strings.builder_destroy(&b)
    strings.write_string(&b,`{"model":`); write_quote(&b,s.config.model,s.allocator)
    strings.write_string(&b,`,"stream":true,`)
    if s.config.api==.Responses { strings.write_string(&b,`"store":false,"input":[`) }
    else { strings.write_string(&b,`"messages":[`) }
    for message,i in s.history { if i>0 { strings.write_string(&b,",") }; strings.write_string(&b,message) }
    strings.write_string(&b,`],"tools":`); strings.write_string(&b,s.tools)
    if s.config.api==.Responses { strings.write_string(&b,`,"max_output_tokens":`) }
    else { strings.write_string(&b,`,"max_completion_tokens":`) }
    strings.write_u64(&b,u64(s.config.max_tokens))
    if s.config.has_temperature { strings.write_string(&b,`,"temperature":`); strings.write_f64(&b,s.config.temperature,'g') }
    strings.write_string(&b,"}")
    if len(strings.to_string(b))>MAX_RESPONSE_BYTES { return "",.Conversation_Limit }
    return strings.clone(strings.to_string(b),s.allocator),.None
}
@(private="package")
record_assistant :: proc(s:^Conversation,r:^Response)->Error {
    for call in r.calls { if s.call_ids[call.id] { s.failed=true; return .Protocol } }
    for call in r.calls { s.call_ids[strings.clone(call.id,s.allocator)]=true }
    if s.config.api==.Responses {
        if len(r.text)>0 { if err:=append_history(s,text_message("assistant",string(r.text[:]),s.allocator)); err!=.None { return err } }
        for call in r.calls {
            id:=quote(call.id,s.allocator); name:=quote(call.name,s.allocator); args:=quote(string(call.arguments[:]),s.allocator)
            message:=fmt.aprintf(`{{"type":"function_call","call_id":%s,"name":%s,"arguments":%s}}`,id,name,args,allocator=s.allocator)
            delete(id,s.allocator); delete(name,s.allocator); delete(args,s.allocator)
            if err:=append_history(s,message); err!=.None { return err }
        }
        return .None
    }
    b:strings.Builder; strings.builder_init(&b,s.allocator); defer strings.builder_destroy(&b)
    strings.write_string(&b,`{"role":"assistant","content":`); write_quote(&b,string(r.text[:]),s.allocator)
    if len(r.calls)>0 {
        strings.write_string(&b,`,"tool_calls":[`)
        for call,i in r.calls {
            if i>0 { strings.write_string(&b,",") }
            strings.write_string(&b,`{"type":"function","id":`); write_quote(&b,call.id,s.allocator)
            strings.write_string(&b,`,"function":{"name":`); write_quote(&b,call.name,s.allocator)
            strings.write_string(&b,`,"arguments":`); write_quote(&b,string(call.arguments[:]),s.allocator); strings.write_string(&b,"}}")
        }
        strings.write_string(&b,"]")
    }
    strings.write_string(&b,"}"); return append_history(s,strings.clone(strings.to_string(b),s.allocator))
}
@(private="package")
record_tool :: proc(s:^Conversation,id,content:string)->Error {
    encoded_id:=quote(id,s.allocator); defer delete(encoded_id,s.allocator)
    encoded:=quote(content,s.allocator); defer delete(encoded,s.allocator)
    message:string
    if s.config.api==.Responses { message=fmt.aprintf(`{{"type":"function_call_output","call_id":%s,"output":%s}}`,encoded_id,encoded,allocator=s.allocator) }
    else { message=fmt.aprintf(`{{"role":"tool","tool_call_id":%s,"content":%s}}`,encoded_id,encoded,allocator=s.allocator) }
    return append_history(s,message)
}
@(private="package")
result_text :: proc(result:editor.Tool_Result,allocator:mem.Allocator)->string {
    b:strings.Builder; strings.builder_init(&b,allocator); defer strings.builder_destroy(&b)
    error_name:=fmt.aprintf("%s",result.error,allocator=allocator); defer delete(error_name,allocator)
    strings.write_string(&b,`{"error":`); write_quote(&b,error_name,allocator)
    strings.write_string(&b,`,"entities":[`)
    for id,i in result.entities {
        if i>0 { strings.write_string(&b,",") }; strings.write_string(&b,"\""); strings.write_u64(&b,u64(id)); strings.write_string(&b,"\"")
    }
    strings.write_string(&b,`],"data":`)
    if len(result.data)>0 { strings.write_bytes(&b,result.data) } else { strings.write_string(&b,"null") }
    strings.write_string(&b,"}"); return strings.clone(strings.to_string(b),allocator)
}
@(private="package")
execute_tool :: proc(s:^Conversation,call:Call,cancellation:^Cancel)->Error {
    if !s.allowed[call.name] { return record_tool(s,call.id,`{"error":"Tool_Not_Allowed"}`) }
    start:=time.tick_now()
    for {
        if is_cancelled(cancellation) { return .Cancelled }
        if time.tick_since(start)>=time.Duration(s.config.timeout_ms)*time.Millisecond { return .Timeout }
        ticket,err:=agent.submit_call(s.mailbox,{call.id,call.name,call.arguments[:]})
        if err==.Mailbox_Full { time.sleep(time.Millisecond); continue }
        if err!=.None {
            content:=fmt.aprintf(`{{"error":"%s"}}`,err,allocator=s.allocator); defer delete(content,s.allocator)
            if err==.Mailbox_Closed || err==.Identifier_Exhausted { return .Tool }
            return record_tool(s,call.id,content)
        }
        s.pending=ticket; break
    }
    for {
        if is_cancelled(cancellation) { conversation_reap(s); return .Cancelled }
        if time.tick_since(start)>=time.Duration(s.config.timeout_ms)*time.Millisecond { conversation_reap(s); return .Timeout }
        reply,ok:=editor.agent_take_result_for(s.mailbox,s.pending)
        if !ok { time.sleep(time.Millisecond); continue }
        defer editor.agent_response_destroy(&reply)
        if reply.ticket!=s.pending || reply.call_id!=call.id { return .Protocol }
        s.pending=0
        if len(reply.result.data)>MAX_TEXT_BYTES || len(reply.result.entities)>256 { return .Limit }
        content:=result_text(reply.result,s.allocator); defer delete(content,s.allocator)
        return record_tool(s,call.id,content)
    }
}
/// Runs provider/tool rounds on a host thread while the application independently ticks its mailbox.
conversation_turn :: proc(s:^Conversation,prompt:string,cancellation:^Cancel=nil,observer:=Text_Observer{})->(Response,Error) {
    if !sync.mutex_try_lock(&s.mutex) { return {},.Busy }; defer sync.mutex_unlock(&s.mutex)
    if s.failed || s.pending!=0 || s.runtime==nil { return {},.Tool }
    if len(prompt)>MAX_TEXT_BYTES { return {},.Limit }
    if err:=append_history(s,text_message("user",prompt,s.allocator)); err!=.None { return {},err }
    for _ in 0..<MAX_ROUNDS {
        request,request_error:=conversation_request(s); if request_error!=.None { s.failed=true; return {},request_error }
        response,err:=complete(s.runtime,s.config,request,cancellation,observer); delete(request,s.allocator)
        if err!=.None { s.failed=true; return {},err }
        if history_error:=record_assistant(s,&response); history_error!=.None { response_destroy(&response); return {},history_error }
        if len(response.calls)==0 { return response,.None }
        for call in response.calls {
            if tool_error:=execute_tool(s,call,cancellation); tool_error!=.None { response_destroy(&response); s.failed=true; return {},tool_error }
        }
        response_destroy(&response)
    }
    s.failed=true; return {},.Conversation_Limit
}
