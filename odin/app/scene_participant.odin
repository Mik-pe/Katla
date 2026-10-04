//! Native scene owners may join CPU staging with explicit prepare and commit or rollback callbacks.
package app

import ecs "../ecs"
import editor "../editor"

Scene_Preparation_Mode :: enum { Insert, Replace, Remove }
/// Installed native consumers prepare genuine resources; CPU authoring has no implicit GPU readiness claim.
Scene_Participant :: struct {
    state:rawptr,
    prepare:proc(rawptr,^Authoring,[]ecs.Entity_Id,Scene_Preparation_Mode)->(rawptr,editor.Scene_Error),
    finish:proc(rawptr,rawptr,bool),
}
/// Owns a native preparation token only when an actual participant is installed.
Scene_Preparation :: struct { participant:Scene_Participant,token:rawptr,active:bool }
/// Joins installed native preparation before authored identities or file replacement are published.
scene_prepare_begin :: proc(app:^Authoring,entities:[]ecs.Entity_Id,mode:Scene_Preparation_Mode)->(Scene_Preparation,editor.Scene_Error) {
    participant,installed:=ecs.get_resource(&app.world,Scene_Participant)
    if !installed { return {},.None }
    if participant.prepare==nil || participant.finish==nil { return {},.Invalid_Operation }
    token,err:=participant.prepare(participant.state,app,entities,mode)
    if err!=.None { return {},err }
    return {participant,token,true},.None
}
/// Consumes one successful preparation token exactly once, publishing or discarding its native resources.
scene_prepare_finish :: proc(preparation:^Scene_Preparation,commit:bool) {
    if preparation.active { preparation.participant.finish(preparation.participant.state,preparation.token,commit) }; preparation^={}
}
