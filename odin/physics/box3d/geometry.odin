//! Authored mesh owners are retained independently of transient scene collection buffers.
package box3d

import "core:mem"

@(private="package")
body_clone :: proc(body:Body,allocator:mem.Allocator)->Body {
    owned:=body
    if body.vertex_count>0 {
        vertices:=make([][3]f32,int(body.vertex_count),allocator)
        copy(vertices,body.vertices[:body.vertex_count]); owned.vertices=raw_data(vertices)
    }
    if body.index_count>0 {
        indices:=make([]u32,int(body.index_count),allocator)
        copy(indices,body.indices[:body.index_count]); owned.indices=raw_data(indices)
    }
    return owned
}
@(private="package")
body_destroy :: proc(body:Body,allocator:mem.Allocator) {
    if body.vertices!=nil { delete(body.vertices[:body.vertex_count],allocator) }
    if body.indices!=nil { delete(body.indices[:body.index_count],allocator) }
}
@(private="package")
geometry_equal :: proc(a,b:Body)->bool {
    if a.shape_kind!=b.shape_kind || a.sensor!=b.sensor || a.half_extents!=b.half_extents || a.radius!=b.radius || a.half_height!=b.half_height || a.vertex_count!=b.vertex_count || a.index_count!=b.index_count { return false }
    for i in 0..<int(a.vertex_count) { if a.vertices[i]!=b.vertices[i] { return false } }
    for i in 0..<int(a.index_count) { if a.indices[i]!=b.indices[i] { return false } }
    return true
}
@(private="package")
body_equal :: proc(a,b:Body)->bool {
    if !geometry_equal(a,b) { return false }
    left,right:=a,b
    left.vertices=nil; right.vertices=nil; left.indices=nil; right.indices=nil
    return left==right
}
