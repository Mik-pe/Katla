#+build darwin, linux
//! Unix inventory uses the already retained directory descriptor.
package resources
import "core:os"
import "core:strings"

@(private="package")
directory_entries_native :: proc(file:^os.File,budget:^int=nil)->(Native_Directory,Error) {
    local_budget:=MAX_ENTRIES; remaining_budget:=budget; if remaining_budget==nil { remaining_budget=&local_budget }
    result:=Native_Directory{entries=make([dynamic]Native_Entry,context.allocator),allocator=context.allocator}
    accepted:=false; defer { if !accepted { native_directory_destroy(&result) } }
    iterator:=os.read_directory_iterator_create(file); defer os.read_directory_iterator_destroy(&iterator)
    for info,_ in os.read_directory_iterator(&iterator) {
        if remaining_budget^==0 { return {},.Limit }; remaining_budget^-=1
        append(&result.entries,Native_Entry{strings.clone(info.name),info.type==.Directory,info.type==.Regular,info.type==.Symlink,info.size})
    }
    if _,error:=os.read_directory_iterator_error(&iterator); error!=nil { return {},.IO }
    accepted=true; return result,.None
}
