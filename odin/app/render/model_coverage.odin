//! Each model coverage phase retains its own base image, sampler and winding policy.
package render

import gfx "../../gfx"
import ecs "../../ecs"
import km "../../math"
import "core:mem"

@(private="package")
Model_Cascade_Uniform :: struct { index:u32,sign:f32,pad:[2]u32 }
@(private="package")
model_cascade_bytes :: proc(values:[]Model_Cascade_Uniform)->[]byte { return mem.slice_to_bytes(values) }
@(private="package")
model_outline_bytes :: proc(values:[]Outline_Constants)->[]byte { return mem.slice_to_bytes(values) }
@(private="package")
model_coverage_mirrored :: proc(model:km.Mat4)->bool { return km.dot(km.cross(km.xyz(model[0]),km.xyz(model[1])),km.xyz(model[2]))<0 }
@(private="package")
Model_Coverage_Binding :: struct { image:[1]gfx.Image_Access,binding:[1]gfx.Image_Binding,sampler:[1]gfx.Sampler_Binding,draw:[1]gfx.Draw_Op }
@(private="package")
model_coverage_image :: proc(cache:^Native_Model($R),index:int)->(gfx.Image_Access,gfx.Sampler_Handle,Native_Error) {
    if index<0 || index>=len(cache.receipts) { return {},{},{gpu=.Invalid_Resource} }
    receipt:=cache.receipts[index];texture,sampler:=receipt.textures[0],receipt.samplers[0]
    if texture<0 || texture>=len(cache.textures) || texture>=len(cache.image_ids) || sampler<0 || sampler>=len(cache.samplers) { return {},{},{gpu=.Invalid_Resource} }
    return {cache.image_ids[texture],gfx.image_full_range(cache.textures[texture].native.desc),.Read,.Sampled},cache.samplers[sampler].handle,{}
}
@(private="package")
model_coverage_selected :: proc(entity:ecs.Entity_Id,selected:[]ecs.Entity_Id)->bool { for id in selected { if id==entity { return true } };return false }
@(private="package")
model_coverage_declared :: proc(images:^[dynamic]gfx.Image_Access,access:gfx.Image_Access) { for previous in images { if previous==access { return } };append(images,access) }
/// Shadow effect zero uses all four atlas quadrants; effects one through four select stencil/outline passes.
feature_model_coverage_packet :: proc(cache:^Native_Model($R),scene:^Scene_Graph,effect:int,enabled:bool,clip_sign:f32,selected:[]ecs.Entity_Id=nil)->Native_Error {
    if cache.graph!=scene || effect<0 || effect>4 { return {gpu=.Invalid_Graph} }
    f:=&scene.features
    shadow:=effect==0
    target:=scene.color if effect!=4 else f.indicator
    target_desc:=scene.color_desc if effect!=4 else f.indicator_desc
    depth:=gfx.Depth_Attachment{enabled=true,access={f.atlas if shadow else scene.depth,gfx.image_full_range(f.atlas_desc if shadow else scene.depth_desc),.Read_Write,.Depth_Attachment},load=.Load,store=.Store}
    colors:=[1]gfx.Color_Attachment{{{target,gfx.image_full_range(target_desc),.Read_Write,.Color_Attachment},.Load,.Store,{}}}
    packet:=gfx.Render{depth=depth};if !shadow { packet.colors=colors[:] }
    images:=make([dynamic]gfx.Image_Access,scene.allocator);defer delete(images)
    append(&images,depth.access);if !shadow { append(&images,colors[0].access) }
    buffers:=[3]gfx.Stage_Buffer_Binding{{0,1,{.Vertex,.Fragment},{cache.objects,{0,cache.object_desc.size},.Read,.Storage}},{0,2,{.Vertex},{cache.geometry,{0,cache.geometry_desc.size},.Read,.Storage}},{}}
    if shadow { buffers[2]={4,0,{.Vertex},{f.buffers[4],{0,352},.Read,.Storage}} }
    else { buffers[2]={0,0,{.Vertex},{cache.frame,{0,cache.frame_desc.size},.Read,.Uniform}} }
    accesses:[3]gfx.Buffer_Access;for binding,i in buffers { accesses[i]=binding.access }
    count:=len(cache.batch.entries)*(4 if shadow else 1)
    bindings:=make([]Model_Coverage_Binding,count,scene.allocator);defer delete(bindings,scene.allocator)
    phases:=make([]gfx.Render_Phase,count,scene.allocator);defer delete(phases,scene.allocator)
    cascade_values:[4]Model_Cascade_Uniform
    cascade_constants:[4]gfx.Constant_Binding
    for &value,i in cascade_values { value={u32(i),clip_sign,{}};cascade_constants[i]={group=4,slot=3,stages={.Vertex},usage=.Uniform,bytes=model_cascade_bytes(cascade_values[i:i+1])} }
    outline:=[1]Outline_Constants{{{0.004*1080/f32(scene.color_desc.height),0,0,0},{1,0.55,0,1}}}
    written:int
    if enabled {
        for entry,index in cache.batch.entries {
            if shadow && entry.material.alpha_mode==.Blend { continue }
            if !shadow && !model_coverage_selected(entry.entity,selected) { continue }
            if int(entry.object_index)>=len(cache.batch.objects) { return {scene=.Invalid_Geometry} }
            image,sampler,error:=model_coverage_image(cache,index);if error!={} { return error }
            model_coverage_declared(&images,image)
            model:=cache.batch.objects[entry.object_index].model
            mirrored:=model_coverage_mirrored(model)
            repeats:=4 if shadow else 1
            for cascade in 0..<repeats {
                binding:=&bindings[written];binding.image[0]=image;binding.binding[0]={group=1,slot=0,stages={.Fragment},accesses=binding.image[:]};binding.sampler[0]={1,5,{.Fragment},sampler};binding.draw[0]=gfx.Draw{entry.vertex_count,1,entry.first_vertex,entry.object_index}
                phase:=&phases[written];phase^={pipeline=feature_model_pipeline(f.pipelines,effect,entry.material.double_sided,mirrored),draws=binding.draw[:],images=binding.binding[:],samplers=binding.sampler[:]}
                if shadow { size:=f64(f.atlas_desc.width/2);phase.viewport={true,f64(cascade%2)*size,f64(cascade/2)*size,size,size,0,1};phase.constants=cascade_constants[cascade:cascade+1] }
                else if effect==3 { phase.constants={{group=5,slot=0,stages={.Vertex,.Fragment},usage=.Uniform,bytes=model_outline_bytes(outline[:])}} }
                written+=1
            }
        }
    }
    if written>0 { packet.buffers=buffers[:];packet.phases=phases[:written] }
    pass:=f.shadows[1] if shadow else f.selection[1][effect-1]
    packet_error,graph_error:=gfx.graph_set_commands(scene_graph_target(scene),pass,packet,accesses[:] if written>0 else nil,images[:])
    if graph_error!=.None { return {gpu=.Invalid_Graph} };return {packet=packet_error}
}

