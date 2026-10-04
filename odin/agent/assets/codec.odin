//! Resource requests validate transport values before reaching application-owned file roots.
package assets

import "core:encoding/json"
import "core:mem"
import ron "../../encoding/ron"

Error :: enum { None, Invalid_JSON, Invalid_Arguments }
Action :: enum { Search, List, Read }
/// Borrows strings from a decoded transport tree and owns its extension array.
Request :: struct { action:Action,query,path,filter:string,extensions:[]string,limit:int,has_filter:bool }
Decoded :: struct { request:Request,tree:json.Value,allocator:mem.Allocator }
/// Releases all transport-owned storage exactly once.
destroy :: proc(decoded:^Decoded) { context.allocator=decoded.allocator; delete(decoded.request.extensions,decoded.allocator); json.destroy_value(decoded.tree); decoded^={} }
/// Only the three implemented file services are admitted, with bounded result counts.
decode :: proc(tool:string,data:[]byte,allocator:=context.allocator)->(Decoded,Error) {
    context.allocator=allocator
    tree,parse_error:=ron.parse_json(data,allocator)
    if parse_error.kind!=.None { return {},.Invalid_JSON }
    result:=Decoded{tree=tree,allocator=allocator,request={limit=64}}
    success:=false; defer { if !success { destroy(&result) } }
    object,ok:=tree.(json.Object); if !ok { return {},.Invalid_Arguments }
    allowed:[]string
    switch tool {
    case "search_assets": result.request.action=.Search; allowed={"query","extensions","limit"}
    case "list_resources": result.request.action=.List; allowed={"path","filter"}; result.request.path="."
    case "read_resource": result.request.action=.Read; allowed={"path"}
    case: return {},.Invalid_Arguments
    }
    for key in object { found:=false; for name in allowed { if key==name { found=true; break } }; if !found { return {},.Invalid_Arguments } }
    for key in ([3]string{"query","path","filter"}) {
        if value,present:=object[key]; present {
            if _,is_null:=value.(json.Null); is_null && result.request.action==.List { continue }
            text,is_text:=value.(string); if !is_text { return {},.Invalid_Arguments }
            switch key {
            case "query": result.request.query=text
            case "path": result.request.path=text
            case "filter": result.request.filter=text; result.request.has_filter=true
            }
        }
    }
    if result.request.action==.Read && result.request.path=="" { return {},.Invalid_Arguments }
    if value,present:=object["limit"]; present {
        if _,is_null:=value.(json.Null); !is_null {
            number:u64
            if integer,is_integer:=value.(json.Integer); is_integer { if integer<0 { return {},.Invalid_Arguments }; number=u64(integer) }
            else { unsigned,valid:=ron.uint_read(value); if !valid { return {},.Invalid_Arguments }; number=unsigned }
            result.request.limit=int(clamp(number,1,256))
        }
    }
    if value,present:=object["extensions"]; present {
        array,is_array:=value.(json.Array); if !is_array || len(array)>64 { return {},.Invalid_Arguments }
        result.request.extensions=make([]string,len(array),allocator)
        for element,i in array { text,is_text:=element.(string); if !is_text { return {},.Invalid_Arguments }; result.request.extensions[i]=text }
    }
    success=true; return result,.None
}
