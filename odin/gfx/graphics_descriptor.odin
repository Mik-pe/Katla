//! Graphics descriptor content defines immutable native pipeline reuse without retaining a cache owner.
package gfx
import "core:mem"
import "core:strings"

@(private="package")
graphics_values_equal :: proc(a,b:[]$T)->bool {
    if len(a)!=len(b) { return false };for value,index in a { if value!=b[index] { return false } };return true
}
/// Compares compilation data and complete render state independently of source allocation addresses.
graphics_desc_equal :: proc(a,b:Graphics_Desc)->bool {
    return a.vertex_entry==b.vertex_entry && a.fragment_entry==b.fragment_entry && a.vertex_metal_entry==b.vertex_metal_entry && a.fragment_metal_entry==b.fragment_metal_entry && a.vertex_metal_source==b.vertex_metal_source && a.fragment_metal_source==b.fragment_metal_source &&
        graphics_values_equal(a.buffers,b.buffers) && graphics_values_equal(a.images,b.images) && graphics_values_equal(a.samplers,b.samplers) &&
        a.vertex_sizes_index==b.vertex_sizes_index && a.fragment_sizes_index==b.fragment_sizes_index && a.vertex_sizes_words==b.vertex_sizes_words && a.fragment_sizes_words==b.fragment_sizes_words &&
        graphics_values_equal(a.vertex_spirv,b.vertex_spirv) && graphics_values_equal(a.fragment_spirv,b.fragment_spirv) &&
        graphics_values_equal(a.vertex.attributes,b.vertex.attributes) && graphics_values_equal(a.vertex.buffers,b.vertex.buffers) &&
        a.stencil==b.stencil && a.depth_bias==b.depth_bias && a.wireframe==b.wireframe && graphics_values_equal(a.colors,b.colors) && a.depth==b.depth && a.topology==b.topology && a.cull==b.cull && a.front_counter_clockwise==b.front_counter_clockwise
}
/// Owns every source string, binary and reflected array used by a retained native pipeline.
graphics_desc_clone :: proc(desc:Graphics_Desc,allocator:mem.Allocator=context.allocator)->Graphics_Desc {
    result:=desc
    result.vertex_entry=strings.clone(desc.vertex_entry,allocator);result.fragment_entry=strings.clone(desc.fragment_entry,allocator)
    result.vertex_metal_entry=strings.clone(desc.vertex_metal_entry,allocator);result.fragment_metal_entry=strings.clone(desc.fragment_metal_entry,allocator)
    result.vertex_metal_source=strings.clone(desc.vertex_metal_source,allocator);result.fragment_metal_source=strings.clone(desc.fragment_metal_source,allocator)
    result.buffers=clone_slice(desc.buffers,allocator);result.images=clone_slice(desc.images,allocator);result.samplers=clone_slice(desc.samplers,allocator)
    result.vertex_spirv=clone_slice(desc.vertex_spirv,allocator);result.fragment_spirv=clone_slice(desc.fragment_spirv,allocator)
    result.vertex.attributes=clone_slice(desc.vertex.attributes,allocator);result.vertex.buffers=clone_slice(desc.vertex.buffers,allocator);result.colors=clone_slice(desc.colors,allocator)
    return result
}
/// Frees a descriptor clone only when its final native owner retires.
graphics_desc_destroy :: proc(desc:^Graphics_Desc,allocator:mem.Allocator=context.allocator) {
    delete(desc.vertex_entry,allocator);delete(desc.fragment_entry,allocator);delete(desc.vertex_metal_entry,allocator);delete(desc.fragment_metal_entry,allocator)
    delete(desc.vertex_metal_source,allocator);delete(desc.fragment_metal_source,allocator)
    delete(desc.buffers,allocator);delete(desc.images,allocator);delete(desc.samplers,allocator);delete(desc.vertex_spirv,allocator);delete(desc.fragment_spirv,allocator)
    delete(desc.vertex.attributes,allocator);delete(desc.vertex.buffers,allocator);delete(desc.colors,allocator);desc^={}
}
