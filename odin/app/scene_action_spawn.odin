//! Spawn proposals own prepared primitive or imported geometry before scene admission.
package app

import ecs "../ecs"
import editor "../editor"
import km "../math"
import "core:strings"
import "core:path/filepath"

@(private="package")
scene_action_spawn_proposal :: proc(owner:^Authoring,id:ecs.Entity_Id,op:editor.Scene_Op,key:u64)->editor.Scene_Error {
    for axis in op.position { if !mesh_finite(axis) { return .Invalid_Field_Value } }
    for axis in op.rotation { if !mesh_finite(axis) { return .Invalid_Field_Value } }
    for axis in op.scale { if !mesh_finite(axis) || axis==0 { return .Invalid_Field_Value } }
    rotation:=km.quat_mul(km.quat_mul(km.quat_axis_angle(km.VEC3_Z,op.rotation[2]),km.quat_axis_angle(km.VEC3_Y,op.rotation[1])),km.quat_axis_angle(km.VEC3_X,op.rotation[0]))
    ecs.add_component(&owner.world,id,Scene_Transform{km.Transform{position=op.position,rotation=rotation,scale=op.scale}})
    ecs.add_component(&owner.world,id,Scene_Key{key})
    name:=op.name; if name=="" { name="Entity" }; if op.kind==.Spawn_Model && op.name=="" { name=op.path }
    ecs.add_component(&owner.world,id,Scene_Name{strings.clone(name,owner.world.allocator)})
    root:=Mesh_Path_Root.Resource; if op.project_asset { root=.Project }; if filepath.is_abs(op.path) { root=.File }
    material:=Surface_Material{roughness=0.5,ao=1}
    if op.kind==.Spawn_Model {
        extension:=strings.to_lower(filepath.ext(op.path),owner.world.allocator); defer delete(extension,owner.world.allocator)
        if extension==".stl" || extension==".katmesh" {
            kind:=Mesh_Source_Kind.Stl; if extension==".katmesh" { kind=.Recipe }
            mesh,error:=scene_mesh_prepare(owner,{kind=kind,path=op.path,root=root}); if error!=.None { return .Invalid_Operation }
            ecs.add_component(&owner.world,id,mesh)
        } else {
            model,error:=scene_model_prepare(owner,{path=op.path,root=root}); if error!=.None { return .Invalid_Operation }
            ecs.add_component(&owner.world,id,model)
            entry:=owner.registry.entries["AnimationModel"]
            cloned:=editor.editor_clone_value(entry,&model.model.animation,owner.world.allocator)
            assert(ecs.insert_component_value(&owner.world,id,entry.T,cloned)); free(cloned,owner.world.allocator)
            ecs.add_component(&owner.world,id,animation_player_stopped())
            if op.default_animation!="" { if error:=animation_play(&owner.world,id,op.default_animation,0,true,1); error!=.None { return error } }
            material.metallic=1; material.roughness=1
        }
    } else {
        descriptor:=""
        switch op.shape {
        case "","cube": descriptor=`{"kind":"cube","size":[1,1,1]}`
        case "sphere": descriptor=`{"kind":"sphere","radius":0.5,"segments":32,"rings":16}`
        case "plane": descriptor=`{"kind":"plane","width":5,"height":5}`
        case "cylinder": descriptor=`{"kind":"cylinder","height":1,"radius":0.5,"segments":32}`
        case "cone": descriptor=`{"kind":"cone","height":1,"radius":0.5,"segments":32}`
        case "torus": descriptor=`{"kind":"torus","radius":0.7,"tube_radius":0.2,"segments":32,"tube_segments":16}`
        case: return .Invalid_Operation
        }
        mesh,error:=scene_mesh_prepare(owner,{kind=.Geometry,geometry=transmute([]byte)descriptor})
        if error!=.None { return .Invalid_Operation }; ecs.add_component(&owner.world,id,mesh)
    }
    ecs.add_component(&owner.world,id,material)
    return .None
}
