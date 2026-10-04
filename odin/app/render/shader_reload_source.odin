//! Bounded UTF-8 source snapshots expand the same application include markers before cache admission.
package render

import "core:crypto/sha2"
import "core:mem"
import "core:os"
import "core:path/filepath"
import "core:strings"
import "core:unicode/utf8"

@(private="package")
Shader_Reload_Read :: struct { root:string,files:map[string][]byte }
@(private="package")
shader_reload_read_bounded :: proc(path:string,limit:i64,allocator:mem.Allocator)->([]byte,bool) {
    file,error:=os.open(path);if error!=nil { return nil,false };defer os.close(file)
    info,stat_error:=os.fstat(file,allocator);if stat_error!=nil { return nil,false };defer os.file_info_delete(info,allocator)
    if info.type!=.Regular || info.size<=0 || info.size>limit { return nil,false }
    bytes:=make([]byte,int(info.size),allocator);offset:=0
    for offset<len(bytes) { count,read_error:=os.read(file,bytes[offset:]);offset+=count;if count==0 || read_error!=nil { delete(bytes,allocator);return nil,false } }
    extra:[1]byte;count,_:=os.read(file,extra[:]);if count!=0 { delete(bytes,allocator);return nil,false }
    return bytes,true
}
@(private="package")
shader_reload_read :: proc(state:rawptr,path:string,allocator:mem.Allocator)->([]byte,bool) {
    source:=cast(^Shader_Reload_Read)state
    if cached,present:=source.files[path];present { result:=make([]byte,len(cached),allocator);copy(result,cached);return result,true }
    absolute,path_error:=filepath.join({source.root,path},allocator=allocator);if path_error!=nil { return nil,false };defer delete(absolute,allocator)
    data,read:=shader_reload_read_bounded(absolute,8*1024*1024,allocator);if !read { return nil,false };defer delete(data,allocator)
    if !utf8.valid_string(string(data)) { return nil,false }
    lines:=strings.split(string(data),"\n",allocator);defer delete(lines,allocator)
    builder:strings.Builder;strings.builder_init(&builder,allocator);defer strings.builder_destroy(&builder)
    for line in lines {
        text:=strings.trim_space(line)
        if strings.has_prefix(text,"// #include ") {
            name:=strings.trim_space(text[len("// #include "):])
            if name=="" || strings.contains_any(name,"/\\\"<>: \t") { return nil,false }
            strings.write_string(&builder,"//include \"");strings.write_string(&builder,name);strings.write_string(&builder,".wgsl\"\n")
        } else { strings.write_string(&builder,line);strings.write_byte(&builder,'\n') }
    }
    cached:=make([]byte,len(strings.to_string(builder)),allocator);copy(cached,transmute([]byte)strings.to_string(builder))
    source.files[strings.clone(path,allocator)]=cached
    result:=make([]byte,len(cached),allocator);copy(result,cached);return result,true
}
@(private="package")
shader_reload_read_destroy :: proc(source:^Shader_Reload_Read,allocator:mem.Allocator) { for key,bytes in source.files { delete(key,allocator);delete(bytes,allocator) };delete(source.files) }
@(private="package")
shader_reload_compiler_identity :: proc(service:^Shader_Reload_Service)->bool {
    info,error:=os.stat(service.compiler.executable,service.allocator);if error!=nil { return false };defer os.file_info_delete(info,service.allocator)
    if info.type!=.Regular || info.size<=0 || info.size>256*1024*1024 { return false }
    if service.compiler_seen && service.compiler_size==info.size && service.compiler_modified==info.modification_time && service.compiler_created==info.creation_time && service.compiler_inode==info.inode && service.compiler_device==info.device { return true }
    bytes,read:=shader_reload_read_bounded(service.compiler.executable,256*1024*1024,service.allocator);if !read { return false };defer delete(bytes,service.allocator)
    digest:sha2.Context_256;sha2.init_256(&digest);sha2.update(&digest,bytes);sha2.final(&digest,service.compiler_digest[:])
    service.compiler_size=info.size;service.compiler_modified=info.modification_time;service.compiler_created=info.creation_time;service.compiler_inode=info.inode;service.compiler_device=info.device;service.compiler_seen=true
    return true
}
