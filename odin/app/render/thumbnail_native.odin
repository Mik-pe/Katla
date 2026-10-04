//! Bounded visible-image uploads publish immutable UI textures without scene or graph policy.
package render

import gfx "../../gfx"
import ui "../../ui"
import resources "../../resources"
import "core:mem"
import "core:strings"
import "core:crypto/sha2"

/// Identity belongs to the retained root generation; revision belongs to the source bytes.
Thumbnail_Request :: struct { root:^resources.Root,root_identity:u64,path:string,revision:u64 }
Thumbnail_Error :: struct { resource:resources.Error,image:Texture_Image_Error,gpu:gfx.Gpu_Error,registry:UI_Error }
/// A failed replacement keeps the accepted texture and revision available to paint.
Thumbnail_View :: struct { texture:ui.Texture_Id,width,height:u32,accepted_revision:u64,ready,loading:bool,error:Thumbnail_Error }
Thumbnail_Receipt :: struct { attempted,published,failed,evicted:int,error:Thumbnail_Error }
@(private="package")
Thumbnail_Entry :: struct { root_identity,attempted_revision,last_visible:u64,path:string,digest:[32]byte,view:Thumbnail_View,native:UI_Texture,job:^Thumbnail_Job }
/// The cache borrows its stationary UI owner and owns every registered thumbnail allocation.
Thumbnail_Cache :: struct($R:typeid) { ui:^UI_GPU(R),entries:[dynamic]Thumbnail_Entry,retired:[dynamic]gfx.Texture_Handle,capacity,max_edge:int,frame,next_id:u64,allocator:mem.Allocator }
@(private="package")
thumbnail_path_clone :: proc(path:string,allocator:mem.Allocator)->string { return strings.clone(path,allocator) }
@(private="package")
thumbnail_digest :: proc(bytes:[]byte)->[32]byte { state:sha2.Context_256; sha2.init_256(&state); sha2.update(&state,bytes); result:[32]byte; sha2.final(&state,result[:]); return result }

