//! Source freshness and material-image publication form one transaction across all active cameras.
package render

import app ".."
import ecs "../../ecs"
import gfx "../../gfx"
import "core:slice"
import "core:time"

/// A host polls before frame acquisition after retiring its combined accepted frame.
Texture_Reload_Service :: struct { next_poll:time.Time, interval:time.Duration }
/// Accepted replacements are independent of scene, document, selection and history revisions.
Texture_Reload_Receipt :: struct { textures,caches:int, cleanup:gfx.Gpu_Error }
@(private="package")
Texture_Reload_Source :: struct { entity:ecs.Entity_Id,image:i32,bytes:[]byte }
@(private="package")
Texture_Reload_Preparation :: struct($R:typeid) {
    cache:^Native_Model(R),candidate:Native_Model(R),changed:[dynamic]int,
    sampler_count:int,revision:u64,patched:bool,plan:gfx.Compiled_Graph,
}
@(private="package")
texture_reload_source :: proc(owner:^app.Authoring,entity:ecs.Entity_Id)->^app.Scene_Model { return ecs.get_component_mut(&owner.world,entity,app.Scene_Model) }
@(private="package")
/// Exact bytes define freshness, including edits with unchanged size or filesystem timestamps.
texture_source_equal :: proc(a,b:[]byte)->bool { return slice.equal(a,b) }

/// Throttles bounded source reads; failure remains retryable without accepting the failed revision.
model_texture_reload_poll :: proc(service:^Texture_Reload_Service,owner:^app.Authoring,caches:[]^Native_Model($R))->(Texture_Reload_Receipt,Native_Error) {
    now:=time.now(); if time.to_unix_nanoseconds(now)<time.to_unix_nanoseconds(service.next_poll) { return {},{} }
    interval:=service.interval; if interval<=0 { interval=time.Second }
    service.next_poll=time.time_add(now,interval)
    return model_texture_reload(owner,caches)
}

