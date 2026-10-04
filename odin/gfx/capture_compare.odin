//! Read-only comparisons distinguish authored execution from actual native scopes.
package gfx
import "core:mem"

/// A finding references immutable snapshot indices; it owns no borrowed strings.
Capture_Divergence_Kind :: enum {
    Invalid_Schema, Invalid_Pass, Missing_Pass, Duplicate_Pass, Pass_Order, Pass_Scope,
    Encoder_Scope, Resource_Ownership, Resource_Range, Undeclared_Resource,
    Missing_Event, Duplicate_Event, Unexpected_Event, Native_Scope, Pipeline_Scope,
    Table_Scope, Residency_Scope, Dependency_Coverage,
}
/// Negative event indices mean that the missing observation has no native event.
Capture_Divergence :: struct { kind:Capture_Divergence_Kind, pass_index,resource_index,expected_index,event_index,dependency_index:int }
@(private="package")
capture_finding :: proc(out:^[dynamic]Capture_Divergence,kind:Capture_Divergence_Kind,event_index:int= -1,expected_index:int= -1,pass_index:int= -1,resource_index:int= -1,dependency_index:int= -1) {
    append(out,Capture_Divergence{kind=kind,pass_index=pass_index,resource_index=resource_index,expected_index=expected_index,event_index=event_index,dependency_index=dependency_index})
}
@(private="package")
capture_pass_index :: proc(c:^Capture_Snapshot,index:int)->int { for pass,i in c.passes { if pass.index==index { return i } };return -1 }
@(private="package")
capture_bytes_contain :: proc(outer,inner:Buffer_Range)->bool {
    return inner.size>0 && inner.offset>=outer.offset && inner.offset-outer.offset<=outer.size && inner.size<=outer.size-(inner.offset-outer.offset)
}
@(private="package")
capture_images_contain :: proc(outer,inner:Image_Range)->bool {
    return inner.mip_count>0 && inner.layer_count>0 && inner.aspects!={} && inner.aspects&outer.aspects==inner.aspects && inner.base_mip>=outer.base_mip && inner.base_mip-outer.base_mip<=outer.mip_count && inner.mip_count<=outer.mip_count-(inner.base_mip-outer.base_mip) && inner.base_layer>=outer.base_layer && inner.base_layer-outer.base_layer<=outer.layer_count && inner.layer_count<=outer.layer_count-(inner.base_layer-outer.base_layer)
}
@(private="package")
capture_event_resource :: proc(c:^Capture_Snapshot,event:Capture_Event)->(bool,bool) {
    #partial switch event.resource_kind {
    case .Buffer:
        for buffer in c.buffers { if buffer.index==event.resource_index { return true,event.buffer_range.size==0 || range_valid(event.buffer_range,buffer.desc.size) } }
    case .Image:
        for image in c.images { if image.index==event.resource_index { return true,event.image_range.aspects=={} || image_range_valid(event.image_range,image.desc) } }
    case .None,.Auxiliary: return event.resource_index<0,true
    }
    return false,false
}
@(private="package")
capture_declared_event :: proc(pass:Capture_Pass,event:Capture_Event)->bool {
    #partial switch event.resource_kind {
    case .Buffer: for access in pass.buffers { if access.resource_index==event.resource_index && capture_bytes_contain(access.range,event.buffer_range) { return true } }
    case .Image: for access in pass.images { if access.resource_index==event.resource_index && capture_images_contain(access.range,event.image_range) { return true } }
    case .None,.Auxiliary: return true
    }
    return false
}
@(private="package")
capture_event_key_equal :: proc(a,b:Capture_Event)->bool {
    return a.kind==b.kind && a.pass_index==b.pass_index && a.phase_index==b.phase_index && a.resource_kind==b.resource_kind && a.resource_index==b.resource_index && a.group==b.group && a.binding==b.binding && a.array_index==b.array_index && a.native_index==b.native_index && a.binding_path==b.binding_path
}
@(private="package")
capture_native_scope_equal :: proc(required,observed:Capture_Event)->bool {
    return required.transfer_value==observed.transfer_value && required.transfer_region==observed.transfer_region && required.alias_previous_kind==observed.alias_previous_kind && required.alias_previous_index==observed.alias_previous_index && required.previous_pass_index==observed.previous_pass_index && required.source_stages==observed.source_stages && required.destination_stages==observed.destination_stages && required.source_access==observed.source_access && required.destination_access==observed.destination_access && required.binding_stages==observed.binding_stages && required.old_layout==observed.old_layout && required.new_layout==observed.new_layout && required.native_visibility==observed.native_visibility && required.emitted==observed.emitted && required.buffer_range==observed.buffer_range && required.image_range==observed.image_range && required.native_load==observed.native_load && required.native_store==observed.native_store && required.clear_color==observed.clear_color && required.clear_depth==observed.clear_depth && required.clear_stencil==observed.clear_stencil && (required.object==0 || required.object==observed.object) && (required.encoder==0 || required.encoder==observed.encoder) && (required.pipeline==0 || required.pipeline==observed.pipeline) && (required.table==0 || required.table==observed.table) && (required.layout==0 || required.layout==observed.layout) && required.offset==observed.offset && required.size==observed.size && required.alignment==observed.alignment && required.memory_flags==observed.memory_flags && required.memory_type==observed.memory_type && (required.heap==0 || required.heap==observed.heap)
}
@(private="package")
capture_synchronization :: proc(kind:Capture_Event_Kind)->bool { return kind==.Buffer_Barrier || kind==.Image_Barrier || kind==.Global_Barrier || kind==.Alias }
@(private="package")
capture_table_known :: proc(c:^Capture_Snapshot,event:Capture_Event)->bool {
    if event.table==0 { return false }
    for table in c.events { if table.kind==.Argument_Table && table.table==event.table && table.pass_index==event.pass_index && (table.pipeline==0 || event.pipeline==0 || table.pipeline==event.pipeline) && (table.layout==0 || event.layout==0 || table.layout==event.layout) { return true } }
    return false
}
@(private="package")
capture_residency_known :: proc(c:^Capture_Snapshot,object:u64)->bool {
    if object==0 { return false }
    for event in c.events {
        #partial switch event.kind {
        case .Allocation: if event.object==object || event.heap==object { return true }
        case .Bind_Buffer,.Bind_Image,.Bind_Sampler,.Bind_Pipeline: if event.object==object || event.pipeline==object { return true }
        }
    };return false
}
@(private="package")
capture_dependency_global :: proc(c:^Capture_Snapshot,after:int)->bool {
    if c.backend!=.Metal { return false }
    for event in c.expected { if event.kind==.Global_Barrier && event.pass_index==after && event.native_visibility&1!=0 && event.source_stages!=0 && event.destination_stages!=0 { return true } };return false
}
@(private="package")
capture_dependency_covered :: proc(c:^Capture_Snapshot,dependency:Capture_Dependency)->bool {
    if capture_dependency_global(c,dependency.after) { return true }
    #partial switch dependency.resource_kind {
    case .Buffer:
        start:=max(dependency.source_bytes.offset,dependency.destination_bytes.offset)
        // Valid declarations make both endpoint additions bounded by their buffer capacity.
        finish:=min(dependency.source_bytes.offset+dependency.source_bytes.size,dependency.destination_bytes.offset+dependency.destination_bytes.size)
        if finish<=start { return false }
        for event in c.expected { if event.kind==.Buffer_Barrier && event.pass_index==dependency.after && event.resource_kind==.Buffer && event.resource_index==dependency.resource_index && event.source_stages!=0 && event.destination_stages!=0 && capture_bytes_contain(event.buffer_range,{start,finish-start}) { return true } };return false
    case .Image:
        first_mip:=max(dependency.source_image.base_mip,dependency.destination_image.base_mip)
        last_mip:=min(dependency.source_image.base_mip+dependency.source_image.mip_count,dependency.destination_image.base_mip+dependency.destination_image.mip_count)
        first_layer:=max(dependency.source_image.base_layer,dependency.destination_image.base_layer)
        last_layer:=min(dependency.source_image.base_layer+dependency.source_image.layer_count,dependency.destination_image.base_layer+dependency.destination_image.layer_count)
        aspects:=dependency.source_image.aspects&dependency.destination_image.aspects
        if first_mip>=last_mip || first_layer>=last_layer || aspects=={} { return false }
        for mip in first_mip..<last_mip { for layer in first_layer..<last_layer { for aspect in aspects {
            covered:=false
            for event in c.expected { if event.kind==.Image_Barrier && event.pass_index==dependency.after && event.resource_kind==.Image && event.resource_index==dependency.resource_index && event.source_stages!=0 && event.destination_stages!=0 && capture_images_contain(event.image_range,{mip,1,layer,1,{aspect}}) { covered=true;break } }
            if !covered { return false }
        } } };return true
    }
    return false
}
/// Returns independently owned typed findings without waiting, touching a graph or editing feedback.
/// Expected synchronization comes from the backend translator, including explicit no-call facts.
/// Metal global barriers retain their global stages/visibility and are never expanded into fake ranges.
capture_compare :: proc(c:^Capture_Snapshot,allocator:mem.Allocator=context.allocator)->[dynamic]Capture_Divergence {
    out:=make([dynamic]Capture_Divergence,allocator)
    if c==nil || c.schema_version!=1 { capture_finding(&out,.Invalid_Schema);return out }
    if !c.native_captured { return out }
    begins:=make([]int,len(c.passes),allocator);defer delete(begins,allocator)
    ends:=make([]int,len(c.passes),allocator);defer delete(ends,allocator)
    matched:=make([]bool,len(c.events),allocator);defer delete(matched,allocator)
    next_order:=0;active_pass:=-1;active_encoder:u64
    for event,i in c.events {
        pass_index:=capture_pass_index(c,event.pass_index)
        if event.pass_index>=0 && (pass_index<0 || !c.passes[pass_index].live) { capture_finding(&out,.Invalid_Pass,i,pass_index=event.pass_index) }
        owned,valid:=capture_event_resource(c,event)
        if !owned { capture_finding(&out,.Resource_Ownership,i,pass_index=event.pass_index,resource_index=event.resource_index) }
        else if !valid { capture_finding(&out,.Resource_Range,i,pass_index=event.pass_index,resource_index=event.resource_index) }
        #partial switch event.kind {
        case .Pass_Begin:
            if pass_index>=0 {
                begins[pass_index]+=1
                if begins[pass_index]>1 { capture_finding(&out,.Duplicate_Pass,i,pass_index=event.pass_index) }
                if c.passes[pass_index].order!=next_order { capture_finding(&out,.Pass_Order,i,pass_index=event.pass_index) }
            }
            if active_pass>=0 { capture_finding(&out,.Pass_Scope,i,pass_index=event.pass_index) }
            active_pass=event.pass_index;next_order+=1
        case .Pass_End:
            if pass_index>=0 { ends[pass_index]+=1 }
            if active_pass!=event.pass_index || (active_encoder!=0 && c.backend!=.Vulkan) { capture_finding(&out,.Pass_Scope,i,pass_index=event.pass_index) }
            active_pass= -1
        case .Encoder_Begin:
            if event.encoder==0 || active_encoder!=0 { capture_finding(&out,.Encoder_Scope,i,pass_index=event.pass_index) }
            active_encoder=event.encoder
        case .Encoder_End:
            if event.encoder==0 || active_encoder!=event.encoder { capture_finding(&out,.Encoder_Scope,i,pass_index=event.pass_index) }
            active_encoder=0
        case .Bind_Pipeline:
            if event.pipeline==0 || event.encoder==0 || event.encoder!=active_encoder { capture_finding(&out,.Pipeline_Scope,i,pass_index=event.pass_index) }
        case .Argument_Table:
            if event.table==0 || event.object==0 { capture_finding(&out,.Table_Scope,i,pass_index=event.pass_index) }
        case .Bind_Buffer,.Bind_Image,.Bind_Sampler:
            if event.binding_path==.Descriptor && !capture_table_known(c,event) { capture_finding(&out,.Table_Scope,i,pass_index=event.pass_index,resource_index=event.resource_index) }
            if pass_index>=0 && !capture_declared_event(c.passes[pass_index],event) { capture_finding(&out,.Undeclared_Resource,i,pass_index=event.pass_index,resource_index=event.resource_index) }
        case .Attachment:
            if pass_index>=0 && !capture_declared_event(c.passes[pass_index],event) { capture_finding(&out,.Undeclared_Resource,i,pass_index=event.pass_index,resource_index=event.resource_index) }
        case .Residency:
            if event.table==0 || !capture_residency_known(c,event.object) { capture_finding(&out,.Residency_Scope,i,pass_index=event.pass_index) }
        }
        if event.encoder!=0 && event.kind!=.Encoder_Begin && event.kind!=.Encoder_End && event.encoder!=active_encoder { capture_finding(&out,.Encoder_Scope,i,pass_index=event.pass_index) }
    }
    if active_pass>=0 { capture_finding(&out,.Pass_Scope,pass_index=active_pass) }
    if active_encoder!=0 { capture_finding(&out,.Encoder_Scope) }
    for pass,i in c.passes { if pass.live && (begins[i]==0 || ends[i]==0) { capture_finding(&out,.Missing_Pass,pass_index=pass.index) };if ends[i]>1 { capture_finding(&out,.Duplicate_Pass,pass_index=pass.index) } }
    for dependency,i in c.dependencies {
        before,after:=capture_pass_index(c,dependency.before),capture_pass_index(c,dependency.after)
        if before<0 || after<0 || !c.passes[before].live || !c.passes[after].live { continue }
        if !capture_dependency_covered(c,dependency) { capture_finding(&out,.Dependency_Coverage,pass_index=dependency.after,resource_index=dependency.resource_index,dependency_index=i) }
    }
    for required,j in c.expected {
        found:=-1;candidate:=-1
        for event,i in c.events {
            if matched[i] || !capture_event_key_equal(required,event) { continue }
            if candidate<0 { candidate=i }
            if capture_native_scope_equal(required,event) { found=i;break }
        }
        if found>=0 { matched[found]=true;continue }
        if candidate>=0 { matched[candidate]=true;capture_finding(&out,.Native_Scope,candidate,j,required.pass_index,required.resource_index) }
        else { capture_finding(&out,.Missing_Event,expected_index=j,pass_index=required.pass_index,resource_index=required.resource_index) }
    }
    for event,i in c.events {
        if matched[i] { continue }
        expected_kind:=capture_synchronization(event.kind);duplicate:=false
        for required in c.expected { if required.kind==event.kind { expected_kind=true };if capture_event_key_equal(required,event) { duplicate=true } }
        if duplicate { capture_finding(&out,.Duplicate_Event,i,pass_index=event.pass_index,resource_index=event.resource_index) }
        else if expected_kind { capture_finding(&out,.Unexpected_Event,i,pass_index=event.pass_index,resource_index=event.resource_index) }
    }
    return out
}
