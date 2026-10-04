//! Owner-routed view actions reply only after their exact accepted color/object-ID capture completes.
package editor_app

import app ".."
import agent "../../agent"
import mcp "../../agent/mcp"
import ecs "../../ecs"
import editor "../../editor"
import render "../render"
import km "../../math"
import "core:encoding/base64"
import "core:encoding/json"
import "core:fmt"
import "core:math"
import "core:mem"
import "core:time"

@(private="package")
View_Pending :: struct { ticket,serial:u64,request:agent.Editor_View_Request,error:editor.Scene_Error,applied:bool }
/// Keep stationary until destruction; callbacks borrow a stationary native capture owner.
View_Service :: struct {
    shell:^Shell,socket:mcp.Socket_Server,pending:[dynamic]View_Pending,allocator:mem.Allocator,
    capture_state:rawptr,request_capture:proc(rawptr)->(u64,bool),
    mutation_state:rawptr,prepare_mutation:proc(rawptr)->editor.Scene_Error,
}
/// An empty endpoint disables external socket admission while retaining mailbox view dispatch.
view_service_init :: proc(service:^View_Service,shell:^Shell,capture_state:rawptr,request_capture:proc(rawptr)->(u64,bool),endpoint:string="",allocator:=context.allocator)->mcp.Socket_Error {
    if shell==nil || shell.state==nil || shell.state.owner==nil || request_capture==nil { return .Endpoint }
    service^={shell=shell,allocator=allocator,capture_state=capture_state,request_capture=request_capture,pending=make([dynamic]View_Pending,allocator)}
    if endpoint!="" { error:=mcp.socket_server_init(&service.socket,endpoint,&shell.state.owner.agent,allocator); if error!=.None { delete(service.pending); service^={}; return error } }
    return .None
}
/// Closes only this endpoint and abandons only its deferred credits; the shared mailbox stays open.
view_service_destroy :: proc(service:^View_Service) {
    if service.shell==nil { return }
    mcp.socket_server_destroy(&service.socket)
    for request in service.pending { editor.agent_abandon(&service.shell.state.owner.agent,request.ticket) }
    delete(service.pending); service^={}
}
@(private="package")
view_service_begin :: proc(raw:rawptr,w:^ecs.World,registry:^editor.Component_Registry,op:editor.Scene_Op,ticket:u64)->bool {
    if op.kind!=.Application || op.tool_name!="editor_view" { return false }
    service:=cast(^View_Service)raw
    assert(w==&service.shell.state.owner.world && registry==&service.shell.state.owner.registry)
    request,error:=agent.editor_view_decode(op.value,service.allocator)
    pending:=View_Pending{ticket=ticket,request=request}
    if error!=.None { pending.error=.Invalid_Operation }
    append(&service.pending,pending); return true
}
@(private="package")
view_service_execute :: proc(raw:rawptr,w:^ecs.World,registry:^editor.Component_Registry,op:editor.Scene_Op)->(editor.Tool_Result,editor.Undo_Group) {
    service:=cast(^View_Service)raw
    readonly:=op.kind in bit_set[editor.Scene_Op_Kind]{.Query_Entities,.Get_Hierarchy,.List_Components,.Get_Attributes}
    if op.kind==.Application { readonly=op.tool_name=="search_assets" || op.tool_name=="list_resources" || op.tool_name=="read_resource" }
    if !readonly && service.prepare_mutation!=nil { if error:=service.prepare_mutation(service.mutation_state); error!=.None { return editor.Tool_Result{error=error,allocator=service.allocator},{} } }
    executor:=app.authoring_executor(service.shell.state.owner)
    return executor.execute(executor.state,w,registry,op)
}
/// Replaces authoring_tick so every ordinary tool and deferred view uses the same world owner.
view_service_tick :: proc(service:^View_Service,now:time.Duration)->(int,mcp.Socket_Error) {
    if service.shell==nil { return 0,.Endpoint }
    socket_error:=mcp.Socket_Error.None
    if service.socket.active { socket_error=mcp.socket_server_tick(&service.socket,now) }
    owner:=service.shell.state.owner
    count:=editor.agent_tick(&owner.agent,&owner.world,&owner.registry,{state=service,execute=view_service_execute,begin=view_service_begin})
    index:=0
    for index<len(service.pending) {
        pending:=&service.pending[index]
        if !editor.agent_is_deferred(&owner.agent,pending.ticket) { ordered_remove(&service.pending,index); continue }
        if pending.error!=.None { view_service_finish_error(service,pending.ticket,pending.error); ordered_remove(&service.pending,index); continue }
        // Only one view action awaits a capture; busy retries never replay an already applied action.
        if index>0 { break }
        if !pending.applied { pending.error=view_service_apply(service,pending.request); pending.applied=true; if pending.error!=.None { continue } }
        if pending.serial==0 { serial,queued:=service.request_capture(service.capture_state); if queued { if serial==0 { pending.error=.Invalid_Operation; continue }; pending.serial=serial } }
        index+=1
    }
    if service.socket.active { output_error:=mcp.socket_server_tick(&service.socket,now); if socket_error==.None { socket_error=output_error } }
    return count,socket_error
}
@(private="package")
view_service_finish_error :: proc(service:^View_Service,ticket:u64,error:editor.Scene_Error) {
    result:=editor.Tool_Result{error=error,allocator=service.allocator}
    editor.agent_complete_reply(&service.shell.state.owner.agent,ticket,&result); editor.tool_result_destroy(&result)
}
/// Fails only captures associated with this serial; zero fails all admitted view captures.
view_service_capture_failed :: proc(service:^View_Service,serial:u64=0) {
    index:=0
    for index<len(service.pending) { pending:=service.pending[index]; if serial!=0 && pending.serial!=serial { index+=1; continue }; view_service_finish_error(service,pending.ticket,.Invalid_Operation); ordered_remove(&service.pending,index) }
}
@(private="package")
view_active :: proc(shell:^Shell)->^Viewport { index:=clamp(shell.viewports.active,0,viewport_count(shell.viewports.layout)-1); return &shell.viewports.slots[index] }
@(private="package")
view_service_apply :: proc(service:^View_Service,request:agent.Editor_View_Request)->editor.Scene_Error {
    state:=service.shell.state; owner:=state.owner
    if request.action!=.Observe && owner.mode!=.Editing { return .Editing_Required }
    if request.action!=.Observe && service.prepare_mutation!=nil { if error:=service.prepare_mutation(service.mutation_state); error!=.None { return error } }
    camera:=&view_active(service.shell).camera
    switch request.action {
    case .Observe: return .None
    case .Undo,.Redo:
        available:=editor.agent_can_redo(&owner.agent.session) if request.action==.Redo else editor.agent_can_undo(&owner.agent.session)
        if !available { return .Invalid_Operation }; return history_apply(state,request.action==.Redo)
    case .Select:
        if !request.has_entity { selection_clear(state); return .None }
        if !selectable(state,request.entity) { return .Entity_Not_Found }
        selection_set(state,request.entity); selection_reveal(state,request.entity); return .None
    case .Set_Camera:
        delta:=[3]f64{f64(request.position[0])-f64(request.target[0]),f64(request.position[1])-f64(request.target[1]),f64(request.position[2])-f64(request.target[2])}
        distance:=math.sqrt(delta[0]*delta[0]+delta[1]*delta[1]+delta[2]*delta[2])
        if distance<0.05 || distance>100000 || math.is_inf(distance) || math.is_nan(distance) { return .Invalid_Operation }
        pitch:=math.asin(delta[1]/distance); if math.abs(pitch)>1.553343 { return .Invalid_Operation }
        camera.target=request.target; camera.distance=f32(distance); camera.yaw=f32(math.atan2(delta[0],delta[2])); camera.pitch=f32(pitch); camera.focus={}; return .None
    case .Focus:
        if !selectable(state,request.entity) { return .Entity_Not_Found }
        bounds,available,error:=view_subtree_bounds(state,request.entity); if error!=.None { return error }; if !available { return .Component_Not_Found }
        viewport:=view_active(service.shell); aspect:=max(0.001,viewport.bounds.width/max(1,viewport.bounds.height))
        half_fov:=camera.fov*math.PI/360; if aspect<1 { half_fov=math.atan(math.tan(half_fov)*aspect) }
        distance:=max(0.5,km.length(bounds.extent)/max(0.001,math.sin(half_fov))*1.3)
        if math.is_inf(distance) || math.is_nan(distance) || distance>100000 { return .Invalid_Operation }
        camera.target=bounds.center; camera.distance=distance; camera.focus={}
        if request.select { selection_set(state,request.entity); selection_reveal(state,request.entity) }
        return .None
    }
    return .Invalid_Operation
}
@(private="package")
view_subtree_bounds :: proc(state:^State,root:ecs.Entity_Id)->(km.AABB,bool,editor.Scene_Error) {
    ids:=ecs.entity_ids(&state.owner.world); defer delete(ids)
    low,high:km.Vec3; found:=false
    for id in ids {
        if !selectable(state,id) { continue }
        cursor:=id; descendant:=false
        for _ in 0..<len(ids)+1 { if cursor==root { descendant=true; break }; parent,has_parent:=ecs.get_component(&state.owner.world,cursor,app.Scene_Parent); if !has_parent { break }; cursor=parent.entity }
        if !descendant { continue }
        bounds,available,error:=app.scene_drawable_bounds(state.owner,id); if error!=.None { return {},false,error }; if !available { continue }
        a,b:=bounds.center-bounds.extent,bounds.center+bounds.extent
        if !found { low=a; high=b; found=true } else { for axis in 0..<3 { low[axis]=min(low[axis],a[axis]); high[axis]=max(high[axis],b[axis]) } }
    }
    return km.AABB{(low+high)*0.5,(high-low)*0.5},found,.None
}

