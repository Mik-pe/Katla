//! Document observers prepare saved baselines before any scene or filesystem publication.
package app

import ecs "../ecs"
import editor "../editor"

/// A stationary editor document owner may preflight and commit its exact saved authored baseline.
Scene_File_Observer :: struct {
    state:rawptr,
    prepare:proc(rawptr,^Authoring,^Scene_Snapshot)->(rawptr,editor.Scene_Error),
    finish:proc(rawptr,rawptr,bool),
}
/// Owns one observer token until publication either commits or rolls back.
Scene_File_Observation :: struct { observer:Scene_File_Observer,token:rawptr,active:bool }
/// Prepares a baseline from genuine owned scene data; failures precede all irreversible publication.
scene_file_observe_begin :: proc(owner:^Authoring,snapshot:^Scene_Snapshot)->(Scene_File_Observation,editor.Scene_Error) {
    observer,installed:=ecs.get_resource(&owner.world,Scene_File_Observer); if !installed { return {},.None }
    if observer.prepare==nil || observer.finish==nil { return {},.Invalid_Operation }
    token,error:=observer.prepare(observer.state,owner,snapshot); if error!=.None { return {},error }
    return {observer,token,true},.None
}
/// Publishes or destroys a precomputed baseline without a fallible postpublication allocation.
scene_file_observe_finish :: proc(observation:^Scene_File_Observation,committed:bool) { if observation.active { observation.observer.finish(observation.observer.state,observation.token,committed) }; observation^={} }
