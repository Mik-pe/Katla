//! Thin cylinders, rings and filled tips are genuine world-space triangles shared by drawing and hit tests.
package render

import km "../../math"
import m "core:math"

@(private="package")
overlay_triangle :: proc(mesh:^Overlay_Mesh,a,b,c:km.Vec3,color:km.Vec4,identity:Overlay_Triangle,uv:[3]km.Vec2={},icon:u32=0) {
    if mesh.overflow || len(mesh.vertices)>4_000_000-3 { mesh.overflow=true; return }
    for point,i in ([3]km.Vec3{a,b,c}) { append(&mesh.vertices,Overlay_Vertex{km.vec4(point,1),color,{uv[i][0],uv[i][1],f32(icon),0}}) }
    append(&mesh.triangles,identity)
}
@(private="package")
overlay_cylinder :: proc(mesh:^Overlay_Mesh,a,b:km.Vec3,radius:f32,color:km.Vec4,identity:Overlay_Triangle,tip:bool=false,segments:int=16) {
    if mesh.overflow { return }
    delta:=b-a; if km.length_squared(delta)<1e-12 { return }
    axis:=km.normalize(delta); helper:=km.Vec3{0,0,1} if abs(axis[1])>0.95 else km.Vec3{0,1,0}
    right:=km.normalize(km.cross(axis,helper)); up:=km.cross(axis,right)
    for i in 0..<segments {
        angle,following:=f32(i)*2*m.PI/f32(segments),f32(i+1)*2*m.PI/f32(segments)
        p:=(right*m.cos(angle)+up*m.sin(angle))*radius
        q:=(right*m.cos(following)+up*m.sin(following))*radius
        if tip { overlay_triangle(mesh,a+p,a+q,b,color,identity) }
        else { overlay_triangle(mesh,a+p,a+q,b+p,color,identity); overlay_triangle(mesh,b+p,a+q,b+q,color,identity); overlay_triangle(mesh,b,b+p,b+q,color,identity) }
        overlay_triangle(mesh,a,a+q,a+p,color,identity)
    }
}
@(private="package")
overlay_ring :: proc(mesh:^Overlay_Mesh,center,right,up:km.Vec3,radius,tube:f32,color:km.Vec4,identity:Overlay_Triangle,segments:int=64,sides:int=8) {
    if mesh.overflow { return }
    normal:=km.normalize(km.cross(right,up))
    for i in 0..<segments {
        theta0,theta1:=f32(i)*2*m.PI/f32(segments),f32(i+1)*2*m.PI/f32(segments)
        radial0,radial1:=right*m.cos(theta0)+up*m.sin(theta0),right*m.cos(theta1)+up*m.sin(theta1)
        for j in 0..<sides {
            if mesh.overflow { return }
            phi0,phi1:=f32(j)*2*m.PI/f32(sides),f32(j+1)*2*m.PI/f32(sides)
            a:=center+radial0*(radius+tube*m.cos(phi0))+normal*(tube*m.sin(phi0))
            b:=center+radial1*(radius+tube*m.cos(phi0))+normal*(tube*m.sin(phi0))
            c:=center+radial0*(radius+tube*m.cos(phi1))+normal*(tube*m.sin(phi1))
            d:=center+radial1*(radius+tube*m.cos(phi1))+normal*(tube*m.sin(phi1))
            overlay_triangle(mesh,a,b,c,color,identity); overlay_triangle(mesh,c,b,d,color,identity)
        }
    }
}
@(private="package")
overlay_box :: proc(mesh:^Overlay_Mesh,world:km.Mat4,extents:km.Vec3,color:km.Vec4,identity:Overlay_Triangle,filled:bool=false) {
    corners:=[8]km.Vec3{{-1,-1,-1},{1,-1,-1},{1,-1,1},{-1,-1,1},{-1,1,-1},{1,1,-1},{1,1,1},{-1,1,1}}
    for &point in corners { point=km.transform_point(world,point*extents) }
    if filled {
        for triangle in ([12][3]int{{0,2,1},{0,3,2},{4,5,6},{4,6,7},{0,1,5},{0,5,4},{1,2,6},{1,6,5},{2,3,7},{2,7,6},{3,0,4},{3,4,7}}) { overlay_triangle(mesh,corners[triangle[0]],corners[triangle[1]],corners[triangle[2]],color,identity) }
    } else {
        for edge in ([12][2]int{{0,1},{1,2},{2,3},{3,0},{4,5},{5,6},{6,7},{7,4},{0,4},{1,5},{2,6},{3,7}}) { overlay_cylinder(mesh,corners[edge[0]],corners[edge[1]],0.005,color,identity,segments=8) }
    }
}
@(private="package")
overlay_sphere :: proc(mesh:^Overlay_Mesh,center:km.Vec3,radius:f32,color:km.Vec4,identity:Overlay_Triangle) {
    for ring in 0..<8 { for segment in 0..<8 {
        if mesh.overflow { return }
        points:[4]km.Vec3
        for &point,i in points {
            latitude:=f32(ring+i/2)*m.PI/8
            longitude:=f32(segment+i%2)*2*m.PI/8
            point=center+km.Vec3{m.sin(latitude)*m.cos(longitude),m.cos(latitude),m.sin(latitude)*m.sin(longitude)}*radius
        }
        overlay_triangle(mesh,points[0],points[1],points[2],color,identity); overlay_triangle(mesh,points[2],points[1],points[3],color,identity)
    } }
}
/// Ray testing consumes the exact rendered world triangles; gizmo handles have overlay priority.
overlay_hit_test :: proc(mesh:^Overlay_Mesh,origin,direction:km.Vec3)->Overlay_Hit {
    if mesh==nil || km.length_squared(direction)<1e-10 { return {} }
    ray:=km.normalize(direction); result:=Overlay_Hit{distance=max(f32)}
    for identity,i in mesh.triangles {
        if identity.handle==.None && !identity.has_entity { continue }
        a,b,c:=km.xyz(mesh.vertices[i*3].position),km.xyz(mesh.vertices[i*3+1].position),km.xyz(mesh.vertices[i*3+2].position)
        edge1,edge2:=b-a,c-a; p:=km.cross(ray,edge2); determinant:=km.dot(edge1,p)
        if abs(determinant)<1e-8 { continue }
        offset:=origin-a; u:=km.dot(offset,p)/determinant; if u<0 || u>1 { continue }
        q:=km.cross(offset,edge1); v:=km.dot(ray,q)/determinant; if v<0 || u+v>1 { continue }
        distance:=km.dot(edge2,q)/determinant; if distance<0 { continue }
        current_gizmo,result_gizmo:=identity.handle!=.None,result.handle!=.None
        if !result.hit || current_gizmo && !result_gizmo || current_gizmo==result_gizmo && distance<result.distance { result={identity.entity,identity.handle,distance,true} }
    }
    return result
}