@(private="package")
texture_reload_release :: proc(prepared:^Texture_Reload_Preparation($R),commit:bool) {
    cache:=prepared.cache; candidate:=&prepared.candidate
    if commit && len(prepared.changed)>0 {
        delete(cache.textures); delete(cache.samplers); delete(cache.receipts,cache.allocator)
        delete(cache.image_ids,cache.allocator); delete(cache.texture_inputs,cache.allocator)
        cache.textures=candidate.textures; cache.samplers=candidate.samplers; cache.receipts=candidate.receipts
        cache.image_ids=candidate.image_ids; cache.texture_inputs=candidate.texture_inputs
        if cache.graph!=nil && cache.graph.target==nil && prepared.patched {
            previous:=cache.graph.plan; cache.graph.plan=prepared.plan; prepared.plan={}; gfx.compiled_graph_destroy(&previous)
        }
    } else {
        for index in prepared.changed { texture:=candidate.textures[index]; cache.operations.gpu.destroy_texture(cache.renderer,texture.native.texture); delete(texture.encoded,cache.allocator) }
        for sampler in candidate.samplers[prepared.sampler_count:] { cache.operations.destroy_sampler(cache.renderer,sampler.handle) }
        delete(candidate.textures); delete(candidate.samplers); delete(candidate.receipts,cache.allocator)
        delete(candidate.image_ids,cache.allocator); delete(candidate.texture_inputs,cache.allocator)
    }
    gfx.compiled_graph_destroy(&prepared.plan); delete(prepared.changed)
}
@(private="package")
texture_reload_rollback_graph :: proc(prepared:^Texture_Reload_Preparation($R)) {
    if !prepared.patched { return }
    cache:=prepared.cache; graph:=scene_graph_target(cache.graph)
    for index in prepared.changed { error:=gfx.graph_replace_image(graph,cache.image_ids[index],cache.textures[index].native.desc); assert(error==.None) }
    for index,i in cache.order { error:=model_graph_packet(cache,cache.graph,cache.passes[i],index,true); assert(error=={}) }
    coverage_error:=model_coverage_rebind(cache); assert(coverage_error=={})
    graph.revision=prepared.revision
}
/// Prepares all uploads and sampler owners before publishing any cache or graph resource mapping.
/// A decode, upload, sampler or graph rejection retains the complete prior accepted revision.
model_texture_reload :: proc(owner:^app.Authoring,caches:[]^Native_Model($R))->(Texture_Reload_Receipt,Native_Error) {
    if owner==nil { return {},{scene=.Invalid_Geometry} }
    allocator:=owner.world.allocator
    prepared:=make([]Texture_Reload_Preparation(R),len(caches),allocator); defer delete(prepared,allocator)
    sources:=make([dynamic]Texture_Reload_Source,allocator)
    defer { for source in sources { delete(source.bytes,allocator) }; delete(sources) }
    count:=0; committed:=false
    defer {
        if !committed { for i:=count-1;i>=0;i-=1 { texture_reload_rollback_graph(&prepared[i]) } }
        for &item in prepared[:count] { texture_reload_release(&item,committed) }
    }
    receipt:Texture_Reload_Receipt
    for cache,c in caches {
        if cache==nil || cache.renderer==nil { return {},{gpu=.Invalid_Resource} }
        for previous in caches[:c] { if previous==cache { return {},{gpu=.Invalid_Resource} } }
        item:=&prepared[count]; count+=1; item.cache=cache; item.candidate=cache^
        item.changed=make([dynamic]int,allocator)
        item.candidate.textures=make([dynamic]Model_Texture,0,len(cache.textures),cache.allocator); append(&item.candidate.textures,..cache.textures[:])
        item.candidate.samplers=make([dynamic]Model_Sampler,0,len(cache.samplers),cache.allocator); append(&item.candidate.samplers,..cache.samplers[:])
        item.sampler_count=len(cache.samplers)
        item.candidate.receipts=slice.clone(cache.receipts,cache.allocator)
        item.candidate.image_ids=slice.clone(cache.image_ids,cache.allocator); item.candidate.texture_inputs=slice.clone(cache.texture_inputs,cache.allocator)
        for texture,index in cache.textures {
            if texture.image<0 { continue }
            current:[]byte; found:=false
            for source in sources { if source.entity==texture.entity && source.image==texture.image { current=source.bytes; found=true; break } }
            if !found {
                source:=texture_reload_source(owner,texture.entity)
                read_error:app.Gltf_Error
                current,read_error=app.scene_model_image_read(owner,source,int(texture.image),allocator)
                if read_error!=.None { return {},{scene=.Invalid_Material} }
                append(&sources,Texture_Reload_Source{texture.entity,texture.image,current})
            }
            bytes:=slice.clone(current,cache.allocator)
            if texture_source_equal(bytes,texture.encoded) { delete(bytes,cache.allocator); continue }
            image,decode_error:=texture_image_decode(bytes,cache.allocator)
            if decode_error!=.None { delete(bytes,cache.allocator); return {},{scene=.Invalid_Material} }
            native,upload_error:=native_texture_upload(cache.renderer,cache.operations.gpu,&image,texture.srgb,cache.allocator)
            texture_image_destroy(&image)
            if upload_error!={} { delete(bytes,cache.allocator); return {},upload_error }
            item.candidate.textures[index].native=native; item.candidate.textures[index].encoded=bytes; append(&item.changed,index)
            receipt.textures+=1
        }
        if len(item.changed)==0 { continue }; receipt.caches+=1
        for entry,index in cache.batch.entries { for role in 0..<5 {
            texture:=item.candidate.receipts[index].textures[role]
            sampler,error:=model_sampler_prepare(&item.candidate,owner,entry,role,texture)
            if error!={} { return {},error }; item.candidate.receipts[index].samplers[role]=sampler
        } }
    }
    // Graph changes happen only after every native candidate has completed its upload.
    for &item in prepared {
        cache:=item.cache; if len(item.changed)==0 || cache.graph==nil { continue }
        graph:=scene_graph_target(cache.graph); item.revision=graph.revision; item.patched=true
        for index in item.changed {
            texture:=item.candidate.textures[index].native
            id:=cache.image_ids[index]
            error:=gfx.graph_replace_image(graph,id,texture.desc)
            if error!=.None { return {},{gpu=.Invalid_Graph} }
            item.candidate.image_ids[index]=id; item.candidate.texture_inputs[index]={id,texture.texture}
        }
        for index,i in cache.order { error:=model_graph_packet(&item.candidate,cache.graph,cache.passes[i],index,true); if error!={} { return {},error } }
        coverage_error:=model_coverage_rebind(&item.candidate); if coverage_error!={} { return {},coverage_error }
    }
    for &item in prepared {
        if !item.patched { continue }
        plan,error:=gfx.graph_compile(scene_graph_target(item.cache.graph)); if error!=.None { return {},{gpu=.Invalid_Graph} }; item.plan=plan
    }
    // Old handles are removed only after all candidates and graph plans are admissible.
    for &item in prepared { for index in item.changed {
        texture:=item.cache.textures[index]
        error:=item.cache.operations.gpu.destroy_texture(item.cache.renderer,texture.native.texture)
        if receipt.cleanup==.None { receipt.cleanup=error }; delete(texture.encoded,item.cache.allocator)
    } }
    committed=true; return receipt,{}
}
