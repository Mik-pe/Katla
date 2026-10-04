//! Bounded SSE framing and strict provider completion assembly.
package llm

import "core:encoding/json"
import "core:mem"
import "core:strings"
import "core:unicode/utf8"
import "core:strconv"

MAX_RESPONSE_BYTES :: 8<<20
MAX_EVENT_BYTES :: 256<<10
MAX_TEXT_BYTES :: 1<<20
MAX_TOOL_CALLS :: 64
MAX_EVENTS :: 16384
/// A complete owned function call; arguments remain JSON bytes rather than lossy numeric values.
Call :: struct { id,name:string, arguments:[dynamic]byte }
/// A complete owned provider response, transferred only after a valid terminal event.
Response :: struct { text:[dynamic]byte, calls:[dynamic]Call, allocator:mem.Allocator }
/// Receives borrowed text on the calling host thread, never on the scene owner.
Text_Observer :: struct { state:rawptr, text:proc(rawptr,string) }
/// Releases response strings, text and arguments with their captured allocator.
response_destroy :: proc(r:^Response) {
    for call in r.calls { delete(call.id,r.allocator); delete(call.name,r.allocator); delete(call.arguments) }
    delete(r.calls); delete(r.text); r^={}
}
/// Owns incremental framing and response assembly; keep it on one producer thread.
Stream :: struct {
    api:API,
    line,event:[dynamic]byte,
    response:Response,
    observer:Text_Observer,
    bytes,events:int,
    terminal,done:bool,
    finish:string,
    error:Error,
    allocator:mem.Allocator,
}
/// Initializes a stream without retaining a world or application pointer.
stream_init :: proc(s:^Stream,api:API,observer:=Text_Observer{},allocator:=context.allocator) {
    s.api=api; s.allocator=allocator; s.observer=observer
    s.line=make([dynamic]byte,allocator); s.event=make([dynamic]byte,allocator)
    s.response=Response{text=make([dynamic]byte,allocator),calls=make([dynamic]Call,allocator),allocator=allocator}
}
/// Releases incomplete streams or the framing of a transferred response.
stream_destroy :: proc(s:^Stream) {
    response_destroy(&s.response); delete(s.line); delete(s.event); delete(s.finish,s.allocator); s^={}
}
@(private="package")
parse_json :: proc(data:string,allocator:mem.Allocator)->(json.Value,bool) {
    if len(data)>MAX_EVENT_BYTES || !utf8.valid_string(data) || strings.contains(data,"\x00") { return {},false }
    tokenizer:=json.make_tokenizer(data,.JSON,true); depth:=0; previous:json.Token
    for {
        token,err:=json.get_token(&tokenizer)
        if err!=nil && err!=.EOF { return {},false }
        if token.kind==.Colon && previous.kind==.String && previous.text==`""` { return {},false }
        previous=token
        #partial switch token.kind {
        case .Open_Brace,.Open_Bracket: depth+=1; if depth>64 { return {},false }
        case .Close_Brace,.Close_Bracket: depth-=1; if depth<0 { return {},false }
        case .Integer:
            text:=token.text; limit:=u64(max(i64))
            if len(text)>0 && text[0]=='-' { text=text[1:]; limit+=1 }
            number,number_valid:=checked_decimal(text)
            if !number_valid || number>limit || len(text)>1 && text[0]=='0' { return {},false }
        case .Float:
            number,number_valid:=strconv.parse_f64(token.text)
            if !json_number_valid(token.text) || !number_valid || !(number>=-max(f64) && number<=max(f64)) { return {},false }
        }
        if token.kind==.EOF { break }
    }
    if depth!=0 { return {},false }
    parser:=json.make_parser(data,.JSON,true,allocator)
    tree,err:=json.parse_value(&parser); if err!=nil { return {},false }
    if parser.curr_token.kind!=.EOF { json.destroy_value(tree,allocator); return {},false }
    return tree,true
}
@(private="package")
get_string :: proc(object:json.Object,key:string)->(string,bool) {
    value,exists:=object[key]; if !exists { return "",false }; return value.(string)
}
@(private="package")
append_text :: proc(s:^Stream,text:string)->Error {
    if len(s.response.text)+len(text)>MAX_TEXT_BYTES { return .Limit }
    append(&s.response.text,..transmute([]byte)text)
    if s.observer.text!=nil && text!="" { s.observer.text(s.observer.state,text) }
    return .None
}
@(private="package")
replace_append :: proc(target:^string,delta:string,allocator:mem.Allocator,limit:int)->Error {
    if len(target^)+len(delta)>limit { return .Limit }
    bytes:=make([]byte,len(target^)+len(delta),allocator)
    copy(bytes,transmute([]byte)target^); copy(bytes[len(target^):],transmute([]byte)delta)
    delete(target^,allocator); target^=string(bytes); return .None
}
@(private="package")
chat_event :: proc(s:^Stream,object:json.Object)->Error {
    if s.terminal { return .Protocol }
    if _,exists:=object["error"]; exists { return .HTTP }
    choices,ok:=object["choices"].(json.Array); if !ok { return .Protocol }
    if len(choices)==0 { return .None }
    if len(choices)!=1 { return .Protocol }
    choice,choice_valid:=choices[0].(json.Object); if !choice_valid { return .Protocol }
    index,has_index:=choice["index"].(json.Integer); if !has_index || index!=0 { return .Protocol }
    delta,has_delta:=choice["delta"].(json.Object); if !has_delta { return .Protocol }
    if _,exists:=delta["refusal"].(string); exists { return .Refused }
    if value,exists:=delta["content"]; exists {
        if text,is_text:=value.(string); is_text { if err:=append_text(s,text); err!=.None { return err } }
        else if _,is_null:=value.(json.Null); !is_null { return .Protocol }
    }
    if calls_value,calls_exist:=delta["tool_calls"]; calls_exist {
        calls,calls_valid:=calls_value.(json.Array); if !calls_valid || len(calls)>MAX_TOOL_CALLS { return .Protocol }
        for entry in calls {
            call,call_valid:=entry.(json.Object); if !call_valid { return .Protocol }
            idx,idx_valid:=call["index"].(json.Integer); if !idx_valid || idx<0 || idx>=MAX_TOOL_CALLS { return .Protocol }
            for len(s.response.calls)<=int(idx) { append(&s.response.calls,Call{arguments=make([dynamic]byte,s.allocator)}) }
            dest:=&s.response.calls[int(idx)]
            if value,exists:=call["id"]; exists {
                id,id_valid:=value.(string); if !id_valid || dest.id!="" || id=="" || len(id)>256 { return .Protocol }; dest.id=strings.clone(id,s.allocator)
            }
            if value,exists:=call["type"]; exists { kind,kind_valid:=value.(string); if !kind_valid || kind!="function" { return .Protocol } }
            if function_value,function_exists:=call["function"]; function_exists {
                function,function_valid:=function_value.(json.Object); if !function_valid { return .Protocol }
                if value,exists:=function["name"]; exists {
                    name,name_valid:=value.(string); if !name_valid { return .Protocol }
                    if err:=replace_append(&dest.name,name,s.allocator,256); err!=.None { return err }
                }
                if value,exists:=function["arguments"]; exists {
                    args,args_valid:=value.(string); if !args_valid { return .Protocol }
                    if len(dest.arguments)+len(args)>MAX_EVENT_BYTES { return .Limit }; append(&dest.arguments,..transmute([]byte)args)
                }
            }
        }
    }
    if value,exists:=choice["finish_reason"]; exists {
        if _,is_null:=value.(json.Null); is_null { return .None }
        reason,reason_valid:=value.(string); if !reason_valid { return .Protocol }
        switch reason {
        case "stop","tool_calls": s.finish=strings.clone(reason,s.allocator); s.terminal=true
        case "length": return .Truncated
        case "content_filter": return .Refused
        case: return .Protocol
        }
    }
    return .None
}
@(private="package")
responses_event :: proc(s:^Stream,object:json.Object)->Error {
    if s.terminal { return .Protocol }
    kind,kind_valid:=get_string(object,"type"); if !kind_valid { return .Protocol }
    switch kind {
    case "error","response.failed": return .HTTP
    case "response.incomplete": return .Truncated
    case "response.refusal.delta","response.refusal.done": return .Refused
    case "response.output_text.delta":
        text,text_valid:=get_string(object,"delta"); if !text_valid { return .Protocol }; return append_text(s,text)
    case "response.completed":
        response,response_valid:=object["response"].(json.Object); if !response_valid { return .Protocol }
        status,status_valid:=get_string(response,"status"); if !status_valid || status!="completed" { return .Protocol }
        output,output_valid:=response["output"].(json.Array); if !output_valid || len(output)>MAX_TOOL_CALLS+64 { return .Protocol }
        final_text:=make([dynamic]byte,s.allocator); defer delete(final_text)
        for entry in output {
            item,item_valid:=entry.(json.Object); if !item_valid { return .Protocol }
            item_kind,item_kind_valid:=get_string(item,"type"); if !item_kind_valid { return .Protocol }
            switch item_kind {
            case "function_call":
                id,id_ok:=get_string(item,"call_id"); name,name_ok:=get_string(item,"name"); args,args_ok:=get_string(item,"arguments")
                if !id_ok || !name_ok || !args_ok || len(s.response.calls)>=MAX_TOOL_CALLS || len(id)>256 || len(name)>256 || len(args)>MAX_EVENT_BYTES { return .Protocol }
                call:=Call{id=strings.clone(id,s.allocator),name=strings.clone(name,s.allocator),arguments=make([dynamic]byte,len(args),s.allocator)}
                copy(call.arguments[:],transmute([]byte)args); append(&s.response.calls,call)
            case "message":
                role,role_valid:=get_string(item,"role"); if !role_valid || role!="assistant" { return .Protocol }
                content,content_valid:=item["content"].(json.Array); if !content_valid || len(content)>64 { return .Protocol }
                for part in content {
                    segment,segment_valid:=part.(json.Object); if !segment_valid { return .Protocol }
                    segment_kind,segment_kind_valid:=get_string(segment,"type"); if !segment_kind_valid { return .Protocol }
                    if segment_kind=="refusal" { return .Refused }
                    if segment_kind!="output_text" { return .Protocol }
                    text,text_valid:=get_string(segment,"text"); if !text_valid { return .Protocol }
                    if len(final_text)+len(text)>MAX_TEXT_BYTES { return .Limit }; append(&final_text,..transmute([]byte)text)
                }
            case "reasoning":
            case: return .Protocol
            }
        }
        if string(final_text[:])!=string(s.response.text[:]) { return .Protocol }
        s.terminal=true; s.done=true
    case "response.created","response.in_progress","response.output_item.added","response.output_item.done","response.content_part.added","response.content_part.done","response.output_text.done","response.function_call_arguments.delta","response.function_call_arguments.done","response.reasoning_summary_part.added","response.reasoning_summary_part.done","response.reasoning_summary_text.delta","response.reasoning_summary_text.done","response.reasoning_text.delta","response.reasoning_text.done":
    case: return .Protocol
    }
    return .None
}
@(private="package")
process_event :: proc(s:^Stream)->Error {
    if len(s.event)==0 { return .None }
    s.events+=1; if s.events>MAX_EVENTS { return .Limit }
    data:=string(s.event[:]); if data[len(data)-1]=='\n' { data=data[:len(data)-1] }
    if data=="[DONE]" {
        if s.api!=.Chat_Completions || !s.terminal || s.done { return .Protocol }; s.done=true; return .None
    }
    if s.done { return .Protocol }
    tree,tree_valid:=parse_json(data,s.allocator); if !tree_valid { return .Protocol }; defer json.destroy_value(tree,s.allocator)
    object,object_valid:=tree.(json.Object); if !object_valid { return .Protocol }
    return chat_event(s,object) if s.api==.Chat_Completions else responses_event(s,object)
}
/// Feeds arbitrary byte fragments, including split UTF-8 and CRLF; never executes tools before completion.
stream_feed :: proc(s:^Stream,data:[]byte)->Error {
    if s.error!=.None { return s.error }
    if s.bytes+len(data)>MAX_RESPONSE_BYTES { s.error=.Limit; return s.error }; s.bytes+=len(data)
    for ch in data {
        if ch=='\n' {
            line:=string(s.line[:]); if !utf8.valid_string(line) || strings.contains(line,"\x00") { s.error=.Protocol; return s.error }
            if len(line)>0 && line[len(line)-1]=='\r' { line=line[:len(line)-1] }
            if line=="" {
                s.error=process_event(s); resize(&s.event,0)
                if s.error!=.None { return s.error }
            } else if strings.has_prefix(line,"data:") {
                value:=line[5:]; if strings.has_prefix(value," ") { value=value[1:] }
                if len(s.event)+len(value)+1>MAX_EVENT_BYTES { s.error=.Limit; return s.error }
                append(&s.event,..transmute([]byte)value); append(&s.event,'\n')
            } else if !strings.has_prefix(line,":") && !strings.has_prefix(line,"event:") && !strings.has_prefix(line,"id:") && !strings.has_prefix(line,"retry:") {
                s.error=.Protocol; return s.error
            }
            resize(&s.line,0)
        } else {
            if len(s.line)>=MAX_EVENT_BYTES { s.error=.Limit; return s.error }; append(&s.line,ch)
        }
    }
    return .None
}
/// Transfers the assembled response only after framing, IDs, arguments and terminal state are valid.
stream_finish :: proc(s:^Stream)->(Response,Error) {
    if s.error!=.None { return {},s.error }
    if !s.done || !s.terminal || len(s.line)!=0 || len(s.event)!=0 { return {},.Truncated }
    if s.api==.Chat_Completions && ((s.finish=="tool_calls")!=(len(s.response.calls)>0)) { return {},.Protocol }
    for call,i in s.response.calls {
        if call.id=="" || call.name=="" { return {},.Protocol }
        for j in 0..<i { if call.id==s.response.calls[j].id { return {},.Protocol } }
        args,args_valid:=parse_json(string(call.arguments[:]),s.allocator); if !args_valid { return {},.Protocol }
        _,object:=args.(json.Object); json.destroy_value(args,s.allocator); if !object { return {},.Protocol }
    }
    response:=s.response; s.response={}; return response,.None
}

@(private="package")
json_number_valid :: proc(text:string)->bool {
    i:=0
    if len(text)>0 && text[0]=='-' { i+=1 }
    if i>=len(text) { return false }
    if text[i]=='0' { i+=1 }
    else {
        if text[i]<'1' || text[i]>'9' { return false }
        for i<len(text) && text[i]>='0' && text[i]<='9' { i+=1 }
    }
    if i<len(text) && text[i]=='.' {
        i+=1; start:=i
        for i<len(text) && text[i]>='0' && text[i]<='9' { i+=1 }
        if i==start { return false }
    }
    if i<len(text) && (text[i]=='e' || text[i]=='E') {
        i+=1
        if i<len(text) && (text[i]=='+' || text[i]=='-') { i+=1 }
        start:=i
        for i<len(text) && text[i]>='0' && text[i]<='9' { i+=1 }
        if i==start { return false }
    }
    return i==len(text)
}
