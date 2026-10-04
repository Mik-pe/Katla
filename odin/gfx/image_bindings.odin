//! Fixed image descriptor arrays own their selected graph accesses in every immutable packet.
package gfx

import "core:mem"

@(private="package")
image_bindings_clone :: proc(bindings:[]Image_Binding,allocator:mem.Allocator)->[]Image_Binding {
    result:=make([]Image_Binding,len(bindings),allocator)
    for binding,index in bindings {
        result[index]=binding
        result[index].accesses=clone_slice(binding.accesses,allocator)
    }
    return result
}
@(private="package")
image_bindings_destroy :: proc(bindings:[]Image_Binding,allocator:mem.Allocator) {
    for binding in bindings { delete(binding.accesses,allocator) }
    delete(bindings,allocator)
}
