//! Authored lights share one application component contract across scene files and rendering.
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"

/// Parallel rays shine along direction; renderers illuminate surfaces from its opposite.
Scene_Directional_Light :: struct { direction:km.Vec3, color:[3]f32, intensity:f32 `min:"0"` }
/// Point-light position comes from the entity's exact world transform.
Scene_Point_Light :: struct { color:[3]f32, intensity:f32 `min:"0"`, range:f32 `min:"0"` }
/// Global ambient illumination remains an application resource rather than GPU state.
Scene_Environment :: struct { color:[3]f32, intensity:f32 }
/// Supplies the canonical directional-light defaults without implicit entity creation.
directional_light_default :: proc()->Scene_Directional_Light { return {{0,-1,0},{1,1,1},1} }
/// Supplies the canonical point-light defaults without implicit entity creation.
point_light_default :: proc()->Scene_Point_Light { return {{1,1,1},1,10} }
/// Registers optional light components and the authored ambient resource.
light_register :: proc(owner:^Authoring) {
    editor.editor_register(&owner.world,&owner.registry,"SceneSource",Scene_Builtin_Source{},spawn_default=false)
    editor.editor_register(&owner.world,&owner.registry,"PointLight",point_light_default(),spawn_default=false)
    editor.editor_register(&owner.world,&owner.registry,"DirectionalLight",directional_light_default(),spawn_default=false)
    if !ecs.contains_resource(&owner.world,Scene_Environment) { ecs.insert_resource(&owner.world,Scene_Environment{{0.1,0.1,0.1},0.1}) }
}
/// Checks finite linear RGB and nonnegative intensity before file or renderer preparation.
light_color_valid :: proc(color:[3]f32,intensity:f32)->bool { if !mesh_finite(intensity) || intensity<0 { return false }; for channel in color { if !mesh_finite(channel) || channel<0 || channel>1 { return false } }; return true }
/// Validates point-light attenuation independently of its entity placement.
point_light_valid :: proc(light:Scene_Point_Light)->bool { return light_color_valid(light.color,light.intensity) && mesh_finite(light.range) && light.range>0 }
/// Validates directional illumination without changing its authored direction.
directional_light_valid :: proc(light:Scene_Directional_Light)->bool { if !light_color_valid(light.color,light.intensity) { return false }; for axis in light.direction { if !mesh_finite(axis) { return false } }; return light.direction!={} }
