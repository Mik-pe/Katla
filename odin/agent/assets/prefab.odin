//! Reusable asset authoring requests retain complete documents until owner-thread execution.
package assets

import "core:encoding/json"
import ron "../../encoding/ron"
import "core:mem"
import "core:strings"
import ecs "../../ecs"

Prefab_Action :: enum { Describe, Read, Validate, Write, Instantiate, Capture, Remove }
/// Mesh documents are complete authoring input; omitted fields are not implicit patches.
Prefab_Request :: struct { action:Prefab_Action,path,name:string,document:json.Value,position:[3]f32,rotation:[4]f32,scale:[3]f32,root_entity:ecs.Entity_Id }
Decoded_Prefab :: struct { request:Prefab_Request,tree:json.Value,allocator:mem.Allocator }
/// Releases the borrowed document's owning transport tree.
prefab_destroy :: proc(decoded:^Decoded_Prefab) { context.allocator=decoded.allocator; json.destroy_value(decoded.tree); decoded^={} }
/// Validates implemented document operations before the application resolves project paths.
prefab_decode :: proc(data:[]byte,allocator:=context.allocator)->(Decoded_Prefab,Error) {
    context.allocator=allocator
    tree,parse_error:=ron.parse_json(data,allocator); if parse_error.kind!=.None { return {},.Invalid_JSON }
    decoded:=Decoded_Prefab{tree=tree,allocator=allocator}; decoded.request.rotation={0,0,0,1}; decoded.request.scale={1,1,1}; success:=false; defer { if !success { prefab_destroy(&decoded) } }
    object,ok:=tree.(json.Object); if !ok { return {},.Invalid_Arguments }
    action,is_action:=object["action"].(string); if !is_action { return {},.Invalid_Arguments }
    allowed:[]string
    switch action {
    case "describe": decoded.request.action=.Describe; allowed={"action"}
    case "read": decoded.request.action=.Read; allowed={"action","path"}
    case "validate": decoded.request.action=.Validate; allowed={"action","path","document"}
    case "write": decoded.request.action=.Write; allowed={"action","path","document"}
    case "instantiate": decoded.request.action=.Instantiate; allowed={"action","path","name","position","rotation","scale"}
    case "capture": decoded.request.action=.Capture; allowed={"action","path","root_entity"}
    case "remove": decoded.request.action=.Remove; allowed={"action","root_entity"}
    case: return {},.Invalid_Arguments
    }
    for key in object { found:=false; for name in allowed { if name==key { found=true; break } }; if !found { return {},.Invalid_Arguments } }
    if decoded.request.action!=.Describe && decoded.request.action!=.Remove {
        path,is_path:=object["path"].(string); if !is_path || len(path)==0 { return {},.Invalid_Arguments }; decoded.request.path=path
    }
    if decoded.request.action==.Validate || decoded.request.action==.Write {
        document,present:=object["document"]; if !present { return {},.Invalid_Arguments }
        if _,is_object:=document.(json.Object); !is_object { return {},.Invalid_Arguments }; decoded.request.document=document
    }
    if decoded.request.action==.Instantiate {
        if v,present:=object["name"]; present { name,valid:=v.(string); if !valid || len(strings.trim_space(name))==0 || len(name)>256 { return {},.Invalid_Arguments }; decoded.request.name=name }
        if v,present:=object["position"]; present { vector,valid:=prefab_vector(v,3); if !valid { return {},.Invalid_Arguments }; decoded.request.position=vector }
        if v,present:=object["scale"]; present { vector,valid:=prefab_vector(v,3); if !valid { return {},.Invalid_Arguments }; for axis in vector { if axis==0 { return {},.Invalid_Arguments } }; decoded.request.scale=vector }
        if v,present:=object["rotation"]; present { vector,valid:=prefab_vector(v,4); if !valid { return {},.Invalid_Arguments }; norm:f32; for axis in vector { norm+=axis*axis }; if abs(1-norm)>=0.001 { return {},.Invalid_Arguments }; decoded.request.rotation=vector }
    }
    if decoded.request.action==.Capture || decoded.request.action==.Remove {
        text,is_text:=object["root_entity"].(string); if !is_text || len(text)==0 { return {},.Invalid_Arguments }
        for digit in text { if digit<'0' || digit>'9' { return {},.Invalid_Arguments } }
        id,is_id:=ron.decimal_u64(text); if !is_id { return {},.Invalid_Arguments }; decoded.request.root_entity=ecs.Entity_Id(id)
    }
    success=true; return decoded,.None
}

@(private="package")
prefab_vector :: proc(value:json.Value,$N:int)->([N]f32,bool) {
    result:[N]f32; values,valid:=value.(json.Array); if !valid || len(values)!=N { return result,false }
    for element,i in values {
        number:f64
        #partial switch n in element {
        case json.Integer: number=f64(n)
        case json.Float: number=f64(n)
        case: return result,false
        }
        if !(number>=-1000000 && number<=1000000) { return result,false }; result[i]=f32(number)
    }
    return result,true
}
