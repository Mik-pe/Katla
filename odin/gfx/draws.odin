//! Typed mesh and indirect commands share the ordinary graphics resource contract.
package gfx

import "core:mem"

Vertex_Format :: enum { Float, Float2, Float3, Float4, Uint, Uint2, Uint3, Uint4, Sint, Sint2, Sint3, Sint4, Unorm8x4 }
Vertex_Step :: enum { Vertex, Instance }
/// Shader locations select attributes; bindings name ordinary vertex allocations.
Vertex_Attribute :: struct { location,binding,offset:u32, format:Vertex_Format }
Vertex_Layout_Binding :: struct { binding,stride:u32, step:Vertex_Step }
Vertex_Layout :: struct { attributes:[]Vertex_Attribute, buffers:[]Vertex_Layout_Binding }
Vertex_Binding :: struct { binding:u32, access:Buffer_Access }
Index_Format :: enum { Uint16, Uint32 }
/// Ordinary vertex-buffer geometry, with explicit instance and vertex offsets.
Draw_Vertices :: struct { vertices:[]Vertex_Binding, vertex_count,instance_count,first_vertex,first_instance:u32 }
/// Index buffers preserve signed base-vertex offsets and first-instance identity.
Draw_Indexed :: struct { vertices:[]Vertex_Binding, index:Buffer_Access, index_format:Index_Format, index_count,instance_count,first_index,first_instance:u32, vertex_offset:i32 }
/// Native indirect commands remain GPU-authored data, declared as graph reads.
Draw_Indirect :: struct { vertices:[]Vertex_Binding, command:Buffer_Access, count,stride:u32 }
Draw_Indexed_Indirect :: struct { vertices:[]Vertex_Binding, index:Buffer_Access, index_format:Index_Format, command:Buffer_Access, count,stride:u32 }
/// Every supported geometry source has one canonical packet form.
Draw_Op :: union { Draw, Draw_Vertices, Draw_Indexed, Draw_Indirect, Draw_Indexed_Indirect }
Draw_Kind :: enum { Generated, Vertices, Indexed, Indirect, Indexed_Indirect }
Draw_Kinds :: bit_set[Draw_Kind]
/// Immutable constants identify actual reflected buffer slots.
Constant_Binding :: struct { group,slot:u32, stages:Shader_Stages, usage:Buffer_Usage, bytes:[]byte }
/// Explicit target-local coordinates; disabled state uses the full attachment.
Viewport :: struct { enabled:bool, x,y,width,height,min_depth,max_depth:f64 }
Scissor :: struct { enabled:bool, x,y,width,height:u32 }
Stencil_Op :: enum { Keep, Zero, Replace, Increment_Clamp, Decrement_Clamp, Invert, Increment_Wrap, Decrement_Wrap }
Stencil_Face :: struct { compare:Compare_Op, fail,depth_fail,pass:Stencil_Op }
Stencil_State :: struct { enabled:bool, front,back:Stencil_Face, reference,read_mask,write_mask:u32 }
Depth_Bias :: struct { constant,slope,clamp:f32 }
/// Byte width of one portable vertex attribute.
vertex_format_size :: proc(format:Vertex_Format)->u32 {
    #partial switch format {
    case .Float2,.Uint2,.Sint2: return 8
    case .Float3,.Uint3,.Sint3: return 12
    case .Float4,.Uint4,.Sint4: return 16
    case: return 4
    }
}
/// A layout cannot address missing bindings, duplicate locations or overlapping attributes.
vertex_layout_valid :: proc(layout:Vertex_Layout)->bool {
    if len(layout.buffers)>16 || len(layout.attributes)>32 { return false }
    for buffer,i in layout.buffers {
        if buffer.stride==0 { return false }
        for old in layout.buffers[:i] { if old.binding==buffer.binding { return false } }
    }
    for attribute,i in layout.attributes {
        found:=false
        for buffer in layout.buffers {
            if buffer.binding!=attribute.binding { continue }; found=true
            size:=vertex_format_size(attribute.format)
            if attribute.offset>buffer.stride || size>buffer.stride-attribute.offset { return false }
        }
        if !found { return false }
        for old in layout.attributes[:i] {
            if old.location==attribute.location { return false }
            if old.binding==attribute.binding && range_overlaps({u64(old.offset),u64(vertex_format_size(old.format))},{u64(attribute.offset),u64(vertex_format_size(attribute.format))}) { return false }
        }
    }
    return len(layout.buffers)==0 || len(layout.attributes)>0
}
/// Resolves the geometry source without depending on a backend command enum.
draw_kind :: proc(draw:Draw_Op)->Draw_Kind {
    switch d in draw {
    case Draw: return .Generated
    case Draw_Vertices: return .Vertices
    case Draw_Indexed: return .Indexed
    case Draw_Indirect: return .Indirect
    case Draw_Indexed_Indirect: return .Indexed_Indirect
    }
    return .Generated
}
@(private="package")
draw_clone :: proc(draw:Draw_Op,allocator:mem.Allocator)->Draw_Op {
    #partial switch d in draw {
    case Draw_Vertices: result:=d; result.vertices=clone_slice(d.vertices,allocator); return result
    case Draw_Indexed: result:=d; result.vertices=clone_slice(d.vertices,allocator); return result
    case Draw_Indirect: result:=d; result.vertices=clone_slice(d.vertices,allocator); return result
    case Draw_Indexed_Indirect: result:=d; result.vertices=clone_slice(d.vertices,allocator); return result
    }
    return draw
}
@(private="package")
draw_destroy :: proc(draw:^Draw_Op,allocator:mem.Allocator) {
    #partial switch d in draw^ {
    case Draw_Vertices: delete(d.vertices,allocator)
    case Draw_Indexed: delete(d.vertices,allocator)
    case Draw_Indirect: delete(d.vertices,allocator)
    case Draw_Indexed_Indirect: delete(d.vertices,allocator)
    }
    draw^={}
}
@(private="package")
draw_buffer_accesses :: proc(draw:Draw_Op,accesses:^[dynamic]Buffer_Access) {
    #partial switch d in draw {
    case Draw_Vertices: for vertex in d.vertices { append(accesses,vertex.access) }
    case Draw_Indexed: for vertex in d.vertices { append(accesses,vertex.access) }; append(accesses,d.index)
    case Draw_Indirect: for vertex in d.vertices { append(accesses,vertex.access) }; append(accesses,d.command)
    case Draw_Indexed_Indirect: for vertex in d.vertices { append(accesses,vertex.access) }; append(accesses,d.index,d.command)
    }
}
