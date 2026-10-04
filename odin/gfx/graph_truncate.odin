//! Bounded graph extensions retire their owned declaration tails without affecting retained execution.
package gfx

/// Validates the whole retained prefix before releasing any tail packet or resource declaration.
/// Native accepted recordings own separate copies; callers retire graph exports before role reuse.
/// Callers discard removed identities before declaring replacements; every earlier compiled plan becomes stale.
graph_truncate :: proc(g:^Graph,pass_count,buffer_count,image_count:int)->Graph_Error {
    if g==nil || pass_count<0 || pass_count>len(g.passes) || buffer_count<0 || buffer_count>len(g.buffers) || image_count<0 || image_count>len(g.images) { return .Invalid_Range }
    for pass in g.passes[:pass_count] {
        for access in pass.accesses { if access.resource.owner!=g || access.resource.index<0 || access.resource.index>=buffer_count { return .Invalid_Resource } }
        for access in pass.images { if access.resource.owner!=g || access.resource.index<0 || access.resource.index>=image_count { return .Invalid_Resource } }
        if pass.has_packet {
            buffers:=packet_accesses(g,pass.packet,g.allocator)
            valid:=true
            for access in buffers { if access.resource.owner!=g || access.resource.index<0 || access.resource.index>=buffer_count { valid=false; break } }
            delete(buffers,g.allocator); if !valid { return .Invalid_Resource }
            images:=packet_image_accesses(g,pass.packet,g.allocator)
            for access in images { if access.resource.owner!=g || access.resource.index<0 || access.resource.index>=image_count { valid=false; break } }
            delete(images,g.allocator); if !valid { return .Invalid_Resource }
        }
    }
    if pass_count==len(g.passes) && buffer_count==len(g.buffers) && image_count==len(g.images) { return .None }
    for len(g.passes)>pass_count {
        pass:=pop(&g.passes)
        delete(pass.name,g.allocator); delete(pass.accesses,g.allocator); delete(pass.images,g.allocator); packet_destroy(&pass.packet,g.allocator)
    }
    for len(g.buffers)>buffer_count { pop(&g.buffers) }
    for len(g.images)>image_count { pop(&g.images) }
    g.revision+=1
    return .None
}
