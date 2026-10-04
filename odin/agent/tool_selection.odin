//! Consumers select capabilities from one canonical tool schema registry.
package agent

import "core:encoding/json"
import "core:mem"
import "core:slice"

/// Missing or repeated capabilities fail instead of silently advertising a different tool set.
Tool_Selection_Error :: enum { None, Invalid_Registry, Missing_Tool, Duplicate_Tool }
/// Returns independently owned schemas sorted by tool name; free with the selected allocator.
tools_select :: proc(names:[]string,allocator:mem.Allocator=context.allocator)->(string,Tool_Selection_Error) {
    registry,parse_error:=json.parse(TOOLS_JSON,allocator=allocator)
    if parse_error!=nil { return "",.Invalid_Registry }; defer json.destroy_value(registry,allocator)
    entries,is_array:=registry.(json.Array); if !is_array { return "",.Invalid_Registry }
    definitions:=make(map[string]json.Value,allocator); defer delete(definitions)
    for entry in entries {
        object,valid:=entry.(json.Object); if !valid { return "",.Invalid_Registry }
        name,name_valid:=object["name"].(string)
        if !name_valid || name=="" || name in definitions { return "",.Invalid_Registry }
        definitions[name]=entry
    }
    sorted:=make([]string,len(names),allocator); defer delete(sorted); copy(sorted,names); slice.sort(sorted)
    selected:=make([dynamic]json.Value,0,len(names),allocator); defer delete(selected)
    for name,i in sorted {
        if i>0 && sorted[i-1]==name { return "",.Duplicate_Tool }
        definition,present:=definitions[name]; if !present { return "",.Missing_Tool }; append(&selected,definition)
    }
    encoded,error:=json.marshal(selected,allocator=allocator)
    if error!=nil { return "",.Invalid_Registry }
    return string(encoded),.None
}
