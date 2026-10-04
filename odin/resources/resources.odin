//! Bounded resource discovery and reads anchored to an explicit retained directory.
package resources

import "core:mem"
import "core:os"
import "core:strings"
import "core:slice"
import "core:unicode/utf8"

/// Distinguishes invalid paths, unavailable native confinement, I/O and allocation budgets.
Error :: enum { None, Invalid_Path, Unsupported_Platform, IO, Not_Regular, Limit, Invalid_UTF8 }
/// Owns an absolute diagnostic path and a retained directory handle.
Root :: struct { path:string, file:^os.File, allocator:mem.Allocator }
/// Owns sorted resource-relative matches; no names are guessed or synthesized.
Search_Result :: struct { assets:[dynamic]string, total:int, truncated:bool, allocator:mem.Allocator }
MAX_ENTRIES :: 20_000
MAX_BYTES :: 64*1024*1024

/// Rejects traversal, absolute paths, drive prefixes, backslashes, empty segments and NULs.
valid_relative_path :: proc(path:string,allow_empty:=false)->bool {
    if path=="" { return allow_empty }
    remaining:=path
    for part in strings.split_iterator(&remaining,"/") {
        if part=="" || part=="." || part==".." { return false }
        for character in part { if character==0 || character=='\\' || character==':' { return false } }
    }
    return path[0]!='/' && path[len(path)-1]!='/'
}

/// Captures an explicit resource root once, independent of later working-directory changes.
root_open :: proc(path:string,allocator:=context.allocator)->(Root,Error) {
    context.allocator=allocator
    absolute,err:=os.get_absolute_path(path,allocator); if err!=nil { return {},.IO }
    file,open_error:=root_open_native(absolute)
    if open_error!=.None { delete(absolute,allocator); return {},open_error }
    return {absolute,file,allocator},.None
}

/// Releases the directory and diagnostic path exactly once.
root_destroy :: proc(root:^Root) { if root.file!=nil { os.close(root.file) }; delete(root.path,root.allocator); root^={} }

/// Releases every owned match with the search's captured allocator.
search_result_destroy :: proc(result:^Search_Result) { for path in result.assets { delete(path,result.allocator) }; delete(result.assets); result^={} }

/// Matches all case-insensitive whitespace words and optional extensions, with explicit truncation.
search :: proc(root:^Root,query:string,extensions:[]string=nil,limit:=64,directory:="")->(Search_Result,Error) {
    if root.file==nil || limit<1 || limit>256 { return {},.Invalid_Path }
    context.allocator=root.allocator
    search_directory:^os.File
    if directory=="" {
        handle,open_error:=open_child_native(root.file,".",true)
        if open_error!=.None { return {},open_error }; search_directory=handle
    } else {
        if !valid_relative_path(directory) { return {},.Invalid_Path }
        handle,open_error:=open_relative_native(root,directory,true)
        if open_error!=.None { return {},open_error }; search_directory=handle
    }
    defer os.close(search_directory)
    result:=Search_Result{assets=make([dynamic]string,root.allocator),allocator=root.allocator}
    success:=false; defer { if !success { search_result_destroy(&result) } }
    words:=make([dynamic]string,root.allocator); defer { for word in words { delete(word,root.allocator) }; delete(words) }
    remaining:=query
    for word in strings.fields_iterator(&remaining) { append(&words,strings.to_lower(word,root.allocator)) }
    normalized:=make([dynamic]string,root.allocator); defer { for ext in normalized { delete(ext,root.allocator) }; delete(normalized) }
    for extension in extensions { append(&normalized,strings.to_lower(strings.trim_left(extension,"."),root.allocator)) }
    budget:=MAX_ENTRIES
    err:=search_visit(root,search_directory,directory,words[:],normalized[:],&result.assets,&budget,0)
    if err!=.None { return {},err }
    slice.sort(result.assets[:]); result.total=len(result.assets)
    if result.total>limit { for path in result.assets[limit:] { delete(path,root.allocator) }; resize(&result.assets,limit); result.truncated=true }
    success=true; return result,.None
}

