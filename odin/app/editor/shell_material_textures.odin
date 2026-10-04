//! Texture roles keep accepted previews separate from independently authored UV and sampler choices.
package editor_app
import app ".."
import agent "../../agent"
import ui "../../ui"
import "core:encoding/json"
import "core:fmt"
import "core:strconv"

MATERIAL_ROLE_LABELS := [5]string{"Albedo","Normal map","Metallic / Roughness","Occlusion","Emission"}
MATERIAL_ROOT_LABELS := [3]string{"Resources","Scene","File"}
MATERIAL_SOURCE_LABELS := [2]string{"Image","glTF image"}
MATERIAL_MIN_LABELS := [6]string{"Nearest","Linear","Nearest / nearest mip","Linear / nearest mip","Nearest / linear mip","Linear / linear mip"}
MATERIAL_MIN_NAMES := [6]string{"nearest","linear","nearest_mipmap_nearest","linear_mipmap_nearest","nearest_mipmap_linear","linear_mipmap_linear"}
MATERIAL_MAG_LABELS := [2]string{"Nearest","Linear"}
MATERIAL_WRAP_LABELS := [3]string{"Repeat","Clamp to edge","Mirrored repeat"}
MATERIAL_WRAP_NAMES := [3]string{"repeat","clamp_to_edge","mirrored_repeat"}

