//! Immediate directory inventories retain the same no-follow boundary used by actual asset reads.
package resources
import "core:mem"
import "core:os"
import "core:strings"
import "core:slice"
import "core:unicode/utf8"

Directory_Entry :: struct { name:string,directory:bool,size:i64 }
Directory_Result :: struct { entries:[dynamic]Directory_Entry,allocator:mem.Allocator }
/// Releases every inventory-owned filename.
directory_result_destroy :: proc(result:^Directory_Result) { for entry in result.entries { delete(entry.name,result.allocator) }; delete(result.entries); result^={} }
/// Returns sorted actual regular files and directories; links and invalid UTF-8 names are omitted.
list_directory :: proc(root:^Root,path:="")->(Directory_Result,Error) {
    if root.file==nil || !valid_relative_path(path,allow_empty=true) { return {},.Invalid_Path }
    context.allocator=root.allocator
    directory:^os.File; error:Error
    if path=="" { directory,error=open_child_native(root.file,".",true) } else { directory,error=open_relative_native(root,path,true) }
    if error!=.None { return {},error }; defer os.close(directory)
    result:=Directory_Result{entries=make([dynamic]Directory_Entry,root.allocator),allocator=root.allocator}
    accepted:=false; defer { if !accepted { directory_result_destroy(&result) } }
    inventory,inventory_error:=directory_entries_native(directory); if inventory_error!=.None { return {},inventory_error }; defer native_directory_destroy(&inventory)
    for info in inventory.entries {
        if info.link || (!info.regular && !info.directory) || !utf8.valid_string(info.name) { continue }
        append(&result.entries,Directory_Entry{strings.clone(info.name,root.allocator),info.directory,info.size})
    }
    slice.sort_by(result.entries[:],proc(a,b:Directory_Entry)->bool {
        if a.directory!=b.directory { return a.directory }
        lower_a:=strings.to_lower(a.name); defer delete(lower_a); lower_b:=strings.to_lower(b.name); defer delete(lower_b)
        if lower_a==lower_b { return a.name<b.name }; return lower_a<lower_b
    })
    accepted=true; return result,.None
}
