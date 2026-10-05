//! Visible scene components produce genuine debug triangles; camera scale is measured through the exact projection.
package render

import app ".."
import ecs "../../ecs"
import editor "../../editor"
import km "../../math"
import m "core:math"

@(private="package")
overlay_screen_basis :: proc(frame:Frame_Data,point:km.Vec3,width,height:u32)->(km.Vec3,km.Vec3,bool) {
    if width==0 || height==0 { return {},{},false }
    for value in point { if m.is_nan(value) || m.is_inf(value) { return {},{},false } }
    for column in frame.view_projection { for value in column { if m.is_nan(value) || m.is_inf(value) { return {},{},false } } }
    inverse,ok:=km.inverse(frame.view_projection); if !ok { return {},{},false }
    clip:=km.matrix_vector(frame.view_projection,km.vec4(point,1)); if abs(clip[3])<1e-6 { return {},{},false }
    ndc:=clip/clip[3]
    a:=km.matrix_vector(inverse,ndc); b:=km.matrix_vector(inverse,ndc+km.Vec4{2/f32(width),0,0,0}); c:=km.matrix_vector(inverse,ndc+km.Vec4{0,-2/f32(height),0,0})
    if abs(a[3])<1e-6 || abs(b[3])<1e-6 || abs(c[3])<1e-6 { return {},{},false }
    return km.xyz(b/b[3]-a/a[3]),km.xyz(c/c[3]-a/a[3]),true
}
@(private="package")
overlay_gizmo :: proc(mesh:^Overlay_Mesh,state:Editor_Overlay_State,frame:Frame_Data,width,height:u32)->bool {
    if !state.gizmo || len(state.selected)==0 { return true }
    right,up,ok:=overlay_screen_basis(frame,state.pivot,width,height); if !ok { return false }
    scale:=km.length(up)*80; if scale<1e-8 { return false }
    directions:[3]km.Vec3
    for &axis,i in directions { axis=km.normalize(km.xyz(state.basis[i])); if km.length_squared(axis)<0.99 { return false } }
    if abs(km.dot(directions[0],directions[1]))>.0001 || abs(km.dot(directions[0],directions[2]))>.0001 || abs(km.dot(directions[1],directions[2]))>.0001 { return false }
    colors:=[3]km.Vec4{{.95,.2,.2,1},{.2,.9,.2,1},{.3,.3,.95,1}}
    highlights:=[3]km.Vec4{{1,.5,.5,1},{.5,1,.5,1},{.6,.6,1,1}}
    for axis,i in directions {
        handle:=Overlay_Handle(int(Overlay_Handle.Axis_X)+i); color:=colors[i]
        if state.hover==handle || state.captured==handle { color=highlights[i] }
        identity:=Overlay_Triangle{entity=state.selected[0],handle=handle,always=true,has_entity=true}
        if state.mode==.Rotate {
            perpendicular:=[3][2]int{{1,2},{0,2},{0,1}}
            overlay_ring(mesh,state.pivot,directions[perpendicular[i][0]],directions[perpendicular[i][1]],scale*.5,scale*.02,color,identity)
        } else {
            length:=f32(1.2) if state.mode==.Translate else f32(1)
            end:=state.pivot+axis*(length*scale)
            overlay_cylinder(mesh,state.pivot,end,scale*.025,color,identity)
            if state.mode==.Translate { overlay_cylinder(mesh,end,end+axis*(scale*.3),scale*.08,color,identity,tip=true) }
            else { world:=state.basis; world[3]=km.vec4(end+axis*(scale*.06),1); overlay_box(mesh,world,{scale*.06,scale*.06,scale*.06},color,identity,filled=true) }
        }
    }
    if state.mode!=.Rotate {
        planes:=[3][2]int{{0,1},{0,2},{1,2}}
        plane_colors:=[3]km.Vec4{{.3,.3,.95,.4},{.95,.9,.2,.4},{.2,.9,.95,.4}}
        for plane,i in planes {
            handle:=Overlay_Handle(int(Overlay_Handle.Plane_XY)+i); color:=plane_colors[i]
            if state.hover==handle || state.captured==handle { color[3]=.8; color[0]=min(1,color[0]+.3); color[1]=min(1,color[1]+.3); color[2]=min(1,color[2]+.3) }
            a:=state.pivot; b:=a+directions[plane[0]]*(scale*.3); c:=a+directions[plane[1]]*(scale*.3); d:=b+c-a
            identity:=Overlay_Triangle{state.selected[0],handle,true,true}
            overlay_triangle(mesh,a,b,c,color,identity); overlay_triangle(mesh,c,b,d,color,identity)
        }
    }
    _=right
    return true
}
@(private="package")
overlay_physics :: proc(mesh:^Overlay_Mesh,owner:^app.Authoring)->editor.Scene_Error {
    bodies,error:=app.physics_collect(owner); if error!=.None { return error }; defer app.physics_collected_destroy(&bodies,owner.world.allocator)
    for body in bodies {
        if mesh.overflow { return .Invalid_Operation }
        entity:=ecs.Entity_Id(body.id); if _,hidden:=ecs.get_component(&owner.world,entity,app.Editor_Hidden); hidden { continue }
        if !body.body.has_collider { continue }
        color:=km.Vec4{.2,.9,.3,1}
        if body.body.body_type==.Fixed { color={.3,.5,1,1} }; if body.body.body_type==.Kinematic { color={1,.85,.1,1} }; if body.body.sensor { color={.7,.3,1,1} }
        world:=km.mat4_trs(body.position,km.Quat(body.rotation),{1,1,1}); center:=km.Vec3(body.position)
        x,y,z:=km.xyz(world[0]),km.xyz(world[1]),km.xyz(world[2]); identity:=Overlay_Triangle{entity=entity,has_entity=true}
        shape:=body.body.shape
        switch shape.kind {
        case .Box: overlay_box(mesh,world,shape.half_extents,color,identity)
        case .Sphere:
            radius:=shape.radius
            overlay_ring(mesh,center,x,y,radius,.015*radius,color,identity); overlay_ring(mesh,center,x,z,radius,.015*radius,color,identity); overlay_ring(mesh,center,y,z,radius,.015*radius,color,identity)
        case .Capsule:
            top,bottom:=center+y*shape.half_height,center-y*shape.half_height
            overlay_ring(mesh,top,x,z,shape.radius,.015*shape.radius,color,identity); overlay_ring(mesh,bottom,x,z,shape.radius,.015*shape.radius,color,identity)
            for i in 0..<4 { angle:=f32(i)*m.PI*.5; offset:=(x*m.cos(angle)+z*m.sin(angle))*shape.radius; overlay_cylinder(mesh,bottom+offset,top+offset,.005,color,identity) }
            // Meridian semicircles retain both actual hemispherical collider ends.
            for direction in ([2]km.Vec3{x,z}) { for i in 0..<32 {
                a,b:=f32(i)*m.PI/32,f32(i+1)*m.PI/32
                for sign in ([2]f32{-1,1}) { base:=center+y*(shape.half_height*sign); p:=base+direction*(shape.radius*m.cos(a))+y*(shape.radius*m.sin(a)*sign); q:=base+direction*(shape.radius*m.cos(b))+y*(shape.radius*m.sin(b)*sign); overlay_cylinder(mesh,p,q,.005,color,identity,segments=6) }
            } }
        case .ConvexHull:
            edges,edge_error:=app.physics_collider_edges(owner,entity); if edge_error!=.None { return edge_error }
            for edge in edges { overlay_cylinder(mesh,edge.start,edge.end,.005,color,identity,segments=6) }; delete(edges,owner.world.allocator)
        case .Trimesh:
            for triangle:=0;triangle<len(body.indices);triangle+=3 { for i in 0..<3 {
                if mesh.overflow { return .Invalid_Operation }
                a:=km.transform_point(world,body.vertices[body.indices[triangle+i]]); b:=km.transform_point(world,body.vertices[body.indices[triangle+(i+1)%3]])
                overlay_cylinder(mesh,a,b,.005,color,identity,segments=6)
            } }
        case .Heightfield:
            for row in 0..<int(shape.rows) { for col in 0..<int(shape.cols) {
                if mesh.overflow { return .Invalid_Operation }
                p:=km.Vec3{(f32(col)-f32(shape.cols-1)*.5)*body.height_scale[0],shape.heights[row*int(shape.cols)+col]*body.height_scale[1],(f32(row)-f32(shape.rows-1)*.5)*body.height_scale[2]}
                if col+1<int(shape.cols) { q:=p+km.Vec3{body.height_scale[0],(shape.heights[row*int(shape.cols)+col+1]-shape.heights[row*int(shape.cols)+col])*body.height_scale[1],0}; overlay_cylinder(mesh,km.transform_point(world,p),km.transform_point(world,q),.005,color,identity,segments=6) }
                if row+1<int(shape.rows) { q:=p+km.Vec3{0,(shape.heights[(row+1)*int(shape.cols)+col]-shape.heights[row*int(shape.cols)+col])*body.height_scale[1],body.height_scale[2]}; overlay_cylinder(mesh,km.transform_point(world,p),km.transform_point(world,q),.005,color,identity,segments=6) }
            } }
        case .None:
        }
    }
    return .None
}
/// Collects exact authored world transforms, native contact witnesses and all three genuine gizmo modes.
overlay_mesh_prepare :: proc(owner:^app.Authoring,state:Editor_Overlay_State,frame:Frame_Data,width,height:u32)->(Overlay_Mesh,editor.Scene_Error) {
    if owner==nil || width==0 || height==0 { return {},.Invalid_Operation }
    if state.mode not_in (bit_set[Overlay_Mode]{.Translate,.Rotate,.Scale}) || state.hover not_in (bit_set[Overlay_Handle]{.None,.Axis_X,.Axis_Y,.Axis_Z,.Plane_XY,.Plane_XZ,.Plane_YZ}) || state.captured not_in (bit_set[Overlay_Handle]{.None,.Axis_X,.Axis_Y,.Axis_Z,.Plane_XY,.Plane_XZ,.Plane_YZ}) { return {},.Invalid_Field_Value }
    if state.gizmo { for entity in state.selected { if !ecs.entity_exists(&owner.world,entity) { return {},.Entity_Not_Found } }; for column in state.basis { for value in column { if m.is_nan(value) || m.is_inf(value) { return {},.Invalid_Field_Value } } } }
    mesh:=Overlay_Mesh{view_projection=frame.view_projection,clip_y=frame.ambient[3],allocator=owner.world.allocator,vertices=make([dynamic]Overlay_Vertex,owner.world.allocator),triangles=make([dynamic]Overlay_Triangle,owner.world.allocator)}; success:=false; defer { if !success { overlay_mesh_destroy(&mesh) } }
    if state.physics { error:=overlay_physics(&mesh,owner); if error!=.None { return {},error }
        for contact in state.contacts {
            if !ecs.entity_exists(&owner.world,contact.a) || !ecs.entity_exists(&owner.world,contact.b) || abs(km.length_squared(contact.normal)-1)>.01 { return {},.Invalid_Field_Value }
            for vector in ([2]km.Vec3{contact.point,contact.normal}) { for value in vector { if m.is_nan(value) || m.is_inf(value) { return {},.Invalid_Field_Value } } }
            overlay_sphere(&mesh,contact.point,.03,{1,.3,.1,1},{})
            overlay_cylinder(&mesh,contact.point,contact.point+contact.normal*.2,.005,{1,1,.2,1},{})
        }
    }
    ids:=ecs.entity_ids(&owner.world); defer delete(ids)
    for entity in ids {
        if mesh.overflow { return {},.Invalid_Operation }
        if _,hidden:=ecs.get_component(&owner.world,entity,app.Editor_Hidden); hidden { continue }
        _,is_zone:=ecs.get_component(&owner.world,entity,app.Reverb_Zone)
        _,is_light:=ecs.get_component(&owner.world,entity,app.Scene_Point_Light)
        _,is_particle:=ecs.get_component(&owner.world,entity,app.Particle_Emitter)
        billboard,explicit_billboard:=ecs.get_component(&owner.world,entity,app.Scene_Billboard)
        if !(state.reverb && is_zone) && !(state.billboards && (is_light || is_particle || explicit_billboard)) { continue }
        world,error:=app.scene_world_matrix(owner,entity); if error!=.None { return {},error }
        if state.reverb { if zone,exists:=ecs.get_component(&owner.world,entity,app.Reverb_Zone); exists {
            if zone.decay<0 || zone.decay>.99 || zone.wet<0 || zone.wet>1 { return {},.Invalid_Field_Value }
            blue,red:=clamp(zone.decay,0,1),clamp(zone.wet,0,1); color:=km.Vec4{red*.6+.2,.15+.15*min(1-blue,1-red),blue*.7+.3,.85}
            overlay_box(&mesh,world,zone.half_extents,color,{entity=entity,has_entity=true})
        } }
        if state.billboards {
            _,light:=ecs.get_component(&owner.world,entity,app.Scene_Point_Light); _,particle:=ecs.get_component(&owner.world,entity,app.Particle_Emitter)
            _,model:=ecs.get_component(&owner.world,entity,app.Scene_Model); surface,has_surface:=ecs.get_component(&owner.world,entity,app.Scene_Mesh)
            if !explicit_billboard && ((!light && !particle) || model || (has_surface && len(surface.geometry.vertices)>0)) { continue }
            if explicit_billboard && !app.billboard_valid(billboard) { return {},.Invalid_Field_Value }
            center:=km.mat4_extract_translation(world); right,up,ok:=overlay_screen_basis(frame,center,width,height); if !ok { return {},.Invalid_Operation }
            size:=billboard.size if explicit_billboard else f32(1)
            right*=20*size; up*=20*size; a,b,c,d:=center-right+up,center+right+up,center-right-up,center+right-up
            icon:=u32(1) if light else u32(2)
            color:=km.Vec4{1,1,1,1}
            if explicit_billboard { icon=u32(1) if billboard.icon==.Lightbulb else u32(2); color=km.color_to_array(km.color_to_linear(km.Color{billboard.color[0],billboard.color[1],billboard.color[2],billboard.color[3]})) }
            identity:=Overlay_Triangle{entity=entity,has_entity=true}
            overlay_triangle(&mesh,a,b,c,color,identity,{{0,0},{1,0},{0,1}},icon); overlay_triangle(&mesh,c,b,d,color,identity,{{0,1},{1,0},{1,1}},icon)
        }
    }
    mesh.gizmo_first=len(mesh.vertices)
    if !overlay_gizmo(&mesh,state,frame,width,height) { return {},.Invalid_Operation }
    if mesh.overflow { return {},.Invalid_Operation }
    success=true; return mesh,.None
}
