//! Optional authored editor icons preserve their descriptor through ordinary component extensions.
package app

import ecs "../ecs"
import editor "../editor"

Billboard_Icon :: enum { Lightbulb,Fire }
/// Authored colors use sRGB code values; editor composition converts RGB to linear.
Scene_Billboard :: struct { icon:Billboard_Icon,color:[4]f32 `inspect:"color"`,size:f32 `min:"0"` }
BILLBOARD_DEFAULT :: Scene_Billboard{color={1,1,1,1},size=1}
/// Registers the optional SDK descriptor without implicitly creating editor icons.
billboard_register :: proc(owner:^Authoring) { editor.editor_register(&owner.world,&owner.registry,"Billboard",BILLBOARD_DEFAULT,spawn_default=false) }
/// Rejects invalid icon, tint or scale before editor/native publication.
billboard_valid :: proc(value:Scene_Billboard)->bool {
    if value.icon>.Fire || !mesh_finite(value.size) || value.size<=0 { return false }
    for channel in value.color { if !mesh_finite(channel) || channel<0 || channel>1 { return false } }
    return true
}
/// Validates actual authored icons independently from automatic meshless light/emitter indicators.
billboard_scene_validate :: proc(owner:^Authoring,entities:[]ecs.Entity_Id)->editor.Scene_Error {
    for entity in entities { if value,present:=ecs.get_component(&owner.world,entity,Scene_Billboard); present && !billboard_valid(value) { return .Invalid_Field_Value } }
    return .None
}
