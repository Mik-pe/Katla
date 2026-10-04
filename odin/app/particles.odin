//! Durable emitter descriptors and bounded commands retain no native graphics handle.
package app
import ecs "../ecs"
import editor "../editor"
import "core:encoding/json"
import "core:math"
import "core:slice"

Particle_Shape :: enum { Point, Line, Circle, Sphere, Box }
/// Serializable linear-color emitter parameters; the render owner consumes accepted bursts.
Particle_Descriptor :: struct {
    emit_rate,base_lifetime,lifetime_variation:f32,
    velocity_direction:[3]f32,velocity_magnitude,velocity_cone_angle:f32,
    base_scale,scale_variation:f32,color:[4]f32,color_variation,gravity,turbulence_strength,turbulence_frequency:f32,
    shape:Particle_Shape,shape_params:[4]f32,active:bool,color_end:[4]f32,scale_end:f32,kill_on_destroy:bool,
    timed_emission:f32,has_timed_emission:bool,burst_queue:[dynamic]u32,
}
Particle_Emitter :: struct { descriptor:Particle_Descriptor `inspect:"skip"` }
/// Default parameters exactly match the durable engine emitter descriptor.
particle_defaults :: proc()->Particle_Descriptor { return {emit_rate=50,base_lifetime=5,lifetime_variation=0.2,velocity_direction={0,1,0},velocity_magnitude=1,velocity_cone_angle=0.5,base_scale=0.1,scale_variation=0.5,color={1,1,1,1},color_variation=0.1,gravity=-9.8,turbulence_frequency=3,active=true,color_end={1,1,1,1},scale_end=1} }
@(private="package")
particle_destroy :: proc(value:rawptr) { emitter:=cast(^Particle_Emitter)value; delete(emitter.descriptor.burst_queue); emitter^={} }
@(private="package")
particle_clone :: proc(dst,src:rawptr) { target:=cast(^Particle_Emitter)dst; source:=cast(^Particle_Emitter)src; target^=source^; target.descriptor.burst_queue=slice.clone_to_dynamic(source.descriptor.burst_queue[:]) }
/// Registers an opt-in owned attachment; generic scene spawn never receives an emitter.
particle_register :: proc(w:^ecs.World,reg:^editor.Component_Registry) { editor.editor_register(w,reg,"ParticleEmitter",Particle_Emitter{},ecs.Value_Ops{particle_destroy,particle_clone},spawn_default=false) }
/// Validates every durable field and bounded burst before publishing an emitter.
particle_descriptor_valid :: proc(p:Particle_Descriptor)->bool {
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
/// Changes runtime activation through the same component used by durable behavior edits.
particle_set_active :: proc(w:^ecs.World,entity:ecs.Entity_Id,active:bool)->editor.Scene_Error { emitter:=ecs.get_component_mut(w,entity,Particle_Emitter); if emitter==nil { return .Component_Not_Found }; emitter.descriptor.active=active; return .None }
/// Transfers accepted bursts to the rendering consumer only after it can retain every emission.
particle_take_bursts :: proc(emitter:^Particle_Emitter)->[dynamic]u32 { result:=emitter.descriptor.burst_queue; emitter.descriptor.burst_queue=nil; return result }