/// Creates a bounded cache; opaque IDs occupy a namespace independent of editor viewport IDs.
thumbnail_cache_init :: proc(cache:^Thumbnail_Cache($R),owner:^UI_GPU(R),capacity:=128,max_edge:=128,allocator:=context.allocator)->Thumbnail_Error {
    if cache.ui!=nil || owner==nil || owner.renderer==nil || owner.ops.create_texture==nil || owner.ops.destroy_texture==nil { return {gpu=.Invalid_Resource} }
    if capacity<1 || capacity>1024 || max_edge<1 || max_edge>256 { return {gpu=.Invalid_Range} }
    cache^={ui=owner,capacity=capacity,max_edge=max_edge,next_id=1<<48,allocator=allocator,entries=make([dynamic]Thumbnail_Entry,allocator),retired=make([dynamic]gfx.Texture_Handle,allocator)}
    return {}
}
/// Returns borrowed accepted image metadata without exposing backend handles to browser state.
thumbnail_cache_lookup :: proc(cache:^Thumbnail_Cache($R),root_identity:u64,path:string)->Thumbnail_View {
    for entry in cache.entries { if entry.root_identity==root_identity && entry.path==path { return entry.view } }
    return {}
}
@(private="package")
thumbnail_resize :: proc(image:^Texture_Image,max_edge:int,allocator:mem.Allocator)->(Texture_Image,Texture_Image_Error) {
    if int(max(image.width,image.height))<=max_edge { return image^,.None }
    longest:=max(image.width,image.height)
    width:=max(u32(1),u32(u64(image.width)*u64(max_edge)/u64(longest)))
    height:=max(u32(1),u32(u64(image.height)*u64(max_edge)/u64(longest)))
    pixels,allocation_error:=mem.make([]byte,int(width*height)*4,allocator)
    if allocation_error!=nil || len(pixels)!=int(width*height)*4 || raw_data(pixels)==nil { return {},.Allocation }
    result:=Texture_Image{width,height,pixels,allocator}
    for y in 0..<height { for x in 0..<width {
        sx:=min(image.width-1,u32((u64(x)*2+1)*u64(image.width)/(u64(width)*2)))
        sy:=min(image.height-1,u32((u64(y)*2+1)*u64(image.height)/(u64(height)*2)))
        source:=int(sy*image.width+sx)*4; target:=int(y*width+x)*4
        copy(result.pixels[target:target+4],image.pixels[source:source+4])
    } }
    return result,.None
}
@(private="package")
thumbnail_entry_release :: proc(cache:^Thumbnail_Cache($R),index:int)->Thumbnail_Error {
    entry:=&cache.entries[index]
    if entry.job!=nil { thumbnail_job_destroy(entry.job,cache.allocator);entry.job=nil }
    if entry.view.ready {
        error:=cache.ui.ops.destroy_texture(cache.ui.renderer,entry.native.handle); if error!=.None { return {gpu=error} }
        registry:=ui_gpu_remove_texture(cache.ui,entry.view.texture); if registry!=.None { return {registry=registry} }
    }
    delete(entry.path,cache.allocator); ordered_remove(&cache.entries,index); return {}
}
/// Queues at most four retained background jobs; completed uploads publish only on this thread.
/// Source revisions superseded during decoding are discarded without changing accepted UI state.
thumbnail_cache_update :: proc(cache:^Thumbnail_Cache($R),requests:[]Thumbnail_Request,budget:=4)->Thumbnail_Receipt {
    receipt:Thumbnail_Receipt
    if cache.ui==nil { receipt.error={gpu=.Invalid_Resource}; return receipt }
    if cache.ui.prepared { receipt.error={gpu=.Busy}; return receipt }
    if budget<0 || budget>4 { receipt.error={gpu=.Invalid_Range}; return receipt }
    for len(cache.retired)>0 { error:=cache.ui.ops.destroy_texture(cache.ui.renderer,cache.retired[len(cache.retired)-1]); if error!=.None { receipt.error={gpu=error}; return receipt }; pop(&cache.retired) }
    cache.frame+=1
    for request in requests { for &entry in cache.entries { if entry.root_identity==request.root_identity && entry.path==request.path { entry.last_visible=cache.frame } } }
    active:=0
    for &entry in cache.entries {
        job:=entry.job;if job==nil { continue }
        if !thumbnail_job_ready(job) { active+=1;continue }
        current_revision:u64
        for request in requests { if request.root_identity==entry.root_identity && request.path==entry.path { current_revision=request.revision;break } }
        if current_revision!=job.revision {
            thumbnail_job_destroy(job,cache.allocator);entry.job=nil;entry.view.loading=false;entry.attempted_revision=0;continue
        }
        if job.error!={} {
            entry.view.error=job.error;entry.view.loading=false;receipt.failed+=1;receipt.error=job.error
        } else if job.unchanged {
            entry.view.accepted_revision=job.revision;entry.view.error={};entry.view.loading=false
        } else {
            desc:=gfx.Texture_Desc{width=job.image.width,height=job.image.height,depth=1,layers=1,mip_levels=1,format=.RGBA8_Srgb,usage={.Sampled,.Transfer_Destination}}
            handle,error:=cache.ui.ops.create_texture(cache.ui.renderer,desc,job.image.pixels)
            if error==.Busy { receipt.error={gpu=error};active+=1;continue }
            if error!=.None { entry.view.error={gpu=error};receipt.failed+=1;receipt.error=entry.view.error;entry.view.loading=false } else {
                id:=entry.view.texture;if id==0 { id=ui.Texture_Id(cache.next_id);cache.next_id+=1 }
                native:=UI_Texture{handle=handle,desc=desc,encoding=.Display}
                registry_error:=ui_gpu_texture(cache.ui,id,native)
                if registry_error!=.None {
                    cleanup:=cache.ui.ops.destroy_texture(cache.ui.renderer,handle);if cleanup!=.None { append(&cache.retired,handle) }
                    entry.view.error={registry=registry_error};entry.view.loading=false;receipt.failed+=1;receipt.error=entry.view.error
                } else {
                    old:=entry.native;entry.native=native;entry.digest=job.digest
                    entry.view={texture=id,width=desc.width,height=desc.height,accepted_revision=job.revision,ready=true};receipt.published+=1
                    if old.handle.owner!=nil { cleanup:=cache.ui.ops.destroy_texture(cache.ui.renderer,old.handle);if cleanup!=.None { append(&cache.retired,old.handle);receipt.error={gpu=cleanup} } }
                }
            }
        }
        thumbnail_job_destroy(job,cache.allocator);entry.job=nil
    }
    for request in requests {
        if receipt.attempted>=budget || active>=4 { break }
        if request.root==nil || request.root.file==nil || request.root_identity==0 || request.revision==0 || !resources.valid_relative_path(request.path) { receipt.failed+=1;receipt.error={resource=.Invalid_Path};continue }
        index:=-1
        for entry,i in cache.entries { if entry.root_identity==request.root_identity && entry.path==request.path { index=i;break } }
        if index>=0 && (cache.entries[index].job!=nil || cache.entries[index].attempted_revision==request.revision) { continue }
        if index<0 {
            if len(cache.entries)==cache.capacity {
                oldest:=-1
                for entry,i in cache.entries { if entry.last_visible!=cache.frame && entry.job==nil && (oldest<0 || entry.last_visible<cache.entries[oldest].last_visible) { oldest=i } }
                if oldest<0 { receipt.error={gpu=.Invalid_Range};break }
                error:=thumbnail_entry_release(cache,oldest);if error!={} { receipt.error=error;break };receipt.evicted+=1
            }
            append(&cache.entries,Thumbnail_Entry{root_identity=request.root_identity,path=thumbnail_path_clone(request.path,cache.allocator),last_visible=cache.frame});index=len(cache.entries)-1
        }
        entry:=&cache.entries[index]
        retained,retain_error:=resources.root_clone(request.root,thumbnail_background_allocator())
        if retain_error!=.None { entry.view.error={resource=retain_error};receipt.failed+=1;receipt.error=entry.view.error;continue }
        job:=new(Thumbnail_Job,cache.allocator)
        job^={root=retained,path=entry.path,revision=request.revision,previous=entry.digest,has_previous=entry.view.ready,max_edge=cache.max_edge}
        if !thumbnail_job_start(job) { thumbnail_job_destroy(job,cache.allocator);entry.view.error={image=.Allocation};receipt.failed+=1;receipt.error=entry.view.error;continue }
        entry.job=job;entry.view.loading=true;entry.attempted_revision=request.revision
        receipt.attempted+=1;active+=1
    }
    return receipt
}
/// Unregisters and releases owned images before the borrowed UI owner is destroyed.
thumbnail_cache_destroy :: proc(cache:^Thumbnail_Cache($R))->Thumbnail_Error {
    if cache.ui==nil { return {} }; if cache.ui.prepared { return {gpu=.Busy} }
    for len(cache.retired)>0 { error:=cache.ui.ops.destroy_texture(cache.ui.renderer,cache.retired[len(cache.retired)-1]); if error!=.None { return {gpu=error} }; pop(&cache.retired) }
    for len(cache.entries)>0 { error:=thumbnail_entry_release(cache,len(cache.entries)-1); if error!={} { return error } }
    delete(cache.entries); delete(cache.retired); cache^={}; return {}
}
