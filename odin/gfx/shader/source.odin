//! Include expansion reads through an explicit confined source owner before artifact hashing.
package shader

import "core:mem"
import "core:strings"

/// Reads an owned bounded UTF-8 file from a caller-defined source root.
Source_Loader :: struct { state:rawptr,read:proc(rawptr,string,mem.Allocator)->([]byte,bool) }
/// Compiles current file contents and transitive includes; cached artifacts share the expanded request key.
compile_source_file :: proc(compiler:^Compiler,loader:Source_Loader,path:string,selections:[]Selection,constants:[]Constant=nil,allocator:=context.allocator)->(Compiled,Error) {
    source,error:=resolve_source(loader,path,allocator); if error!=.None { return {},error }; defer delete(source,allocator)
    return compile(compiler,source,selections,constants,allocator)
}
/// Resolves quoted local and bracketed nearest-common includes once within the source root.
resolve_source :: proc(loader:Source_Loader,path:string,allocator:=context.allocator)->(string,Error) {
    if loader.read==nil { return "",.Invalid_Request }
    normalized,valid:=source_path_normalize("",path,allocator); if !valid { return "",.Invalid_Request }; defer delete(normalized,allocator)
    seen:=make(map[string]bool,allocator); defer { for key in seen { delete(key,allocator) }; delete(seen) }
    builder:strings.Builder; strings.builder_init(&builder,allocator); defer strings.builder_destroy(&builder)
    budget:=MAX_REQUEST
    error:=source_expand(loader,normalized,&seen,&builder,&budget,0,allocator)
    if error!=.None { return "",error }
    return strings.clone(strings.to_string(builder),allocator),.None
}
@(private="package")
source_path_normalize :: proc(base,path:string,allocator:mem.Allocator)->(string,bool) {
    if path=="" || path[0]=='/' || strings.contains(path,"\\") || strings.contains(path,":") || strings.contains(path,"\x00") { return "",false }
    joined:=path; if base!="" { joined=strings.concatenate({base,"/",path},allocator) }; defer { if base!="" { delete(joined,allocator) } }
    parts:=strings.split(joined,"/",allocator); defer delete(parts,allocator)
    stack:=make([dynamic]string,allocator); defer delete(stack)
    for part in parts {
        if part=="" || part=="." { continue }
        if part==".." { if len(stack)==0 { return "",false }; resize(&stack,len(stack)-1); continue }
        append(&stack,part)
    }
    if len(stack)==0 { return "",false }
    return strings.join(stack[:],"/",allocator),true
}
@(private="package")
source_directory :: proc(path:string)->string { at:=strings.last_index(path,"/"); if at<0 { return "" }; return path[:at] }
@(private="package")
source_expand :: proc(loader:Source_Loader,path:string,seen:^map[string]bool,builder:^strings.Builder,budget:^int,depth:int,allocator:mem.Allocator)->Error {
    if depth>64 || len(seen^)>4096 { return .Invalid_Request }
    if seen^[path] { return .None }
    seen^[strings.clone(path,allocator)]=true
    data,read:=loader.read(loader.state,path,allocator)
    if !read { return .Load_Failed }; defer delete(data,allocator)
    if len(data)==0 || len(data)>budget^ { return .Invalid_Request }; budget^-=len(data)
    lines:=strings.split(string(data),"\n",allocator); defer delete(lines,allocator)
    for line in lines {
        trimmed:=strings.trim_space(line)
        directive:=""
        if strings.has_prefix(trimmed,"#include ") { directive=strings.trim_space(trimmed[len("#include "):]) }
        else if strings.has_prefix(trimmed,"//include ") { directive=strings.trim_space(trimmed[len("//include "):]) }
        else { strings.write_string(builder,line); strings.write_byte(builder,'\n'); continue }
        if len(directive)<3 { return .Invalid_Request }
        target:=""; accepted:=false
        if directive[0]=='"' && directive[len(directive)-1]=='"' {
            target,accepted=source_path_normalize(source_directory(path),directive[1:len(directive)-1],allocator)
        } else if directive[0]=='<' && directive[len(directive)-1]=='>' {
            base:=source_directory(path)
            for {
                common:=strings.concatenate({base,"/common"},allocator); if base=="" { delete(common,allocator); common=strings.clone("common",allocator) }
                candidate,valid:=source_path_normalize(common,directive[1:len(directive)-1],allocator); delete(common,allocator)
                if !valid { return .Invalid_Request }
                probe,exists:=loader.read(loader.state,candidate,allocator); delete(probe,allocator)
                if exists { target=candidate; accepted=true; break }; delete(candidate,allocator)
                if base=="" { break }; base=source_directory(base)
            }
        }
        if !accepted { return .Invalid_Request }
        error:=source_expand(loader,target,seen,builder,budget,depth+1,allocator); delete(target,allocator)
        if error!=.None { return error }
        strings.write_byte(builder,'\n')
    }
    if len(strings.to_string(builder^))>MAX_REQUEST { return .Invalid_Request }
    return .None
}
