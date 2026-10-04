//! Owned passive recordings join graph declarations with facts observed at native call sites.
package gfx
import "core:mem"
import "core:strings"

/// Identifies the driver whose native scope numbers are recorded verbatim.
Capture_Backend :: enum { None, Metal, Vulkan }
/// Feedback is updated only by the submission's actual completion owner.
Capture_Feedback :: enum { Pending, Completed, Failed, Not_Submitted }
/// Auxiliary allocations have no authored graph resource.
Capture_Resource_Kind :: enum { None, Buffer, Image, Auxiliary }
/// Descriptor resources and direct draw buffers have distinct native binding contracts.
Capture_Binding_Path :: enum { Descriptor, Vertex, Index, Indirect, Transfer }
/// An encoder identity denotes an actual native encoder or command buffer.
Capture_Event_Kind :: enum { Allocation, Encoder_Begin, Encoder_End, Pass_Begin, Pass_End, Buffer_Barrier, Image_Barrier, Global_Barrier, Alias, Bind_Buffer, Bind_Image, Bind_Sampler, Bind_Pipeline, Argument_Table, Residency, Attachment, Submit }
/// Native facts contain stable encounter ordinals, never pointers or native handles.
/// Negative pass/resource indices denote auxiliary operations or allocations.
Capture_Event :: struct {
    kind:Capture_Event_Kind,
    pass_index:int,
    phase_index:int,
    resource_kind:Capture_Resource_Kind,
    binding_path:Capture_Binding_Path,
    resource_index:int,
    alias_previous_kind:Capture_Resource_Kind,
    alias_previous_index,previous_pass_index:int,
    object,encoder,heap,pipeline,table,layout:u64,
    offset,size,alignment,memory_flags:u64,
    memory_type:u32,
    source_stages,destination_stages,source_access,destination_access,binding_stages:u64,
    old_layout,new_layout,native_visibility:u64,
    native_load,native_store:u64,
    clear_color:[4]f64,
    clear_depth:f64,
    clear_stencil:u32,
    transfer_value:u32,
    transfer_region:Image_Region,
    buffer_range:Buffer_Range,
    image_range:Image_Range,
    group,binding,array_index,native_index:u32,
    emitted:bool,
    label,reason:string,
}
/// A range dependency uses pointer-free logical indices and exact authored modes.
Capture_Dependency :: struct {
    before,after,resource_index:int,
    source_access_index,destination_access_index:int,
    resource_kind:Capture_Resource_Kind,
    source_mode,destination_mode:Access_Mode,
    source_usage,destination_usage:u32,
    source_bytes,destination_bytes:Buffer_Range,
    source_image,destination_image:Image_Range,
}
Capture_Buffer_Access :: struct { resource_index:int,range:Buffer_Range,mode:Access_Mode,usage:Buffer_Usage }
Capture_Image_Access :: struct { resource_index:int,range:Image_Range,mode:Access_Mode,usage:Texture_Usage }
/// Culled declarations remain visible together with the compiler's actual live order.
Capture_Liveness :: enum { Side_Effect_Root, Exported_Producer, Required_Predecessor, Not_Required }
Capture_Pass :: struct { index,order:int,liveness:Capture_Liveness,name:string,kind:Pass_Kind,live,side_effect,has_packet:bool,buffers:[]Capture_Buffer_Access,images:[]Capture_Image_Access }
Capture_Buffer :: struct { index:int,desc:Buffer_Desc,imported,exported,live:bool,first,last:int }
Capture_Image :: struct { index:int,desc:Texture_Desc,contract:Image_Import,imported,exported,live:bool,first,last:int }
/// Independent owned data survives graph mutation, native retirement and frame-slot reuse.
Capture_Snapshot :: struct {
    schema_version:u32,
    native_captured:bool,
    backend:Capture_Backend,
    revision,generation,submission:u64,
    slot:int,
    feedback:Capture_Feedback,
    passes:[dynamic]Capture_Pass,
    buffers:[dynamic]Capture_Buffer,
    images:[dynamic]Capture_Image,
    dependencies:[dynamic]Capture_Dependency,
    events,expected:[dynamic]Capture_Event,
}
/// The stationary backend owner records on its submission thread; callbacks do not mutate this store.
Capture_Store :: struct { enabled,recording:bool,allocator:mem.Allocator,candidate:Capture_Snapshot,accepted:[dynamic]Capture_Snapshot,identities:map[rawptr]u64 }
/// Initializes optional diagnostics without affecting scheduling or resource lifetimes.
capture_init :: proc(s:^Capture_Store,allocator:=context.allocator) { s.allocator=allocator; s.accepted=make([dynamic]Capture_Snapshot,allocator); s.identities=make(map[rawptr]u64,allocator) }
/// Releases an independent capture using the allocator that received ownership.
capture_snapshot_destroy :: proc(c:^Capture_Snapshot,allocator:=context.allocator) {
    for pass in c.passes { delete(pass.name,allocator); delete(pass.buffers,allocator); delete(pass.images,allocator) }
    for events in ([2][dynamic]Capture_Event{c.events,c.expected}) { for event in events { delete(event.label,allocator); delete(event.reason,allocator) }; delete(events) }
    delete(c.passes); delete(c.buffers); delete(c.images); delete(c.dependencies); c^={}
}
/// Cancels a rejected candidate while preserving every accepted recording.
capture_abandon :: proc(s:^Capture_Store) { if s.recording { capture_snapshot_destroy(&s.candidate,s.allocator) }; s.recording=false; clear(&s.identities) }
/// Releases diagnostics after native submission owners have stopped accessing the store.
capture_destroy :: proc(s:^Capture_Store) { capture_abandon(s); for &c in s.accepted { capture_snapshot_destroy(&c,s.allocator) }; delete(s.accepted); delete(s.identities); s^={} }
/// Starts a candidate only for a validated graph; backend call sites add observed events afterwards.
capture_begin :: proc(s:^Capture_Store,backend:Capture_Backend,token:Frame_Token,g:^Graph,plan:^Compiled_Graph) {
    capture_abandon(s); if !s.enabled || g==nil || plan.owner!=g || plan.revision!=g.revision { return }
    s.recording=true; c:=&s.candidate
    c^={schema_version=1,backend=backend,revision=g.revision,generation=token.generation,slot=token.slot,
        passes=make([dynamic]Capture_Pass,s.allocator),buffers=make([dynamic]Capture_Buffer,s.allocator),images=make([dynamic]Capture_Image,s.allocator),dependencies=make([dynamic]Capture_Dependency,s.allocator),events=make([dynamic]Capture_Event,s.allocator),expected=make([dynamic]Capture_Event,s.allocator)}
    for pass,i in g.passes {
        order:=-1; for live,n in plan.order { if live.index==i { order=n; break } }
        p:=Capture_Pass{index=i,order=order,name=strings.clone(pass.name,s.allocator),kind=pass.kind,live=order>=0,side_effect=pass.side_effect,has_packet=pass.has_packet,buffers=make([]Capture_Buffer_Access,len(pass.accesses),s.allocator),images=make([]Capture_Image_Access,len(pass.images),s.allocator)}
        p.liveness=.Not_Required
        if p.live {
            p.liveness=.Required_Predecessor
            for access in pass.accesses { if access_writes(access.mode) && g.buffers[access.resource.index].exported { p.liveness=.Exported_Producer } }
            for access in pass.images { if access_writes(access.mode) && g.images[access.resource.index].exported { p.liveness=.Exported_Producer } }
            if pass.side_effect { p.liveness=.Side_Effect_Root }
        }
        for access,j in pass.accesses { p.buffers[j]={access.resource.index,access.range,access.mode,access.usage} }
        for access,j in pass.images { p.images[j]={access.resource.index,access.range,access.mode,access.usage} }; append(&c.passes,p)
    }
    for b,i in g.buffers { first,last,_,_,live:=resource_lifetime(g,plan,Resource_Id{g,i}); append(&c.buffers,Capture_Buffer{i,b.desc,b.imported,b.exported,live,first,last}) }
    for image,i in g.images { first,last,_,_,live:=resource_lifetime(g,plan,Image_Id{g,i}); append(&c.images,Capture_Image{i,image.desc,image.contract,image.imported,image.exported,live,first,last}) }
    for h in plan.hazards { append(&c.dependencies,Capture_Dependency{before=h.before.index,after=h.after.index,resource_index=h.resource.index,resource_kind=.Buffer,source_mode=h.source.mode,destination_mode=h.destination.mode,source_usage=u32(h.source.usage),destination_usage=u32(h.destination.usage),source_bytes=h.source.range,destination_bytes=h.destination.range}) }
    for h in plan.image_hazards { append(&c.dependencies,Capture_Dependency{before=h.before.index,after=h.after.index,resource_index=h.resource.index,resource_kind=.Image,source_mode=h.source.mode,destination_mode=h.destination.mode,source_usage=u32(h.source.usage),destination_usage=u32(h.destination.usage),source_image=h.source.range,destination_image=h.destination.range}) }
    for &d in c.dependencies {
        if d.resource_kind==.Buffer {
            for a,i in g.passes[d.before].accesses { if a.resource.index==d.resource_index && a.range==d.source_bytes { d.source_access_index=i; break } }
            for a,i in g.passes[d.after].accesses { if a.resource.index==d.resource_index && a.range==d.destination_bytes { d.destination_access_index=i; break } }
        } else {
            for a,i in g.passes[d.before].images { if a.resource.index==d.resource_index && a.range==d.source_image { d.source_access_index=i; break } }
            for a,i in g.passes[d.after].images { if a.resource.index==d.resource_index && a.range==d.destination_image { d.destination_access_index=i; break } }
        }
    }
}
/// Returns a deterministic recording-local ordinal for an actual object identity.
capture_object :: proc(s:^Capture_Store,identity:rawptr)->u64 { if !s.recording || identity==nil { return 0 }; if id,found:=s.identities[identity]; found { return id }; id:=u64(len(s.identities))+1; s.identities[identity]=id; return id }
@(private="package")
capture_event_clone :: proc(e:Capture_Event,a:mem.Allocator)->Capture_Event { result:=e; result.label=strings.clone(e.label,a); result.reason=strings.clone(e.reason,a); return result }
/// Appends a native observation at the actual allocation, encoding or binding site.
capture_record :: proc(s:^Capture_Store,event:Capture_Event) { if s.recording { append(&s.candidate.events,capture_event_clone(event,s.allocator)) } }
/// Records an independently translated compiler expectation for native-scope comparison.
capture_expect :: proc(s:^Capture_Store,event:Capture_Event) { if s.recording { append(&s.candidate.expected,capture_event_clone(event,s.allocator)) } }
/// Publishes a candidate only after the native queue accepts this exact submission.
capture_accept :: proc(s:^Capture_Store,submission:Submission) {
    if !s.recording { return }; assert(submission.token.slot==s.candidate.slot && submission.token.generation==s.candidate.generation)
    s.candidate.native_captured=true; s.candidate.submission=submission.id; append(&s.accepted,s.candidate); s.candidate={}; s.recording=false; clear(&s.identities)
    if len(s.accepted)>16 { capture_snapshot_destroy(&s.accepted[0],s.allocator); ordered_remove(&s.accepted,0) }
}
/// Joins real completion feedback without introducing waits or polling solely for diagnostics.
capture_feedback :: proc(s:^Capture_Store,submission:u64,feedback:Capture_Feedback) { for &c in s.accepted { if c.submission==submission { c.feedback=feedback; return } } }
/// Clones the selected accepted submission, or the latest when zero is requested.
capture_snapshot :: proc(s:^Capture_Store,submission:u64=0,allocator:=context.allocator)->(Capture_Snapshot,bool) {
    selected:^Capture_Snapshot
    for &c in s.accepted { if submission==0 || c.submission==submission { selected=&c } }; if selected==nil { return {},false }
    result:=selected^; result.passes=make([dynamic]Capture_Pass,allocator); result.buffers=make([dynamic]Capture_Buffer,allocator); result.images=make([dynamic]Capture_Image,allocator); result.dependencies=make([dynamic]Capture_Dependency,allocator); result.events=make([dynamic]Capture_Event,allocator); result.expected=make([dynamic]Capture_Event,allocator)
    for p in selected.passes { q:=p; q.name=strings.clone(p.name,allocator); q.buffers=make([]Capture_Buffer_Access,len(p.buffers),allocator); copy(q.buffers,p.buffers); q.images=make([]Capture_Image_Access,len(p.images),allocator); copy(q.images,p.images); append(&result.passes,q) }
    append(&result.buffers,..selected.buffers[:]); append(&result.images,..selected.images[:]); append(&result.dependencies,..selected.dependencies[:])
    for e in selected.events { append(&result.events,capture_event_clone(e,allocator)) }; for e in selected.expected { append(&result.expected,capture_event_clone(e,allocator)) }; return result,true
}

/// Returns compiler diagnostics before execution, explicitly without native observations.
capture_graph_snapshot :: proc(g:^Graph,plan:^Compiled_Graph,allocator:=context.allocator)->(Capture_Snapshot,bool) {
    store:Capture_Store; capture_init(&store,allocator); defer capture_destroy(&store); store.enabled=true
    capture_begin(&store,.None,{},g,plan); if !store.recording { return {},false }
    result:=store.candidate; result.feedback=.Not_Submitted; store.candidate={}; store.recording=false; return result,true
}
