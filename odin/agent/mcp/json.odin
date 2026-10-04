//! Bounded strict JSON parsing and escaped JSON-RPC response construction.
package mcp

import agent ".."
import "core:encoding/json"
import "core:unicode/utf8"
import "core:strconv"
import "core:mem"
import "core:fmt"
import "core:strings"

MAX_MESSAGE_BYTES :: 1<<20
MAX_JSON_DEPTH :: 64
PROTOCOL_VERSION :: "2026-07-28"
SERVER_META :: `{"io.modelcontextprotocol/serverInfo":{"name":"katla-odin","version":"0.1.0"}}`

@(private="package")
parse_message :: proc(data:string,allocator:mem.Allocator)->(json.Value,bool) {
    context.allocator=allocator
    if len(data)>MAX_MESSAGE_BYTES || !utf8.valid_string(data) || strings.contains(data,"\x00") || strings.contains(data,"\n") { return {},false }
    tokenizer:=json.make_tokenizer(data,.JSON,true)
    depth:=0
    previous:json.Token
    for {
        token,err:=json.get_token(&tokenizer)
        if err!=nil && err!=.EOF { return {},false }
        if token.kind==.Colon && previous.kind==.String && previous.text==`""` { return {},false }
        previous=token
        #partial switch token.kind {
        case .Open_Brace,.Open_Bracket: depth+=1; if depth>MAX_JSON_DEPTH { return {},false }
        case .Close_Brace,.Close_Bracket: depth-=1; if depth<0 { return {},false }
        case .Integer:
            text:=token.text; limit:=u64(max(i64))
            if len(text)>0 && text[0]=='-' { text=text[1:]; limit+=1 }
            value,valid:=agent.parse_entity_id(text)
            if !valid || u64(value)>limit || (len(text)>1 && text[0]=='0') { return {},false }
        case .Float:
            if !number_syntax_valid(token.text) { return {},false }
            n,valid:=strconv.parse_f64(token.text)
            if !valid || !(n>=-max(f64) && n<=max(f64)) { return {},false }
        case .EOF: break
        }
        if token.kind==.EOF { break }
    }
    if depth!=0 { return {},false }
    parser:=json.make_parser(data,.JSON,true,allocator)
    tree,err:=json.parse_value(&parser)
    if err!=nil { return {},false }
    if parser.curr_token.kind!=.EOF { json.destroy_value(tree); return {},false }
    return tree,true
}
@(private="package")
request_id :: proc(value:json.Value,allocator:mem.Allocator)->(string,bool) {
    #partial switch v in value {
    case string,json.Integer:
        bytes,err:=json.marshal(v,allocator=allocator)
        return string(bytes),err==nil
    }
    return "",false
}
@(private="package")
rpc_error :: proc(id:string,code:int,message:string,data:="",allocator:=context.allocator)->string {
    encoded,err:=json.marshal(message,allocator=allocator); assert(err==nil); defer delete(encoded,allocator)
    id_field:=""; data_field:=""
    if id!="" { id_field=fmt.aprintf(`,"id":%s`,id,allocator=allocator) }; defer delete(id_field,allocator)
    if data!="" { data_field=fmt.aprintf(`,"data":%s`,data,allocator=allocator) }; defer delete(data_field,allocator)
    return fmt.aprintf(`{{"jsonrpc":"2.0"%s,"error":{{"code":%d,"message":%s%s}}}}`,id_field,code,string(encoded),data_field,allocator=allocator)
}
@(private="package")
rpc_result :: proc(id,result:string,allocator:mem.Allocator)->string {
    return fmt.aprintf(`{{"jsonrpc":"2.0","id":%s,"result":%s}}`,id,result,allocator=allocator)
}
@(private="package")
allowed_fields :: proc(object:json.Object,names:[]string)->bool {
    for key in object {
        found:=false
        for name in names { if key==name { found=true; break } }
        if !found { return false }
    }
    return true
}

@(private="package")
number_syntax_valid :: proc(text:string)->bool {
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
