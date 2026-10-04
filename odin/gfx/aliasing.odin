//! Physical allocation reuse requires explicit disjoint graph lifetimes.
package gfx

/// Alias barriers name logical resources rather than native allocation addresses.
Alias_Resource :: union { Resource_Id, Image_Id }
/// Native encoders must order the previous owner's last use before its successor.
Alias_Handoff :: struct { before,after:Pass_Id, previous,next:Alias_Resource }
@(private="package")
resource_lifetime :: proc(g:^Graph,plan:^Compiled_Graph,resource:Alias_Resource)->(first,last:int,imported,exported,used:bool) {
    first=len(plan.order); last=-1
    for id,i in plan.order {
        found:=false
        switch r in resource {
        case Resource_Id:
            for access in g.passes[id.index].accesses { if access.resource==r { found=true; break } }
        case Image_Id:
            for access in g.passes[id.index].images { if access.resource==r { found=true; break } }
        }
        if found { first=min(first,i); last=max(last,i); used=true }
    }
    switch r in resource {
    case Resource_Id: imported=g.buffers[r.index].imported; exported=g.buffers[r.index].exported
    case Image_Id: imported=g.images[r.index].imported && g.images[r.index].contract.initialized; exported=g.images[r.index].exported
    }
    if exported && used { last=len(plan.order) }
    return
}
@(private="package")
prepare_alias :: proc(prepared:^Prepared_Graph,g:^Graph,plan:^Compiled_Graph,a,b:Alias_Resource)->Packet_Error {
    af,al,ai,_,au:=resource_lifetime(g,plan,a); bf,bl,bi,_,bu:=resource_lifetime(g,plan,b)
    if !au || !bu { return .Aliased_Resource }
    previous,next:=a,b; previous_last,next_first:=al,bf; next_imported:=bi
    if bl<af { previous,next=b,a; previous_last,next_first=bl,af; next_imported=ai }
    else if al>=bf { return .Aliased_Resource }
    if next_imported { return .Aliased_Resource }
    if prepared.aliases.allocator.procedure==nil { prepared.aliases=make([dynamic]Alias_Handoff,g.allocator) }
    append(&prepared.aliases,Alias_Handoff{plan.order[previous_last],plan.order[next_first],previous,next})
    return .None
}
@(private="package")
prepare_buffer_alias :: proc(prepared:^Prepared_Graph,g:^Graph,plan:^Compiled_Graph,a,b:Resource_Id)->Packet_Error { return prepare_alias(prepared,g,plan,a,b) }
@(private="package")
prepare_image_alias :: proc(prepared:^Prepared_Graph,g:^Graph,plan:^Compiled_Graph,a,b:Image_Id)->Packet_Error { return prepare_alias(prepared,g,plan,a,b) }
