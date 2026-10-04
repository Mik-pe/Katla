//! Resource requests validate transport values before reaching application-owned file roots.
package assets

import "core:encoding/json"
import "core:mem"

Error :: enum { None, Invalid_JSON, Invalid_Arguments }
Action :: enum { Search, List, Read }
/// Borrows strings from a decoded transport tree and owns its extension array.
Request :: struct { action:Action,query,path,filter:string,extensions:[]string,limit:int }
Decoded :: struct { request:Request,tree:json.Value,allocator:mem.Allocator }
/// Releases all transport-owned storage exactly once.
destroy :: proc(decoded:^Decoded) { context.allocator=decoded.allocator; delete(decoded.request.extensions,decoded.allocator); json.destroy_value(decoded.tree); decoded^={} }
/// Only the three implemented file services are admitted, with bounded result counts.
decode :: proc(tool:string,data:[]byte,allocator:=context.allocator)->(Decoded,Error) {
    context.allocator=allocator
    tree,parse_error:=json.parse(data,spec=.JSON,parse_integers=true,allocator=allocator)
    if parse_error!=nil { return {},.Invalid_JSON }
    result:=Decoded{tree=tree,allocator=allocator,request={limit=64}}
    success:=false; defer { if !success { destroy(&result) } }
    object,ok:=tree.(json.Object); if !ok { return {},.Invalid_Arguments }
    allowed:[]string
    switch tool {
    case "search_assets": result.request.action=.Search; allowed={"query","extensions","limit"}
    case "list_resources": result.request.action=.List; allowed={"path","filter","limit"}
    case "read_resource": result.request.action=.Read; allowed={"path"}
    case: return {},.Invalid_Arguments
    }
    for key in object { found:=false; for name in allowed { if key==name { found=true; break } }; if !found { return {},.Invalid_Arguments } }
    for key in ([3]string{"query","path","filter"}) {
        if value,present:=object[key]; present {
            text,is_text:=value.(string); if !is_text { return {},.Invalid_Arguments }
            switch key {
            case "query": result.request.query=text
            case "path": result.request.path=text
            case "filter": result.request.filter=text
            }
        }
    }
    if result.request.action!=.Search && result.request.path=="" { return {},.Invalid_Arguments }
    if value,present:=object["limit"]; present {
        n,is_integer:=value.(json.Integer); if !is_integer || n<1 || n>256 { return {},.Invalid_Arguments }; result.request.limit=int(n)
    }
    if value,present:=object["extensions"]; present {
        array,is_array:=value.(json.Array); if !is_array || len(array)>64 { return {},.Invalid_Arguments }
        result.request.extensions=make([]string,len(array),allocator)
        for element,i in array { text,is_text:=element.(string); if !is_text || len(text)==0 || len(text)>32 { return {},.Invalid_Arguments }; result.request.extensions[i]=text }
    }
    success=true; return result,.None
}
