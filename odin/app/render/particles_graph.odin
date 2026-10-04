//! Particle passes compose ordinary imports, compute, transfers and an indirect billboard draw.
package render

import gfx "../../gfx"
import km "../../math"
import "core:mem"
import "core:slice"
import "core:math"

@(private="package")
particle_camera :: proc(frame:Frame_Data)->(Particle_Camera,bool) {
    for column in frame.view_projection { for value in column { if math.is_nan(value) || math.is_inf(value) { return {},false } } }
    if frame.ambient[3]!=1 && frame.ambient[3]!=-1 { return {},false }
    right:=km.Vec3{frame.view_projection[0][0],frame.view_projection[1][0],frame.view_projection[2][0]}
    up:=km.Vec3{frame.view_projection[0][1],frame.view_projection[1][1],frame.view_projection[2][1]}
    if km.length_squared(right)==0 || km.length_squared(up)==0 { return {},false }
    return {frame.view_projection,km.vec4(km.normalize(right)),km.vec4(km.normalize(up)),{frame.ambient[3],0,0,0}},true
}
@(private="package")
particle_upload_bytes :: proc(frame:^Particle_Frame,configs:[]Particle_Config,indices:[]u32,camera:^Particle_Camera)->[4][]byte {
    return {mem.slice_to_bytes(slice.from_ptr(frame,1)),mem.slice_to_bytes(configs),mem.slice_to_bytes(indices),mem.slice_to_bytes(slice.from_ptr(camera,1))}
}

