#+feature dynamic-literals
//! Live scene authoring uses the canonical private MCP proxy and paired GPU captures.
package main
import "../wire"
import image "../../odin/image"
import "core:os"
import "core:fmt"
import "core:time"
import "core:strings"
import "core:strconv"
import "core:path/filepath"
import "core:encoding/base64"
import "core:mem/virtual"

Options :: struct { command,socket,proxy,binary,name,save,output,project:string,size,origin:[3]f64,doorway:[2]f64,thickness:f64,ceiling,dry_run:bool }
Part :: struct { name,shape,preset:string,position,scale:[3]f64 }
room_plan :: proc(options:Options)->([]Part,bool) {
    w,h,d:=options.size[0],options.size[1],options.size[2]; dw,dh:=options.doorway[0],options.doorway[1]; t:=options.thickness; x,y,z:=options.origin[0],options.origin[1],options.origin[2]
    for value in options.size { if !finite(value) || value<=0 || value>1000 { return nil,false } }
    for value in options.origin { if !finite(value) || abs(value)>1e6 { return nil,false } }
    if strings.trim_space(options.name)=="" || !finite(t) || t<=0 || t>min(w,h,d) || !finite(dw) || !finite(dh) || dw<=0 || dw>=w || dh<=0 || dh>=h { return nil,false }
    parts:=make([dynamic]Part); front:=z+d/2+t/2; side:=(w-dw)/2
    append(&parts,Part{"Floor","cube","oak",{x,y-t/2,z},{w+2*t,t,d+2*t}},Part{"Back wall","cube","plaster",{x,y+h/2,z-d/2-t/2},{w+2*t,h,t}},Part{"Left wall","cube","plaster",{x-w/2-t/2,y+h/2,z},{t,h,d}},Part{"Right wall","cube","plaster",{x+w/2+t/2,y+h/2,z},{t,h,d}},Part{"Front left","cube","plaster",{x-dw/2-side/2,y+h/2,front},{side,h,t}},Part{"Front right","cube","plaster",{x+dw/2+side/2,y+h/2,front},{side,h,t}},Part{"Door lintel","cube","plaster",{x,y+(h+dh)/2,front},{dw,h-dh,t}})
    if options.ceiling { append(&parts,Part{"Ceiling","cube","plaster",{x,y+h+t/2,z},{w+2*t,t,d+2*t}}) }
    for &part in parts { part.name=strings.concatenate({options.name," / ",part.name}) }; return parts[:],true
}
vec :: proc(value:[3]f64)->wire.Array { return {value[0],value[1],value[2]} }
view :: proc(client:^wire.Client,action:string,args:wire.Object,label:string="",output:string="")->wire.Value {
    args:=args; args["action"]=action; reply:=wire.tool(client,"editor_view",args); value:=wire.get(reply,"structuredContent")
    for key in ([]string{"frame_id","capture_serial","submission"}) { text:=wire.s(value,key); _,ok:=strconv.parse_u64(text); wire.require(ok && !strings.contains_any(text,"+-. "),"GPU identity must be an exact decimal string") }
    wire.require(wire.s(wire.get(value,"gpu_provenance"),"submission_id")==wire.s(value,"submission"),"GPU image/metadata submission mismatch")
    png:[]byte
    for block in wire.a(reply,"content") { if text,ok:=wire.get(block,"type").(string); ok && text=="image" { wire.require(wire.s(block,"mimeType")=="image/png","Capture MIME mismatch"); decoded,error:=base64.decode(wire.s(block,"data")); wire.require(error==nil,"Invalid capture base64"); png=decoded } }
    wire.require(len(png)>32 && len(png)<=32<<20,"Missing or oversized GPU PNG")
    decoded,decode_error:=image.texture_image_decode(png); wire.require(decode_error==.None,"Invalid GPU PNG"); defer image.texture_image_destroy(&decoded)
    size:=wire.a(value,"image_size"); wire.require(len(size)==2 && wire.number(size[0])==f64(decoded.width) && wire.number(size[1])==f64(decoded.height),"Capture dimensions mismatch")
    if label!="" && output!="" { path,error:=filepath.join({output,label}); wire.require(error==nil,"Invalid output path"); wire.write(strings.concatenate({path,".png"}),string(png)); wire.write(strings.concatenate({path,".json"}),wire.encode(value)) }
    return value
}
build_room :: proc(client:^wire.Client,parts:[]Part)->wire.Value {
    created:=make(wire.Array); operations:=0
    for part in parts {
        reply:=wire.tool(client,"spawn_entity",{"name"=part.name,"shape"=part.shape,"position"=vec(part.position),"scale"=vec(part.scale)},true)
        if wire.b(reply,"isError") { rollback(client,operations); wire.require(false,"Room creation rejected; accepted edits undone") }
        operations+=1; entity:=wire.a(wire.get(reply,"structuredContent"),"entity_ids")[0]
        append(&created,wire.Object{"name"=part.name,"preset"=part.preset,"entity_id"=entity,"position"=vec(part.position),"scale"=vec(part.scale)})
    }
    for preset in ([]string{"oak","plaster"}) {
        ids:=make(wire.Array); for part in created { if wire.s(part,"preset")==preset { append(&ids,wire.get(part,"entity_id")) } }; if len(ids)==0 { continue }
        reply:=wire.tool(client,"material",{"action"="set","entity_ids"=ids,"preset"=preset},true)
        if wire.b(reply,"isError") { rollback(client,operations); wire.require(false,"Room material assignment rejected; accepted edits undone") }; operations+=1
    }
    return wire.Object{"parts"=created,"undo_steps"=i64(operations),"coordinate_contract"="Meters, Y up; position is each box center. Front is +Z."}
}
rollback :: proc(client:^wire.Client,operations:int) { for i:=0;i<operations;i+=1 { _=view(client,"undo",{}) } }
query :: proc(client:^wire.Client,name:string)->wire.Array { return wire.a(wire.data(client,"query_entities",{"name_filter"=name,"limit"=i64(256)}),"entities") }
check_bounds :: proc(rows:wire.Array,parts:[]Part) {
    wire.require(len(rows)==len(parts),"Authored part count mismatch")
    for part in parts {
        found:=false
        for row in rows { if wire.s(row,"name")!=part.name { continue }; found=true; bounds:=wire.get(row,"bounds"); center,extent:=wire.a(bounds,"center"),wire.a(bounds,"extent")
            for value,i in part.position { wire.require(abs(wire.number(center[i])-value)<0.001 && abs(wire.number(extent[i])-part.scale[i]/2)<0.001,"Actual authored bounds mismatch") }
        }; wire.require(found,"Authored part missing")
    }
}
pixel_changes :: proc(first,second:string)->int {
    a,ae:=image.texture_image_decode(transmute([]byte)wire.read(first)); wire.require(ae==.None,"Invalid first PNG"); defer image.texture_image_destroy(&a)
    b,be:=image.texture_image_decode(transmute([]byte)wire.read(second)); wire.require(be==.None,"Invalid second PNG"); defer image.texture_image_destroy(&b)
    wire.require(a.width==b.width && a.height==b.height && a.format==.RGBA8 && b.format==.RGBA8,"Capture layout mismatch")
    changes:=0; for i:=0;i<len(a.pixels);i+=4 { if a.pixels[i]!=b.pixels[i] || a.pixels[i+1]!=b.pixels[i+1] || a.pixels[i+2]!=b.pixels[i+2] { changes+=1 } }; return changes
}
join :: proc(parts:..string)->string { path,error:=filepath.join(parts); wire.require(error==nil,"Invalid path"); return path }
validate_authoring :: proc(client:^wire.Client,options:Options)->wire.Value {
    _=wire.content(client,"load_scene",{"path"=join(options.project,"assets/scenes/material-studio.katla")})
    rows:=query(client,"Materials / Porcelain"); wire.require(len(rows)>0,"Material scene entity missing"); entity:=wire.get(rows[0],"entity_id")
    before:=wire.data(client,"material",{"action"="inspect","entity_id"=entity})
    _=view(client,"set_camera",{"position"=wire.Array{f64(7.5),f64(5.5),i64(9)},"target"=wire.Array{i64(0),i64(1),f64(-0.6)}})
    _=view(client,"select",{"entity_id"=entity}); settle(client,options.output); _=view(client,"observe",{},"before",options.output)
    _=wire.content(client,"material",{"action"="set","entity_ids"=wire.Array{entity},"base_color"=wire.Array{f64(0.05),f64(0.2),f64(0.95),i64(1)},"metallic"=f64(0.15),"roughness"=f64(0.2)})
    _=view(client,"observe",{},"changed",options.output); changes:=pixel_changes(join(options.output,"before.png"),join(options.output,"changed.png")); wire.require(changes>100,"Material edit did not change GPU RGB pixels")
    wire.require(wire.b(wire.tool(client,"material",{"action"="set","entity_ids"=wire.Array{entity,"18446744073709551615"},"roughness"=f64(0.8)},true),"isError"),"Stale-ID batch accepted")
    _=view(client,"undo",{},"restored",options.output); wire.require(pixel_changes(join(options.output,"before.png"),join(options.output,"restored.png"))==0,"Undo changed accepted GPU pixels")
    wire.require(wire.encode(wire.data(client,"material",{"action"="inspect","entity_id"=entity}))==wire.encode(before),"Undo changed material state")
    room:=options; room.name="Agent study"; room.origin={20,0,-4}; plan,ok:=room_plan(room); wire.require(ok,"Invalid validation room"); receipt:=build_room(client,plan); check_bounds(query(client,"Agent study /"),plan)
    saved:=join(options.output,"authored.katla"); _=wire.content(client,"save_scene",{"path"=saved}); _=wire.content(client,"load_scene",{"path"=saved}); check_bounds(query(client,"Agent study /"),plan)
    return wire.Object{"checks"="PASS","material_changed_pixels"=i64(changes),"undo_changed_pixels"=i64(0),"room_receipt"=receipt,"saved_scene"=saved}
}
validate_view :: proc(client:^wire.Client,options:Options)->wire.Value {
    spawned:=wire.content(client,"spawn_entity",{"name"="Native View Fixture","shape"="cube","position"=wire.Array{i64(40),i64(0),i64(0)}}); entity:=wire.get(spawned,"entity_ids"); id:=wire.array(entity)[0]
    _=view(client,"select",{"entity_id"=nil},"cleared",options.output)
    camera:=view(client,"set_camera",{"position"=wire.Array{i64(40),i64(2),i64(10)},"target"=wire.Array{i64(40),i64(0),i64(0)}},"camera",options.output)
    wire.require(wire.s(camera,"center_pick")==wire.text(id),"GPU center pick missed authored entity")
    _=view(client,"select",{"entity_id"=id},"selected",options.output); _=view(client,"focus",{"entity_id"=id,"select"=true},"focused",options.output)
    _=view(client,"select",{"entity_id"=nil}); undone:=view(client,"undo",{},"undone",options.output); wire.require(wire.b(undone,"redo_available"),"Undo missing redo")
    wire.require(wire.b(wire.tool(client,"editor_view",{"action"="select","entity_id"=id},true),"isError"),"Stale selection accepted")
    redone:=view(client,"redo",{},"redone",options.output); wire.require(wire.b(redone,"undo_available"),"Redo missing undo")
    _=wire.content(client,"save_scene",{"path"="native-view.katla"}); _=wire.content(client,"save_scene",{"path"=nil}); _=wire.content(client,"load_scene",{"path"="native-view.katla"}); _=view(client,"observe",{"limit"=i64(1)},"loaded",options.output)
    return wire.Object{"checks"="PASS","entity"=id,"scope"="actual private MCP and paired GPU PNG camera/selection/focus/undo/redo/save/load"}
}
main :: proc() {
    arena:virtual.Arena; wire.require(virtual.arena_init_growing(&arena)==nil,"Cannot allocate tool arena"); defer virtual.arena_destroy(&arena); context.allocator=virtual.arena_allocator(&arena)
    project,error:=filepath.abs("."); wire.require(error==nil,"Cannot locate project")
    options:=Options{project=project,command="room",name="Room",size={6,3,8},doorway={1.1,2.2},thickness=0.15,output=join(project,"target/author-proof")}
    for i:=1;i<len(os.args);i+=1 {
        arg:=os.args[i]
        switch arg {
        case "room","view","validate","prefabs","furnish","shared-view": options.command=arg
        case "--help": fmt.println("odin run tools/author -- room|view|validate|prefabs|furnish|shared-view [--socket FILE --proxy FILE|--binary FILE] [--name NAME --size W H D --origin X Y Z --doorway W H --thickness T --ceiling --dry-run --save FILE --output DIR --project DIR]"); return
        case "--ceiling": options.ceiling=true
        case "--dry-run": options.dry_run=true
        case "--size","--origin","--doorway":
            count:=2 if arg=="--doorway" else 3; wire.require(i+count<len(os.args),"Missing vector components")
            for j:=0;j<count;j+=1 { i+=1; value,ok:=strconv.parse_f64(os.args[i]); wire.require(ok,"Invalid numeric option"); if arg=="--size" { options.size[j]=value } else if arg=="--origin" { options.origin[j]=value } else { options.doorway[j]=value } }
        case "--socket","--proxy","--binary","--name","--save","--output","--project","--thickness":
            i+=1; wire.require(i<len(os.args),"Missing option value"); value:=os.args[i]
            switch arg {
            case "--socket": options.socket=value
            case "--proxy": options.proxy=value
            case "--binary": options.binary=value
            case "--name": options.name=value
            case "--save": options.save=value
            case "--output": options.output=value
            case "--project": options.project=value
            case "--thickness": number,ok:=strconv.parse_f64(value); wire.require(ok,"Invalid thickness"); options.thickness=number
            }
        case: wire.require(false,strings.concatenate({"Unknown author option: ",arg}))
        }
    }
    plan,valid:=room_plan(options); wire.require(valid,"Room dimensions, doorway or origin invalid")
    if options.dry_run { fmt.println(wire.encode(wire.Object{"parts"=wire.parse(wire.encode(plan))})); return }
    command:=make([dynamic]string)
    wire.require((options.binary!="")!=(options.socket!=""),"Choose a private socket or explicit stdio binary")
    if options.binary!="" { append(&command,options.binary,options.project,join(options.project,"resources")) }
    else { wire.require(options.proxy!="","Provide the built MCP --proxy for the private socket"); append(&command,options.proxy,options.socket) }
    _=os.set_env("KATLA_MCP_SOCKET",""); _=os.set_env("KATLA_CODEX_SOCKET",""); _=os.set_env("KATLA_CODEX_THREAD","")
    client:=wire.client(command[:]); defer wire.abort(&client.child)
    discovery:=wire.rpc(&client,"server/discover",{}); wire.require(wire.text(wire.a(discovery,"supportedVersions")[0])=="2026-07-28","Unsupported MCP version"); _=wire.rpc(&client,"tools/list",{})
    wire.require(os.is_dir(options.output) || os.make_directory_all(options.output)==nil,"Cannot create evidence directory")
    result:wire.Value
    switch options.command {
    case "room": result=build_room(&client,plan)
    case "validate": result=validate_authoring(&client,options)
    case "view": result=validate_view(&client,options)
    case "prefabs": result=validate_prefabs(&client,options)
    case "furnish": result=furnish(&client,options)
    case "shared-view": result=shared_view(&client,options)
    }
    if options.save!="" { _=wire.content(&client,"save_scene",{"path"=options.save}) }
    wire.write(join(options.output,"receipt.json"),wire.encode(result)); fmt.println(wire.encode(result)); wire.finish(&client.child)
}

finite :: proc(value:f64)->bool { return value>= -max(f64) && value<=max(f64) }

settle :: proc(client:^wire.Client,output:string) {
    _=view(client,"observe",{},"settle-first",output); started:=time.tick_now()
    for time.tick_since(started)<20*time.Second { time.sleep(100*time.Millisecond); _=view(client,"observe",{},"settle-second",output); if pixel_changes(join(output,"settle-first.png"),join(output,"settle-second.png"))==0 { return }; wire.write(join(output,"settle-first.png"),wire.read(join(output,"settle-second.png"))) }; wire.require(false,"Static authoring fixture did not settle within 20 seconds")
}
