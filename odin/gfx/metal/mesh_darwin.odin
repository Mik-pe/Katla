#+build darwin, arm64
//! Mesh layouts and native direct/indirect draws use ordinary declared buffer allocations.
package metal

import gfx ".."
import MTL "vendor:darwin/Metal"
import NS "core:sys/darwin/Foundation"

@(private="package")
vertex_format :: proc(format:gfx.Vertex_Format)->MTL.VertexFormat {
    switch format {
    case .Float: return .Float
    case .Float2: return .Float2
    case .Float3: return .Float3
    case .Float4: return .Float4
    case .Uint: return .UInt
    case .Uint2: return .UInt2
    case .Uint3: return .UInt3
    case .Uint4: return .UInt4
    case .Sint: return .Int
    case .Sint2: return .Int2
    case .Sint3: return .Int3
    case .Sint4: return .Int4
    case .Unorm8x4: return .UChar4Normalized
    }
    unreachable()
}

@(private="package")
vertex_descriptor :: proc(layout:gfx.Vertex_Layout)->(^MTL.VertexDescriptor,gfx.Gpu_Error) {
    if !gfx.vertex_layout_valid(layout) { return nil,.Invalid_Shader }
    native:=MTL.VertexDescriptor.alloc()->init()
    if native==nil { return nil,.Allocation_Failed }
    success:=false; defer { if !success { native->release() } }
    attributes:=send(^NS.Object,native,"attributes")
    for attribute in layout.attributes {
        if attribute.location>=31 || attribute.binding>20 { return nil,.Invalid_Shader }
        target:=send(^NS.Object,attributes,"objectAtIndexedSubscript:",NS.UInteger(attribute.location))
        send(nil,target,"setFormat:",vertex_format(attribute.format)); send(nil,target,"setOffset:",NS.UInteger(attribute.offset)); send(nil,target,"setBufferIndex:",NS.UInteger(10+attribute.binding))
    }
    buffers:=send(^NS.Object,native,"layouts")
    for binding in layout.buffers {
        if binding.binding>20 { return nil,.Invalid_Shader }
        target:=send(^NS.Object,buffers,"objectAtIndexedSubscript:",NS.UInteger(10+binding.binding))
        send(nil,target,"setStride:",NS.UInteger(binding.stride))
        send(nil,target,"setStepFunction:",MTL.VertexStepFunction.PerVertex if binding.step==.Vertex else MTL.VertexStepFunction.PerInstance)
        send(nil,target,"setStepRate:",NS.UInteger(1))
    }
    success=true
    return native,.None
}

@(private="package")
stencil_op :: proc(op:gfx.Stencil_Op)->MTL.StencilOperation {
    switch op {
    case .Keep: return .Keep
    case .Zero: return .Zero
    case .Replace: return .Replace
    case .Increment_Clamp: return .IncrementClamp
    case .Decrement_Clamp: return .DecrementClamp
    case .Invert: return .Invert
    case .Increment_Wrap: return .IncrementWrap
    case .Decrement_Wrap: return .DecrementWrap
    }
    unreachable()
}

@(private="package")
stencil_descriptor :: proc(face:gfx.Stencil_Face,state:gfx.Stencil_State)->^MTL.StencilDescriptor {
    native:=MTL.StencilDescriptor.alloc()->init()
    if native==nil { return nil }
    native->setStencilCompareFunction(compare_op(face.compare))
    native->setStencilFailureOperation(stencil_op(face.fail)); native->setDepthFailureOperation(stencil_op(face.depth_fail)); native->setDepthStencilPassOperation(stencil_op(face.pass))
    native->setReadMask(state.read_mask); native->setWriteMask(state.write_mask)
    return native
}

@(private="package")
bind_vertices :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,pipeline:^Native_Graphics,packet:gfx.Render,constants:[]gfx.Constant_Binding,encoder:^NS.Object,vertices:[]gfx.Vertex_Binding)->gfx.Gpu_Error {
    table,err:=render_table(r,slot,pipeline,prepared,packet,constants,.Vertex)
    if err!=.None { return err }
    for binding in vertices {
        if binding.binding>20 { return .Invalid_Range }
        buffer,ok:=resolve_buffer(r,prepared,binding.access.resource)
        if !ok { return .Invalid_Resource }
        send(nil,table,"setAddress:atIndex:",buffer.object->gpuAddress()+binding.access.range.offset,NS.UInteger(10+binding.binding))
    }
    send(nil,encoder,"setArgumentTable:atStages:",table,NS.UInteger(1))
    return .None
}

