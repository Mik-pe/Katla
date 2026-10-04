//! View preparations freeze inputs while the host owns one shared graph and one accepted submission.
package render

import gfx "../../gfx"

/// Exclusive CPU preparation owner; acceptance or abort consumes its owned input arrays once.
Native_Prepared :: struct($R:typeid) {
    scene:^Native_Scene(R),
    token:gfx.Frame_Token,
    buffers:[dynamic]gfx.Buffer_Input,
    textures:[dynamic]gfx.Texture_Input,
    composition:bool,
}
/// Releases an unaccepted preparation and rolls back its staged composition without consuming the token.
native_scene_prepared_abort :: proc(prepared:^Native_Prepared($R)) {
    if prepared.scene==nil { return }
    if prepared.composition { prepared.scene.composition.aborted(prepared.scene.composition.state) }
    prepared.scene.prepared=false
    delete(prepared.buffers); delete(prepared.textures); prepared^={}
}
/// Publishes after one genuine accepted submission; external graph retirement remains with its host.
native_scene_prepared_accept :: proc(prepared:^Native_Prepared($R),submission:gfx.Submission) {
    assert(prepared.scene!=nil && submission.token==prepared.token)
    scene:=prepared.scene
    if scene.graph.target==nil { append(&scene.pending,submission) }
    if prepared.composition { scene.composition.accepted(scene.composition.state,submission) }
    scene.prepared=false
    delete(prepared.buffers); delete(prepared.textures); prepared^={}
}
@(private="package")
native_scene_append_graph :: proc(scene:^Native_Scene($R),destination:^gfx.Graph,namespace:string,depth_sense:Depth_Sense)->Native_Error {
    if scene.renderer==nil || destination==nil || destination==&scene.graph.graph || namespace=="" { return {gpu=.Invalid_Graph} }
    pass_count,buffer_count,image_count:=len(destination.passes),len(destination.buffers),len(destination.images)
    previous:=scene.graph; scene.graph={}
    installed:=false
    defer {
        if !installed {
            scene_graph_destroy(&scene.graph); scene.graph=previous
            gfx.graph_truncate(destination,pass_count,buffer_count,image_count)
        }
    }
    error:=scene_graph_append(&scene.graph,destination,namespace,(scene.reverse_pipeline if depth_sense==.Reverse else scene.pipeline),scene.display_pipeline,int(previous.geometry_desc.size/u64(size_of(Vertex))),int(previous.object_desc.size/u64(size_of(Object_Data))),previous.color_desc.width,previous.color_desc.height,previous.output_desc.format,scene.allocator,(scene.features.reverse_pipelines if depth_sense==.Reverse else scene.features.pipelines),scene.features.shadow_sampler,scene.feature_settings,depth_sense)
    if error!=.None { return {scene=error} }
    binding:^Model_Binding(R)
    defer { if binding!=nil { model_native_bind_abort(binding) } }
    if scene.models!=nil {
        bind_error:Native_Error
        binding,bind_error=model_native_bind_prepare(scene.models,&scene.graph)
        if bind_error!={} { return bind_error }
    }
    if previous.target==nil {
        release_error:=scene.operations.release_exports(scene.renderer,&scene.graph.graph)
        if release_error!=.None { return {gpu=release_error} }
    }
    if binding!=nil { model_native_bind_commit(scene.models,binding); binding=nil }
    scene_graph_destroy(&previous)
    installed=true; return {}
}
