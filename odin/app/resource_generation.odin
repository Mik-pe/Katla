//! Deterministic descriptive templates publish through the confined exclusive resource writer.
package app

import agent "../agent"
import editor "../editor"
import km "../math"
import ron "../encoding/ron"
import "core:math"
import "core:encoding/json"
import "core:strings"
import "core:fmt"

@(private="package")
Resource_Particle_Template :: struct { rate:f32,lifetime:[2]f32,velocity:[3]f32,color_start,color_end:[3]f32,size_start,size_end:f32 }
@(private="package")
resource_particle_template :: proc(description:string)->Resource_Particle_Template {
    if strings.contains(description,"fire") || strings.contains(description,"flame") || strings.contains(description,"campfire") { return {150,{.3,1.5},{0,3,0},{1,.3,0},{1,.8,0},.15,.02} }
    if strings.contains(description,"rain") { return {500,{.3,.8},{0,-8,0},{.5,.6,.8},{.3,.4,.7},.02,.01} }
    if strings.contains(description,"snow") { return {80,{2,5},{.1,-1,.1},{.95,.95,1},{.8,.8,.9},.05,.03} }
    if strings.contains(description,"spark") || strings.contains(description,"sparkle") || strings.contains(description,"firework") { return {200,{.2,.8},{0,2,0},{1,1,.5},{1,.5,0},.04,.01} }
    if strings.contains(description,"smoke") || strings.contains(description,"steam") { return {40,{1,4},{0,1.5,0},{.5,.5,.5},{.3,.3,.3},.3,.8} }
    if strings.contains(description,"dust") || strings.contains(description,"sand") { return {60,{1,3},{.2,.3,.2},{.8,.7,.5},{.6,.5,.3},.03,.06} }
    if strings.contains(description,"magic") || strings.contains(description,"enchant") || strings.contains(description,"mystic") { return {120,{.5,2},{0,2,0},{.5,0,1},{0,.5,1},.08,.02} }
    if strings.contains(description,"explosion") || strings.contains(description,"burst") { return {300,{.1,.6},{0,0,0},{1,.6,0},{.5,.1,0},.2,.02} }
    return {100,{.5,2},{0,1,0},{1,1,1},{.5,.5,.5},.1,.02}
}
/// Owns a descriptor accepted by the particle service or a current loadable scene document.
resource_generation_content :: proc(owner:^Authoring,request:agent.Resource_Generation_Request)->([]byte,bool) {
    allocator:=owner.world.allocator; context.allocator=allocator
    description:=scene_query_lower(request.description,allocator); defer delete(description,allocator)
    switch request.kind {
    case .Particle_System:
        value:=resource_particle_template(description)
        descriptor:=particle_defaults()
        descriptor.emit_rate=value.rate; descriptor.base_lifetime=(value.lifetime[0]+value.lifetime[1])*.5
        descriptor.lifetime_variation=(value.lifetime[1]-value.lifetime[0])/(value.lifetime[1]+value.lifetime[0])
        magnitude:=math.sqrt(value.velocity[0]*value.velocity[0]+value.velocity[1]*value.velocity[1]+value.velocity[2]*value.velocity[2])
        descriptor.velocity_magnitude=magnitude; descriptor.velocity_direction={0,1,0}; if magnitude>0 { descriptor.velocity_direction=value.velocity/magnitude }
        descriptor.velocity_cone_angle=0; descriptor.gravity=0; descriptor.base_scale=value.size_start; descriptor.scale_end=value.size_end/value.size_start
        descriptor.scale_variation=0; descriptor.color_variation=0
        descriptor.color={value.color_start[0],value.color_start[1],value.color_start[2],1}; descriptor.color_end={value.color_end[0],value.color_end[1],value.color_end[2],1}
        if !particle_descriptor_valid(descriptor) { return nil,false }
        document:=particle_document(descriptor); if document==nil { return nil,false }; defer json.destroy_value(document)
        bytes,error:=json.marshal(document,allocator=allocator); return bytes,error==nil
    case .Scene:
        color:=[3]f32{.9,.95,1}; intensity:f32=.5
        if strings.contains(description,"night") || strings.contains(description,"dark") { color={.05,.05,.1}; intensity=.1 }
        else if strings.contains(description,"sunset") || strings.contains(description,"dawn") { color={.8,.4,.2}; intensity=.4 }
        else if strings.contains(description,"indoor") || strings.contains(description,"interior") { color={.9,.85,.7}; intensity=.3 }
        snapshot:=Scene_Snapshot{entities=make([dynamic]Scene_Entity,allocator),next_entity_id=2,allocator=allocator}; defer scene_snapshot_destroy(&snapshot)
        append(&snapshot.entities,Scene_Entity{key=1,components=make([dynamic]Scene_Component,allocator)}); row:=&snapshot.entities[0]
        if scene_row_component(owner,row,"SceneName",Scene_Name{"Generated light"})!=.None || scene_row_component(owner,row,"SceneTransform",Scene_Transform{km.TRANSFORM_IDENTITY})!=.None || scene_row_component(owner,row,"DirectionalLight",Scene_Directional_Light{{0,-1,0},color,intensity})!=.None { return nil,false }
        document,error:=scene_document_encode(owner,&snapshot,"Generated scene",request.path); if error!=.None { return nil,false }; defer json.destroy_value(document)
        bytes,write_error:=ron.write(document,allocator); return bytes,write_error.kind==.None
    }
    return nil,false
}
/// Creates a real new definition with exclusive publication and no scene-history command.
resource_generation_execute :: proc(owner:^Authoring,request:agent.Resource_Generation_Request)->(editor.Tool_Result,editor.Undo_Group) {
    content,valid:=resource_generation_content(owner,request); defer delete(content,owner.world.allocator)
    if !valid { return error_result(&owner.world,.Invalid_Operation),{} }
    kind:="particle_system" if request.kind==.Particle_System else "scene"
    message:=fmt.aprintf("Generated %s as %s (%d bytes)",request.path,kind,len(content)); defer delete(message,owner.world.allocator)
    response,error:=json.marshal(struct{success:bool,message,path,resource_type:string}{true,message,request.path,kind},allocator=owner.world.allocator)
    if error!=nil { return error_result(&owner.world,.Decode_Failed),{} }
    result,group:=resource_write_execute(owner,{action=.Create,path=request.path,content=string(content)})
    if result.data!=nil { delete(result.data,owner.world.allocator); result.data=response } else { delete(response,owner.world.allocator) }
    return result,group
}
