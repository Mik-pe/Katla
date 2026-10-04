//! Draw validation checks declared native ranges without interpreting GPU-authored commands.
package gfx

import "core:math"

/// Checks descriptor structure before a native pipeline can be published.
graphics_desc_valid :: proc(desc:Graphics_Desc)->bool {
    if len(desc.vertex_entry)==0 || !vertex_layout_valid(desc.vertex) || len(desc.colors)>8 { return false }
    if len(desc.colors)==0 && !desc.depth.enabled { return false }
    if desc.depth.enabled && .Color in texture_aspects(desc.depth.format) { return false }
    if desc.stencil.enabled && !(.Stencil in texture_aspects(desc.depth.format)) { return false }
    for value in ([3]f32{desc.depth_bias.constant,desc.depth_bias.slope,desc.depth_bias.clamp}) { if math.is_nan(value) || math.is_inf(value) { return false } }
    for color in desc.colors { if texture_aspects(color.format)!={.Color} { return false } }
    return true
}
@(private="package")
validate_vertex_bindings :: proc(vertices:[]Vertex_Binding)->Packet_Error {
    for vertex,i in vertices {
        if vertex.access.mode!=.Read || vertex.access.usage!=.Vertex { return .Invalid_Binding }
        for previous in vertices[:i] { if previous.binding==vertex.binding { return .Invalid_Binding } }
    }
    return .None
}
@(private="package")
validate_index :: proc(access:Buffer_Access,format:Index_Format)->Packet_Error {
    size:u64=format==.Uint16 ? 2 : 4
    if access.mode!=.Read || access.usage!=.Index || access.range.offset%size!=0 || access.range.size%size!=0 { return .Invalid_Binding }
    return .None
}
@(private="package")
validate_indirect :: proc(access:Buffer_Access,count,stride:u32,indexed:bool)->Packet_Error {
    size:u64=indexed ? 20 : 16
    if access.mode!=.Read || access.usage!=.Indirect || access.range.offset%4!=0 || u64(stride)<size || stride%4!=0 { return .Invalid_Binding }
    if count>0 && u64(count-1)*u64(stride)+size>access.range.size { return .Invalid_Binding }
    return .None
}
@(private="package")
validate_draw_packet :: proc(draw:Draw_Op)->Packet_Error {
    #partial switch d in draw {
    case Draw_Vertices: return validate_vertex_bindings(d.vertices)
    case Draw_Indexed:
        err:=validate_vertex_bindings(d.vertices); if err!=.None { return err }
        err=validate_index(d.index,d.index_format); if err!=.None { return err }
        size:u64=d.index_format==.Uint16 ? 2 : 4
        if (u64(d.first_index)+u64(d.index_count))*size>d.index.range.size { return .Invalid_Binding }
    case Draw_Indirect:
        err:=validate_vertex_bindings(d.vertices); if err!=.None { return err }
        return validate_indirect(d.command,d.count,d.stride,false)
    case Draw_Indexed_Indirect:
        err:=validate_vertex_bindings(d.vertices); if err!=.None { return err }
        err=validate_index(d.index,d.index_format); if err!=.None { return err }
        return validate_indirect(d.command,d.count,d.stride,true)
    }
    return .None
}
@(private="package")
preflight_vertices :: proc(layout:Vertex_Layout,vertices:[]Vertex_Binding,vertex_count,instance_count,first_vertex,first_instance:u32,known_vertex,known_instance:bool)->Packet_Error {
    if len(layout.buffers)!=len(vertices) { return .Invalid_Binding }
    for requirement in layout.buffers {
        found:=false
        for binding in vertices {
            if binding.binding!=requirement.binding { continue }; found=true
            if binding.access.range.size<u64(requirement.stride) { return .Invalid_Binding }
            if (requirement.step==.Vertex && known_vertex) || (requirement.step==.Instance && known_instance) {
                count,first:=vertex_count,first_vertex
                if requirement.step==.Instance { count,first=instance_count,first_instance }
                if count==0 { continue }
                entries:=u64(count)+u64(first)
                if entries>max(u64)/u64(requirement.stride) || entries*u64(requirement.stride)>binding.access.range.size { return .Invalid_Binding }
            }
        }
        if !found { return .Missing_Binding }
    }
    return .None
}
@(private="package")
preflight_draw :: proc(draw:Draw_Op,info:Graphics_Info)->Packet_Error {
    if !(draw_kind(draw) in info.supported_draws) { return .Invalid_Pipeline }
    if !vertex_layout_valid(info.vertex) { return .Invalid_Pipeline }
    switch d in draw {
    case Draw: if len(info.vertex.buffers)>0 { return .Invalid_Binding }
    case Draw_Vertices: return preflight_vertices(info.vertex,d.vertices,d.vertex_count,d.instance_count,d.first_vertex,d.first_instance,true,true)
    case Draw_Indexed: return preflight_vertices(info.vertex,d.vertices,0,d.instance_count,0,d.first_instance,false,true)
    case Draw_Indirect: return preflight_vertices(info.vertex,d.vertices,0,0,0,0,false,false)
    case Draw_Indexed_Indirect: return preflight_vertices(info.vertex,d.vertices,0,0,0,0,false,false)
    }
    return .None
}
