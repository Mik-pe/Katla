//! Tool image receipts describe accepted revisions and their effective role transfer.
package app

import ecs "../ecs"
import agent "../agent"
import editor "../editor"
import image "../image"
import "core:strings"
import "core:encoding/json"
import "core:fmt"

/// Native hosts supply actual accepted binding status; CPU-only authoring retains an unknown result.
Material_Native_Inspection :: struct {state:rawptr,using_fallback:proc(rawptr,ecs.Entity_Id,int)->(bool,bool)}
/// Returns an owned binding DTO; neutral and inheritance have no decoded image allocation.
material_binding_inspect :: proc(owner:^Authoring,source:Texture_Source,prepared:^Material_Image,role:int)->json.Value {
    context.allocator=owner.world.allocator
    encoded,valid:=material_source_encode(owner,source,""); if !valid { return nil }
    fields:=make(json.Object,owner.world.allocator); scene_json_put(&fields,"source",encoded)
    if prepared!=nil && prepared.image.width>0 {
        width,height:=prepared.image.width,prepared.image.height
        mips:u32=1; extent:=max(width,height); for extent>1 { extent>>=1; mips+=1 }
        color:=role==0 || role==4
        decoded:="Rgba8"; native:="Rgba8UnormSrgb" if color else "Rgba8Unorm"; transfer:="srgb" if color else "linear"
        switch prepared.image.format {
        case .RGBA8:
        case .RGBA16: decoded="Rgba16"; native="Rgba16Float" if color else "Rgba16Unorm"
        case .RGBA32_Float: decoded="Rgba32Float"; native="Rgba16Float"; transfer="linear"
        }
        scene_json_put(&fields,"image",trigger_json_value(struct{width,height,mip_levels:u32,gpu_format,decoded_format,source_color_space:string,digest:[32]byte}{width,height,mips,native,decoded,transfer,prepared.digest}))
    } else { scene_json_put(&fields,"image",nil) }
    scene_json_put(&fields,"sampled_color_space",trigger_json_value("linear"))
    return fields
}
/// Returns authored choices and source provenance without reloading accepted images.
material_provenance :: proc(owner:^Authoring,id:ecs.Entity_Id)->json.Value {
    context.allocator=owner.world.allocator
    choices,_:=ecs.get_component(&owner.world,id,Texture_Assignments)
    images:=ecs.get_component_mut(&owner.world,id,Material_Images)
    authored:=make(json.Object,owner.world.allocator)
    for source,index in texture_assignments_roles(&choices) {
        prepared:^Material_Image; if images!=nil { prepared=&images.roles[index] }
        scene_json_put(&authored,agent.material_texture_role_name(agent.Material_Texture_Role(index)),material_binding_inspect(owner,source^,prepared,index))
    }
    imported:json.Value
    tangent:json.Value=trigger_json_value(struct{kind:string}{"reconstructed_from_current_uv"})
    model,material,error:=material_imported_material(owner,id)
    if error==.None && model!=nil {
        entries:=make(json.Object,owner.world.allocator); imported=entries
        _,primitive,primitive_error:=material_imported_primitive(owner,id)
        if primitive_error==.None && primitive!=nil {
            json.destroy_value(tangent)
            if primitive.tangent_generated {
                generation:=primitive.tangent_uv; original:=Uv_Transform{u32(generation.texcoord),generation.offset,generation.rotation,generation.uv_scale}
                sampling,sampling_error:=material_effective_sampling(owner,id); current:=original; if sampling_error==.None { current=sampling.normal.uv }
                tangent=trigger_json_value(struct{kind:string,original_generation_uv,current_normal_uv:Uv_Transform}{"generated_mikktspace" if original==current else "reconstructed_from_current_uv",original,current})
            } else { tangent=trigger_json_value(struct{kind:string}{"provided"}) }
        }
        for view,index in material_imported_views(material) {
            descriptor:=make(json.Object,owner.world.allocator)
            active:=texture_assignments_roles(&choices)[index].kind==.Inherit
            scene_json_put(&descriptor,"active",trigger_json_value(active))
            if view.texture>=0 && int(view.texture)<len(model.model.textures) && model.model.textures[view.texture].image>=0 && int(model.model.textures[view.texture].image)<len(model.model.images) {
                texture:=model.model.textures[view.texture]; loaded:=model.model.images[texture.image]
                source:=Texture_Source{kind=.GltfImage,root=model.source.root,path=model.source.path,image_index=u32(texture.image)}
                encoded,valid:=material_source_encode(owner,source,""); if valid { scene_json_put(&descriptor,"source",encoded) }
                scene_json_put(&descriptor,"image_source",strings.clone(loaded.origin if loaded.origin!="" else loaded.source_path,owner.world.allocator))
                decoded,decode_error:=image.texture_image_decode(loaded.encoded,owner.world.allocator)
                if decode_error==.None {
                    mips:u32=1; extent:=max(decoded.width,decoded.height); for extent>1 { extent>>=1; mips+=1 }
                    color:=index==0 || index==4 || (index==2 && material.workflow==.Specular_Glossiness)
                    precision:="Rgba8"; if decoded.format==.RGBA16 { precision="Rgba16" }; if decoded.format==.RGBA32_Float { precision="Rgba32Float" }
                    scene_json_put(&descriptor,"width",json.Integer(decoded.width)); scene_json_put(&descriptor,"height",json.Integer(decoded.height)); scene_json_put(&descriptor,"mip_levels",json.Integer(mips))
                    scene_json_put(&descriptor,"decoded_format",strings.clone(precision,owner.world.allocator)); scene_json_put(&descriptor,"source_color_space",strings.clone("srgb" if color && decoded.format!=.RGBA32_Float else "linear",owner.world.allocator))
                    image.texture_image_destroy(&decoded)
                }
                known:=false; fallback:=false
                if native:=ecs.get_resource_mut(&owner.world,Material_Native_Inspection); native!=nil && native.using_fallback!=nil { fallback,known=native.using_fallback(native.state,id,index) }
                scene_json_put(&descriptor,"using_fallback",trigger_json_value(fallback) if known else nil)
                scene_json_put(&descriptor,"metadata_origin",strings.clone("loaded_model_revision",owner.world.allocator))
            } else { scene_json_put(&descriptor,"source",nil); scene_json_put(&descriptor,"using_fallback",trigger_json_value(true)) }
            scene_json_put(&descriptor,"sampled_color_space",strings.clone("linear",owner.world.allocator))
            scene_json_put(&entries,agent.material_texture_role_name(agent.Material_Texture_Role(index)),descriptor)
        }
    }
    result:=make(json.Object,owner.world.allocator); scene_json_put(&result,"authored_textures",authored); scene_json_put(&result,"imported_textures",imported)
    scene_json_put(&result,"tangent_basis",tangent); scene_json_put(&result,"texture_assignment_editable",trigger_json_value(scene_material_editable(owner,id)))
    return result
}
@(private="package")
material_texture_receipt :: proc(owner:^Authoring,op:agent.Material_Set_Texture,command:^Scene_Action_Command,source:Texture_Source)->([]byte,bool) {
    context.allocator=owner.world.allocator
    rows:=make(json.Array,0,owner.world.allocator); defer json.destroy_value(rows)
    for row in command.rows {
        object:=make(json.Object,owner.world.allocator); scene_json_put(&object,"entity_id",fmt.aprintf("%d",u64(row.entity)))
        for values,index in ([2][]editor.Component_Snapshot{row.before[:],row.after[:]}) {
            assignments:=scene_action_component_find(values,owner.registry.entries["MaterialTextures"])
            images:=scene_action_component_find(values,owner.registry.entries["MaterialImages"])
            choices:Texture_Assignments; if assignments!=nil { choices=(cast(^Texture_Assignments)assignments)^ }
            prepared:^Material_Image; if images!=nil { prepared=&(cast(^Material_Images)images).roles[int(op.role)] }
            scene_json_put(&object,"before" if index==0 else "texture",material_binding_inspect(owner,texture_assignments_roles(&choices)[int(op.role)]^,prepared,int(op.role)))
        }
        append(&rows,object)
    }
    requested,valid:=material_source_encode(owner,source,""); if !valid { return nil,false }; defer json.destroy_value(requested)
    bytes,error:=json.marshal(struct{role:string,requested_source:json.Value,materials:json.Array,batch_atomic,sampling_preserved,factors_preserved:bool}{agent.material_texture_role_name(op.role),requested,rows,true,true,true},allocator=owner.world.allocator)
    return bytes,error==nil
}
