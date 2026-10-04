//! Native particle layout and commit metadata remain application-owned.
package render

import ecs "../../ecs"
import gfx "../../gfx"
import km "../../math"
import "core:mem"
import app ".."

/// Exact WGSL particle storage stride; no field contains a native resource identity.
Particle_Data :: struct { position:[3]f32,scale:f32,velocity:[3]f32,lifetime:f32,color:[4]f32,emitter_index:u32,max_lifetime,initial_scale,padding:f32 }
Particle_Frame :: struct { delta_time:f32,total_emit_count,emitter_count,random_seed,total_simulate_count,burst_count,frame_index,max_particles:u32 }
Particle_Counters :: struct { alive,dead,emitted,workgroups_finished:u32 }
/// Initialized alignment fields preserve the canonical 160-byte emitter storage ABI.
Particle_Config :: struct {
    position:[3]f32,pad_position:f32,
    shape:u32,emit_rate,base_lifetime,lifetime_variation:f32,
    velocity_direction:[3]f32,pad_velocity:f32,
    velocity_magnitude,velocity_cone_angle,base_scale,scale_variation:f32,
    color:[4]f32,color_variation:f32,pad_color:[3]f32,
    color_end,shape_params:[4]f32,
    gravity,turbulence_strength,turbulence_frequency:f32,kill_all:u32,
    scale_end:f32,padding:[3]f32,
}
Particle_Camera :: struct { view_projection:km.Mat4,right,up,clip:km.Vec4 }
/// Buffer-only native requirements keep particle policy out of the generic GPU core.
Particle_GPU_Ops :: struct($R:typeid) {
    create_compute:proc(^R,gfx.Compute_Desc)->(gfx.Pipeline_Handle,gfx.Gpu_Error),
    destroy_compute:proc(^R,gfx.Pipeline_Handle)->gfx.Gpu_Error,
    create_graphics:proc(^R,gfx.Graphics_Desc)->(gfx.Graphics_Pipeline_Handle,gfx.Gpu_Error),
    destroy_graphics:proc(^R,gfx.Graphics_Pipeline_Handle)->gfx.Gpu_Error,
    create_buffer:proc(^R,gfx.Buffer_Desc,[]byte)->(gfx.Buffer_Handle,gfx.Gpu_Error),
    destroy_buffer:proc(^R,gfx.Buffer_Handle)->gfx.Gpu_Error,
    write_buffer:proc(^R,gfx.Frame_Token,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error,
    read_buffer:proc(^R,gfx.Buffer_Handle,u64,[]byte)->gfx.Gpu_Error,
}
Particle_Error_Code :: enum { None, Invalid_Configuration, Emitter_Capacity, Particle_Capacity, Invalid_Frame, Invalid_Graph, Queue_Changed }
Particle_Error :: struct { code:Particle_Error_Code,gpu:gfx.Gpu_Error,packet:gfx.Packet_Error }
@(private="package")
Particle_Emitter_State :: struct { entity:ecs.Entity_Id,config:Particle_Config,accumulator:f64,active,kill_on_destroy:bool }
@(private="package")
Particle_Burst :: struct { entity:ecs.Entity_Id,counts:[]u32 }
@(private="package")
Particle_Preparation :: struct { token:gfx.Frame_Token,states:[]Particle_Emitter_State,bursts:[]Particle_Burst,indices:[]u32,requested,burst_count:u32,ready,deferred:bool,delta:f32 }
@(private="package")
Particle_Slot :: struct { alive,working,counters,indirect,dispatch,frame,configs,indices,camera,readback:gfx.Buffer_Handle,sequence:u64 }
@(private="package")
Particle_Record :: struct { sequence:u64,requested:u32 }
@(private="package")
Particle_Owner :: ^app.Authoring
@(private="package")
Particle_Selection :: []ecs.Entity_Id
@(private="package")
Particle_Allocator :: mem.Allocator
/// Shader generation and all mutable uploads are explicit owners of their acquired frame slot.
Particle_Consumer :: struct($R:typeid) {
    owner:Particle_Owner,renderer:^R,operations:Particle_GPU_Ops(R),
    shaders:Particle_Shader,pipelines:[4]gfx.Pipeline_Handle,pipeline:gfx.Graphics_Pipeline_Handle,
    data,dead,initial_counters:gfx.Buffer_Handle,slots:[]Particle_Slot,
    states:[]Particle_Emitter_State,pending:Particle_Preparation,records:[dynamic]Particle_Record,
    previous_slot:int,has_previous:bool,capacity,emitter_capacity:u32,
    sequence,observed_sequence:u64,observed_alive,alive_upper:u32,capture_state:bool,delta_time:f32,
    graph:^Scene_Graph,resources:Particle_Resources,
    inputs:[dynamic]gfx.Buffer_Input,allocator:Particle_Allocator,
}
@(private="package")
Particle_Resources :: struct {
    data,dead,previous_alive,working,alive,previous_counters,counters,indirect,dispatch,frame,configs,indices,camera,readback:gfx.Resource_Id,
    compute_passes:[4]gfx.Pass_Id,
}
#assert(size_of(Particle_Data)==64)
#assert(size_of(Particle_Frame)==32)
#assert(size_of(Particle_Config)==160)
#assert(size_of(Particle_Camera)==112)
