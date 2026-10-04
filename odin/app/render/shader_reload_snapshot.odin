//! Reloaded descriptors own their selected artifacts independently of watcher cancellation and future revisions.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import adapter "../../gfx/shader_adapter"
import "core:mem"
import "core:strings"
import "core:log"

@(private="package")
shader_reload_clone_io :: proc(source:[]shader.IO,allocator:mem.Allocator)->[]shader.IO {
    values:=make([]shader.IO,len(source),allocator); copy(values,source)
    for &value in values { value.name=strings.clone(value.name,allocator); value.builtin=strings.clone(value.builtin,allocator); value.interpolation=strings.clone(value.interpolation,allocator); value.sampling=strings.clone(value.sampling,allocator) }
    return values
}
/// Copies complete immutable entry metadata and target binaries, including all nested strings.
shader_reload_snapshot :: proc(source:^shader.Compiled,allocator:=context.allocator)->shader.Compiled {
    result:=shader.Compiled{compiler=strings.clone(source.compiler,allocator),message=strings.clone(source.message,allocator),entries=make([]shader.Entry,len(source.entries),allocator),allocator=allocator}
    for entry,i in source.entries {
        value:=entry
        value.name=strings.clone(entry.name,allocator); value.metal_name=strings.clone(entry.metal_name,allocator); value.metal_source=strings.clone(entry.metal_source,allocator)
        value.spirv=make([]u32,len(entry.spirv),allocator); copy(value.spirv,entry.spirv)
        value.bindings=make([]shader.Binding,len(entry.bindings),allocator); copy(value.bindings,entry.bindings)
        for &binding in value.bindings { binding.name=strings.clone(binding.name,allocator); binding.storage_format=strings.clone(binding.storage_format,allocator) }
        value.inputs=shader_reload_clone_io(entry.inputs,allocator); value.outputs=shader_reload_clone_io(entry.outputs,allocator)
        result.entries[i]=value
    }
    return result
}
@(private="package")
shader_reload_colors :: proc(source:[]gfx.Color_Target,allocator:mem.Allocator)->[]gfx.Color_Target { result:=make([]gfx.Color_Target,len(source),allocator); copy(result,source); return result }
@(private="package")
shader_reload_map :: proc(compiled:^shader.Compiled,descriptor:gfx.Graphics_Desc,allocator:mem.Allocator)->(adapter.Graphics,Shader_Reload_Error) {
    mapping,error:=adapter.graphics(compiled,descriptor.vertex_entry,descriptor.fragment_entry,shader_reload_graphics_state(descriptor),allocator)
    return mapping,.None if error==.None else .Prepare
}
@(private="package")
shader_reload_compute_map :: proc(compiled:^shader.Compiled,entry:string,allocator:mem.Allocator)->(adapter.Compute,Shader_Reload_Error) {
    mapping,error:=adapter.compute(compiled,entry,allocator)
    return mapping,.None if error==.None else .Prepare
}
@(private="package")
shader_reload_native_report :: proc(error:gfx.Gpu_Error,cleanup:bool) { if cleanup { log.error("Shader family native cleanup failed",error) } else { log.warn("Shader family native candidate failed",error) } }

Shader_Raster_Replacement :: struct { target:^gfx.Graphics_Pipeline_Handle,handle:gfx.Graphics_Pipeline_Handle }
Shader_Compute_Replacement :: struct { target:^gfx.Pipeline_Handle,handle:gfx.Pipeline_Handle }
/// Updates only future authored packet references; accepted native recordings retain immutable old owners.
shader_reload_graph_references :: proc(graph:^gfx.Graph,rasters:[]Shader_Raster_Replacement,computes:[]Shader_Compute_Replacement) {
    changed:=false
    for &pass in graph.passes {
        #partial switch &packet in pass.packet {
        case gfx.Render:
            for &phase in packet.phases { for replacement in rasters { if phase.pipeline==replacement.handle { phase.pipeline=replacement.target^; changed=true; break } } }
        case gfx.Dispatch:
            for replacement in computes { if packet.pipeline==replacement.handle { packet.pipeline=replacement.target^; changed=true; break } }
        case:
        }
    }
    if changed { graph.revision+=1 }
}
