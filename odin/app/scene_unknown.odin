//! Unknown application payloads survive ordinary scene load and save without pretending to decode them.
package app

import ecs "../ecs"
import editor "../editor"
import "core:slice"

/// Owns the original versioned extension map; prefab capture rejects opaque references.
Scene_Unknown :: struct { components:[]byte `inspect:"skip"` }
@(private="package")
scene_unknown_destroy :: proc(value:rawptr) { unknown:=cast(^Scene_Unknown)value; delete(unknown.components); unknown^={} }
@(private="package")
scene_unknown_clone :: proc(dst,src:rawptr) { (cast(^Scene_Unknown)dst)^={slice.clone((cast(^Scene_Unknown)src).components)} }
/// Installs unknown-payload ownership before a scene file is prepared.
scene_document_register :: proc(app:^Authoring) { editor.editor_register(&app.world,&app.registry,"SceneUnknown",Scene_Unknown{},ecs.Value_Ops{scene_unknown_destroy,scene_unknown_clone},spawn_default=false) }