@(private="package")
particle_resource_desc :: proc(consumer:^Particle_Consumer($R),index:int)->gfx.Buffer_Desc {
    storage:=gfx.Buffer_Usages{.Storage,.Transfer_Source,.Transfer_Destination}
    sizes:=[14]u64{u64(consumer.capacity)*64,u64(consumer.capacity)*4,u64(consumer.capacity)*4,u64(consumer.capacity)*4,u64(consumer.capacity)*4,16,16,16,12,32,u64(consumer.emitter_capacity)*160,u64(consumer.capacity)*4,112,particle_readback_size(consumer)}
    usage:=storage; domain:=gfx.Memory_Domain.GPU_Private
    if index==7 || index==8 { usage|={.Indirect} }
    if index==9 || index==12 { usage={.Uniform}; domain=.CPU_Visible }
    if index==10 || index==11 { usage={.Storage}; domain=.CPU_Visible }
    if index==13 { usage={.Readback,.Transfer_Destination}; domain=.CPU_Visible }
    return {size=sizes[index],usage=usage,memory=domain}
}
@(private="package")
particle_add_copy :: proc(graph:^gfx.Graph,name:string,source,destination:gfx.Resource_Id,source_offset,destination_offset,size:u64)->Particle_Error {
    pass,error:=gfx.graph_pass(graph,name,.Transfer,{{source,{source_offset,size},.Read,.Transfer_Source},{destination,{destination_offset,size},.Write,.Transfer_Destination}})
    if error!=.None { return {code=.Invalid_Graph} }
    packet:=gfx.graph_set_packet(graph,pass,gfx.Copy_Buffer{source,destination,source_offset,destination_offset,size})
    return {packet=packet}
}
@(private="package")
particle_add_fill :: proc(graph:^gfx.Graph,name:string,destination:gfx.Resource_Id,offset,size:u64)->Particle_Error {
    pass,error:=gfx.graph_pass(graph,name,.Transfer,{{destination,{offset,size},.Write,.Transfer_Destination}})
    if error!=.None { return {code=.Invalid_Graph} }
    return {packet=gfx.graph_set_packet(graph,pass,gfx.Fill_Buffer{destination,offset,size,0})}
}
@(private="package")
particle_binding_resource :: proc(consumer:^Particle_Consumer($R),pipeline:int,group,slot:u32)->gfx.Resource_Id {
    r:=consumer.resources
    if pipeline==2 || pipeline==3 { return r.counters if slot==0 else (r.indirect if pipeline==2 else r.dispatch) }
    if group==0 {
        switch slot {
        case 0: return r.data
        case 1: return r.dead
        case 2: return r.working
        case 3: return r.alive
        case 4: return r.counters
        }
    } else if group==1 {
        switch slot {
        case 0: return r.frame
        case 1: return r.configs
        case 2: return r.indices
        }
    }
    return {}
}
@(private="package")
particle_dispatch_packet :: proc(consumer:^Particle_Consumer($R),pipeline:int,groups:[3]u32)->(gfx.Dispatch,[]gfx.Buffer_Access) {
    descriptor:=consumer.shaders.compute[pipeline].descriptor
    packet:=gfx.Dispatch{pipeline=consumer.pipelines[pipeline],groups=groups}
    packet.bindings=make([]gfx.Buffer_Binding,len(descriptor.buffers),consumer.allocator)
    accesses:=make([]gfx.Buffer_Access,len(descriptor.buffers)+int(pipeline==1),consumer.allocator)
    for requirement,i in descriptor.buffers {
        resource:=particle_binding_resource(consumer,pipeline,requirement.group,requirement.slot)
        size:=scene_graph_target(consumer.graph).buffers[resource.index].desc.size
        mode:=requirement.mode
        // Emission appends to initialized pools; unmodified survivors are consumed later.
        if pipeline==0 && requirement.group==0 && (requirement.slot==0 || requirement.slot==2) { mode=.Read_Write }
        access:=gfx.Buffer_Access{resource,{0,size},mode,requirement.usage}
        packet.bindings[i]={requirement.group,requirement.slot,access}; accesses[i]=access
    }
    if pipeline==1 {
        packet.groups={}; packet.indirect={true,{consumer.resources.dispatch,{0,12},.Read,.Indirect}}
        accesses[len(descriptor.buffers)]=packet.indirect.command
    }
    return packet,accesses
}
/// Installs particle passes on a new stationary scene graph; current GPU state remains in the consumer.
particle_bind_graph :: proc(consumer:^Particle_Consumer($R),scene:^Scene_Graph)->Particle_Error {
    if consumer.pending.ready || scene==nil || scene.color_desc.format!=consumer.shaders.graphics.descriptor.colors[0].format { return {code=.Invalid_Graph} }
    extend_error:=scene_graph_extend(scene); if extend_error!={} { return {gpu=extend_error.gpu,packet=extend_error.packet} }
    consumer.graph=scene
    r:=&consumer.resources
    resources:=[14]^gfx.Resource_Id{&r.data,&r.dead,&r.previous_alive,&r.working,&r.alive,&r.previous_counters,&r.counters,&r.indirect,&r.dispatch,&r.frame,&r.configs,&r.indices,&r.camera,&r.readback}
    for resource,i in resources {
        id,error:=gfx.graph_buffer(scene_graph_target(scene),particle_resource_desc(consumer,i),true,false)
        if error!=.None { return {code=.Invalid_Graph} }; resource^=id
    }
    error:=particle_add_copy(scene_graph_target(scene),"Particle survivor rollover",r.previous_alive,r.working,0,0,u64(consumer.capacity)*4); if error!={} { return error }
    error=particle_add_copy(scene_graph_target(scene),"Particle alive count rollover",r.previous_counters,r.counters,0,8,4); if error!={} { return error }
    error=particle_add_copy(scene_graph_target(scene),"Particle dead count rollover",r.previous_counters,r.counters,4,4,4); if error!={} { return error }
    error=particle_add_fill(scene_graph_target(scene),"Particle reset survivors",r.counters,0,4); if error!={} { return error }
    error=particle_add_fill(scene_graph_target(scene),"Particle reset workgroups",r.counters,12,4); if error!={} { return error }
    for pipeline in ([4]int{0,3,1,2}) {
        names:=[4]string{"Particle spawn","Particle simulate","Particle draw command","Particle dispatch command"}
        packet,accesses:=particle_dispatch_packet(consumer,pipeline,{1,1,1})
        pass,graph_error:=scene_graph_pass(scene,names[pipeline],.Compute,accesses)
        if graph_error!=.None { delete(packet.bindings,consumer.allocator); delete(accesses,consumer.allocator); return {code=.Invalid_Graph} }
        packet_error:=gfx.graph_set_packet(scene_graph_target(scene),pass,packet)
        delete(packet.bindings,consumer.allocator); delete(accesses,consumer.allocator)
        if packet_error!=.None { return {packet=packet_error} }
        r.compute_passes[pipeline]=pass
    }
    buffers:=[3]gfx.Stage_Buffer_Binding{
        {0,0,{.Vertex},{r.data,{0,u64(consumer.capacity)*64},.Read,.Storage}},
        {0,2,{.Vertex},{r.alive,{0,u64(consumer.capacity)*4},.Read,.Storage}},
        {1,0,{.Vertex},{r.camera,{0,112},.Read,.Uniform}},
    }
    command:=gfx.Buffer_Access{r.indirect,{0,16},.Read,.Indirect}
    accesses:=[4]gfx.Buffer_Access{buffers[0].access,buffers[1].access,buffers[2].access,command}
    color:=gfx.Color_Attachment{{scene.color,gfx.image_full_range(scene.color_desc),.Read_Write,.Color_Attachment},.Load,.Store,{}}
    depth:=gfx.Depth_Attachment{true,{scene.depth,gfx.image_full_range(scene.depth_desc),.Read_Write,.Depth_Attachment},.Load,.Store,1,0}
    pass,graph_error:=scene_graph_pass(scene,"Particle billboards",.Graphics,accesses[:],images={color.access,depth.access})
    if graph_error!=.None { return {code=.Invalid_Graph} }
    draw:=gfx.Draw_Indirect{command=command,count=1,stride=16}
    packet_error:=gfx.graph_set_packet(scene_graph_target(scene),pass,gfx.Render{colors={color},depth=depth,buffers=buffers[:],phases={{pipeline=(consumer.reverse_pipeline if scene.depth_sense==.Reverse else consumer.pipeline),draws={draw}}}})
    if packet_error!=.None { return {packet=packet_error} }
    // Persistent snapshots decouple simulation continuity from renderer acquisitions used by uploads.
    for item,i in ([2]struct {source,destination:gfx.Resource_Id,size:u64}{{r.alive,r.previous_alive,u64(consumer.capacity)*4},{r.counters,r.previous_counters,16}}) {
        names:=[2]string{"Particle retain survivors","Particle retain counters"}
        retained:=particle_add_copy(scene_graph_target(scene),names[i],item.source,item.destination,0,0,item.size); if retained!={} { return retained }
        scene_graph_target(scene).passes[len(scene_graph_target(scene).passes)-1].side_effect=true
    }
    // Counters are observed independently of whether the current image is exported.
    readback_error:=particle_add_copy(scene_graph_target(scene),"Particle completed counters",r.counters,r.readback,0,0,16); if readback_error!={} { return readback_error }
    scene_graph_target(scene).passes[len(scene_graph_target(scene).passes)-1].side_effect=true
    if consumer.capture_state {
        for item,i in ([3]struct {source:gfx.Resource_Id,offset,size:u64}{{r.indirect,16,16},{r.alive,32,u64(consumer.capacity)*4},{r.data,32+u64(consumer.capacity)*4,u64(consumer.capacity)*64}}) {
            names:=[3]string{"Particle captured command","Particle captured survivors","Particle captured storage"}
            copy_error:=particle_add_copy(scene_graph_target(scene),names[i],item.source,r.readback,0,item.offset,item.size); if copy_error!={} { return copy_error }
            scene_graph_target(scene).passes[len(scene_graph_target(scene).passes)-1].side_effect=true
        }
    }
    final_error:=scene_graph_finalize(scene); if final_error!={} { return {gpu=final_error.gpu,packet=final_error.packet} }
    return {}
}
/// Stages exact emitter requests and selects resources from the acquired native token's slot.
particle_prepare :: proc(consumer:^Particle_Consumer($R),scene:^Scene_Graph,token:gfx.Frame_Token,frame:Frame_Data,delta:f32)->([]gfx.Buffer_Input,Particle_Error) {
    if token.slot<0 || token.slot>=len(consumer.slots) || consumer.pending.ready { return nil,{code=.Invalid_Frame} }
    observed:=particle_observe(consumer); if observed!={} { return nil,observed }
    prepared:Particle_Preparation; plan_error:Particle_Error
    if consumer.reset_requested { prepared,plan_error=particle_reset_prepare(consumer) }
    else { prepared,plan_error=particle_plan(consumer.owner,consumer.states,consumer.capacity-consumer.alive_upper,consumer.emitter_capacity,delta,consumer.allocator,pool_capacity=consumer.capacity,reclaim=consumer.alive_upper==0) }
    if plan_error!={} { return nil,plan_error }
    consumer.pending=prepared; consumer.pending.token=token
    success:=false; defer { if !success { particle_abort(consumer) } }
    bound:=consumer.graph==scene && consumer.resources.compute_passes[0].owner==scene_graph_target(scene) && consumer.resources.compute_passes[0].index>=0 && consumer.resources.compute_passes[0].index<len(scene_graph_target(scene).passes)
    if bound {
        packet,dispatch:=scene_graph_target(scene).passes[consumer.resources.compute_passes[0].index].packet.(gfx.Dispatch)
        bound=dispatch && packet.pipeline==consumer.pipelines[0]
    }
    if !bound { bind_error:=particle_bind_graph(consumer,scene); if bind_error!={} { return nil,bind_error } }
    slot:=consumer.slots[token.slot]
    initial:=prepared.reset_counters if prepared.reset else consumer.rollover_counters
    frame_values:=[1]Particle_Frame{{prepared.delta,prepared.requested,u32(len(prepared.states)),u32(consumer.sequence+1),consumer.capacity,prepared.burst_count,u32(consumer.sequence),consumer.capacity}}
    configs:=make([]Particle_Config,int(consumer.emitter_capacity),consumer.allocator); defer delete(configs,consumer.allocator)
    for state,i in prepared.states { configs[i]=state.config }
    indices:=make([]u32,int(consumer.capacity),consumer.allocator); defer delete(indices,consumer.allocator); copy(indices,prepared.indices)
    camera,camera_valid:=particle_camera(frame)
    if !camera_valid { return nil,{code=.Invalid_Configuration} }
    upload_bytes:=particle_upload_bytes(&frame_values[0],configs,indices,&camera)
    uploads:=[4]struct {handle:gfx.Buffer_Handle,bytes:[]byte}{{slot.frame,upload_bytes[0]},{slot.configs,upload_bytes[1]},{slot.indices,upload_bytes[2]},{slot.camera,upload_bytes[3]}}
    for upload in uploads { error:=consumer.operations.write_buffer(consumer.renderer,token,upload.handle,0,upload.bytes); if error!=.None { return nil,{gpu=error} } }
    packet,accesses:=particle_dispatch_packet(consumer,0,{max(u32(1),(prepared.requested+255)/256),1,1})
    packet_error:=gfx.graph_set_packet(scene_graph_target(scene),consumer.resources.compute_passes[0],packet)
    delete(packet.bindings,consumer.allocator); delete(accesses,consumer.allocator)
    if packet_error!=.None { return nil,{packet=packet_error} }
    clear(&consumer.inputs)
    r:=consumer.resources
    mappings:=[14]gfx.Buffer_Input{{r.data,(prepared.reset_data if prepared.reset else consumer.data)},{r.dead,(prepared.reset_dead if prepared.reset else consumer.dead)},{r.previous_alive,consumer.rollover_alive},{r.working,slot.working},{r.alive,slot.alive},{r.previous_counters,initial},{r.counters,slot.counters},{r.indirect,slot.indirect},{r.dispatch,slot.dispatch},{r.frame,slot.frame},{r.configs,slot.configs},{r.indices,slot.indices},{r.camera,slot.camera},{r.readback,slot.readback}}
    append(&consumer.inputs,..mappings[:])
    consumer.pending.ready=true; success=true; return consumer.inputs[:],{}
}
