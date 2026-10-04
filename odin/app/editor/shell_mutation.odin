//! Authored previews become one shared history command before another owner-thread mutation.
package editor_app

import app ".."
import editor "../../editor"

/// Preserves a rejected gesture so the user can retry or cancel its exact accepted preview.
shell_finish_gestures :: proc(shell:^Shell)->editor.Scene_Error {
    if shell.gizmo.gesture.active { if error:=gizmo_finish(&shell.gizmo); error!=.None { shell.state.last_error=error; return error } }
    if shell.field_gesture.active {
        if error:=app.scene_gesture_finish(shell.state.owner,&shell.field_gesture); error!=.None { shell.state.last_error=error; return error }
        shell.field_gesture_node=0
    }
    if shell.material.active {
        if error:=app.material_gesture_finish(shell.state.owner,&shell.material); error!=.None { shell.state.last_error=error; return error }
    }
    return .None
}
/// Installs the same admission gate for native controls and tools routed through Authoring.
shell_before_mutation :: proc(state:rawptr)->editor.Scene_Error { return shell_finish_gestures(cast(^Shell)state) }
