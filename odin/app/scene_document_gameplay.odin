//! Scene gameplay descriptors become registered owned components before shared reference staging.
package app

import scene "../agent/scene"
import agent "../agent"
import editor "../editor"
import "core:encoding/json"
import "core:strings"
import "core:fmt"

@(private="package")
scene_gameplay_present :: proc(fields:json.Object,name:string)->(json.Value,bool) { value,present:=fields[name]; if !present { return nil,false }; if _,is_null:=value.(json.Null); is_null { return nil,false }; return value,true }
@(private="package")
scene_gameplay_required :: proc(object:json.Object,names:[]string)->bool { for name in names { if _,present:=object[name]; !present { return false } }; return true }
@(private="package")
scene_gameplay_number :: proc(object:json.Object,name:string,target:^f32)->bool { value,present:=object[name]; if !present { return true }; number,valid:=recipe_number(value); if valid { target^=number }; return valid }
@(private="package")
scene_gameplay_bool :: proc(object:json.Object,name:string,target:^bool)->bool { value,present:=object[name]; if !present { return true }; boolean,valid:=value.(bool); if valid { target^=boolean }; return valid }
@(private="package")
scene_gameplay_vector :: proc(object:json.Object,name:string,target:^[3]f32)->bool { value,present:=object[name]; if !present { return true }; vector,valid:=recipe_vector(value,3); if valid { copy(target[:],vector[:]) }; return valid }
@(private="package")
scene_gameplay_u32 :: proc(object:json.Object,name:string,target:^u32)->bool { value,present:=object[name]; if !present { return true }; number,valid:=value.(json.Integer); if !valid || number<0 || u64(number)>u64(max(u32)) { return false }; target^=u32(number); return true }
@(private="package")
scene_gameplay_shape :: proc(value:json.Value)->(Physics_Shape,bool) {
    kind,payload,variant_valid:=scene_variant(value); if !variant_valid { return {},false }
    if tuple,is_tuple:=payload.(json.Array); is_tuple && len(tuple)==1 { payload=tuple[0] }
    shape:Physics_Shape; transferred:=false; defer { if !transferred { delete(shape.heights) } }
    switch kind {
    case "Box": vector,valid:=recipe_vector(payload,3); if !valid { return {},false }; shape.kind=.Box; copy(shape.half_extents[:],vector[:])
    case "Sphere": radius,valid:=recipe_number(payload); if !valid { return {},false }; shape.kind=.Sphere; shape.radius=radius
    case "Capsule": object,valid:=payload.(json.Object); if !valid || !recipe_keys(object,{"radius","half_height"}) || !scene_gameplay_required(object,{"radius","half_height"}) || !scene_gameplay_number(object,"radius",&shape.radius) || !scene_gameplay_number(object,"half_height",&shape.half_height) { return {},false }; shape.kind=.Capsule
    case "Heightfield":
        object,valid:=payload.(json.Object)
        if !valid || !recipe_keys(object,{"rows","cols","heights"}) || !scene_gameplay_required(object,{"rows","cols","heights"}) || !scene_gameplay_u32(object,"rows",&shape.rows) || !scene_gameplay_u32(object,"cols",&shape.cols) { return {},false }
        heights,is_heights:=object["heights"].(json.Array)
        if !is_heights || shape.rows<2 || shape.cols<2 || u64(shape.rows)*u64(shape.cols)>1_000_000 || u64(len(heights))!=u64(shape.rows)*u64(shape.cols) { return {},false }
        shape.kind=.Heightfield; shape.heights=make([]f32,len(heights))
        for height_value,i in heights { number,finite:=recipe_number(height_value); if !finite { return {},false }; shape.heights[i]=number }
    case "Trimesh": if payload!=nil { return {},false }; shape.kind=.Trimesh
    case "ConvexHull": if payload!=nil { return {},false }; shape.kind=.ConvexHull
    case: return {},false
    }
    if !physics_body_valid(physics_body(shape)) { return {},false }; transferred=true; return shape,true
}
@(private="package")
scene_gameplay_animation :: proc(app:^Authoring,row:^Scene_Entity,value:json.Value)->editor.Scene_Error {
    object,is_object:=value.(json.Object)
    if !is_object || !recipe_keys(object,{"current_clip","playing","loop_animation","speed","time","duration","blending","target_clip","blend_weight","blend_time","blend_duration","target_time","target_duration","completed","target_completed","target_loop_animation","target_loop_count","loop_count"}) { return .Decode_Failed }
    if !scene_gameplay_required(object,{"playing","loop_animation","speed","time"}) { return .Decode_Failed }
    player:=animation_player_stopped(); player.blend_weight=0
    for name in ([2]string{"current_clip","target_clip"}) { if clip,present:=scene_gameplay_present(object,name); present { text,is_text:=clip.(string); if !is_text { return .Invalid_Field_Value }; if name=="current_clip" { player.clip=text; player.clip_present=true } else { player.target_clip=text; player.target_clip_present=true } } }
    if !scene_gameplay_bool(object,"playing",&player.playing) || !scene_gameplay_bool(object,"loop_animation",&player.looping) || !scene_gameplay_bool(object,"blending",&player.blending) || !scene_gameplay_bool(object,"completed",&player.completed) || !scene_gameplay_bool(object,"target_completed",&player.target_completed) || !scene_gameplay_bool(object,"target_loop_animation",&player.target_looping) { return .Decode_Failed }
    if !scene_gameplay_number(object,"speed",&player.speed) || !scene_gameplay_number(object,"time",&player.time) || !scene_gameplay_number(object,"duration",&player.duration) || !scene_gameplay_number(object,"blend_weight",&player.blend_weight) || !scene_gameplay_number(object,"blend_time",&player.blend_time) || !scene_gameplay_number(object,"blend_duration",&player.blend_duration) || !scene_gameplay_number(object,"target_time",&player.target_time) || !scene_gameplay_number(object,"target_duration",&player.target_duration) || !scene_gameplay_u32(object,"loop_count",&player.loop_count) || !scene_gameplay_u32(object,"target_loop_count",&player.target_loop_count) { return .Decode_Failed }
    for number in ([7]f32{player.time,player.duration,player.blend_weight,player.blend_time,player.blend_duration,player.target_time,player.target_duration}) { if !finite_nonnegative(number) { return .Invalid_Field_Value } }
    if !finite_nonnegative(abs(player.speed)) || player.blend_weight>1 || (player.blending && (player.target_clip=="" || player.blend_duration<=0)) || (player.playing && player.clip=="") { return .Invalid_Field_Value }
    return scene_row_component(app,row,"AnimationPlayer",player)
}
@(private="package")
scene_gameplay_rules :: proc(app:^Authoring,row:^Scene_Entity,value:json.Value)->editor.Scene_Error {
    tree,cloned:=scene_value_clone(value); if !cloned { return .Decode_Failed }; defer json.destroy_value(tree)
    rules,is_rules:=tree.(json.Array); if !is_rules { return .Decode_Failed }
    for rule in rules {
        object,is_object:=rule.(json.Object); if !is_object { return .Decode_Failed }
        if other,present:=scene_gameplay_present(object,"other_entity"); present { id,is_id:=scene_document_key(other); if !is_id || id==0 { return .Invalid_Field_Value }; json.destroy_value(other); object["other_entity"]=fmt.aprintf("%d",id) }
        actions,is_actions:=object["actions"].(json.Array); if !is_actions { return .Decode_Failed }
        for action in actions { action_object,is_action:=action.(json.Object); if !is_action { return .Decode_Failed }; if target,has_target:=action_object["target"].(json.Object); has_target { if reference,present:=target["entity"]; present { id,is_id:=scene_document_key(reference); if !is_id || id==0 { return .Invalid_Field_Value }; json.destroy_value(reference); target["entity"]=fmt.aprintf("%d",id) } } }
    }
    request,request_error:=json.marshal(struct {action,entity_id:string,rules:json.Value}{"set_rules","0",tree},allocator=app.world.allocator); if request_error!=nil { return .Decode_Failed }; defer delete(request,app.world.allocator)
    decoded,decode_error:=scene.decode_trigger(request,app.world.allocator); if decode_error!=.None { return .Decode_Failed }; defer scene.decoded_trigger_destroy(&decoded)
    fired:=make([]bool,len(decoded.operation.rules),app.world.allocator); defer delete(fired,app.world.allocator)
    return scene_row_component(app,row,"TriggerRules",Trigger_Rules{rules=decoded.operation.rules,fired=fired})
}
/// Decodes all supported gameplay fields strictly; unsupported runtime subsystems fail explicitly.
scene_builtin_components_decode :: proc(app:^Authoring,row:^Scene_Entity,fields:json.Object,origin:string="")->editor.Scene_Error {
    context.allocator=app.world.allocator
    if error:=light_scene_decode(app,row,fields); error!=.None { return error }
    if error:=audio_scene_decode(app,row,fields,origin); error!=.None { return error }
    if error:=perspective_scene_decode(app,row,fields); error!=.None { return error }
    if value,present:=scene_gameplay_present(fields,"animation"); present { if err:=scene_gameplay_animation(app,row,value); err!=.None { return err } }
    if value,present:=scene_gameplay_present(fields,"particle_emitter"); present {
        object,is_object:=value.(json.Object); if !is_object { return .Decode_Failed }
        normalized:=make(json.Object,app.world.allocator); defer delete(normalized); for key,item in object { normalized[key]=item }
        if shape,has_shape:=object["shape"]; has_shape { kind,payload,valid:=scene_variant(shape); if !valid || payload!=nil { return .Decode_Failed }; normalized["shape"]=kind }
        descriptor,valid:=particle_decode(normalized,app.world.allocator); if !valid { return .Invalid_Field_Value }; defer delete(descriptor.burst_queue)
        if err:=scene_row_component(app,row,"ParticleEmitter",Particle_Emitter{descriptor}); err!=.None { return err }
        // Loaded legacy documents may explicitly schedule a burst; disk export owns settings only.
        if len(descriptor.burst_queue)>0 {
            entry:=app.registry.entries["ParticleEmitter"]; source:=Particle_Emitter{descriptor}; component:=&row.components[len(row.components)-1]
            component.owned_entry=entry; component.owned_ops=entry.ops; component.owned_value=editor.editor_clone_value(entry,&source,app.world.allocator); component.wire_hash=scene_wire_hash(component.data)
        }
    }
    if value,present:=scene_gameplay_present(fields,"script"); present {
        object,is_object:=value.(json.Object); if !is_object || !recipe_keys(object,{"path"}) { return .Decode_Failed }
        path,root,valid:=scene_asset_path(app,object["path"],origin); if !valid { delete(path,app.world.allocator); return .Invalid_Field_Value }; defer delete(path,app.world.allocator)
        normalized,valid_name:=script_source_name(path,root,app.world.allocator); if !valid_name { return .Invalid_Field_Value }; defer delete(normalized,app.world.allocator)
        if err:=scene_row_component(app,row,"Script",Script_Component{path=normalized,root=root}); err!=.None { return err }
    }
    if value,present:=scene_gameplay_present(fields,"joint"); present { if err:=scene_gameplay_joint_decode(app,row,value); err!=.None { return err } }
    body:=physics_body(Physics_Shape{kind=.None},.Fixed); body.has_rigid_body=false; has_body,has_shape:=false,false; defer delete(body.shape.heights)
    if value,present:=scene_gameplay_present(fields,"rigid_body"); present {
        object,is_object:=value.(json.Object); if !is_object || !recipe_keys(object,{"kind","gravity_scale","ccd_enabled","linear_velocity"}) { return .Decode_Failed }
        kind,payload,valid:=scene_variant(object["kind"]); if !valid || payload!=nil { return .Decode_Failed }
        switch kind {
        case "Dynamic": body.body_type=.Dynamic
        case "Kinematic": body.body_type=.Kinematic
        case "Static": body.body_type=.Fixed
        case: return .Invalid_Field_Value
        }
        if !scene_gameplay_number(object,"gravity_scale",&body.gravity_scale) || !scene_gameplay_bool(object,"ccd_enabled",&body.ccd) || !scene_gameplay_vector(object,"linear_velocity",&body.linear_velocity) { return .Decode_Failed }; has_body=true; body.has_rigid_body=true
    }
    if value,present:=scene_gameplay_present(fields,"collider_shape"); present { shape,valid:=scene_gameplay_shape(value); if !valid { return .Invalid_Field_Value }; body.shape=shape; body.has_collider=true; has_body=true; has_shape=true }
    if value,present:=scene_gameplay_present(fields,"physics_material"); present {
        object,is_object:=value.(json.Object); if !is_object || !recipe_keys(object,{"friction","restitution","density"}) || !scene_gameplay_required(object,{"friction","restitution","density"}) || !scene_gameplay_number(object,"friction",&body.friction) || !scene_gameplay_number(object,"restitution",&body.restitution) || !scene_gameplay_number(object,"density",&body.density) { return .Decode_Failed }; has_body=true; body.has_material=true
    }
    if value,present:=scene_gameplay_present(fields,"collision_filter"); present {
        object,is_object:=value.(json.Object); if !is_object || !recipe_keys(object,{"layers","mask"}) || !scene_gameplay_required(object,{"layers","mask"}) || !scene_gameplay_u32(object,"layers",&body.layers) || !scene_gameplay_u32(object,"mask",&body.mask) { return .Decode_Failed }; has_body=true; body.has_filter=true
    }
    if value,present:=scene_gameplay_present(fields,"trigger_volume"); present { kind,payload,valid:=scene_variant(value); if !valid || kind!="TriggerVolumeDescriptor" || payload!=nil { return .Decode_Failed }; body.sensor=true; has_body=true; if err:=scene_row_component(app,row,"TriggerVolume",Trigger_Volume{}); err!=.None { return err } }
    if has_body { if !has_shape && body.sensor { return .Invalid_Operation }; if !physics_body_valid(body) { return .Invalid_Field_Value }; if err:=scene_row_component(app,row,"PhysicsBody",body); err!=.None { return err } }
    if value,present:=scene_gameplay_present(fields,"trigger_rules"); present { if err:=scene_gameplay_rules(app,row,value); err!=.None { return err } }
    if value,present:=scene_gameplay_present(fields,"velocity"); present {
        object,is_object:=value.(json.Object); component:Scene_Velocity
        if !is_object || !recipe_keys(object,{"velocity","acceleration"}) || !scene_gameplay_required(object,{"velocity","acceleration"}) || !scene_gameplay_vector(object,"velocity",&component.velocity) || !scene_gameplay_vector(object,"acceleration",&component.acceleration) { return .Decode_Failed }
        if err:=scene_row_component(app,row,"Velocity",component); err!=.None { return err }
    }
    return .None
}

