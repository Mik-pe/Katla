//! Billboard picking reuses exact visible glyph masks and immutable world triangles in the existing integer pass.
package render

import gfx "../../gfx"
import km "../../math"
import "core:mem"

@(private="package")
overlay_identity :: proc()->Object_Data { return {model=km.identity(km.Mat4),normal_model=km.identity(km.Mat4),linear_color={1,1,1,1}} }
@(private="package")
overlay_identity_bytes :: proc(values:[]Object_Data)->[]byte { return mem.slice_to_bytes(values) }

/// Returns owned alpha-tested billboard draw ranges with full generational entity lookup.
/// Existing encoded scene identities are preserved; newly visible meshless entities receive the next free code.
/// The host appends these before picking_graph_append and releases the returned slice after cloning.
overlay_picking_draws :: proc(owner:^Overlay_Native($R),upload:^Overlay_Frame,mesh:^Overlay_Mesh,scene:^Scene_Graph,imports:^Overlay_Graph,entries:[]Picking_Entry,masked:gfx.Graphics_Pipeline_Handle)->([]Picking_Draw,Native_Error) {
    if mesh==nil || scene==nil || imports==nil || upload==nil || masked.owner==nil { return nil,{gpu=.Invalid_Resource} }
    if len(mesh.vertices)==0 { return nil,{} }
    g:=scene_graph_target(scene)
    if upload.owner!=owner || upload.resource.owner!=g || upload.resource.index<0 || upload.resource.index>=len(g.buffers) || g.buffers[upload.resource.index].desc!=upload.desc || imports.owner!=owner || imports.graph!=g { return nil,{gpu=.Invalid_Resource} }
    for image in imports.images { if image.owner!=g || image.index<0 || image.index>=len(g.images) || g.images[image.index].desc!=overlay_glyph_desc() { return nil,{gpu=.Invalid_Resource} } }
    if upload.identity.owner!=nil { return nil,{gpu=.Busy} }
    encoded:=make([dynamic]Picking_Entry,owner.allocator); defer delete(encoded); append(&encoded,..entries)
    next:=u32(0)
    for entry,i in entries {
        if entry.encoded==0 { return nil,{gpu=.Invalid_Resource} }; next=max(next,entry.encoded)
        for earlier in entries[:i] { if earlier.encoded==entry.encoded && earlier.entity!=entry.entity { return nil,{gpu=.Invalid_Resource} } }
    }
    draws:=make([dynamic]Picking_Draw,owner.allocator); defer delete(draws)
    for triangle,i in mesh.triangles {
        icon:=u32(mesh.vertices[i*3].uv[2]); if triangle.handle!=.None || !triangle.has_entity || icon==0 { continue }
        if icon>2 { return nil,{gpu=.Invalid_Resource} }
        identity:=u32(0)
        for entry in encoded { if entry.entity==triangle.entity { identity=entry.encoded; break } }
        if identity==0 { if next==max(u32) { return nil,{gpu=.Invalid_Resource} }; next+=1; identity=next; append(&encoded,Picking_Entry{identity,triangle.entity}) }
        if len(draws)>0 {
            previous:=&draws[len(draws)-1]
            if previous.entity==triangle.entity && previous.first_vertex+previous.vertex_count==u32(i*3) && previous.mask.texture.handle==owner.glyphs[icon-1] { previous.vertex_count+=3; continue }
        }
        append(&draws,Picking_Draw{geometry={resource=upload.resource,handle=upload.vertices,desc=upload.desc},vertex_stride=48,object_stride=160,first_vertex=u32(i*3),vertex_count=3,encoded=identity,entity=triangle.entity,pipeline=masked,mask={enabled=true,texture={handle=owner.glyphs[icon-1],desc=overlay_glyph_desc(),resource=imports.images[icon-1],encoding=.Linear},sampler=owner.sampler,uv_offset=32,vertex_alpha_offset=28,object_alpha_offset=140,vertex_alpha=true,cutoff=.01}})
    }
    if len(draws)==0 { return nil,{} }
    desc:=gfx.Buffer_Desc{size=160,usage={.Storage,.Transfer_Destination},memory=.GPU_Private}
    object:=[1]Object_Data{overlay_identity()}
    handle,error:=owner.ops.create_buffer(owner.renderer,desc,overlay_identity_bytes(object[:])); if error!=.None { return nil,{gpu=error} }
    resource,graph_error:=gfx.graph_buffer(g,desc,true,false); if graph_error!=.None { owner.ops.destroy_buffer(owner.renderer,handle); return nil,{gpu=.Invalid_Graph} }
    upload.identity=handle
    for &draw in draws { draw.objects={resource,handle,desc} }
    result:=make([]Picking_Draw,len(draws),owner.allocator); copy(result,draws[:]); return result,{}
}
