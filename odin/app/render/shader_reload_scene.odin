//! Scene/model/feature publication prepares every native variant and an owned future-upload descriptor snapshot.
package render

import gfx "../../gfx"
import shader "../../gfx/shader"
import "core:mem"

Scene_Shader_Reload_Candidate :: struct($R:typeid) {
    consumers:[]^Native_Consumer(R),surface:^Surface_Shader,model:^Model_Shader,
    next_surface:Surface_Shader,next_model:Model_Shader,
    rasters:[dynamic]Shader_Raster_Replacement,computes:[dynamic]Shader_Compute_Replacement,
    renderer:^R,operations:GPU_Ops(R),allocator:mem.Allocator,
}
@(private="package")
scene_shader_reload_snapshot :: proc(reference:^Surface_Shader,model:^Model_Shader,artifacts:[]shader.Compiled,allocator:mem.Allocator)->(Surface_Shader,Model_Shader,Shader_Reload_Error) {
    surface:=Surface_Shader{allocator=allocator,output_format=reference.output_format}; result_model:=Model_Shader{allocator=allocator}
    success:=false; defer { if !success { surface_shader_destroy(&surface); model_shader_destroy(&result_model) } }
    surface.compiled=shader_reload_snapshot(&artifacts[0],allocator)
    surface.descriptor=reference.descriptor; surface.descriptor.colors=shader_reload_colors(reference.descriptor.colors,allocator)
    error:Shader_Reload_Error
    surface.mapping,error=shader_reload_map(&surface.compiled,surface.descriptor,allocator); if error!=.None { return {},{},error }; surface.descriptor=surface.mapping.descriptor
    surface.postprocess={compiled=shader_reload_snapshot(&artifacts[1],allocator),colors=shader_reload_colors(reference.postprocess.colors,allocator),allocator=allocator}
    post_descriptor:=reference.postprocess.mapping.descriptor; post_descriptor.colors=surface.postprocess.colors
    surface.postprocess.mapping,error=shader_reload_map(&surface.postprocess.compiled,post_descriptor,allocator); if error!=.None { return {},{},error }
    surface.features=new(Feature_Shader,allocator); surface.features.allocator=allocator; surface.features.colors=shader_reload_colors(reference.features.colors,allocator)
    for &compiled,index in surface.features.compiled { compiled=shader_reload_snapshot(&artifacts[index+2],allocator) }
    for &mapping,index in surface.features.graphics {
        descriptor:=reference.features.graphics[index].descriptor
        descriptor.colors=surface.features.colors[index:index+1] if len(descriptor.colors)>0 else nil
        compiled_index:=index if index<2 else 3+(index-2)/5
        mapping,error=shader_reload_map(&surface.features.compiled[compiled_index],descriptor,allocator); if error!=.None { return {},{},error }
    }
    surface.features.compute,error=shader_reload_compute_map(&surface.features.compiled[2],reference.features.compute.descriptor.entry,allocator); if error!=.None { return {},{},error }
    result_model.compiled=shader_reload_snapshot(&artifacts[7],allocator); result_model.colors=shader_reload_colors(model.colors,allocator)
    for &mapping,index in result_model.mappings {
        descriptor:=model.descriptors[index]; descriptor.colors=result_model.colors[index:index+1]
        mapping,error=shader_reload_map(&result_model.compiled,descriptor,allocator); if error!=.None { return {},{},error }; result_model.descriptors[index]=mapping.descriptor
    }
    success=true; return surface,result_model,.None
}
@(private="package")
scene_shader_reload_raster :: proc(candidate:^Scene_Shader_Reload_Candidate($R),target:^gfx.Graphics_Pipeline_Handle,descriptor:gfx.Graphics_Desc)->Shader_Reload_Error {
    handle,error:=candidate.operations.create_pipeline(candidate.renderer,descriptor)
    if error!=.None { shader_reload_native_report(error,false); return .Prepare }
    append(&candidate.rasters,Shader_Raster_Replacement{target,handle}); return .None
}
/// Module order is Surface, Postprocess, Sky, Grid, Cull, ShadowPrimitives, ShadowModels, Model.
/// Any prepared frame or changed logical packet ABI rejects the entire candidate before publication.
scene_shader_reload_prepare :: proc(consumers:[]^Native_Consumer($R),surface:^Surface_Shader,model:^Model_Shader,artifacts:[]shader.Compiled,allocator:=context.allocator)->(^Scene_Shader_Reload_Candidate(R),Shader_Reload_Error) {
    if len(consumers)==0 || surface==nil || model==nil || surface.features==nil || len(artifacts)!=8 { return nil,.Prepare }
    references:=[8]^shader.Compiled{&surface.compiled,&surface.postprocess.compiled,&surface.features.compiled[0],&surface.features.compiled[1],&surface.features.compiled[2],&surface.features.compiled[3],&surface.features.compiled[4],&model.compiled}
    for reference,index in references { if !shader_reload_interface_compatible(reference,&artifacts[index]) { return nil,.Prepare } }
    first:=consumers[0]; if first==nil || first.active==nil || first.renderer==nil { return nil,.Prepare }
    for consumer,index in consumers {
        if consumer==nil || consumer.active==nil || consumer.renderer!=first.renderer || consumer.model_config.shader!=model { return nil,.Prepare }
        if consumer.active.prepared { return nil,.Busy }
        for previous in consumers[:index] { if previous==consumer { return nil,.Prepare } }
    }
    candidate:=new(Scene_Shader_Reload_Candidate(R),allocator); candidate^={surface=surface,model=model,renderer=first.renderer,operations=first.operations,allocator=allocator,consumers=make([]^Native_Consumer(R),len(consumers),allocator),rasters=make([dynamic]Shader_Raster_Replacement,allocator),computes=make([dynamic]Shader_Compute_Replacement,allocator)}; copy(candidate.consumers,consumers)
    snapshot_error:Shader_Reload_Error
    candidate.next_surface,candidate.next_model,snapshot_error=scene_shader_reload_snapshot(surface,model,artifacts,allocator); if snapshot_error!=.None { return candidate,snapshot_error }
    descriptors:=surface_pipelines(&candidate.next_surface)
    for consumer in consumers {
        scene:=consumer.active
        targets:=[3]^gfx.Graphics_Pipeline_Handle{&scene.pipeline,&scene.reverse_pipeline,&scene.display_pipeline}
        stages:=[3]gfx.Graphics_Desc{descriptors.surface,depth_descriptor(descriptors.surface,.Reverse),descriptors.postprocess}
        for target,index in targets { if error:=scene_shader_reload_raster(candidate,target,stages[index]); error!=.None { return candidate,error } }
        features:=&scene.features
        for sense in 0..<2 {
            set:=&features.pipelines if sense==0 else &features.reverse_pipelines
            if sense==0 { if error:=scene_shader_reload_raster(candidate,&set.sky,descriptors.features.sky); error!=.None { return candidate,error } }
            if error:=scene_shader_reload_raster(candidate,&set.grid,depth_descriptor(descriptors.features.grid,Depth_Sense(sense))); error!=.None { return candidate,error }
            for kind in 0..<2 { for effect in 0..<5 {
                if sense==1 && effect==0 { continue }
                descriptor:=descriptors.features.geometry[kind][effect]; if effect!=0 { descriptor=depth_descriptor(descriptor,Depth_Sense(sense)) }
                if error:=scene_shader_reload_raster(candidate,&set.geometry[kind][effect],descriptor); error!=.None { return candidate,error }
            } }
        }
        compute,compute_error:=candidate.operations.create_compute(candidate.renderer,descriptors.features.cull); if compute_error!=.None { shader_reload_native_report(compute_error,false); return candidate,.Prepare }; append(&candidate.computes,Shader_Compute_Replacement{&features.pipelines.cull,compute})
        if scene.models!=nil && len(scene.models.batch.entries)>0 {
            for sense in 0..<2 { for index in 0..<8 {
                target:=&scene.models.pipelines[index] if sense==0 else &scene.models.reverse_pipelines[index]
                if error:=scene_shader_reload_raster(candidate,target,depth_descriptor(candidate.next_model.descriptors[index],Depth_Sense(sense))); error!=.None { return candidate,error }
            } }
        }
    }
    return candidate,.None
}
/// The aggregate owner calls this only after every other family candidate has prepared successfully.
scene_shader_reload_publish :: proc(candidate:^Scene_Shader_Reload_Candidate($R))->^Scene_Shader_Reload_Candidate(R) {
    for &replacement in candidate.rasters { replacement.target^,replacement.handle=replacement.handle,replacement.target^ }
    for &replacement in candidate.computes { replacement.target^,replacement.handle=replacement.handle,replacement.target^ }
    candidate.surface^,candidate.next_surface=candidate.next_surface,candidate.surface^
    candidate.model^,candidate.next_model=candidate.next_model,candidate.model^
    for consumer in candidate.consumers {
        consumer.descriptor=surface_pipelines(candidate.surface)
        scene:=consumer.active; features:=&scene.features
        features.reverse_pipelines.sky=features.pipelines.sky; features.reverse_pipelines.cull=features.pipelines.cull
        for kind in 0..<2 { features.reverse_pipelines.geometry[kind][0]=features.pipelines.geometry[kind][0] }
        scene.graph.pipeline=scene.reverse_pipeline if scene.graph.depth_sense==.Reverse else scene.pipeline
        scene.graph.display_pipeline=scene.display_pipeline
        scene.graph.features.pipelines=features.reverse_pipelines if scene.graph.depth_sense==.Reverse else features.pipelines
        shader_reload_graph_references(scene_graph_target(&scene.graph),candidate.rasters[:],candidate.computes[:])
    }
    return candidate
}
/// Public shader and pipeline parents can retire while accepted GPU recordings keep their native owners.
scene_shader_reload_destroy :: proc(candidate:^Scene_Shader_Reload_Candidate($R)) {
    if candidate==nil { return }
    for replacement in candidate.rasters { error:=candidate.operations.destroy_pipeline(candidate.renderer,replacement.handle); if error!=.None { shader_reload_native_report(error,true) } }
    for replacement in candidate.computes { error:=candidate.operations.destroy_compute(candidate.renderer,replacement.handle); if error!=.None { shader_reload_native_report(error,true) } }
    surface_shader_destroy(&candidate.next_surface); model_shader_destroy(&candidate.next_model)
    delete(candidate.consumers,candidate.allocator); delete(candidate.rasters); delete(candidate.computes); free(candidate,candidate.allocator)
}