@(private="package")
scene_gameplay_store :: proc(fields:^json.Object,name:string,value:json.Value) { fields^[strings.clone(name)]=value }
@(private="package")
scene_gameplay_shape_encode :: proc(shape:Physics_Shape)->json.Value {
    switch shape.kind {
    case .Box: return trigger_json_value(struct {Box:[3]f32}{shape.half_extents})
    case .Sphere: return trigger_json_value(struct {Sphere:f32}{shape.radius})
    case .Capsule: return trigger_json_value(struct {Capsule:struct {half_height,radius:f32}}{ {shape.half_height,shape.radius} })
    case .Heightfield: return trigger_json_value(struct {Heightfield:struct {rows,cols:u32,heights:[]f32}}{ {shape.rows,shape.cols,shape.heights} })
    case .Trimesh: return strings.clone("Trimesh")
    case .ConvexHull: return strings.clone("ConvexHull")
    case .None: return nil
    }; return nil
}
@(private="package")
scene_gameplay_rule_keys :: proc(value:json.Value)->bool {
    rules,is_rules:=value.(json.Array); if !is_rules { return false }
    for rule in rules {
        object,is_object:=rule.(json.Object); if !is_object { return false }
        if reference,present:=object["other_entity"]; present { text,is_text:=reference.(string); id,valid:=agent.parse_entity_id(text); if !is_text || !valid || id==0 { return false }; object["other_entity"]=scene_document_key_value(u64(id)); delete(text) }
        actions,is_actions:=object["actions"].(json.Array); if !is_actions { return false }
        for action in actions { action_object,is_action:=action.(json.Object); if !is_action { return false }; if target,has_target:=action_object["target"].(json.Object); has_target { if reference,present:=target["entity"]; present { text,is_text:=reference.(string); id,valid:=agent.parse_entity_id(text); if !is_text || !valid || id==0 { return false }; target["entity"]=scene_document_key_value(u64(id)); delete(text) } } }
    }
    return true
}
/// Encodes authored gameplay state with document keys and portable source descriptors.
scene_builtin_components_encode :: proc(app:^Authoring,row:Scene_Entity,fields:^json.Object,origin:string="")->editor.Scene_Error {
    context.allocator=app.world.allocator
    for name in ([8]string{"AnimationPlayer","ParticleEmitter","Script","PhysicsBody","TriggerVolume","TriggerRules","Velocity","PhysicsJoint"}) {
        present:=false; for component in row.components { if component.name==name { present=true; break } }; if !present { continue }
        value,decoded:=scene_row_owned_decode(app,row,name); defer scene_row_owned_destroy(app,name,value); if !decoded { return .Decode_Failed }
        switch name {
        case "AnimationPlayer":
            player:=(cast(^Animation_Player)value)^; current_clip,target_clip:json.Value=json.Null{},json.Null{}; if player.clip!="" || player.clip_present { current_clip=player.clip }; if player.target_clip!="" || player.target_clip_present { target_clip=player.target_clip }
            descriptor:=trigger_json_value(struct {current_clip:json.Value,playing,loop_animation:bool,speed,time,duration:f32,blending:bool,target_clip:json.Value,blend_weight,blend_time,blend_duration,target_time,target_duration:f32,completed,target_completed,target_loop_animation:bool,target_loop_count,loop_count:u32}{current_clip,player.playing,player.looping,player.speed,player.time,player.duration,player.blending,target_clip,player.blend_weight,player.blend_time,player.blend_duration,player.target_time,player.target_duration,player.completed,player.target_completed,player.target_looping,player.target_loop_count,player.loop_count})
            if descriptor==nil { return .Decode_Failed }; scene_gameplay_store(fields,"animation",descriptor)
        case "ParticleEmitter": descriptor:=particle_document((cast(^Particle_Emitter)value).descriptor); if descriptor==nil { return .Decode_Failed }; scene_gameplay_store(fields,"particle_emitter",descriptor)
        case "Script":
            script:=(cast(^Script_Component)value)^; path,valid:=scene_asset_reference(app,script.path,script.root,origin); if !valid { return .Invalid_Operation }
            descriptor:=trigger_json_value(struct {path:json.Value}{path}); json.destroy_value(path); if descriptor==nil { return .Decode_Failed }; scene_gameplay_store(fields,"script",descriptor)
        case "PhysicsBody":
            body:=(cast(^Physics_Body)value)^; if !physics_body_valid(body) { return .Invalid_Field_Value }; if !body.has_collider && body.sensor { return .Invalid_Operation }; kind:="Dynamic"; if body.body_type==.Kinematic { kind="Kinematic" } else if body.body_type==.Fixed { kind="Static" }
            if body.has_rigid_body { scene_gameplay_store(fields,"rigid_body",trigger_json_value(struct {kind:string,gravity_scale:f32,ccd_enabled:bool,linear_velocity:[3]f32}{kind,body.gravity_scale,body.ccd,body.linear_velocity})) }
            if body.has_collider { scene_gameplay_store(fields,"collider_shape",scene_gameplay_shape_encode(body.shape)) }
            if body.has_material { scene_gameplay_store(fields,"physics_material",trigger_json_value(struct {friction,restitution,density:f32}{body.friction,body.restitution,body.density})) }
            if body.has_filter { scene_gameplay_store(fields,"collision_filter",trigger_json_value(struct {layers,mask:u32}{body.layers,body.mask})) }
        case "TriggerVolume": scene_gameplay_store(fields,"trigger_volume",strings.clone("TriggerVolumeDescriptor"))
        case "TriggerRules":
            rules:=(cast(^Trigger_Rules)value).rules; descriptor:=trigger_wire_rules(rules); if !scene_gameplay_rule_keys(descriptor) { json.destroy_value(descriptor); return .Invalid_Operation }; scene_gameplay_store(fields,"trigger_rules",descriptor)
        case "PhysicsJoint": descriptor,error:=scene_gameplay_joint_encode((cast(^Physics_Joint)value)^); if error!=.None { return error }; scene_gameplay_store(fields,"joint",descriptor)
        case "Velocity": velocity:=(cast(^Scene_Velocity)value)^; scene_gameplay_store(fields,"velocity",trigger_json_value(velocity))
        }
    }
    return .None
}
