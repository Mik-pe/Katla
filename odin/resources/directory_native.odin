//! Native directory entries are enumerated through the retained directory owner.
package resources
import "core:mem"

@(private="package")
Native_Entry :: struct {name:string,directory,regular,link:bool,size:i64}
@(private="package")
Native_Directory :: struct {entries:[dynamic]Native_Entry,allocator:mem.Allocator}
@(private="package")
native_directory_destroy :: proc(directory:^Native_Directory) { for entry in directory.entries { delete(entry.name,directory.allocator) }; delete(directory.entries); directory^={} }
