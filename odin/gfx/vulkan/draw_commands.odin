//! Ordinary vertex, index and GPU-authored indirect geometry share one render encoder.
package katla_vulkan

import gfx ".."
import vk "vendor:vulkan"

@(private="package")
bind_vertices :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,bindings:[]gfx.Vertex_Binding)->gfx.Gpu_Error {
    for binding in bindings {
        buffer,ok:=resolve_buffer(r,prepared,binding.access.resource)
        if !ok { return .Invalid_Resource }
        offset:=vk.DeviceSize(binding.access.range.offset)
        capture_buffer_binding_expect(r,binding.access.range,binding.access.resource.index,0,binding.binding,path=.Vertex)
        r.table.CmdBindVertexBuffers(slot.command,binding.binding,1,&buffer.object,&offset)
        capture_buffer_binding(r,buffer,{u64(offset),binding.access.range.size},0,binding.binding,label="vkCmdBindVertexBuffers",path=.Vertex)
    }
    return .None
}
@(private="package")
bind_index :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,index:gfx.Buffer_Access,format:gfx.Index_Format)->gfx.Gpu_Error {
    buffer,ok:=resolve_buffer(r,prepared,index.resource)
    if !ok { return .Invalid_Resource }
    capture_buffer_binding_expect(r,index.range,index.resource.index,0,0,path=.Index)
    r.table.CmdBindIndexBuffer(slot.command,buffer.object,vk.DeviceSize(index.range.offset),.UINT16 if format==.Uint16 else .UINT32)
    capture_buffer_binding(r,buffer,index.range,0,0,label="vkCmdBindIndexBuffer",path=.Index)
    return .None
}
@(private="package")
encode_draw :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,draw:gfx.Draw_Op)->gfx.Gpu_Error {
    switch d in draw {
    case gfx.Draw: r.table.CmdDraw(slot.command,d.vertex_count,d.instance_count,d.first_vertex,d.first_instance)
    case gfx.Draw_Vertices:
        error:=bind_vertices(r,slot,prepared,d.vertices); if error!=.None { return error }
        r.table.CmdDraw(slot.command,d.vertex_count,d.instance_count,d.first_vertex,d.first_instance)
    case gfx.Draw_Indexed:
        error:=bind_vertices(r,slot,prepared,d.vertices); if error!=.None { return error }
        error=bind_index(r,slot,prepared,d.index,d.index_format); if error!=.None { return error }
        r.table.CmdDrawIndexed(slot.command,d.index_count,d.instance_count,d.first_index,d.vertex_offset,d.first_instance)
    case gfx.Draw_Indirect:
        if !r.indirect_first_instance { return .Unsupported }
        error:=bind_vertices(r,slot,prepared,d.vertices); if error!=.None { return error }
        buffer,ok:=resolve_buffer(r,prepared,d.command.resource); if !ok { return .Invalid_Resource }
        for i in 0..<d.count { offset:=d.command.range.offset+u64(i)*u64(d.stride);capture_buffer_binding_expect(r,{offset,16},d.command.resource.index,0,0,path=.Indirect);r.table.CmdDrawIndirect(slot.command,buffer.object,vk.DeviceSize(offset),1,d.stride);capture_buffer_binding(r,buffer,{offset,16},0,0,label="vkCmdDrawIndirect",path=.Indirect) }
    case gfx.Draw_Indexed_Indirect:
        if !r.indirect_first_instance { return .Unsupported }
        error:=bind_vertices(r,slot,prepared,d.vertices); if error!=.None { return error }
        error=bind_index(r,slot,prepared,d.index,d.index_format); if error!=.None { return error }
        buffer,ok:=resolve_buffer(r,prepared,d.command.resource); if !ok { return .Invalid_Resource }
        for i in 0..<d.count { offset:=d.command.range.offset+u64(i)*u64(d.stride);capture_buffer_binding_expect(r,{offset,20},d.command.resource.index,0,0,path=.Indirect);r.table.CmdDrawIndexedIndirect(slot.command,buffer.object,vk.DeviceSize(offset),1,d.stride);capture_buffer_binding(r,buffer,{offset,20},0,0,label="vkCmdDrawIndexedIndirect",path=.Indirect) }
    }
    return .None
}