@(private="package")
encode_draw :: proc(r:^Renderer,slot:^Native_Frame,prepared:^gfx.Prepared_Graph,pipeline:^Native_Graphics,packet:gfx.Render,phase:gfx.Render_Phase,encoder:^NS.Object,operation:gfx.Draw_Op)->gfx.Gpu_Error {
    topology:=primitive(pipeline.desc.topology)
    switch draw in operation {
    case gfx.Draw:
        if draw.vertex_count>0 && draw.instance_count>0 { send(nil,encoder,"drawPrimitives:vertexStart:vertexCount:instanceCount:baseInstance:",topology,NS.UInteger(draw.first_vertex),NS.UInteger(draw.vertex_count),NS.UInteger(draw.instance_count),NS.UInteger(draw.first_instance)) }
    case gfx.Draw_Vertices:
        err:=bind_vertices(r,slot,prepared,pipeline,packet,phase.constants,encoder,draw.vertices); if err!=.None { return err }
        if draw.vertex_count>0 && draw.instance_count>0 { send(nil,encoder,"drawPrimitives:vertexStart:vertexCount:instanceCount:baseInstance:",topology,NS.UInteger(draw.first_vertex),NS.UInteger(draw.vertex_count),NS.UInteger(draw.instance_count),NS.UInteger(draw.first_instance)) }
    case gfx.Draw_Indexed:
        err:=bind_vertices(r,slot,prepared,pipeline,packet,phase.constants,encoder,draw.vertices); if err!=.None { return err }
        index,ok:=resolve_buffer(r,prepared,draw.index.resource); if !ok { return .Invalid_Resource }
        size:=u64(2) if draw.index_format==.Uint16 else u64(4)
        offset:=u64(draw.first_index)*size
        if offset>draw.index.range.size { return .Invalid_Range }
        if draw.index_count>0 && draw.instance_count>0 { send(nil,encoder,"drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferLength:instanceCount:baseVertex:baseInstance:",topology,NS.UInteger(draw.index_count),MTL.IndexType.UInt16 if draw.index_format==.Uint16 else MTL.IndexType.UInt32,index.object->gpuAddress()+draw.index.range.offset+offset,NS.UInteger(draw.index.range.size-offset),NS.UInteger(draw.instance_count),NS.Integer(draw.vertex_offset),NS.UInteger(draw.first_instance)) }
    case gfx.Draw_Indirect:
        err:=bind_vertices(r,slot,prepared,pipeline,packet,phase.constants,encoder,draw.vertices); if err!=.None { return err }
        buffer,ok:=resolve_buffer(r,prepared,draw.command.resource); if !ok { return .Invalid_Resource }
        for i in 0..<draw.count { send(nil,encoder,"drawPrimitives:indirectBuffer:",topology,buffer.object->gpuAddress()+draw.command.range.offset+u64(i)*u64(draw.stride)) }
    case gfx.Draw_Indexed_Indirect:
        err:=bind_vertices(r,slot,prepared,pipeline,packet,phase.constants,encoder,draw.vertices); if err!=.None { return err }
        index,index_ok:=resolve_buffer(r,prepared,draw.index.resource)
        buffer,buffer_ok:=resolve_buffer(r,prepared,draw.command.resource)
        if !index_ok || !buffer_ok { return .Invalid_Resource }
        for i in 0..<draw.count { send(nil,encoder,"drawIndexedPrimitives:indexType:indexBuffer:indexBufferLength:indirectBuffer:",topology,MTL.IndexType.UInt16 if draw.index_format==.Uint16 else MTL.IndexType.UInt32,index.object->gpuAddress()+draw.index.range.offset,NS.UInteger(draw.index.range.size),buffer.object->gpuAddress()+draw.command.range.offset+u64(i)*u64(draw.stride)) }
    }
    return .None
}