@(private="package")
search_visit :: proc(root:^Root,dir:^os.File,prefix:string,words,extensions:[]string,matches:^[dynamic]string,budget:^int,depth:int)->Error {
    if depth>256 { return .Limit }
    iterator:=os.read_directory_iterator_create(dir); defer os.read_directory_iterator_destroy(&iterator)
    for info,_ in os.read_directory_iterator(&iterator) {
        if budget^==0 { return .Limit }; budget^-=1
        if info.type!=.Directory && info.type!=.Regular { continue }
        relative:=info.name
        if prefix!="" { relative=strings.concatenate({prefix,"/",info.name},root.allocator) }
        defer { if prefix!="" { delete(relative,root.allocator) } }
        if info.type==.Directory {
            child,err:=open_child_native(dir,info.name,true)
            if err!=.None { return err }; defer os.close(child)
            err=search_visit(root,child,relative,words,extensions,matches,budget,depth+1)
            if err!=.None { return err }
        } else {
            lower:=strings.to_lower(relative,root.allocator); defer delete(lower,root.allocator)
            accepted:=true; for word in words { if !strings.contains(lower,word) { accepted=false; break } }
            if len(extensions)>0 {
                dot:=strings.last_index(lower,"."); extension:=""; if dot>=0 { extension=lower[dot+1:] }
                extension_matches:=false; for ext in extensions { if extension==ext { extension_matches=true; break } }
                accepted=accepted && extension_matches
            }
            if accepted { append(matches,strings.clone(relative,root.allocator)) }
        }
    }
    if _,err:=os.read_directory_iterator_error(&iterator); err!=nil { return .IO }
    return .None
}

/// Reads a regular file below the retained root using bounded incremental I/O.
read_bytes :: proc(root:^Root,path:string,max_bytes:=MAX_BYTES)->([]byte,Error) {
    if !valid_relative_path(path) || max_bytes<0 || max_bytes>MAX_BYTES { return nil,.Invalid_Path }
    file,err:=open_relative_native(root,path); if err!=.None { return nil,err }; defer os.close(file)
    info,stat_error:=os.fstat(file,root.allocator)
    if stat_error!=nil { return nil,.IO }; defer os.file_info_delete(info,root.allocator)
    if info.type!=.Regular { return nil,.Not_Regular }
    if info.size>i64(max_bytes) { return nil,.Limit }
    bytes:=make([dynamic]byte,root.allocator); defer delete(bytes)
    buffer:[4096]byte
    for {
        count,read_error:=os.read(file,buffer[:])
        if count>max_bytes-len(bytes) { return nil,.Limit }
        if count>0 { append(&bytes,..buffer[:count]) }
        if read_error==.EOF || (read_error==nil && count==0) { break }
        if read_error!=nil { return nil,.IO }
    }
    return slice.clone(bytes[:],root.allocator),.None
}

/// Reads a regular UTF-8 file; binary assets use read_bytes through the same confined boundary.
read_text :: proc(root:^Root,path:string,max_bytes:=MAX_BYTES)->([]byte,Error) {
    bytes,err:=read_bytes(root,path,max_bytes)
    if err!=.None { return nil,err }
    if !utf8.valid_string(string(bytes)) { delete(bytes,root.allocator); return nil,.Invalid_UTF8 }
    return bytes,.None
}

/// Publishes validated bytes using a synced sibling temporary file and an atomic rename.
/// published=true with an error means rename committed but the directory sync failed.
write_atomic :: proc(root:^Root,path:string,data:[]byte)->(published:bool,error:Error) {
    if !valid_relative_path(path) { return false,.Invalid_Path }
    if len(data)>MAX_BYTES { return false,.Limit }
    return write_atomic_native(root,path,data)
}

/// Creates missing confined parent directories and atomically admits a new file only.
create_atomic :: proc(root:^Root,path:string,data:[]byte)->(published:bool,error:Error) {
    if root.file==nil || !valid_relative_path(path) { return false,.Invalid_Path }
    if len(data)>MAX_BYTES { return false,.Limit }
    if err:=make_parents_native(root,path); err!=.None { return false,err }
    return write_atomic_native(root,path,data,exclusive=true)
}
