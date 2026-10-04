//! Independent directory capability owners preserve confined background read lifetimes.
package resources
import "core:strings"

/// Retains the actual directory handle rather than reopening a potentially replaced pathname.
root_clone :: proc(root:^Root,allocator:=context.allocator)->(Root,Error) {
    if root==nil || root.file==nil { return {},.Invalid_Path }
    file,error:=open_child_native(root.file,".",true); if error!=.None { return {},error }
    return {strings.clone(root.path,allocator),file,allocator},.None
}
