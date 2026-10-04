//! One accepted scene submission owns both surfaces and its particle simulation/draw.
package render

import gfx "../../gfx"
import "core:log"

@(private="package")
particle_publication_failed :: proc(error:Particle_Error) { log.error("Accepted particle state publication failed",error) }
/// Updates the simulation interval before acquiring the next application frame.
particle_frame_delta :: proc(consumer:^Particle_Consumer($R),delta:f32)->Particle_Error {
    if consumer.pending.ready || !particle_delta_valid(delta) { return {code=.Invalid_Configuration} }
    consumer.delta_time=delta; return {}
}
@(private="package")
particle_prepare_callback :: proc($R:typeid)->proc(rawptr,^Native_Scene(R),gfx.Frame_Token,Frame_Data)->(Scene_Inputs,Native_Error) {
    return proc(state:rawptr,scene:^Native_Scene(R),token:gfx.Frame_Token,frame:Frame_Data)->(Scene_Inputs,Native_Error) {
        consumer:=cast(^Particle_Consumer(R))state
        buffers,error:=particle_prepare(consumer,&scene.graph,token,frame,consumer.delta_time)
        if error!={} {
            if error.gpu!=.None || error.packet!=.None { return {},{gpu=error.gpu,packet=error.packet} }
            return {},{scene=.Invalid_Geometry}
        }
        return {buffers=buffers},{}
    }
}
@(private="package")
particle_accepted_callback :: proc($R:typeid)->proc(rawptr,gfx.Submission) {
    return proc(state:rawptr,submission:gfx.Submission) {
        error:=particle_committed(cast(^Particle_Consumer(R))state,submission)
        if error!={} { particle_publication_failed(error) }
    }
}
@(private="package")
particle_aborted_callback :: proc($R:typeid)->proc(rawptr) { return proc(state:rawptr) { particle_abort(cast(^Particle_Consumer(R))state) } }
@(private="package")
particle_validate_callback :: proc($R:typeid)->proc(rawptr,Particle_Owner,Particle_Selection)->Native_Error {
    return proc(state:rawptr,owner:Particle_Owner,entities:Particle_Selection)->Native_Error {
        error:=particle_scene_validate(cast(^Particle_Consumer(R))state,owner,entities)
        if error!={} { return {gpu=error.gpu,packet=error.packet,scene=.Invalid_Geometry} }
        return {}
    }
}
/// Installs a genuine particle composition on the existing stationary native scene consumer.
particle_composition :: proc(consumer:^Particle_Consumer($R))->Scene_Composition(R) {
    return {state=consumer,prepare=particle_prepare_callback(R),accepted=particle_accepted_callback(R),aborted=particle_aborted_callback(R),validate=particle_validate_callback(R)}
}
