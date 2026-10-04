//! Backward range demand retains only producers whose output still survives.
package gfx

@(private="package")
subtract_buffer_demand :: proc(needs:^[dynamic]Buffer_Access,write:Buffer_Access) {
    for i:=len(needs^)-1; i>=0; i-=1 {
        need:=needs^[i]
        if need.resource!=write.resource || !range_overlaps(need.range,write.range) { continue }
        ordered_remove(needs,i)
        begin:=need.range.offset; end:=begin+need.range.size
        cut_begin:=max(begin,write.range.offset); cut_end:=min(end,write.range.offset+write.range.size)
        if begin<cut_begin { part:=need; part.range={begin,cut_begin-begin}; append(needs,part) }
        if cut_end<end { part:=need; part.range={cut_end,end-cut_end}; append(needs,part) }
    }
}
@(private="package")
append_image_piece :: proc(needs:^[dynamic]Image_Access,need:Image_Access,mip_begin,mip_end,layer_begin,layer_end:u32,aspects:Image_Aspects) {
    if mip_begin==mip_end || layer_begin==layer_end || aspects=={} { return }
    part:=need; part.range={mip_begin,mip_end-mip_begin,layer_begin,layer_end-layer_begin,aspects}; append(needs,part)
}
@(private="package")
subtract_image_demand :: proc(needs:^[dynamic]Image_Access,write:Image_Access) {
    for i:=len(needs^)-1; i>=0; i-=1 {
        need:=needs^[i]
        if need.resource!=write.resource || !image_ranges_overlap(need.range,write.range) { continue }
        ordered_remove(needs,i)
        a,b:=need.range,write.range
        am,al:=a.base_mip+a.mip_count,a.base_layer+a.layer_count
        bm,bl:=b.base_mip+b.mip_count,b.base_layer+b.layer_count
        cm0,cm1:=max(a.base_mip,b.base_mip),min(am,bm)
        cl0,cl1:=max(a.base_layer,b.base_layer),min(al,bl)
        shared:=a.aspects&b.aspects
        append_image_piece(needs,need,a.base_mip,am,a.base_layer,al,a.aspects&~b.aspects)
        append_image_piece(needs,need,a.base_mip,cm0,a.base_layer,al,shared)
        append_image_piece(needs,need,cm1,am,a.base_layer,al,shared)
        append_image_piece(needs,need,cm0,cm1,a.base_layer,cl0,shared)
        append_image_piece(needs,need,cm0,cm1,cl1,al,shared)
    }
}
@(private="package")
compile_liveness :: proc(g:^Graph,live:[]bool) {
    buffers:=make([dynamic]Buffer_Access,g.allocator); defer delete(buffers)
    images:=make([dynamic]Image_Access,g.allocator); defer delete(images)
    for buffer,i in g.buffers { if buffer.exported { append(&buffers,Buffer_Access{{g,i},{0,buffer.desc.size},.Read,.Readback}) } }
    for image,i in g.images { if image.exported { append(&images,Image_Access{{g,i},image_full_range(image.desc),.Read,.Transfer_Source}) } }
    for i:=len(g.passes)-1; i>=0; i-=1 {
        pass:=g.passes[i]; required:=pass.side_effect
        for write in pass.accesses {
            if !access_writes(write.mode) { continue }
            for need in buffers { if need.resource==write.resource && range_overlaps(need.range,write.range) { required=true; break } }
        }
        for write in pass.images {
            if !access_writes(write.mode) { continue }
            for need in images { if need.resource==write.resource && image_ranges_overlap(need.range,write.range) { required=true; break } }
        }
        live[i]=required
        if !required { continue }
        for access in pass.accesses { if access_writes(access.mode) { subtract_buffer_demand(&buffers,access) } }
        for access in pass.images { if access_writes(access.mode) { subtract_image_demand(&images,access) } }
        for access in pass.accesses { if access_reads(access.mode) { append(&buffers,access) } }
        for access in pass.images { if access_reads(access.mode) { append(&images,access) } }
    }
}
