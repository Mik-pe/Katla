//! Durable emitter descriptors and bounded commands retain no native graphics handle.
package app
import ecs "../ecs"
import editor "../editor"
import "core:encoding/json"
import "core:math"
import "core:mem"
import "core:slice"

Particle_Shape :: enum { Point, Line, Circle, Sphere, Box }
/// Serializable linear-color emitter parameters; the render owner consumes accepted bursts.
Particle_Descriptor :: struct {
    emit_rate:f32 `min:"0" speed:"1"`,
    base_lifetime:f32 `min:"0.001" speed:"0.05"`,
    lifetime_variation:f32 `min:"0" max:"1" speed:"0.01"`,
    velocity_direction:[3]f32,velocity_magnitude:f32 `min:"0"`,velocity_cone_angle:f32 `min:"0"`,
    base_scale:f32 `min:"0.001" speed:"0.01"`,scale_variation:f32 `min:"0" max:"1" speed:"0.01"`,
    color:[4]f32 `inspect:"color" min:"0"`,color_variation:f32 `min:"0" max:"1" speed:"0.01"`,gravity:f32,
    turbulence_strength,turbulence_frequency:f32 `min:"0"`,
    shape:Particle_Shape,shape_params:[4]f32,active:bool,color_end:[4]f32 `inspect:"color" min:"0"`,scale_end:f32 `min:"0"`,kill_on_destroy:bool,
    timed_emission:f32 `min:"0"`,has_timed_emission:bool,emission_revision:u64 `inspect:"skip" json:"-"`,burst_queue:[dynamic]u32 `inspect:"skip" json:"-"`,
}
Particle_Emitter :: struct { descriptor:Particle_Descriptor }
/// Default parameters exactly match the durable engine emitter descriptor.
particle_defaults :: proc()->Particle_Descriptor { return {emit_rate=50,base_lifetime=5,lifetime_variation=0.2,velocity_direction={0,1,0},velocity_magnitude=1,velocity_cone_angle=0.5,base_scale=0.1,scale_variation=0.5,color={1,1,1,1},color_variation=0.1,gravity=-9.8,turbulence_frequency=3,active=true,color_end={1,1,1,1},scale_end=1} }
@(private="package")
particle_destroy :: proc(value:rawptr) { emitter:=cast(^Particle_Emitter)value; delete(emitter.descriptor.burst_queue); emitter^={} }
@(private="package")
particle_clone :: proc(dst,src:rawptr) { target:=cast(^Particle_Emitter)dst; source:=cast(^Particle_Emitter)src; target^=source^; target.descriptor.burst_queue=slice.clone_to_dynamic(source.descriptor.burst_queue[:]) }
/// Registers an opt-in owned attachment; generic scene spawn never receives an emitter.
particle_register :: proc(w:^ecs.World,reg:^editor.Component_Registry) { editor.editor_register(w,reg,"ParticleEmitter",Particle_Emitter{particle_defaults()},ecs.Value_Ops{particle_destroy,particle_clone},spawn_default=false)
    entry:=reg.entries["ParticleEmitter"]; entry.encode_owned=particle_encode; entry.decode_owned=particle_owned_decode }
