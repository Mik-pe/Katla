//! Committed editor captures return separate image content and owned metadata without image duplication.
package mcp

import "core:encoding/json"
import "core:fmt"
import "core:mem"
import "core:strings"

@(private="package")
view_error_message :: proc(bytes:[]byte,allocator:mem.Allocator)->string {
    if len(bytes)==0 || len(bytes)>2048 { return "" }
    value,error:=json.parse(bytes,spec=.JSON,allocator=allocator); if error!=nil { return "" }; defer json.destroy_value(value)
    object,okay:=value.(json.Object); if !okay || len(object)!=1 { return "" }
    message,present:=object["capture_error"].(string)
    if !present || len(message)==0 || len(message)>1024 { return "" }
    return strings.clone(message,allocator)
}

@(private="package")
view_result :: proc(id:string,bytes:[]byte,allocator:mem.Allocator)->string {
    context.allocator=allocator
    if len(bytes)>32<<20 { return tool_error(id,"Editor capture exceeded its reply bound",allocator) }
    tree,err:=json.parse(bytes,spec=.JSON,parse_integers=true,allocator=allocator)
    if err!=nil { return tool_error(id,"Editor capture metadata is invalid",allocator) }; defer json.destroy_value(tree)
    object,valid:=tree.(json.Object); if !valid { return tool_error(id,"Editor capture metadata must be an object",allocator) }
    png,has_png:=object["image_png_base64"].(string)
    if !has_png || !strings.has_prefix(png,"iVBORw0KGgo") || len(png)>24<<20 { return tool_error(id,"Editor capture lacks a bounded committed PNG",allocator) }
    // Copy only the map; removing the image from the borrowed original would leak its JSON owner.
    metadata:=make(json.Object,allocator); defer delete(metadata)
    for key,value in object { if key!="image_png_base64" { metadata[key]=value } }
    data,marshal_error:=json.marshal(metadata,allocator=allocator); if marshal_error!=nil { return tool_error(id,"Could not serialize editor metadata",allocator) }; defer delete(data,allocator)
    text,text_error:=json.marshal(string(data),allocator=allocator); assert(text_error==nil); defer delete(text,allocator)
    image,image_error:=json.marshal(png,allocator=allocator); assert(image_error==nil); defer delete(image,allocator)
    result:=fmt.aprintf(`{{"resultType":"complete","_meta":%s,"isError":false,"structuredContent":%s,"content":[{{"type":"text","text":%s}},{{"type":"image","data":%s,"mimeType":"image/png"}}]}}`,SERVER_META,string(data),string(text),string(image),allocator=allocator)
    defer delete(result,allocator)
    return rpc_result(id,result,allocator)
}
