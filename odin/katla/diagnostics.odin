//! CLI diagnostics serialize actual retained layout and accepted compiled GPU execution.
package main
import editor_app "../app/editor"
import render "../app/render"
import gfx "../gfx"
import ui "../ui"
import "core:encoding/json"
import "core:fmt"
import "core:mem"
import "core:path/filepath"
import "core:strings"
import "core:os"
import "core:time"

Diagnostic_Node :: struct {key:u64,kind,text:string,bounds:[4]f32,mounted,disabled:bool}
Diagnostic_Pass :: struct {index:int,name,kind:string,buffer_accesses,image_accesses:int}
@(private="package")
diagnostic_write :: proc(value:any,path:string)->bool { bytes,error:=json.marshal(value,opt=json.Marshal_Options{use_enum_names=true,indentation=2}); if error!=nil { return false }; defer delete(bytes); if path!="" { return os.write_entire_file(path,bytes)==nil }; fmt.println(string(bytes)); return true }
@(private="package")
diagnostic_layout :: proc(shell:^editor_app.Shell,path:string)->bool {
    nodes:=make([dynamic]Diagnostic_Node); defer delete(nodes)
    for identity,node in shell.ctx.nodes { if !node.mounted { continue }; append(&nodes,Diagnostic_Node{identity,fmt.aprintf("%v",node.descriptor.kind),node.descriptor.text,{node.bounds.x,node.bounds.y,node.bounds.width,node.bounds.height},node.mounted,node.input_disabled}) }
    defer { for node in nodes { delete(node.kind) } }
    return diagnostic_write(struct {width,height:f32,nodes:[]Diagnostic_Node}{shell.ctx.logical_size.x,shell.ctx.logical_size.y,nodes[:]},path)
}
@(private="package")
diagnostic_graph :: proc(gpu:^editor_app.GPU_Owner($R),snapshot:proc(^R,u64,mem.Allocator)->(gfx.Capture_Snapshot,bool),path:string)->bool {
    capture,valid:=snapshot(gpu.renderer,gpu.pending.id,context.allocator)
    if !valid { fmt.eprintln("Accepted native graph capture unavailable for submission",gpu.pending.id); return false }
    defer gfx.capture_snapshot_destroy(&capture)
    extension:=strings.to_lower(filepath.ext(path)); defer delete(extension)
    if extension==".json" { bytes,encoded:=gfx.capture_json(&capture); if !encoded { return false }; defer delete(bytes); return os.write_entire_file(path,bytes)==nil }
    text:=gfx.capture_dot(&capture) if extension==".dot" else gfx.capture_text(&capture); defer delete(text)
    if path!="" { return os.write_entire_file(path,transmute([]byte)text)==nil }; fmt.print(text); return true
}
@(private="package")
diagnostic_pixels :: proc(gpu:^editor_app.GPU_Owner($R),width,height:u32)->(gfx.Readback_Data,bool) {
    source,error:=gpu.pick_ops.source(gpu.renderer,gpu.pending,gpu.output); if error!=.None { fmt.eprintln("Editor diagnostic source:",error); return {},false }
    ticket,queue_error:=gpu.pick_ops.queue(gpu.renderer,source,{width=width,height=height,depth=1}); if queue_error!=.None { fmt.eprintln("Editor diagnostic readback:",queue_error); return {},false }; defer gpu.pick_ops.destroy(gpu.renderer,ticket)
    start:=time.tick_now()
    for time.tick_since(start)<5*time.Second { data,complete,poll_error:=gpu.pick_ops.poll(gpu.renderer,ticket); if poll_error!=.None { fmt.eprintln("Diagnostic poll:",poll_error); return {},false }; if complete { return data,true }; time.sleep(time.Millisecond) }
    fmt.eprintln("Diagnostic readback timeout"); return {},false
}
@(private="package")
diagnostic_image :: proc(gpu:^editor_app.GPU_Owner($R),width,height:u32,path:string,check_black:bool)->bool {
    data,valid:=diagnostic_pixels(gpu,width,height); if !valid { return false }; defer gfx.readback_data_destroy(&data)
    if check_black { x,y:=width/2,height/2; offset:=u64(y)*data.row_pitch+u64(x)*4; if offset+3>=u64(len(data.bytes)) || data.bytes[offset]==0 && data.bytes[offset+1]==0 && data.bytes[offset+2]==0 { fmt.eprintln("Black frame detected at accepted frame",gpu.serial); return false } }
    if path=="" { return true }
    snapshot:=render.Picking_Snapshot{metadata={width=width,height=height},color=data}
    png,valid_png:=editor_app.capture_png(&snapshot); if !valid_png { fmt.eprintln("Diagnostic PNG invalid:",width,height,data.source.desc.format,len(data.bytes),data.row_pitch); return false }; defer delete(png)
    write_error:=os.write_entire_file(path,png); if write_error!=nil { fmt.eprintln("Diagnostic image write:",path,write_error) }; return write_error==nil
}

@(private="package")
diagnostic_duplicates :: proc(root:ui.Descriptor) {
    seen:=make(map[u64]string); defer delete(seen)
    stack:=make([dynamic]ui.Descriptor); defer delete(stack); append(&stack,root)
    for len(stack)>0 { node:=pop(&stack); if before,present:=seen[node.key]; present { fmt.eprintln("Duplicate retained control",node.key,before,node.text,node.kind) } else { seen[node.key]=node.text }; append(&stack,..node.children) }
}