@(private="package")
shell_material_role :: proc(shell:^Shell)->int { value,valid:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(120,"selected-role",u64(shell.inspector.entity)),0,f32(0)),f32);return clamp(int(value),0,4) if valid else 0 }
@(private="package")
shell_material_source_key :: proc(shell:^Shell,role:int)->u64 {
    source:=shell.material_info.sources[role]
    return key(key(key(120,"source",u64(shell.inspector.entity)),source.path,u64(role)),"identity",u64(source.kind)|u64(source.root)<<8|u64(source.image_index)<<16)
}
@(private="package")
shell_material_combo :: proc(shell:^Shell,scope:u64,label:string,options:[]string,current:int,action:Action,payload:u64)->ui.Descriptor {
    identity:=key(scope,label);state:=ui.state(shell.ctx,identity,0,f32(current))
    if shell.ctx.popup.key!=identity && shell.ctx.focused.key!=identity { ui.state_set(shell.ctx,state,f32(current)) }
    return {key=key(identity,"row"),kind=.Column,layout={width=ui.percent(1),gap={0,2},no_shrink=true},children=nodes(shell,{text(identity,label),ui.Descriptor{key=identity,kind=.Combo,state=state,options=options,action=u64(action),payload=payload,disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1),no_shrink=true}}})}
}
@(private="package")
shell_material_textures :: proc(shell:^Shell)->ui.Descriptor {
    expansion:=key(120,"expanded");expanded_state:=ui.state(shell.ctx,expansion,0,false);expanded,_:=ui.state_get(shell.ctx,expanded_state,bool)
    header:=ui.Descriptor{key=expansion,kind=.Section,text="Texture images and sampling",state=expanded_state,expanded=expanded,action=u64(Action.Material_Texture_Expand),layout={height=ui.pixels(30),width=ui.percent(1),no_shrink=true}}
    if !expanded { return header }
    if !shell.material_info_ready { return {key=key(120,"section"),kind=.Column,layout={width=ui.percent(1),no_shrink=true},children=nodes(shell,{header,text(120,"Select one drawable material to edit its image roles")})} }
    role:=shell_material_role(shell);width:=max(1,shell_panel_width(shell)-24)
    columns:=clamp(int((width+6)/100),1,5);cell:=(width-6*f32(columns-1))/f32(columns)
    choices:=make([dynamic]ui.Descriptor,shell.allocator);defer delete(choices)
    for label,index in MATERIAL_ROLE_LABELS {
        image:=ui.Descriptor{key=key(122,"preview",u64(index)),kind=.Text,text="Neutral",disabled=true,layout={width=ui.percent(1),height=ui.pixels(48)}}
        if shell.material_preview_has_entity && shell.material_preview_entity==shell.inspector.entity && shell.material_preview_textures[index]!=0 { image.kind=.Image;image.text="";image.texture=shell.material_preview_textures[index] }
        else if shell.material_info.sources[index].kind!=.Neutral { image.text="Preview pending" }
        name:=text(123,label);name.key=key(123,"role-name",u64(index));name.text_max_width=max(1,cell-12);name.disabled=true
        row:=ui.Descriptor{key=key(121,"role",u64(index)),kind=.Button,action=u64(Action.Material_Role),payload=u64(index),has_background=true,background=shell.ctx.theme.active if index==role else shell.ctx.theme.control,layout={width=ui.pixels(cell),height=ui.pixels(86),padding={6,6,6,6},no_shrink=true},children=nodes(shell,{ui.Descriptor{key=key(122,"content",u64(index)),kind=.Column,layout={width=ui.percent(1),gap={0,4}},children=nodes(shell,{image,name})}})}
        append(&choices,row)
    }
    items:=make([dynamic]ui.Descriptor,shell.allocator);defer delete(items)
    append(&items,header,ui.Descriptor{key=key(120,"roles"),kind=.Grid,layout={width=ui.percent(1),columns=u32(columns),cell_size={cell,86},gap={6,6},no_shrink=true},children=nodes(shell,choices[:])})
    instruction:=text(120,"Drop a browser image onto a role. Image choices preserve surface factors and sampling.");instruction.layout.height={};instruction.layout.no_shrink=true;append(&items,instruction)
    neutral:=button("Neutral",.Material_Neutral,shell.state.owner.mode!=.Editing);original:=button("Original",.Material_Original,shell.state.owner.mode!=.Editing);browser:=button("Browser image",.Material_Browser,shell.state.owner.mode!=.Editing || shell.browser==nil || shell.browser.selected=="")
    controls:=ui.Descriptor{key=key(120,"image-buttons"),kind=.Row,layout={width=ui.percent(1),gap={6,0},wrap=true,no_shrink=true},children=nodes(shell,{neutral,original,browser})}
    if width<260 { controls.kind=.Column;controls.layout.gap={0,6};for &child in controls.children { child.layout.width=ui.percent(1) } };append(&items,controls)
    source:=shell.material_info.sources[role];source_key:=shell_material_source_key(shell,role)
    source_kind:=1 if source.kind==.GltfImage else 0
    for item in ([2]struct{label:string,options:[]string,value:int,payload:u64}{{"Source kind",MATERIAL_SOURCE_LABELS[:],source_kind,0},{"Source root",MATERIAL_ROOT_LABELS[:],int(source.root),1}}) {
        identity:=key(source_key,item.label)
        append(&items,ui.Descriptor{key=key(identity,"row"),kind=.Column,layout={width=ui.percent(1),gap={0,2},no_shrink=true},children=nodes(shell,{text(identity,item.label),ui.Descriptor{key=identity,kind=.Combo,options=item.options,state=ui.state(shell.ctx,identity,0,f32(item.value)),action=u64(Action.Material_Source_Choice),payload=item.payload,layout={height=ui.pixels(30),width=ui.percent(1),no_shrink=true}}})})
    }
    for item in ([2]struct{label,initial:string}{{"Image path",source.path},{"glTF image index",fmt.aprintf("%d",source.image_index,allocator=shell.allocator)}}) {
        if item.label=="glTF image index" { append(&shell.texts,item.initial) }
        identity:=key(source_key,item.label)
        append(&items,text(identity,item.label),ui.Descriptor{key=identity,kind=.Text_Input,state=ui.state(shell.ctx,identity,0,item.initial),placeholder=item.label,action=u64(Action.Material_Source_Text),layout={height=ui.pixels(30),width=ui.percent(1),no_shrink=true}})
    }
    append(&items,button("Assign image",.Material_Assign,shell.state.owner.mode!=.Editing))
    sampling:=app.material_sampling_roles(shell.material_info.sampling)[role]
    uv_buttons:=make([dynamic]ui.Descriptor,shell.allocator);defer delete(uv_buttons)
    for available,index in shell.material_info.uv_available {
        control:=button("UV 0" if index==0 else "UV 1",.Material_UV,!available || shell.state.owner.mode!=.Editing);control.payload=u64(index);control.has_background=true;control.background=shell.ctx.theme.active if sampling.uv.tex_coord==u32(index) else shell.ctx.theme.control;append(&uv_buttons,control)
    }
    append(&items,ui.Descriptor{key=key(120,"uv-sets"),kind=.Row,layout={width=ui.percent(1),gap={6,0},no_shrink=true},children=nodes(shell,uv_buttons[:])})
    names:=[6]string{"Offset U","Offset V","Rotation · radians","Scale U","Scale V","Anisotropy"}
    values:=[6]f32{sampling.uv.offset[0],sampling.uv.offset[1],sampling.uv.rotation,sampling.uv.scale[0],sampling.uv.scale[1],f32(sampling.sampler.max_anisotropy)}
    scope:=key(124,"sampling",u64(shell.inspector.entity)*5+u64(role))
    for label,index in names {
        identity:=key(scope,label);state:=ui.state(shell.ctx,identity,0,values[index]);if shell.ctx.captured.key!=identity && shell.ctx.focused.key!=identity { ui.state_set(shell.ctx,state,values[index]) }
        append(&items,text(identity,label),ui.Descriptor{key=identity,kind=.Numeric_Input if index<5 else .Slider,state=state,minimum= -max(f32) if index<5 else 1,maximum=max(f32) if index<5 else 16,step=0 if index<5 else 1,action=u64(Action.Material_Sampling),payload=u64(index),disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1),no_shrink=true}})
    }
    minification:=int(sampling.sampler.min_filter);if sampling.sampler.mip_filter!=.None { minification=2+2*(int(sampling.sampler.mip_filter)-1)+int(sampling.sampler.min_filter) }
    wrap_u:=0 if sampling.sampler.address_u==.Repeat else 1 if sampling.sampler.address_u==.Clamp_Edge else 2
    wrap_v:=0 if sampling.sampler.address_v==.Repeat else 1 if sampling.sampler.address_v==.Clamp_Edge else 2
    append(&items,shell_material_combo(shell,scope,"Minification",MATERIAL_MIN_LABELS[:],minification,.Material_Filter,0),shell_material_combo(shell,scope,"Magnification",MATERIAL_MAG_LABELS[:],int(sampling.sampler.mag_filter),.Material_Filter,1),shell_material_combo(shell,scope,"Wrap U",MATERIAL_WRAP_LABELS[:],wrap_u,.Material_Filter,2),shell_material_combo(shell,scope,"Wrap V",MATERIAL_WRAP_LABELS[:],wrap_v,.Material_Filter,3),shell_material_asset_controls(shell))
    return {key=key(120,"section"),kind=.Column,layout={width=ui.percent(1),gap={0,6},no_shrink=true},children=nodes(shell,items[:])}
}

