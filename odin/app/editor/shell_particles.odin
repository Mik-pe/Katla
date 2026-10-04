//! Emitter controls edit authored descriptors while reporting only accepted native simulation state.
package editor_app
import app ".."
import ecs "../../ecs"
import ui "../../ui"
import "core:encoding/json"
import "core:fmt"

@(private="package")
shell_particles :: proc(shell:^Shell)->ui.Descriptor {
    children:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(children)
    append(&children,button("Reset particle system",.Particle_Reset_All,shell.particle_reset_requested))
    emitter,present:=ecs.get_component(&shell.state.owner.world,shell.inspector.entity,app.Particle_Emitter)
    if !shell.inspector.has_entity || !present {
        message:=text(100,"Select an entity with a ParticleEmitter. Add the component in Inspector to begin.")
        message.layout.height={};message.layout.no_shrink=true
        append(&children,message)
    } else {
        controls:=make([dynamic]ui.Descriptor,shell.allocator); defer delete(controls)
        append(&controls,button("Disable" if emitter.descriptor.active else "Enable",.Particle_Active),button("Restart emission",.Particle_Restart),button("Burst",.Particle_Burst,!emitter.descriptor.active))
        count_key:=key(100,"burst-count",u64(shell.inspector.entity))
        append(&children,ui.Descriptor{key=count_key,kind=.Numeric_Input,text="Burst count",state=ui.state(shell.ctx,count_key,0,f32(32)),minimum=1,maximum=100000,step=1,layout={height=ui.pixels(30),width=ui.percent(1)}})
        append(&children,ui.Descriptor{key=key(100,"controls"),kind=.Row,layout={gap={6,0},wrap=true},children=nodes(shell,controls[:])})
        if shell.particle_statistics!="" { append(&children,text(100,shell.particle_statistics)) }
        fields:=shell_inspector(shell,true,100); fields.key=key(100,"fields"); fields.layout.grow=1
        append(&children,fields)
    }
    return {key=key(100,"panel"),kind=.Column,layout={padding={12,12,12,12},gap={0,6}},children=nodes(shell,children[:])}
}
@(private="package")
shell_particle_click :: proc(shell:^Shell,event:ui.Click_Action)->bool {
    if Action(event.action)==.Particle_Reset_All { shell.particle_reset_requested=true;return true }
    action:=Action(event.action); if action!=.Particle_Burst && action!=.Particle_Active && action!=.Particle_Restart { return false }
    if !shell.state.selection.has_primary { return true }
    entity:=shell.state.selection.primary
    if action==.Particle_Restart { shell.state.last_error=app.particle_restart(&shell.state.owner.world,entity); return true }
    emitter,present:=ecs.get_component(&shell.state.owner.world,entity,app.Particle_Emitter); if !present { shell.state.last_error=.Component_Not_Found; return true }
    count:=f32(32); if value,ok:=ui.state_get(shell.ctx,ui.state(shell.ctx,key(100,"burst-count",u64(entity)),0,count),f32); ok { count=value }
    identity:=fmt.aprintf("%d",u64(entity),allocator=shell.allocator); defer delete(identity,shell.allocator)
    bytes:[]byte; error:json.Marshal_Error
    if action==.Particle_Burst { bytes,error=json.marshal(struct {action,entity_id:string,count:u32}{"burst",identity,u32(clamp(count,1,100000))},allocator=shell.allocator) }
    else { bytes,error=json.marshal(struct {action,entity_id:string,active:bool}{"set_active",identity,!emitter.descriptor.active},allocator=shell.allocator) }
    if error==nil { execute(shell.state,{kind=.Application,tool_name="behavior",value=bytes}); delete(bytes,shell.allocator) }
    return true
}
