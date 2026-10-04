//! Candidate graph bindings preserve the published model cache until replacement succeeds.
package render

import "core:mem"

Model_Binding_Allocator :: mem.Allocator
Model_Binding :: struct($R:typeid) { candidate:Native_Model(R), allocator:Model_Binding_Allocator }
/// Borrows immutable model owners while preparing independent graph declarations.
model_native_bind_prepare :: proc(cache:^Native_Model($R),scene:^Scene_Graph)->(^Model_Binding(R),Native_Error) {
    token:=new(Model_Binding(R),cache.allocator); token.allocator=cache.allocator
    token.candidate=cache^
    token.candidate.batch.entries=make([]Model_Entry,len(cache.batch.entries),cache.allocator)
    copy(token.candidate.batch.entries,cache.batch.entries)
    token.candidate.graph=nil; token.candidate.image_ids=nil; token.candidate.passes=nil; token.candidate.order=nil; token.candidate.texture_inputs=nil
    token.candidate.slots=make([]Model_Slot,len(cache.slots),cache.allocator)
    copy(token.candidate.slots,cache.slots)
    for &slot in token.candidate.slots { slot.copied_color={}; slot.linear_color={}; slot.color_staging={} }
    error:=model_composite_slots_prepare(&token.candidate,scene)
    if error=={} { error=model_native_bind_graph(&token.candidate,scene) }
    if error!={} { model_native_bind_abort(token); return nil,error }
    return token,{}
}
/// Discards only the candidate graph arrays; borrowed native/source owners remain published.
model_native_bind_abort :: proc(token:^Model_Binding($R)) {
    if token==nil { return }
    allocator:=token.allocator; model_graph_release(&token.candidate); model_composite_slots_destroy(&token.candidate); delete(token.candidate.slots,allocator); delete(token.candidate.batch.entries,allocator); free(token,allocator)
}
/// Publishes independent graph metadata and consumes the candidate exactly once.
model_native_bind_commit :: proc(cache:^Native_Model($R),token:^Model_Binding(R)) {
    model_graph_release(cache)
    model_composite_slots_destroy(cache); delete(cache.slots,cache.allocator); cache.slots=token.candidate.slots
    cache.copied_color=token.candidate.copied_color; cache.linear_color=token.candidate.linear_color; cache.color_staging=token.candidate.color_staging
    cache.copied_desc=token.candidate.copied_desc; cache.linear_desc=token.candidate.linear_desc; cache.staging_desc=token.candidate.staging_desc
    cache.graph=token.candidate.graph
    cache.frame=token.candidate.frame; cache.objects=token.candidate.objects; cache.geometry=token.candidate.geometry
    cache.image_ids=token.candidate.image_ids; cache.passes=token.candidate.passes; cache.order=token.candidate.order; cache.texture_inputs=token.candidate.texture_inputs
    delete(token.candidate.batch.entries,token.allocator); free(token,token.allocator)
}
/// Initial binding uses the same candidate publication path as a native resize.
model_native_bind :: proc(cache:^Native_Model($R),scene:^Scene_Graph)->Native_Error {
    token,error:=model_native_bind_prepare(cache,scene); if error!={} { return error }
    model_native_bind_commit(cache,token); return {}
}