@(private="package")
View_Sample :: struct { pixel:[2]i32,raw_id:u32,entity_id:Maybe(string) }
@(private="package")
View_Raw_Sample :: struct { encoded_object_id:u32,meaning:string }
@(private="package")
View_Envelope :: struct { submission:string,image_size:[2]u32,center_pick,pointer_pick:Maybe(string),center_pick_sample:View_Raw_Sample,pointer_pick_sample:Maybe(View_Raw_Sample),pointer_pixel:Maybe([2]i32) }
@(private="package")
View_Provenance :: struct { submission_id,frame_generation,color_generation,id_generation:string,color_resource,id_resource:int,samples:[dynamic]View_Sample }
/// Borrows completed CPU owners only for this call; no live world/camera data enters the response.
view_service_capture :: proc(service:^View_Service,snapshot:^render.Picking_Snapshot,frozen_context,png:[]byte)->bool {
    if service.shell==nil || snapshot==nil { return false }
    index:=-1; for request,i in service.pending { if request.serial!=0 && request.serial==snapshot.metadata.serial { index=i; break } }; if index<0 { return false }
    request:=service.pending[index]
    if !editor.agent_is_deferred(&service.shell.state.owner.agent,request.ticket) { ordered_remove(&service.pending,index); return false }
    data,valid:=view_reply_encode(snapshot,frozen_context,png,request.request.limit,service.allocator)
    result:=editor.Tool_Result{data=data,allocator=service.allocator}
    if !valid { result.error=.Invalid_Operation }
    accepted:=editor.agent_complete_reply(&service.shell.state.owner.agent,request.ticket,&result); editor.tool_result_destroy(&result)
    ordered_remove(&service.pending,index); return accepted && valid
}
@(private="package")
view_reply_encode :: proc(snapshot:^render.Picking_Snapshot,frozen_context,png:[]byte,limit:int,allocator:mem.Allocator)->([]byte,bool) {
    if len(png)>18<<20 {
        data,error:=json.marshal(struct{capture_error:string}{"Viewport PNG exceeds the 18 MiB image limit (24 MiB base64). Reduce the viewport size and request a new committed capture."},allocator=allocator)
        if error!=nil { delete(data,allocator); return nil,false }; return data,false
    }
    if snapshot==nil || len(png)<33 { return nil,false }
    expected:=[8]byte{137,80,78,71,13,10,26,10}; for byte,index in expected { if png[index]!=byte { return nil,false } }
    if string(png[12:16])!="IHDR" { return nil,false }
    width:=u32(png[16])<<24|u32(png[17])<<16|u32(png[18])<<8|u32(png[19]); height:=u32(png[20])<<24|u32(png[21])<<16|u32(png[22])<<8|u32(png[23])
    if width!=snapshot.metadata.width || height!=snapshot.metadata.height { return nil,false }
    metadata,valid:=view_committed_metadata(snapshot,frozen_context,limit,allocator); if !valid { return nil,false }; defer delete(metadata,allocator)
    tree,error:=json.parse(metadata,parse_integers=true,allocator=allocator); if error!=nil { return nil,false }; defer json.destroy_value(tree)
    object,okay:=tree.(json.Object); if !okay { return nil,false }
    encoded,encode_error:=base64.encode(png,allocator=allocator); if encode_error!=nil { return nil,false }; defer delete(encoded,allocator)
    // Borrow the base64 allocation in a separate map so the parsed tree retains its complete owner.
    result:=make(json.Object,allocator); defer delete(result); for key,value in object { result[key]=value }; result["image_png_base64"]=encoded
    output,output_error:=json.marshal(result,allocator=allocator); if output_error!=nil { delete(output,allocator); return nil,false }; return output,true
}
/// Returns owned immutable GPU/camera context shared by MCP and the selected external conversation.
view_committed_metadata :: proc(snapshot:^render.Picking_Snapshot,frozen_context:[]byte,limit:int=64,allocator:=context.allocator)->([]byte,bool) {
    context.allocator=allocator
    if snapshot==nil { return nil,false }
    metadata:=snapshot.metadata
    if snapshot.submission.owner==nil || snapshot.submission.token.owner==nil || snapshot.color_source.owner!=snapshot.submission.owner || snapshot.id_source.owner!=snapshot.submission.owner || snapshot.color_source.submission!=snapshot.submission || snapshot.id_source.submission!=snapshot.submission || snapshot.color.source!=snapshot.color_source || snapshot.id_pixels.source!=snapshot.id_source || snapshot.id_source.desc.format!=.R32_Uint || metadata.width==0 || metadata.height==0 || metadata.width>8192 || metadata.height>8192 || u64(metadata.width)*u64(metadata.height)>16*1024*1024 || len(frozen_context)>1<<20 { return nil,false }
    if snapshot.color_source.desc.width!=metadata.width || snapshot.color_source.desc.height!=metadata.height || snapshot.id_source.desc.width!=metadata.width || snapshot.id_source.desc.height!=metadata.height || (snapshot.color_source.desc.format!=.RGBA8_Unorm && snapshot.color_source.desc.format!=.BGRA8_Unorm) { return nil,false }
    for pixels in ([2]struct{row_pitch:u64,bytes:[]byte}{{snapshot.color.row_pitch,snapshot.color.bytes},{snapshot.id_pixels.row_pitch,snapshot.id_pixels.bytes}}) {
        row_bytes:=u64(metadata.width)*4
        if pixels.row_pitch<row_bytes || u64(len(pixels.bytes))<row_bytes || (metadata.height>1 && pixels.row_pitch>(u64(len(pixels.bytes))-row_bytes)/u64(metadata.height-1)) { return nil,false }
    }
    tree,error:=json.parse(frozen_context,spec=.JSON,parse_integers=true,allocator=allocator); if error!=nil { return nil,false }; defer json.destroy_value(tree)
    object,valid:=tree.(json.Object); if !valid { return nil,false }
    frame:=fmt.aprintf("%d",metadata.frame,allocator=allocator); defer delete(frame,allocator)
    serial:=fmt.aprintf("%d",metadata.serial,allocator=allocator); defer delete(serial,allocator)
    frame_value,frame_ok:=object["frame_id"].(string); serial_value,serial_ok:=object["capture_serial"].(string)
    width,width_ok:=object["width"].(json.Integer); height,height_ok:=object["height"].(json.Integer)
    if !frame_ok || !serial_ok || frame_value!=frame || serial_value!=serial || !width_ok || !height_ok || width!=json.Integer(metadata.width) || height!=json.Integer(metadata.height) { return nil,false }
    candidates,has_candidates:=object["frustum_candidates"].(json.Array); if !has_candidates { return nil,false }
    result:=make(json.Object,allocator); defer delete(result)
    for key,value in object { result[key]=value }
    selected,has_selected:=object["selected_entities"].(json.Array); if !has_selected { return nil,false }
    result["selected_entity"]=json.Null{}; if len(selected)>0 { result["selected_entity"]=selected[0] }; result["selected_entity_id"]=result["selected_entity"]
    limited:=make(json.Array,allocator); defer delete(limited); append(&limited,..candidates[:min(len(candidates),clamp(limit,0,256))])
    result["frustum_candidates"]=limited; result["candidates"]=limited; result["total"]=json.Integer(len(candidates)); result["candidate_count"]=json.Integer(len(candidates)); result["truncated"]=len(limited)<len(candidates)
    provenance:=View_Provenance{samples=make([dynamic]View_Sample,allocator),color_resource=snapshot.color_source.resource.index,id_resource=snapshot.id_source.resource.index}
    provenance.submission_id=fmt.aprintf("%d",snapshot.submission.id,allocator=allocator); provenance.frame_generation=fmt.aprintf("%d",snapshot.submission.token.generation,allocator=allocator); provenance.color_generation=fmt.aprintf("%d",snapshot.color_source.generation,allocator=allocator); provenance.id_generation=fmt.aprintf("%d",snapshot.id_source.generation,allocator=allocator)
    defer { delete(provenance.submission_id,allocator); delete(provenance.frame_generation,allocator); delete(provenance.color_generation,allocator); delete(provenance.id_generation,allocator); for sample in provenance.samples { if entity,present:=sample.entity_id.(string); present { delete(entity,allocator) } }; delete(provenance.samples) }
    positions:=[5][2]i32{{i32(metadata.width/2),i32(metadata.height/2)},{0,0},{i32(metadata.width)-1,0},{0,i32(metadata.height)-1},{i32(metadata.width)-1,i32(metadata.height)-1}}
    for pixel in positions { view_sample_append(&provenance,snapshot,pixel,allocator) }; if metadata.has_pointer { view_sample_append(&provenance,snapshot,metadata.pointer,allocator) }
    bytes,marshal_error:=json.marshal(provenance,allocator=allocator); if marshal_error!=nil { return nil,false }; defer delete(bytes,allocator)
    extra,extra_error:=json.parse(bytes,parse_integers=true,allocator=allocator); if extra_error!=nil { return nil,false }; defer json.destroy_value(extra)
    result["gpu_provenance"]=extra
    meaning:="A mapped ID identifies a scene draw. Zero is background; unmapped nonzero may be editor overlay geometry. This pixel helper does not establish user intent."
    envelope:=View_Envelope{submission=provenance.submission_id,image_size={metadata.width,metadata.height},center_pick=provenance.samples[0].entity_id,center_pick_sample={provenance.samples[0].raw_id,meaning}}
    if metadata.has_pointer { pointer:=provenance.samples[len(provenance.samples)-1]; envelope.pointer_pick=pointer.entity_id; envelope.pointer_pick_sample=View_Raw_Sample{pointer.raw_id,meaning}; envelope.pointer_pixel=metadata.pointer }
    envelope_bytes,envelope_error:=json.marshal(envelope,allocator=allocator); if envelope_error!=nil { return nil,false }; defer delete(envelope_bytes,allocator)
    envelope_tree,envelope_parse:=json.parse(envelope_bytes,parse_integers=true,allocator=allocator); if envelope_parse!=nil { return nil,false }; defer json.destroy_value(envelope_tree)
    for key,value in envelope_tree.(json.Object) { result[key]=value }
    output,output_error:=json.marshal(result,allocator=allocator); if output_error!=nil { delete(output,allocator); return nil,false }; return output,true
}
@(private="package")
view_sample_append :: proc(value:^View_Provenance,snapshot:^render.Picking_Snapshot,pixel:[2]i32,allocator:mem.Allocator) {
    sample:=render.picking_sample(snapshot,pixel[0],pixel[1]); row:=View_Sample{pixel=pixel,raw_id=sample.encoded}
    if sample.mapped { row.entity_id=fmt.aprintf("%d",u64(sample.entity),allocator=allocator) }
    append(&value.samples,row)
}
