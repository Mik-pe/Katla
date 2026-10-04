//! The application owns scene resources and composes ordinary GPU operations per acquired slot.
package render

import gfx "../../gfx"
import "core:mem"
import app ".."
import ecs "../../ecs"

/// Generic GPU operations are explicit; scene/material policy stays in this application owner.
GPU_Ops :: struct($Renderer:typeid) {
    create_pipeline:proc(^Renderer,gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error),
    destroy_pipeline:proc(^Renderer,gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error,
    create_buffer:proc(^Renderer,gfx.Buffer_Desc,[]byte)->(gfx.Buffer_Handle,gfx.Gpu_Error),
    destroy_buffer:proc(^Renderer,gfx.Buffer_Handle)->gfx.Gpu_Error,
    write_buffer:proc(^Renderer,gfx.Frame_Token,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error,
    create_texture:proc(^Renderer,gfx.Texture_Desc)->(gfx.Texture_Handle,gfx.Gpu_Error),
    destroy_texture:proc(^Renderer,gfx.Texture_Handle)->gfx.Gpu_Error,
    acquire:proc(^Renderer)->(gfx.Frame_Token,gfx.Gpu_Error),
    abort:proc(^Renderer,gfx.Frame_Token)->gfx.Gpu_Error,
    submit:proc(^Renderer,gfx.Frame_Token,^gfx.Graph,^gfx.Compiled_Graph,[]gfx.Buffer_Input,[]gfx.Texture_Input)->(gfx.Submission,gfx.Gpu_Error,gfx.Packet_Error),
    wait:proc(^Renderer,gfx.Submission)->gfx.Gpu_Error,
    release_exports:proc(^Renderer,^gfx.Graph)->gfx.Gpu_Error,
    create_compute:proc(^Renderer,gfx.Compute_Desc)->(gfx.Pipeline_Handle,gfx.Gpu_Error),
    destroy_compute:proc(^Renderer,gfx.Pipeline_Handle)->gfx.Gpu_Error,
    create_sampler:proc(^Renderer,gfx.Sampler_Desc)->(gfx.Sampler_Handle,gfx.Gpu_Error),
    destroy_sampler:proc(^Renderer,gfx.Sampler_Handle)->gfx.Gpu_Error,
}
/// One application-owned allocation for every mutable resource in every native frame slot.
Native_Slot :: struct { frame,objects:gfx.Buffer_Handle, color,depth,output:gfx.Texture_Handle }
/// Additional application passes supply borrowed inputs to the same acquired submission.
Scene_Inputs :: struct { buffers:[]gfx.Buffer_Input, textures:[]gfx.Texture_Input }
Scene_Authoring :: ^app.Authoring
Scene_Entities :: []ecs.Entity_Id
/// Optional installed effects prepare atomically and publish only after native acceptance.
Scene_Composition :: struct($Renderer:typeid) {
    state:rawptr,
    prepare:proc(rawptr,^Native_Scene(Renderer),gfx.Frame_Token,Frame_Data)->(Scene_Inputs,Native_Error),
    accepted:proc(rawptr,gfx.Submission),
    aborted:proc(rawptr),
    validate:proc(rawptr,Scene_Authoring,Scene_Entities)->Native_Error,
}
/// A stationary, exclusive scene consumer retains native resources and accepted submission order.
Native_Scene :: struct($Renderer:typeid) {
    renderer:^Renderer,
    operations:GPU_Ops(Renderer),
    graph:Scene_Graph,
    slots:[]Native_Slot,
    geometry:gfx.Buffer_Handle,
    pipeline,reverse_pipeline,display_pipeline:gfx.Graphics_Pipeline_Handle,
    pending:[dynamic]gfx.Submission,
    composition:Scene_Composition(Renderer),
    models:^Native_Model(Renderer),
    features:Native_Features(Renderer),
    feature_settings:Feature_Settings,
    authoring:^app.Authoring,
    batch:^Scene_Batch,
    selected:[]ecs.Entity_Id,
    allocator:mem.Allocator,
    prepared:bool,
}
/// Keeps GPU, graph and CPU input failures distinct for host diagnostics.
Native_Error :: struct { gpu:gfx.Gpu_Error, packet:gfx.Packet_Error, scene:Scene_Error, particle:Particle_Error_Code }
/// Prepares native scene resources without compiling anything on the frame path.
native_scene_init :: proc(scene:^Native_Scene($R),renderer:^R,operations:GPU_Ops(R),descriptor:Scene_Pipelines,geometry:^Geometry,object_capacity,slot_count:int,width,height:u32,allocator:mem.Allocator=context.allocator,settings:Feature_Settings=FEATURE_SETTINGS_DEFAULT)->Native_Error {
    if renderer==nil || slot_count<1 || slot_count>16 || len(descriptor.surface.colors)!=1 || descriptor.postprocess.fragment_entry=="" || (len(geometry.vertices)==0 && object_capacity!=0) { return {scene=.Invalid_Geometry} }
    if operations.create_pipeline==nil || operations.destroy_pipeline==nil || operations.create_buffer==nil || operations.destroy_buffer==nil || operations.write_buffer==nil || operations.create_texture==nil || operations.destroy_texture==nil || operations.acquire==nil || operations.abort==nil || operations.submit==nil || operations.wait==nil || operations.release_exports==nil { return {gpu=.Unsupported} }
    scene.feature_settings=settings
    scene.renderer=renderer; scene.operations=operations; scene.allocator=allocator
    scene.pending=make([dynamic]gfx.Submission,0,slot_count,allocator)
    success:=false; defer { if !success { native_scene_destroy(scene) } }
    error:gfx.Gpu_Error
    scene.pipeline,error=operations.create_pipeline(renderer,descriptor.surface); if error!=.None { return {gpu=error} }
    scene.reverse_pipeline,error=operations.create_pipeline(renderer,depth_descriptor(descriptor.surface,.Reverse)); if error!=.None { return {gpu=error} }
    scene.display_pipeline,error=operations.create_pipeline(renderer,descriptor.postprocess); if error!=.None { return {gpu=error} }
    feature_error:=native_features_init(&scene.features,renderer,operations,descriptor.features,slot_count,width,height,scene.feature_settings.shadow_size,allocator); if feature_error!=.None { return {gpu=feature_error} }
    scene_error:=scene_graph_init(&scene.graph,scene.pipeline,scene.display_pipeline,len(geometry.vertices),object_capacity,width,height,descriptor.output_format,allocator,scene.features.pipelines,scene.features.shadow_sampler,scene.feature_settings)
    if scene_error!=.None { return {scene=scene_error} }
    vertex_bytes:=mem.slice_to_bytes(geometry.vertices)
    if len(vertex_bytes)>0 { scene.geometry,error=operations.create_buffer(renderer,scene.graph.geometry_desc,vertex_bytes); if error!=.None { return {gpu=error} } }
    scene.slots=make([]Native_Slot,slot_count,allocator)
    zero_frame:=make([]byte,int(scene.graph.frame_desc.size),allocator); defer delete(zero_frame,allocator)
    zero_objects:=make([]byte,int(scene.graph.object_desc.size),allocator); defer delete(zero_objects,allocator)
    for &slot in scene.slots {
        slot.frame,error=operations.create_buffer(renderer,scene.graph.frame_desc,zero_frame); if error!=.None { return {gpu=error} }
        if len(vertex_bytes)>0 {
            slot.objects,error=operations.create_buffer(renderer,scene.graph.object_desc,zero_objects); if error!=.None { return {gpu=error} }
        }
        slot.color,error=operations.create_texture(renderer,scene.graph.color_desc); if error!=.None { return {gpu=error} }
        slot.depth,error=operations.create_texture(renderer,scene.graph.depth_desc); if error!=.None { return {gpu=error} }
        slot.output,error=operations.create_texture(renderer,scene.graph.output_desc); if error!=.None { return {gpu=error} }
    }
    success=true; return {}
}
/// Retires an exact accepted scene submission, preserving other queued frame owners.
native_scene_wait :: proc(scene:^Native_Scene($R),submission:gfx.Submission)->gfx.Gpu_Error {
    found_index:= -1
    for accepted,i in scene.pending { if accepted==submission { found_index=i; break } }
    if found_index<0 { return .Invalid_Resource }
    error:=scene.operations.wait(scene.renderer,submission)
    if error==.None { ordered_remove(&scene.pending,found_index) }
    return error
}
/// Acquires the next exact native slot, retiring only the scene's oldest accepted owner when busy.
native_scene_acquire :: proc(scene:^Native_Scene($R))->(gfx.Frame_Token,gfx.Gpu_Error) {
    if scene.renderer==nil { return {},.Invalid_Resource }
    token,error:=scene.operations.acquire(scene.renderer)
    if error==.Busy && len(scene.pending)>0 {
        error=native_scene_wait(scene,scene.pending[0]); if error!=.None { return {},error }
        token,error=scene.operations.acquire(scene.renderer)
    }
    return token,error
}
/// Prepares one view into ordinary graph packets without acquiring or submitting another token.
/// A supplied destination is stationary on its caller; the caller owns its exports and accepted waits.
native_scene_prepare :: proc(scene:^Native_Scene($R),token:gfx.Frame_Token,frame:Frame_Data,objects:[]Object_Data,draws:[]gfx.Draw_Op,color_override:=gfx.Texture_Handle{},destination:^gfx.Graph=nil,namespace:string="",depth_sense:Depth_Sense=.Forward)->(Native_Prepared(R),Native_Error) {
    if !depth_sense_valid(depth_sense) { return {},{scene=.Invalid_Camera} }
    if scene.prepared { return {},{gpu=.Busy} }
    assembled:=false
    pass_count,buffer_count,image_count:int
    if destination!=nil {
        pass_count,buffer_count,image_count=len(destination.passes),len(destination.buffers),len(destination.images)
    }
    defer { if destination!=nil && !assembled { gfx.graph_truncate(destination,pass_count,buffer_count,image_count) } }
    if destination!=nil {
        attach_error:=native_scene_append_graph(scene,destination,namespace,depth_sense)
        if attach_error!={} { return {},attach_error }
    } else if scene.graph.target!=nil { return {},{gpu=.Invalid_Graph} }
    if destination==nil {
        scene.graph.depth_sense=depth_sense
        scene.graph.pipeline=scene.reverse_pipeline if depth_sense==.Reverse else scene.pipeline
        scene.graph.features.pipelines=scene.features.reverse_pipelines if depth_sense==.Reverse else scene.features.pipelines
    }
    if scene.renderer==nil || u64(len(objects))>scene.graph.object_desc.size/u64(size_of(Object_Data)) { return {},{scene=.Invalid_Geometry} }
    for operation in draws { draw,generated:=operation.(gfx.Draw); if !generated { return {},{scene=.Invalid_Geometry} }; if u64(draw.first_instance)>u64(len(objects)) || u64(draw.instance_count)>u64(len(objects))-u64(draw.first_instance) { return {},{scene=.Invalid_Geometry} } }
    effective_frame,feature_inputs,feature_error:=native_features_prepare(scene,token,frame,draws); if feature_error!={} { return {},feature_error }
    defer { delete(feature_inputs.buffers,scene.allocator); delete(feature_inputs.textures,scene.allocator) }
    packet_error:=scene_graph_draws(&scene.graph,draws); if packet_error!=.None { return {},{packet=packet_error} }
    error:gfx.Gpu_Error
    if token.slot<0 || token.slot>=len(scene.slots) { return {},{gpu=.Invalid_Resource} }
    slot:=scene.slots[token.slot]
    buffers:=scene_graph_inputs(&scene.graph,slot.frame,slot.objects,scene.geometry)
    inputs:[]gfx.Buffer_Input
    frame_values:=[1]Frame_Data{effective_frame}
    error=scene.operations.write_buffer(scene.renderer,token,slot.frame,0,mem.slice_to_bytes(frame_values[:])); if error!=.None { return {},{gpu=error} }
    inputs=buffers[:1]
    if len(draws)>0 {
        error=scene.operations.write_buffer(scene.renderer,token,slot.objects,0,mem.slice_to_bytes(objects)); if error!=.None { return {},{gpu=error} }
        inputs=buffers[:]
    }
    output:=slot.output; if color_override.owner!=nil { output=color_override }
    textures:=scene_graph_textures(&scene.graph,slot.color,slot.depth,output)
    all_buffers:=make([dynamic]gfx.Buffer_Input,0,len(inputs),scene.allocator)
    all_textures:=make([dynamic]gfx.Texture_Input,0,len(textures),scene.allocator)
    success:=false
    defer { if !success { delete(all_buffers); delete(all_textures) } }
    append(&all_buffers,..inputs); append(&all_textures,..textures[:]); append(&all_buffers,..feature_inputs.buffers); append(&all_textures,..feature_inputs.textures)
    if scene.models!=nil {
        extra,model_error:=model_native_prepare(scene.models,&scene.graph,token,effective_frame)
        if model_error!={} { return {},model_error }
        append(&all_buffers,..extra.buffers); append(&all_textures,..extra.textures)
    }
    prepared:=false
    defer { if prepared && !success { scene.composition.aborted(scene.composition.state) } }
    if scene.composition.prepare!=nil {
        if scene.composition.state==nil || scene.composition.accepted==nil || scene.composition.aborted==nil { return {},{gpu=.Unsupported} }
        prepared=true
        extra,composition_error:=scene.composition.prepare(scene.composition.state,scene,token,effective_frame)
        if composition_error!={} { return {},composition_error }
        append(&all_buffers,..extra.buffers); append(&all_textures,..extra.textures)
    }
    late_error:=native_features_finish(scene,token,draws); if late_error!={} { return {},late_error }
    final_error:=scene_graph_finalize(&scene.graph); if final_error!={} { return {},final_error }
    success=true; assembled=true; scene.prepared=true
    return {scene=scene,token=token,buffers=all_buffers,textures=all_textures,composition=prepared},{}
}
/// Submits the same preparation API for one standalone scene and closes its owner on every outcome.
native_scene_render :: proc(scene:^Native_Scene($R),token:gfx.Frame_Token,frame:Frame_Data,objects:[]Object_Data,draws:[]gfx.Draw_Op,color_override:=gfx.Texture_Handle{},depth_sense:Depth_Sense=.Forward)->(gfx.Submission,Native_Error) {
    prepared,prepare_error:=native_scene_prepare(scene,token,frame,objects,draws,color_override,depth_sense=depth_sense)
    if prepare_error!={} { return {},prepare_error }
    defer native_scene_prepared_abort(&prepared)
    submission,error,packet_error:=scene.operations.submit(scene.renderer,token,scene_graph_target(&scene.graph),&scene.graph.plan,prepared.buffers[:],prepared.textures[:])
    if error!=.None || packet_error!=.None { return {},{gpu=error,packet=packet_error} }
    native_scene_prepared_accept(&prepared,submission)
    return submission,{}
}
/// Drains accepted work before releasing scene-owned handles; the native renderer remains caller-owned.
native_scene_destroy :: proc(scene:^Native_Scene($R))->gfx.Gpu_Error {
    if scene.prepared { return .Busy }
    error:=gfx.Gpu_Error.None
    if scene.renderer!=nil {
        for len(scene.pending)>0 {
            wait_error:=native_scene_wait(scene,scene.pending[0]); if wait_error!=.None { error=wait_error; break }
        }
        if scene.operations.release_exports!=nil && scene.graph.target==nil { release_error:=scene.operations.release_exports(scene.renderer,scene_graph_target(&scene.graph)); if release_error!=.None { error=release_error } }
        if scene.models!=nil { model_error:=model_native_destroy(scene.models); if model_error!=.None { error=model_error }; free(scene.models,scene.allocator); scene.models=nil }
        for slot in scene.slots {
            if slot.frame.owner!=nil { destroy_error:=scene.operations.destroy_buffer(scene.renderer,slot.frame); if destroy_error!=.None { error=destroy_error } }
            if slot.objects.owner!=nil { destroy_error:=scene.operations.destroy_buffer(scene.renderer,slot.objects); if destroy_error!=.None { error=destroy_error } }
            if slot.color.owner!=nil { destroy_error:=scene.operations.destroy_texture(scene.renderer,slot.color); if destroy_error!=.None { error=destroy_error } }
            if slot.depth.owner!=nil { destroy_error:=scene.operations.destroy_texture(scene.renderer,slot.depth); if destroy_error!=.None { error=destroy_error } }
            if slot.output.owner!=nil { destroy_error:=scene.operations.destroy_texture(scene.renderer,slot.output); if destroy_error!=.None { error=destroy_error } }
        }
        if scene.geometry.owner!=nil { destroy_error:=scene.operations.destroy_buffer(scene.renderer,scene.geometry); if destroy_error!=.None { error=destroy_error } }
        feature_error:=native_features_destroy(&scene.features); if feature_error!=.None { error=feature_error }
        if scene.reverse_pipeline.owner!=nil { destroy_error:=scene.operations.destroy_pipeline(scene.renderer,scene.reverse_pipeline); if destroy_error!=.None { error=destroy_error } }
        if scene.pipeline.owner!=nil { destroy_error:=scene.operations.destroy_pipeline(scene.renderer,scene.pipeline); if destroy_error!=.None { error=destroy_error } }
        if scene.display_pipeline.owner!=nil { destroy_error:=scene.operations.destroy_pipeline(scene.renderer,scene.display_pipeline); if destroy_error!=.None { error=destroy_error } }
    }
    scene_graph_destroy(&scene.graph); delete(scene.slots,scene.allocator); delete(scene.pending); delete(scene.selected,scene.allocator); scene^={}
    return error
}
/// Replaces scene attachments before acquiring another frame, preserving separately retained readback tickets.
native_scene_resize :: proc(scene:^Native_Scene($R),width,height:u32)->Native_Error {
    if scene.prepared { return {gpu=.Busy} }
    if scene.renderer==nil || width==0 || height==0 { return {scene=.Invalid_Geometry} }
    if width==scene.graph.color_desc.width && height==scene.graph.color_desc.height && scene.feature_settings.shadow_size==scene.graph.features.atlas_desc.width { return {} }
    for len(scene.pending)>0 { error:=native_scene_wait(scene,scene.pending[0]); if error!=.None { return {gpu=error} } }
    replacements:=make([][3]gfx.Texture_Handle,len(scene.slots),scene.allocator)
    allocator:=scene.allocator; defer delete(replacements,allocator)
    color_desc,depth_desc,output_desc:=scene.graph.color_desc,scene.graph.depth_desc,scene.graph.output_desc
    color_desc.width,color_desc.height=width,height; depth_desc.width,depth_desc.height=width,height; output_desc.width,output_desc.height=width,height
    installed:=false; defer {
        if !installed {
            for pair in replacements {
                for handle in pair { if handle.owner!=nil { consumer_cleanup_error(scene.operations.destroy_texture(scene.renderer,handle)) } }
            }
        }
    }
    for &pair in replacements {
        error:gfx.Gpu_Error
        pair[0],error=scene.operations.create_texture(scene.renderer,color_desc); if error!=.None { return {gpu=error} }
        pair[1],error=scene.operations.create_texture(scene.renderer,depth_desc); if error!=.None { return {gpu=error} }
        pair[2],error=scene.operations.create_texture(scene.renderer,output_desc); if error!=.None { return {gpu=error} }
    }
    candidate_features:=scene.features; candidate_features.slots=nil
    features_installed:=false
    defer { if !features_installed { consumer_cleanup_error(native_features_slots_destroy(&candidate_features)) } }
    features_error:=native_features_slots_create(&candidate_features,len(scene.slots),width,height,scene.feature_settings.shadow_size); if features_error!=.None { return {gpu=features_error} }
    previous_graph:=scene.graph
    scene.graph={}
    graph_installed:=false
    defer {
        if !graph_installed {
            scene_graph_destroy(&scene.graph)
            scene.graph=previous_graph
        }
    }
    vertex_count:=int(previous_graph.geometry_desc.size/u64(size_of(Vertex)))
    object_count:=int(previous_graph.object_desc.size/u64(size_of(Object_Data)))
    scene_error:=scene_graph_init(&scene.graph,scene.pipeline,scene.display_pipeline,vertex_count,object_count,width,height,output_desc.format,allocator,scene.features.pipelines,scene.features.shadow_sampler,scene.feature_settings)
    if scene_error!=.None { return {scene=scene_error} }
    binding:^Model_Binding(R)
    defer { if binding!=nil { model_native_bind_abort(binding) } }
    if scene.models!=nil {
        model_error:Native_Error
        binding,model_error=model_native_bind_prepare(scene.models,&scene.graph)
        if model_error!={} { return model_error }
    }
    release_error:=scene.operations.release_exports(scene.renderer,&scene.graph.graph)
    if release_error!=.None { return {gpu=release_error} }
    if binding!=nil { model_native_bind_commit(scene.models,binding); binding=nil }
    for &slot,i in scene.slots {
        consumer_cleanup_error(scene.operations.destroy_texture(scene.renderer,slot.color))
        consumer_cleanup_error(scene.operations.destroy_texture(scene.renderer,slot.depth))
        consumer_cleanup_error(scene.operations.destroy_texture(scene.renderer,slot.output))
        slot.color,slot.depth,slot.output=replacements[i][0],replacements[i][1],replacements[i][2]
    }
    previous_features:=scene.features; scene.features.slots=candidate_features.slots; candidate_features.slots=nil
    consumer_cleanup_error(native_features_slots_destroy(&previous_features))
    features_installed=true
    scene_graph_destroy(&previous_graph)
    graph_installed=true; installed=true; return {}
}
