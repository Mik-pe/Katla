//! Shader-only edits preserve the application's packet ABI while native resource indices can change.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"

/// Logical selected-entry resources, IO and workgroup dimensions must match existing application packets.
shader_reload_interface_compatible :: proc(reference,candidate:^shader.Compiled)->bool {
    if reference==nil || candidate==nil || len(reference.entries)!=len(candidate.entries) { return false }
    for original,index in reference.entries {
        changed:=candidate.entries[index]
        if original.name!=changed.name || original.stage!=changed.stage || original.workgroup_size!=changed.workgroup_size || len(original.bindings)!=len(changed.bindings) || len(original.inputs)!=len(changed.inputs) || len(original.outputs)!=len(changed.outputs) { return false }
        for binding in original.bindings {
            matched:=false
            for value in changed.bindings {
                if binding.group!=value.group || binding.binding!=value.binding { continue }
                if binding.kind!=value.kind || binding.access!=value.access || binding.uniform!=value.uniform || binding.minimum_size!=value.minimum_size || binding.alignment!=value.alignment || binding.runtime_array!=value.runtime_array || binding.runtime_array_offset!=value.runtime_array_offset || binding.runtime_array_stride!=value.runtime_array_stride || binding.array_count!=value.array_count || binding.dimension!=value.dimension || binding.arrayed!=value.arrayed || binding.multisampled!=value.multisampled || binding.depth!=value.depth || binding.comparison!=value.comparison || binding.sample_type!=value.sample_type || binding.storage_format!=value.storage_format || binding.metal_kind!=value.metal_kind { return false }
                matched=true;break
            }
            if !matched { return false }
        }
        for set in 0..<2 {
            before:=original.inputs if set==0 else original.outputs
            after:=changed.inputs if set==0 else changed.outputs
            for value,io_index in before {
                other:=after[io_index]
                if value.location!=other.location || value.builtin!=other.builtin || value.scalar!=other.scalar || value.width!=other.width || value.components!=other.components || value.interpolation!=other.interpolation || value.sampling!=other.sampling || value.blend_source!=other.blend_source || value.per_primitive!=other.per_primitive { return false }
            }
        }
    }
    return true
}
/// Carries application-authored raster state into a descriptor adapted from the new reflected shader.
shader_reload_graphics_state :: proc(descriptor:gfx.Graphics_Desc)->adapter.Graphics_State {
    return {vertex=descriptor.vertex,stencil=descriptor.stencil,depth_bias=descriptor.depth_bias,wireframe=descriptor.wireframe,colors=descriptor.colors,depth=descriptor.depth,topology=descriptor.topology,cull=descriptor.cull,front_counter_clockwise=descriptor.front_counter_clockwise}
}
