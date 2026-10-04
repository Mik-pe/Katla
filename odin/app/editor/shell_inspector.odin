//! Generic controls bind encoded component leaves to canonical authored operations.
package editor_app

import ui "../../ui"
import "core:encoding/json"
import "core:strconv"
import "core:fmt"

@(private="package")
shell_field_label :: proc(shell:^Shell,component:string,field:^Inspector_Field)->string {
    if component!="SceneTransform" || len(field.label)!=1 || field.label[0]<'0' || field.label[0]>'3' { return field.label }
    group:=""
    for part in ([3]string{"position","rotation","scale"}) {
        if len(field.path)>=len(part)+3 && field.path[len(field.path)-len(part)-2:len(field.path)-2]==part { group=part;break }
    }
    if group=="" { return field.label }
    axes:=[4]string{"X","Y","Z","W"}
    title:="Position" if group=="position" else "Scale" if group=="scale" else "Rotation quaternion"
    label:=fmt.aprintf("%s %s",title,axes[int(field.label[0]-'0')],allocator=shell.allocator);append(&shell.texts,label);return label
}

@(private="package")
shell_inspector :: proc(shell:^Shell,particle_only:bool=false,scope:u64=30)->ui.Descriptor {
    content:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(content)
    if !shell.inspector.has_entity { append(&content,text(scope+0,"Select an entity to inspect its components")) }
    for &component,index in shell.inspector.components {
        if particle_only && component.name!="ParticleEmitter" { continue }
        if component.name=="Script" { append(&content,shell_script(shell)) }
        if component.name=="SurfaceMaterial" { append(&content,shell_material(shell)); continue }
        header:=text(scope+1,component.name); header.key=key(scope+1,component.name)
        if component.removable {
            remove:=button("Remove",.Remove_Component,shell.state.owner.mode!=.Editing); remove.key=key(scope+2,component.name); remove.payload=u64(index+1)
            header={key=key(scope+3,component.name),kind=.Row,layout={gap={8,0},padding={16,0,0,0}},children=nodes(shell,{header,remove})}
        }
        append(&content,header)
        for &field in component.fields {
            append(&shell.bindings,Field_Binding{component=component.name,field=&field})
            field_key:=key(key(scope+4,component.name),field.path,u64(shell.inspector.entity))
            control:=ui.Descriptor{key=field_key,text=shell_field_label(shell,component.name,&field),action=u64(Action.Field),payload=u64(len(shell.bindings)),disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1)}}
            value,parse_error:=json.parse(field.value,spec=.JSON,parse_integers=true,allocator=shell.allocator)
            if parse_error!=.None { continue }
            switch decoded in value {
            case json.Float,json.Integer:
                number,valid:=strconv.parse_f32(string(field.value)); if !valid { number=0 }
                if field.kind==.Enum && len(field.variants)>0 && len(field.variant_values)==len(field.variants) {
                    control.kind=.Combo;control.options=field.variants;current:f32=-1
                    integer,integer_valid:=strconv.parse_i64(string(field.value))
                    if integer_valid { for variant,i in field.variant_values { if variant==integer { current=f32(i);break } } }
                    control.state=ui.state(shell.ctx,field_key,0,current)
                    if shell.ctx.focused.key!=field_key { ui.state_set(shell.ctx,control.state,current) }
                    json.destroy_value(value);append(&content,control);continue
                }
                control.kind=.Drag_Value
                control.minimum=-100000; control.maximum=100000; control.step=0.01
                if field.constraints.has_min { control.minimum=field.constraints.min }; if field.constraints.has_max { control.maximum=field.constraints.max }
                if field.constraints.speed>0 { control.step=field.constraints.speed }; if field.kind==.Int { control.step=max(1,control.step) }
                control.state=ui.state(shell.ctx,field_key,0,number)
                if shell.ctx.focused.key!=field_key && shell.ctx.captured.key!=field_key { ui.state_set(shell.ctx,control.state,number) }
            case json.Boolean:
                control.kind=.Checkbox; control.state=ui.state(shell.ctx,field_key,0,decoded)
                if shell.ctx.focused.key!=field_key { ui.state_set(shell.ctx,control.state,decoded) }
            case json.String:
                if field.kind==.Enum && len(field.variants)>0 {
                    control.kind=.Combo; control.options=field.variants; current:f32=0
                    for variant,i in field.variants { if variant==decoded { current=f32(i); break } }
                    control.state=ui.state(shell.ctx,field_key,0,current)
                    if shell.ctx.focused.key!=field_key { ui.state_set(shell.ctx,control.state,current) }
                } else {
                    control.kind=.Text_Input; control.placeholder=field.label; control.state=ui.state(shell.ctx,field_key,0,decoded)
                    if shell.ctx.focused.key!=field_key { ui.state_set(shell.ctx,control.state,decoded) }
                }
            case json.Null: control.kind=.Text; control.text="None"
            case json.Array:
                if field.kind==.Color && (len(decoded)==3 || len(decoded)==4) { append(&content,shell_color(shell,component.name,&field,field_key,decoded)); json.destroy_value(value); continue }; control.kind=.Text
            case json.Object: control.kind=.Text; control.text=""
            }
            json.destroy_value(value)
            append(&content,control)
        }
    }
    if shell.inspector.has_entity && len(shell.inspector.available)>0 && !particle_only {
        add_key:=key(scope+0,"add-component",u64(shell.inspector.entity))
        append(&content,ui.Descriptor{key=add_key,kind=.Combo,text="Add component",action=u64(Action.Add_Component),options=shell.inspector.available[:],state=ui.state(shell.ctx,add_key,0,f32(-1)),disabled=shell.state.owner.mode!=.Editing,layout={height=ui.pixels(30),width=ui.percent(1)}})
    }
    return {key=key(scope+0,"panel"),kind=.Scroll_Area,children=nodes(shell,{ui.Descriptor{key=key(scope+0,"content"),kind=.Column,layout={padding={12,12,12,12},gap={0,6},width=ui.percent(1)},children=nodes(shell,content[:])}})}
}
