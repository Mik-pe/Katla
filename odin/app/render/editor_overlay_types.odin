//! Editor overlay state is borrowed from the shell; triangles and GPU owners stay in the renderer.
package render

import ecs "../../ecs"
import km "../../math"
import "core:mem"

Overlay_Mode :: enum { Translate, Rotate, Scale }
Overlay_Handle :: enum { None, Axis_X, Axis_Y, Axis_Z, Plane_XY, Plane_XZ, Plane_YZ }
Overlay_Contact :: struct { a,b:ecs.Entity_Id,point,normal:km.Vec3 }
/// Gizmo state, visibility and completed native contacts are collected on the application thread.
Editor_Overlay_State :: struct {
    selected:[]ecs.Entity_Id,
    pivot:km.Vec3,
    basis:km.Mat4,
    mode:Overlay_Mode,
    hover,captured:Overlay_Handle,
    gizmo,physics,reverb,billboards:bool,
    contacts:[]Overlay_Contact,
}
Overlay_Vertex :: struct { position,color,uv:km.Vec4 }
Overlay_Triangle :: struct { entity:ecs.Entity_Id,handle:Overlay_Handle,always,has_entity:bool }
/// World triangles also define exact gizmo and billboard hit testing, rather than separate proxy shapes.
Overlay_Mesh :: struct { vertices:[dynamic]Overlay_Vertex,triangles:[dynamic]Overlay_Triangle,gizmo_first:int,overflow:bool,view_projection:km.Mat4,clip_y:f32,allocator:mem.Allocator }
Overlay_Hit :: struct { entity:ecs.Entity_Id,handle:Overlay_Handle,distance:f32,hit:bool }
/// Releases every CPU triangle after native preparation has cloned or uploaded it.
overlay_mesh_destroy :: proc(mesh:^Overlay_Mesh) { delete(mesh.vertices); delete(mesh.triangles); mesh^={} }