@(private="package")
shell_material_request :: proc(shell:^Shell,action:string,role:int,value:json.Value) {
    if error:=shell_finish_gestures(shell);error!=.None { shell.state.last_error=error;return }
    ids:=material_targets(shell);defer delete(ids);names:=make(json.Array,0,shell.allocator);defer {for name in names {delete(name.(string),shell.allocator)};delete(names)}
    for id in ids { append(&names,fmt.aprintf("%d",u64(id),allocator=shell.allocator)) }
    request:=make(json.Object,shell.allocator);defer delete(request);request["action"]=action;request["entity_ids"]=names;request["role"]=agent.material_texture_role_name(agent.Material_Texture_Role(role));request["source" if action=="set_texture" else "patch"]=value
    bytes,error:=json.marshal(request,allocator=shell.allocator);if error!=nil { shell.state.last_error=.Decode_Failed;return };defer delete(bytes,shell.allocator)
    execute(shell.state,{kind=.Application,tool_name="material",value=bytes})
}
@(private="package")
shell_material_image :: proc(shell:^Shell,role:int,kind:string,path:string="",root:app.Mesh_Path_Root=.Resource,index:u32=0) {
    source:=make(json.Object,shell.allocator);defer delete(source);source["kind"]=kind
    asset:=make(json.Object,shell.allocator);defer delete(asset)
    if kind=="file" || kind=="gltf_image" { roots:=[3]string{"Resource","Scene","File"};asset[roots[int(root)]]=path;source["asset"]=asset;if kind=="gltf_image" {source["image_index"]=json.Integer(index)} }
    shell_material_request(shell,"set_texture",role,source)
}
@(private="package")
shell_material_texture_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    action:=Action(event.action)
    if action==.Material_Role {ui.state_set(shell.ctx,ui.state(shell.ctx,key(120,"selected-role",u64(shell.inspector.entity)),0,f32(0)),f32(min(event.payload,4)));return true}
    if action!=.Material_Neutral && action!=.Material_Original && action!=.Material_Browser && action!=.Material_Assign && action!=.Material_UV {return false}
    role:=shell_material_role(shell)
    #partial switch action {
    case .Material_Neutral:shell_material_image(shell,role,"neutral")
    case .Material_Original:shell_material_image(shell,role,"inherit")
    case .Material_Browser:
        if shell.browser!=nil {for entry in shell.browser.entries {if entry.path==shell.browser.selected && entry.kind==.Image {shell_material_image(shell,role,"file",entry.path,shell.browser.root);return true}}};message(shell,"Select an image in the asset browser")
    case .Material_Assign:
        source_key:=shell_material_source_key(shell,role);path,_:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(source_key,"Image path"),0,""),string);kind,_:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(source_key,"Source kind"),0,f32(0)),f32);root,_:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(source_key,"Source root"),0,f32(0)),f32)
        index:u64=0;if kind==1 {text_index,_:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(source_key,"glTF image index"),0,"0"),string);parsed,valid:=strconv.parse_u64(text_index);if !valid || parsed>u64(max(u32)) {shell.state.last_error=.Invalid_Field_Value;return true};index=parsed}
        shell_material_image(shell,role,"gltf_image" if kind==1 else "file",path,app.Mesh_Path_Root(int(root)),u32(index))
    case .Material_UV:
        patch:=make(json.Object,shell.allocator);defer delete(patch);patch["tex_coord"]=json.Integer(event.payload);shell_material_request(shell,"set_sampling",role,patch)
    case:
    };return true
}
@(private="package")
shell_material_filter :: proc(shell:^Shell,event:ui.Selection_Action)->bool {
    if Action(event.action)!=.Material_Filter {return false}
    patch:=make(json.Object,shell.allocator);defer delete(patch)
    switch event.payload {
    case 0:if event.index<0 || event.index>=6 {return true};patch["minification"]=MATERIAL_MIN_NAMES[event.index];if event.index%2==0 {patch["anisotropy"]=json.Integer(1)}
    case 1:if event.index<0 || event.index>=2 {return true};patch["magnification"]="nearest" if event.index==0 else "linear";if event.index==0 {patch["anisotropy"]=json.Integer(1)}
    case 2,3:if event.index<0 || event.index>=3 {return true};patch["wrap_u" if event.payload==2 else "wrap_v"]=MATERIAL_WRAP_NAMES[event.index]
    }
    shell_material_request(shell,"set_sampling",shell_material_role(shell),patch);return true
}

@(private="package")
shell_material_drop :: proc(shell:^Shell,position:ui.Vec2)->bool {
    if len(shell.asset_drag.items)!=1 || shell.asset_drag.items[0].kind!=.Image { return false }
    // Walk the painted ancestry of the topmost mounted rectangle so floating panels cannot receive drops through each other.
    for index:=len(shell.ctx.order)-1;index>=0;index-=1 {
        node,present:=shell.ctx.nodes[shell.ctx.order[index].key]
        if !present || !node.mounted || node.descriptor.hidden || !inside(node.bounds,position) || !inside(node.clip,position) {continue}
        for node!=nil {
            if Action(node.descriptor.action)==.Material_Role {
                role:=int(node.descriptor.payload);item:=shell.asset_drag.items[0]
                shell_material_image(shell,role,"file",item.path,item.root);return true
            }
            node= shell.ctx.nodes[node.parent.key]
        }
        return false
    }
    return false
}
