//! Editor and tools inspect one effective surface through the same imported baseline.
package app

import agent "../agent"
import ecs "../ecs"
import editor "../editor"
import "core:strings"

/// Source paths are owned by this snapshot; image choices remain separate from role policies.
Material_Inspector :: struct {
    values:agent.Material_Values,
    sampling:Material_Sampling,
    sources:[5]Texture_Source,
    uv_available:[2]bool,
}
/// Releases one read snapshot after UI or tool output consumes its borrowed fields.
material_inspector_destroy :: proc(value:^Material_Inspector,allocator:=context.allocator) { for source in value.sources { delete(source.path,allocator) }; value^={} }
/// Reads effective factors and imported sampling while retaining authored source choices.
material_inspector_read :: proc(owner:^Authoring,id:ecs.Entity_Id)->(Material_Inspector,editor.Scene_Error) {
    if !scene_material_editable(owner,id) { return {},.Component_Not_Found }
    captured,error:=material_asset_capture(owner,id); if error!=.None { return {},error }; defer material_asset_destroy(&captured,owner.world.allocator)
    values,values_error:=material_entity_values(owner,id); if values_error!=.None { return {},values_error }
    result:=Material_Inspector{values=values,sampling=captured.material.sampling}
    if sources,present:=ecs.get_component(&owner.world,id,Texture_Assignments); present {
        for source,index in ([5]Texture_Source{sources.albedo,sources.normal,sources.metallic_roughness,sources.occlusion,sources.emission}) { result.sources[index]=source; result.sources[index].path=strings.clone(source.path,owner.world.allocator) }
    }
    for coord in 0..<2 { result.uv_available[coord]=material_target_uv(owner,id,.Albedo,u32(coord)) }
    return result,.None
}