@(private="package")
// Retains the accepted phase selection, constants and winding while rebinding image generations.
model_coverage_rebind :: proc(cache:^Native_Model($R))->Native_Error {
    if cache.graph==nil { return {gpu=.Invalid_Graph} }
    scene:=cache.graph;graph:=scene_graph_target(scene)
    passes:=[5]gfx.Pass_Id{scene.features.shadows[1],scene.features.selection[1][0],scene.features.selection[1][1],scene.features.selection[1][2],scene.features.selection[1][3]}
    count:=len(passes) if scene.features.late_bound else 1
    for pass in passes[:count] {
        if pass.owner!=graph || pass.index<0 || pass.index>=len(graph.passes) { return {gpu=.Invalid_Graph} }
        previous:=graph.passes[pass.index]
        packet,rendered:=previous.packet.(gfx.Render)
        if !rendered { return {gpu=.Invalid_Graph} }
        if len(packet.phases)==0 { continue }
        bindings:=make([]Model_Coverage_Binding,len(packet.phases),cache.allocator);defer delete(bindings,cache.allocator)
        phases:=make([]gfx.Render_Phase,len(packet.phases),cache.allocator);defer delete(phases,cache.allocator)
        images:=make([dynamic]gfx.Image_Access,cache.allocator);defer delete(images)
        for access in previous.images {
            sampled:=false;for id in cache.image_ids { if access.resource==id { sampled=true;break } }
            if !sampled { append(&images,access) }
        }
        for phase,i in packet.phases {
            if len(phase.draws)!=1 || len(phase.images)!=1 || len(phase.samplers)!=1 { return {gpu=.Invalid_Graph} }
            draw,valid:=phase.draws[0].(gfx.Draw);if !valid || draw.instance_count!=1 { return {scene=.Invalid_Geometry} }
            entry_index:= -1
            for entry,index in cache.batch.entries { if entry.object_index==draw.first_instance { entry_index=index;break } }
            if entry_index<0 { return {scene=.Invalid_Geometry} }
            image,sampler,error:=model_coverage_image(cache,entry_index);if error!={} { return error }
            model_coverage_declared(&images,image)
            binding:=&bindings[i];binding.image[0]=image;binding.binding[0]=phase.images[0];binding.binding[0].accesses=binding.image[:]
            binding.sampler[0]=phase.samplers[0];binding.sampler[0].handle=sampler
            phases[i]=phase;phases[i].images=binding.binding[:];phases[i].samplers=binding.sampler[:]
        }
        packet.phases=phases
        packet_error,graph_error:=gfx.graph_set_commands(graph,pass,packet,previous.accesses,images[:])
        if graph_error!=.None { return {gpu=.Invalid_Graph} };if packet_error!=.None { return {packet=packet_error} }
    }
    return {}
}
