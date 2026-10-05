#+feature dynamic-literals
//! Reversible live room/prefab journeys retain exact scene identities and native captures.
package main
import "../wire"
import "core:strings"
import "core:os"

ids :: proc(rows:wire.Array)->map[string]bool { result:=make(map[string]bool); for row in rows { result[wire.s(row,"entity_id")]=true }; return result }
same_ids :: proc(first,second:map[string]bool)->bool { if len(first)!=len(second) { return false }; for id in first { if !second[id] { return false } }; return true }
shared_view :: proc(client:^wire.Client,options:Options)->wire.Value {
    _=wire.content(client,"load_scene",{"path"=join(options.project,"assets/scenes/shared-room.katla")})
    _=view(client,"set_camera",{"position"=wire.Array{i64(0),f64(1.6),i64(1)},"target"=wire.Array{i64(0),f64(1.2),i64(-6)}})
    room:=view(client,"select",{"entity_id"=nil},"room",options.output); wire.require(wire.is_null(wire.get(room,"selected_entity_id")),"Selection not cleared")
    left,right:wire.Value
    for candidate in wire.a(room,"candidates") {
        name:=wire.s(candidate,"name"); wire.require(name!="Föremål bakom kameran","Behind-camera entity reported in frustum")
        wire.require(wire.s(candidate,"visibility")=="frustum_candidate_occlusion_unknown","Candidate incorrectly claims pixel visibility")
        if name=="Dörr vänster" { left=candidate }; if name=="Dörr höger" { right=candidate }
    }
    wire.require(left!=nil && right!=nil && wire.number(wire.a(left,"screen_rect")[2])<0.5 && wire.number(wire.a(right,"screen_rect")[0])>0.5,"Room door framing incorrect")
    limited:=view(client,"observe",{"limit"=i64(1)}); wire.require(wire.b(limited,"truncated") && len(wire.a(limited,"candidates"))==1,"View truncation missing")
    entity:=wire.get(left,"entity_id"); focused:=view(client,"focus",{"entity_id"=entity,"select"=false},"focused",options.output); wire.require(wire.is_null(wire.get(focused,"selected_entity_id")) && wire.s(focused,"center_pick")==wire.text(entity),"Focus changed selection or missed GPU pick")
    _=view(client,"select",{"entity_id"=entity},"selected",options.output)
    transform:=wire.data(client,"get_component_attributes",{"entity_id"=entity,"component"="SceneTransform"}); local:=wire.object(wire.get(transform,"local")); scale:=wire.array(local["scale"]); scale[0]=wire.number(scale[0])*1.5; local["scale"]=scale
    _=wire.content(client,"set_field",{"entity_id"=entity,"component"="SceneTransform","field"="local","value"=local}); _=view(client,"observe",{},"widened",options.output); _=view(client,"undo",{},"restored",options.output)
    wire.require(wire.b(wire.tool(client,"editor_view",{"action"="focus","entity_id"="18446744073709551615"},true),"isError"),"Stale focus accepted")
    _=wire.content(client,"spawn_entity",{"name"="Provstol","shape"="cube","position"=wire.Array{i64(1),f64(0.5),i64(-3)},"scale"=wire.Array{i64(1),f64(0.5),i64(1)}}); _=view(client,"observe",{},"placed",options.output); _=view(client,"undo",{})
    wire.require(len(query(client,"Provstol"))==0,"Placement undo failed")
    return wire.Object{"checks"="PASS","door_left"=entity,"door_right"=wire.get(right,"entity_id"),"scope"="actual shared editor scene, frustum, GPU center pick and reversible placement"}
}
placement_valid :: proc(position,scale:[3]f64)->bool {
    low,high:[3]f64; for p,i in position { if !finite(p) || !finite(scale[i]) || scale[i]<=0 { return false }; low[i]=p-scale[i]/2; high[i]=p+scale[i]/2 }
    if low[0]< -3.9 || high[0]>3.9 || low[2]< -6.8 || high[2]>2.9 || low[1]< -0.00001 || high[1]>3 { return false }
    if high[1]>0.2 {
        if !(high[0]<= -1 || low[0]>=1) { return false }
        for x in ([]f64{-2.2,2.2}) { if !(high[0]<=x-0.8 || low[0]>=x+0.8 || high[2]<= -7 || low[2]>= -5) { return false } }
    }; return true
}
furnish :: proc(client:^wire.Client,options:Options)->wire.Value {
    plan:=wire.parse(wire.read(join(options.project,"assets/scenes/teen-room-plan.json"))); placements:=wire.a(plan,"placements"); parts:=make([]Part,len(placements))
    for placement,i in placements { parts[i].name=wire.s(placement,"name"); for j:=0;j<3;j+=1 { parts[i].position[j]=wire.number(wire.a(placement,"position")[j]); parts[i].scale[j]=wire.number(wire.a(placement,"scale")[j]) }; wire.require(placement_valid(parts[i].position,parts[i].scale),"Blockout occupies central passage, door approach or room bounds") }
    _=wire.content(client,"load_scene",{"path"=join(options.project,"assets/scenes/shared-room.katla")}); _=view(client,"set_camera",wire.object(wire.get(plan,"camera")))
    baseline:=view(client,"select",{"entity_id"=nil},"before",options.output); before:=ids(query(client,"")); placed:=0
    for placement in placements {
        reply:=wire.tool(client,"spawn_entity",wire.object(placement),true)
        if wire.b(reply,"isError") { rollback(client,placed); wire.require(false,"Blockout rejected; accepted placements undone") }; placed+=1
    }
    _=view(client,"observe",{},"furnished",options.output); check_bounds(query(client,"Blockout -"),parts); rollback(client,placed)
    restored:=view(client,"observe",{},"restored",options.output); wire.require(same_ids(before,ids(query(client,""))) && wire.encode(wire.get(restored,"camera"))==wire.encode(wire.get(baseline,"camera")),"Blockout undo changed baseline")
    for placement in placements { _=wire.content(client,"spawn_entity",wire.object(placement)) }; final:=view(client,"observe",{},"ready",options.output)
    return wire.Object{"checks"="PASS","placements"=i64(placed),"final_submission"=wire.get(final,"submission"),"kind"="real scene-tool blockout, not live-model QA"}
}
validate_prefabs :: proc(client:^wire.Client,options:Options)->wire.Value {
    scratch:=join(options.project,"resources/.prefab-proof-odin"); wire.require(!os.exists(scratch),"Prefab proof directory already exists"); wire.require(os.make_directory_all(scratch)==nil,"Cannot create prefab scratch"); defer { wire.require(os.remove_all(scratch)==nil,"Cannot remove owned prefab scratch") }
    path:="resources/.prefab-proof-odin/chair.katprefab"; capture:="resources/.prefab-proof-odin/captured.katprefab"
    _=wire.content(client,"load_scene",{"path"=join(options.project,"assets/scenes/shared-room.katla")})
    described:=wire.data(client,"prefab",{"action"="describe"}); mesh:=wire.get(described,"mesh"); mesh_path:="resources/.prefab-proof-odin/seat.katmesh"
    stats:=wire.data(client,"prefab",{"action"="validate","path"=mesh_path,"document"=mesh}); wire.require(wire.number(wire.get(stats,"vertices"))>0 && !wire.b(stats,"published"),"Prefab validation changed file")
    wire.require(wire.b(wire.data(client,"prefab",{"action"="write","path"=mesh_path,"document"=mesh}),"published"),"Mesh publication missing")
    template:=wire.get(wire.data(client,"prefab",{"action"="read","path"="resources/prefabs/chair.katprefab"}),"document")
    for row in wire.a(wire.get(template,"scene"),"entities") { row:=wire.object(row); row["name"]=strings.concatenate({"Prefab proof / ",wire.text(row["name"])}) }
    _=wire.data(client,"prefab",{"action"="write","path"=path,"document"=template})
    rejected:=wire.parse(wire.encode(template)); bad:=wire.object(wire.a(wire.get(rejected,"scene"),"entities")[1]); bad["id"]=wire.get(rejected,"root")
    wire.require(wire.b(wire.tool(client,"prefab",{"action"="write","path"=path,"document"=rejected},true),"isError"),"Duplicate document ID accepted")
    wire.require(wire.encode(wire.get(wire.data(client,"prefab",{"action"="read","path"=path}),"document"))==wire.encode(template),"Rejected prefab write changed file")
    inserted:=wire.content(client,"prefab",{"action"="instantiate","path"=path,"position"=wire.Array{i64(0),i64(0),i64(-3)}}); wire.require(len(wire.a(inserted,"entity_ids"))==3,"Prefab hierarchy missing")
    root:=wire.get(wire.get(inserted,"data"),"root_entity"); _=view(client,"focus",{"entity_id"=root},"instantiated",options.output)
    _=wire.data(client,"prefab",{"action"="capture","path"=capture,"root_entity"=root}); original:=ids(query(client,"Prefab proof /"))
    copied:=wire.content(client,"prefab",{"action"="instantiate","path"=capture,"position"=wire.Array{i64(3),i64(0),i64(-3)}}); copied_ids:=wire.a(copied,"entity_ids"); wire.require(len(copied_ids)==3,"Captured prefab lost hierarchy")
    for id in copied_ids { wire.require(!original[wire.text(id)],"Prefab copy reused identity") }
    copy_root:=wire.get(wire.get(copied,"data"),"root_entity"); removed:=wire.data(client,"prefab",{"action"="remove","root_entity"=copy_root}); wire.require(wire.number(wire.get(removed,"removed"))==3,"Prefab subtree removal incomplete")
    _=view(client,"undo",{},"remove-undo",options.output); wire.require(len(query(client,"Prefab proof /"))==6,"Subtree undo failed"); _=view(client,"redo",{},"remove-redo",options.output); wire.require(same_ids(original,ids(query(client,"Prefab proof /"))),"Subtree redo changed original")
    wire.require(wire.s(wire.data(client,"simulation",{"action"="play"}),"mode")=="playing","Play not admitted"); wire.require(wire.b(wire.tool(client,"prefab",{"action"="instantiate","path"=capture},true),"isError"),"Prefab admitted while playing")
    _=wire.data(client,"simulation",{"action"="pause"}); stopped:=wire.data(client,"simulation",{"action"="stop"}); wire.require(wire.b(stopped,"runtime_ids_replaced"),"Stop retained runtime identities")
    saved:=join(options.output,"authored.katla"); _=wire.content(client,"save_scene",{"path"=saved}); _=wire.content(client,"load_scene",{"path"=saved}); wire.require(len(query(client,"Prefab proof /"))==3,"Reload lost prefab entities"); _=view(client,"observe",{},"reloaded",options.output)
    return wire.Object{"checks"="PASS","mesh_stats"=stats,"scope"="native mesh/write rejection, hierarchy/capture/copy, undo/redo, play gate, stop and save/reload"}
}
