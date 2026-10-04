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
    if body.heights!=nil { heights:=make([]f32,int(body.rows)*int(body.cols),allocator); copy(heights,body.heights[:len(heights)]); owned.heights=raw_data(heights) }
    return owned
}
@(private="package")
body_destroy :: proc(body:Body,allocator:mem.Allocator) {
    if body.vertices!=nil { delete(body.vertices[:body.vertex_count],allocator) }
    if body.indices!=nil { delete(body.indices[:body.index_count],allocator) }
    if body.heights!=nil { delete(body.heights[:int(body.rows)*int(body.cols)],allocator) }
}
@(private="package")
geometry_equal :: proc(a,b:Body)->bool {
    if a.shape_kind!=b.shape_kind { return false }
    if a.shape_kind==.None { return true }
    if a.sensor!=b.sensor || a.half_extents!=b.half_extents || a.radius!=b.radius || a.half_height!=b.half_height || a.vertex_count!=b.vertex_count || a.index_count!=b.index_count { return false }
    if a.rows!=b.rows || a.cols!=b.cols || a.height_scale!=b.height_scale { return false }
    for i in 0..<int(a.rows)*int(a.cols) { if a.heights[i]!=b.heights[i] { return false } }
    for i in 0..<int(a.vertex_count) { if a.vertices[i]!=b.vertices[i] { return false } }
    for i in 0..<int(a.index_count) { if a.indices[i]!=b.indices[i] { return false } }
    return true
}
@(private="package")
body_equal :: proc(a,b:Body)->bool {
    if !geometry_equal(a,b) { return false }
    left,right:=a,b
    left.heights=nil; right.heights=nil; left.vertices=nil; right.vertices=nil; left.indices=nil; right.indices=nil
    return left==right
}

@(private="package")
preserve_motion :: proc(b:^Backend,entry:Entry,body:Body)->Body {
    result:=body
    pose:Pose; b.pose(entry.native,&pose)
    if entry.body.position==body.position && entry.body.rotation==body.rotation { result.position=pose.position; result.rotation=pose.rotation }
    if entry.body.linear_velocity==body.linear_velocity { result.linear_velocity=pose.linear_velocity }
    return result
}