@(private="package")
particle_encode :: proc(_:rawptr,value:rawptr,allocator:mem.Allocator)->([]byte,bool) {
    encoded,error:=json.marshal((cast(^Particle_Emitter)value)^,opt=json.Marshal_Options{use_enum_names=true},allocator=allocator); return encoded,error==nil
}
@(private="package")
particle_owned_decode :: proc(_:rawptr,data:[]byte,allocator:mem.Allocator)->(rawptr,bool) {
    value:=new(Particle_Emitter,allocator)
    error:=json.unmarshal(data,value,spec=.JSON,allocator=allocator)
    return value,error==nil && particle_descriptor_valid(value.descriptor)
}
/// Authored history owns settings; only a live emitter owns scheduled render work.
@(private="package")
particle_history_clear :: proc(entry:^editor.Editor_Entry,value:rawptr) {
    if entry.T!=Particle_Emitter || value==nil { return }
    emitter:=cast(^Particle_Emitter)value; delete(emitter.descriptor.burst_queue); emitter.descriptor.burst_queue=nil
}
@(private="package")
particle_history_clone :: proc(entry:^editor.Editor_Entry,target,live:rawptr,allocator:mem.Allocator)->rawptr {
    result:=editor.editor_clone_value(entry,target,allocator)
    if entry.T==Particle_Emitter {
        emitter:=cast(^Particle_Emitter)result; delete(emitter.descriptor.burst_queue); emitter.descriptor.burst_queue=nil
        if live!=nil {
            source:=cast(^Particle_Emitter)live; emitter.descriptor.burst_queue=slice.clone_to_dynamic(source.descriptor.burst_queue[:],allocator); emitter.descriptor.emission_revision=source.descriptor.emission_revision
            if emitter.descriptor.active && !source.descriptor.active { emitter.descriptor.emission_revision=particle_next_revision(emitter.descriptor.emission_revision) }
        }
    }
    return result
}
/// Replaces authored fields on a proposal while retaining every pending burst.
@(private="package")
particle_edit_field :: proc(owner:^Authoring,id:ecs.Entity_Id,op:editor.Scene_Op)->editor.Scene_Error {
    emitter:=ecs.get_component_mut(&owner.world,id,Particle_Emitter); if emitter==nil { return .Component_Not_Found }
    queued:=slice.clone_to_dynamic(emitter.descriptor.burst_queue[:],owner.world.allocator); revision:=emitter.descriptor.emission_revision; active:=emitter.descriptor.active
    error:=editor.editor_set_field(&owner.world,&owner.registry,id,op.component,op.field,op.value)
    if error!=.None { delete(queued); return error }
    updated:=ecs.get_component_mut(&owner.world,id,Particle_Emitter)
    delete(updated.descriptor.burst_queue); updated.descriptor.burst_queue=queued; updated.descriptor.emission_revision=revision
    if updated.descriptor.active && !active { updated.descriptor.emission_revision=particle_next_revision(revision) }; return .None
}
/// Validates every durable field and bounded burst before publishing an emitter.
particle_descriptor_valid :: proc(p:Particle_Descriptor)->bool {
    if p.shape not_in (bit_set[Particle_Shape]{.Point,.Line,.Circle,.Sphere,.Box}) { return false }
    for value in ([10]f32{p.emit_rate,p.velocity_magnitude,p.velocity_cone_angle,p.scale_end,p.turbulence_strength,p.turbulence_frequency,p.base_lifetime,p.base_scale,p.timed_emission,p.lifetime_variation}) { if !finite_nonnegative(value) { return false } }
    if p.base_lifetime<=0 || p.base_scale<=0 || p.lifetime_variation>1 { return false }
    for value in ([3]f32{p.scale_variation,p.color_variation,p.lifetime_variation}) { if !finite_nonnegative(value) || value>1 { return false } }
    for color in ([2][4]f32{p.color,p.color_end}) { for value in color { if !finite_nonnegative(value) { return false } } }
    for value in p.velocity_direction { if math.is_nan(value) || math.is_inf(value) { return false } }; if p.velocity_direction==([3]f32{}) { return false }
    for value in p.shape_params { if math.is_nan(value) || math.is_inf(value) { return false } }
    if math.is_nan(p.gravity) || math.is_inf(p.gravity) || len(p.burst_queue)>1024 { return false }
    for count in p.burst_queue { if count<1 || count>100000 { return false } }; return true
}
@(private="package")
particle_finite :: proc(value:json.Value)->(f32,bool) {
    number:f64
    #partial switch v in value {
    case json.Integer: number=f64(v)
    case json.Float: number=f64(v)
    case: return 0,false
    }
    converted:=f32(number); return converted,!math.is_nan(converted) && !math.is_inf(converted)
}
/// Parses full or partial descriptor documents with defaults, rejecting every unknown field.
particle_decode :: proc(document:json.Value,allocator:=context.allocator)->(Particle_Descriptor,bool) {
    context.allocator=allocator; object,ok:=document.(json.Object); if !ok { return {},false }; p:=particle_defaults(); success:=false; defer { if !success { delete(p.burst_queue) } }
    for key,value in object {
        switch key {
        case "emit_rate","base_lifetime","lifetime_variation","velocity_magnitude","velocity_cone_angle","base_scale","scale_variation","color_variation","gravity","turbulence_strength","turbulence_frequency","scale_end","timed_emission":
            if key=="timed_emission" { if _,is_null:=value.(json.Null); is_null { continue }; p.has_timed_emission=true }
            factor,valid:=particle_finite(value); if !valid { return {},false }
            switch key {
            case "emit_rate": p.emit_rate=factor
            case "base_lifetime": p.base_lifetime=factor
            case "lifetime_variation": p.lifetime_variation=factor
            case "velocity_magnitude": p.velocity_magnitude=factor
            case "velocity_cone_angle": p.velocity_cone_angle=factor
            case "base_scale": p.base_scale=factor
            case "scale_variation": p.scale_variation=factor
            case "color_variation": p.color_variation=factor
            case "gravity": p.gravity=factor
            case "turbulence_strength": p.turbulence_strength=factor
            case "turbulence_frequency": p.turbulence_frequency=factor
            case "scale_end": p.scale_end=factor
            case "timed_emission": p.timed_emission=factor
            }
        case "velocity_direction","color","color_end","shape_params":
            array,is_array:=value.(json.Array); count:=3 if key=="velocity_direction" else 4; if !is_array || len(array)!=count { return {},false }
            values:[4]f32; for element,i in array { factor,valid:=particle_finite(element); if !valid { return {},false }; values[i]=factor }
            switch key {
            case "velocity_direction": p.velocity_direction={values[0],values[1],values[2]}
            case "color": p.color=values
            case "color_end": p.color_end=values
            case "shape_params": p.shape_params=values
            }
        case "active","kill_on_destroy": boolean,is_bool:=value.(bool); if !is_bool { return {},false }; if key=="active" { p.active=boolean } else { p.kill_on_destroy=boolean }
        case "shape": text,is_text:=value.(string); if !is_text { return {},false }; found:=false; for shape,name in ([5]string{"Point","Line","Circle","Sphere","Box"}) { if text==shape { p.shape=Particle_Shape(name); found=true; break } }; if !found { return {},false }
        case "burst_queue": array,is_array:=value.(json.Array); if !is_array || len(array)>1024 { return {},false }; p.burst_queue=make([dynamic]u32,0,len(array),allocator); for element in array { count,is_count:=element.(json.Integer); if !is_count || count<1 || count>100000 { return {},false }; append(&p.burst_queue,u32(count)) }
        case: return {},false
        }
    }
    if !particle_descriptor_valid(p) { return {},false }; success=true; return p,true
}
/// Queues a bounded burst on an active existing emitter without draining prior work.
particle_burst :: proc(w:^ecs.World,entity:ecs.Entity_Id,count:u32)->editor.Scene_Error {
    if count<1 || count>100000 { return .Invalid_Field_Value }; emitter:=ecs.get_component_mut(w,entity,Particle_Emitter); if emitter==nil { return .Component_Not_Found }
    if !emitter.descriptor.active || len(emitter.descriptor.burst_queue)>=1024 { return .Invalid_Operation }
    if emitter.descriptor.burst_queue.allocator.procedure==nil { emitter.descriptor.burst_queue=make([dynamic]u32,w.allocator) }
    append(&emitter.descriptor.burst_queue,count); return .None
}
@(private="package")
particle_next_revision :: proc(revision:u64)->u64 { return 1 if revision==max(u64) else revision+1 }
/// Explicitly restarts the configured timer on the next accepted particle frame.
particle_restart :: proc(w:^ecs.World,entity:ecs.Entity_Id)->editor.Scene_Error { emitter:=ecs.get_component_mut(w,entity,Particle_Emitter); if emitter==nil { return .Component_Not_Found }; emitter.descriptor.emission_revision=particle_next_revision(emitter.descriptor.emission_revision); return .None }
/// Activating an emitter explicitly restarts its configured timer; deactivation retains pending work.
particle_set_active :: proc(w:^ecs.World,entity:ecs.Entity_Id,active:bool)->editor.Scene_Error { emitter:=ecs.get_component_mut(w,entity,Particle_Emitter); if emitter==nil { return .Component_Not_Found }; emitter.descriptor.active=active; if active { emitter.descriptor.emission_revision=particle_next_revision(emitter.descriptor.emission_revision) }; return .None }
/// Transfers accepted bursts to the rendering consumer only after it can retain every emission.
particle_take_bursts :: proc(emitter:^Particle_Emitter)->[dynamic]u32 { result:=emitter.descriptor.burst_queue; emitter.descriptor.burst_queue=nil; return result }
