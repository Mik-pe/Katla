#+build darwin, arm64
//! Expected native scopes are translated from immutable prepared packets and selected reflection.
package metal

import gfx ".."
import NS "core:sys/darwin/Foundation"

@(private="package")
capture_expect_event :: proc(r:^Renderer,pass,phase:int,stages:u64,event:gfx.Capture_Event) {
    value:=event;value.pass_index=pass;value.phase_index=phase;value.binding_stages=stages;value.emitted=true
    gfx.capture_expect(&r.capture,value)
}
@(private="package")
capture_expect_image :: proc(r:^Renderer,pass,phase:int,stages:u64,binding:gfx.Image_Binding,index:u32) {
    for access,i in binding.accesses { capture_expect_event(r,pass,phase,stages,{kind=.Bind_Image,resource_kind=.Image,resource_index=access.resource.index,group=binding.group,binding=binding.slot,native_index=index,array_index=u32(i),image_range=access.range}) }
}
@(private="package")
capture_expect_graphics_table :: proc(r:^Renderer,pass,phase_index:int,packet:gfx.Render,phase:gfx.Render_Phase,desc:gfx.Graphics_Desc,stage:gfx.Shader_Stage) {
    vertex:=stage==.Vertex;stages:=u64(1)<<u32(stage)
    capture_expect_event(r,pass,phase_index,stages,{kind=.Argument_Table,resource_index= -1,native_index=u32(stage)})
    for binding in packet.buffers { if stage in binding.stages {
        for requirement in desc.buffers { if requirement.group==binding.group && requirement.slot==binding.slot && stage in requirement.stages {
            index:=u32(requirement.vertex_index if vertex else requirement.fragment_index)
            capture_expect_event(r,pass,phase_index,stages,{kind=.Bind_Buffer,resource_kind=.Buffer,resource_index=binding.access.resource.index,group=binding.group,binding=binding.slot,native_index=index,buffer_range=binding.access.range})
        } }
    } }
    for constant in phase.constants { if stage in constant.stages {
        for requirement in desc.buffers { if requirement.group==constant.group && requirement.slot==constant.slot && stage in requirement.stages {
            index:=u32(requirement.vertex_index if vertex else requirement.fragment_index)
            capture_expect_event(r,pass,phase_index,stages,{kind=.Bind_Buffer,resource_kind=.Auxiliary,resource_index= -1,group=constant.group,binding=constant.slot,native_index=index,buffer_range={0,u64(len(constant.bytes))}})
        } }
    } }
    for binding in phase.images { if stage in binding.stages {
        for requirement in desc.images { if requirement.group==binding.group && requirement.slot==binding.slot && stage in requirement.stages {
            index:=u32(requirement.vertex_index if vertex else requirement.fragment_index)
            capture_expect_image(r,pass,phase_index,stages,binding,index)
            if requirement.metal_kind==.Argument_Buffer { capture_expect_event(r,pass,phase_index,stages,{kind=.Bind_Buffer,resource_kind=.Auxiliary,resource_index= -1,group=binding.group,binding=binding.slot,native_index=index,buffer_range={0,u64(requirement.array_count)*8}}) }
        } }
    } }
    for binding in phase.samplers { if stage in binding.stages {
        for requirement in desc.samplers { if requirement.group==binding.group && requirement.slot==binding.slot && stage in requirement.stages {
            capture_expect_event(r,pass,phase_index,stages,{kind=.Bind_Sampler,resource_index= -1,group=binding.group,binding=binding.slot,native_index=u32(requirement.vertex_index if vertex else requirement.fragment_index)})
        } }
    } }
    words:=desc.vertex_sizes_words if vertex else desc.fragment_sizes_words
    if words>0 { capture_expect_event(r,pass,phase_index,stages,{kind=.Bind_Buffer,resource_kind=.Auxiliary,resource_index= -1,native_index=u32(desc.vertex_sizes_index if vertex else desc.fragment_sizes_index),buffer_range={0,u64(words)*4}}) }
}
@(private="package")
capture_expect_attachment :: proc(r:^Renderer,pass:int,access:gfx.Image_Access,load:gfx.Load_Op,store:gfx.Store_Op,index:u32,color:[4]f64,depth:f64,stencil:u32) {
    clear_color,clear_depth,clear_stencil:=color,depth,stencil
    if load!=.Clear { clear_color={};clear_depth=0;clear_stencil=0 }
    capture_expect_event(r,pass,-1,0,{kind=.Attachment,resource_kind=.Image,resource_index=access.resource.index,native_index=index,image_range=access.range,native_load=u64(load_op(load)),native_store=u64(1 if store==.Store else 0),clear_color=clear_color,clear_depth=clear_depth,clear_stencil=clear_stencil})
}
@(private="package")
capture_expect_vertices :: proc(r:^Renderer,pass,phase_index:int,packet:gfx.Render,phase:gfx.Render_Phase,desc:gfx.Graphics_Desc,vertices:[]gfx.Vertex_Binding) {
    capture_expect_graphics_table(r,pass,phase_index,packet,phase,desc,.Vertex)
    for binding in vertices { capture_expect_event(r,pass,phase_index,1,{kind=.Bind_Buffer,binding_path=.Vertex,resource_kind=.Buffer,resource_index=binding.access.resource.index,binding=binding.binding,native_index=10+binding.binding,buffer_range=binding.access.range}) }
}
@(private="package")
capture_expect_direct :: proc(r:^Renderer,pass,phase_index:int,path:gfx.Capture_Binding_Path,access:gfx.Buffer_Access,index:u32,stages:u64) { capture_expect_event(r,pass,phase_index,stages,{kind=.Bind_Buffer,binding_path=path,resource_kind=.Buffer,resource_index=access.resource.index,native_index=index,buffer_range=access.range}) }
@(private="package")
capture_expect_render :: proc(r:^Renderer,pass:int,packet:gfx.Render) {
    for attachment,i in packet.colors { capture_expect_attachment(r,pass,attachment.access,attachment.load,attachment.store,u32(i),{f64(attachment.clear[0]),f64(attachment.clear[1]),f64(attachment.clear[2]),f64(attachment.clear[3])},0,0) }
    if packet.depth.enabled {
        attachment:=packet.depth
        if .Depth in attachment.access.range.aspects { access:=attachment.access;access.range.aspects={.Depth};capture_expect_attachment(r,pass,access,attachment.load,attachment.store,0,{},attachment.clear_depth,0) }
        if .Stencil in attachment.access.range.aspects { access:=attachment.access;access.range.aspects={.Stencil};capture_expect_attachment(r,pass,access,attachment.load,attachment.store,1,{},0,attachment.clear_stencil) }
    }
    for phase,phase_index in packet.phases {
        if len(phase.draws)==0 { continue }
        owner,ok:=gfx.storage_get(&r.graphics,phase.pipeline);if !ok { continue };desc:=owner^.desc
        capture_expect_graphics_table(r,pass,phase_index,packet,phase,desc,.Vertex)
        capture_expect_graphics_table(r,pass,phase_index,packet,phase,desc,.Fragment)
        capture_expect_event(r,pass,phase_index,3,{kind=.Bind_Pipeline,resource_index= -1})
        for operation in phase.draws { #partial switch draw in operation {
        case gfx.Draw_Vertices: capture_expect_vertices(r,pass,phase_index,packet,phase,desc,draw.vertices)
        case gfx.Draw_Indexed:
            capture_expect_vertices(r,pass,phase_index,packet,phase,desc,draw.vertices)
            if draw.index_count>0 && draw.instance_count>0 { access:=draw.index;offset:=u64(draw.first_index)*u64(2 if draw.index_format==.Uint16 else 4);access.range.offset+=offset;access.range.size-=offset;capture_expect_direct(r,pass,phase_index,.Index,access,0,1) }
        case gfx.Draw_Indirect:
            capture_expect_vertices(r,pass,phase_index,packet,phase,desc,draw.vertices)
            for i in 0..<draw.count { access:=draw.command;access.range={draw.command.range.offset+u64(i)*u64(draw.stride),16};capture_expect_direct(r,pass,phase_index,.Indirect,access,i,1) }
        case gfx.Draw_Indexed_Indirect:
            capture_expect_vertices(r,pass,phase_index,packet,phase,desc,draw.vertices)
            for i in 0..<draw.count { capture_expect_direct(r,pass,phase_index,.Index,draw.index,i,1);access:=draw.command;access.range={draw.command.range.offset+u64(i)*u64(draw.stride),20};capture_expect_direct(r,pass,phase_index,.Indirect,access,i,1) }
        } }
    }
}
@(private="package")
capture_expect_compute :: proc(r:^Renderer,pass:int,packet:gfx.Dispatch) {
    owner,ok:=gfx.storage_get(&r.pipelines,packet.pipeline);if !ok { return };desc:=owner^.desc
    capture_expect_event(r,pass,-1,4,{kind=.Argument_Table,resource_index= -1,native_index=u32(gfx.Shader_Stage.Compute)})
    capture_expect_event(r,pass,-1,4,{kind=.Bind_Pipeline,resource_index= -1})
    for binding in packet.bindings { for requirement in desc.buffers { if requirement.group==binding.group && requirement.slot==binding.slot { capture_expect_event(r,pass,-1,4,{kind=.Bind_Buffer,resource_kind=.Buffer,resource_index=binding.access.resource.index,group=binding.group,binding=binding.slot,native_index=u32(requirement.metal_index),buffer_range=binding.access.range}) } } }
    for binding in packet.images { for requirement in desc.images { if requirement.group==binding.group && requirement.slot==binding.slot {
        capture_expect_image(r,pass,-1,4,binding,u32(requirement.metal_index))
        if requirement.metal_kind==.Argument_Buffer { capture_expect_event(r,pass,-1,4,{kind=.Bind_Buffer,resource_kind=.Auxiliary,resource_index= -1,group=binding.group,binding=binding.slot,native_index=u32(requirement.metal_index),buffer_range={0,u64(requirement.array_count)*8}}) }
    } } }
    for binding in packet.samplers { for requirement in desc.samplers { if requirement.group==binding.group && requirement.slot==binding.slot { capture_expect_event(r,pass,-1,4,{kind=.Bind_Sampler,resource_index= -1,group=binding.group,binding=binding.slot,native_index=u32(requirement.metal_index)}) } } }
    if desc.runtime_sizes_words>0 { capture_expect_event(r,pass,-1,4,{kind=.Bind_Buffer,resource_kind=.Auxiliary,resource_index= -1,native_index=u32(desc.runtime_sizes_index),buffer_range={0,u64(desc.runtime_sizes_words)*4}}) }
    if packet.indirect.enabled { capture_expect_direct(r,pass,-1,.Indirect,packet.indirect.command,0,4) }
}
@(private="package")
capture_prepared_image_desc :: proc(prepared:^gfx.Prepared_Graph,resource:gfx.Image_Id)->gfx.Texture_Desc {
    for image in prepared.images { if image.input.resource==resource { return image.desc } }
    return {}
}
@(private="package")
capture_expect_transfer_buffer :: proc(r:^Renderer,pass:int,resource:gfx.Resource_Id,range:gfx.Buffer_Range,role:u32,value:u32=0,region:gfx.Image_Region={},row:u32=0) {
    capture_expect_event(r,pass,-1,0,{kind=.Bind_Buffer,binding_path=.Transfer,resource_kind=.Buffer,resource_index=resource.index,buffer_range=range,native_index=role,array_index=row,transfer_value=value,transfer_region=region})
}
@(private="package")
capture_expect_transfer_image :: proc(r:^Renderer,pass:int,resource:gfx.Image_Id,range:gfx.Image_Range,role:u32,region:gfx.Image_Region={}) {
    capture_expect_event(r,pass,-1,0,{kind=.Bind_Image,binding_path=.Transfer,resource_kind=.Image,resource_index=resource.index,image_range=range,native_index=role,transfer_region=region})
}
@(private="package")
capture_expect_transfer_rows :: proc(r:^Renderer,pass:int,resource:gfx.Resource_Id,offset:u64,role:u32,region:gfx.Image_Region,desc:gfx.Texture_Desc) {
    layout,valid:=gfx.image_region_layout(region,desc);if !valid { return }
    resolved:=region;resolved.bytes_per_row=layout.bytes_per_row;resolved.bytes_per_image=layout.bytes_per_image
    for z in 0..<u64(region.depth) { for row in 0..<layout.block_rows {
        capture_expect_transfer_buffer(r,pass,resource,{offset+z*layout.bytes_per_image+row*layout.bytes_per_row,layout.row_bytes},role,region=resolved,row=u32(z*layout.block_rows+row))
    } }
}
@(private="package")
capture_expectations :: proc(r:^Renderer,prepared:^gfx.Prepared_Graph) {
    if !r.capture.recording { return }
    for alias in prepared.aliases { gfx.capture_expect(&r.capture,capture_alias_event(alias)) }
    for pass in prepared.passes {
        visibility:=NS.UInteger(1)
        for input in prepared.buffers { entry,ok:=gfx.storage_get(&r.buffers,input.handle);if ok && entry^.heap!=nil { visibility|=2 } }
        for input in prepared.textures { entry,ok:=gfx.storage_get(&r.textures,input.handle);if ok && entry^.heap!=nil { visibility|=2 } }
        for alias in prepared.aliases { if alias.after==pass.id { visibility|=2 } }
        gfx.capture_expect(&r.capture,capture_barrier_event(pass.id.index,visibility))
        #partial switch packet in pass.packet {
        case gfx.Render:capture_expect_render(r,pass.id.index,packet)
        case gfx.Dispatch:capture_expect_compute(r,pass.id.index,packet)
        case gfx.Copy_Buffer:
            capture_expect_transfer_buffer(r,pass.id.index,packet.source,{packet.source_offset,packet.size},0)
            capture_expect_transfer_buffer(r,pass.id.index,packet.destination,{packet.destination_offset,packet.size},1)
        case gfx.Copy_Image_Buffer:
            desc:=capture_prepared_image_desc(prepared,packet.source);layout,valid:=gfx.image_region_layout(packet.region,desc)
            if valid {
                region:=packet.region;region.bytes_per_row=layout.bytes_per_row;region.bytes_per_image=layout.bytes_per_image
                capture_expect_transfer_image(r,pass.id.index,packet.source,{region.mip,1,region.layer,1,{region.aspect}},0,region)
                capture_expect_transfer_rows(r,pass.id.index,packet.destination,packet.destination_offset,1,packet.region,desc)
            }
        case gfx.Copy_Buffer_Image:
            desc:=capture_prepared_image_desc(prepared,packet.destination);layout,valid:=gfx.image_region_layout(packet.region,desc)
            if valid {
                region:=packet.region;region.bytes_per_row=layout.bytes_per_row;region.bytes_per_image=layout.bytes_per_image
                capture_expect_transfer_rows(r,pass.id.index,packet.source,packet.source_offset,0,packet.region,desc)
                capture_expect_transfer_image(r,pass.id.index,packet.destination,{region.mip,1,region.layer,1,{region.aspect}},1,region)
            }
        case gfx.Generate_Mips:
            base:=packet.range;base.mip_count=1
            lower:=packet.range;lower.base_mip+=1;lower.mip_count-=1
            capture_expect_transfer_image(r,pass.id.index,packet.resource,base,0)
            capture_expect_transfer_image(r,pass.id.index,packet.resource,lower,1)
        case gfx.Fill_Buffer:
            capture_expect_transfer_buffer(r,pass.id.index,packet.destination,{packet.offset,packet.size},0,packet.value)
            if packet.value!=u32(byte(packet.value))*0x01010101 {
                capture_expect_event(r,pass.id.index,-1,4,{kind=.Argument_Table,resource_index= -1,native_index=u32(gfx.Shader_Stage.Compute)})
                capture_expect_event(r,pass.id.index,-1,4,{kind=.Bind_Pipeline,resource_index= -1})
                capture_expect_event(r,pass.id.index,-1,4,{kind=.Bind_Buffer,resource_kind=.Buffer,resource_index=packet.destination.index,buffer_range={packet.offset,packet.size}})
                capture_expect_event(r,pass.id.index,-1,4,{kind=.Bind_Buffer,resource_kind=.Auxiliary,resource_index= -1,binding=1,native_index=1,buffer_range={0,16}})
            }
        }
    }
}
