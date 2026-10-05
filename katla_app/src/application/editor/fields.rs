//! Reversible inspector field edits through the canonical component registry.
use super::Application;
use katla_ecs::{
    EntityId,
    scene_tool::{SceneOp, SceneToolExecutor, UndoGroup},
};

pub(crate) struct FieldDrag {
    target: (EntityId, String, String),
    undo: UndoGroup,
}

pub(super) fn edit(
    app: &mut Application,
    entity: EntityId,
    component: String,
    field: String,
    value: serde_json::Value,
) -> Result<(), String> {
    if app.play_mode != crate::application::game_state::PlayMode::Editing {
        return Err("Stop simulation before editing properties".into());
    }
    if value.as_f64().is_some_and(|v| !v.is_finite()) {
        return Err("Enter a finite value".into());
    }
    log::debug!("Inspector edit {entity} {component}.{field} = {value}");
    let op = SceneOp::SetField {
        entity,
        component: component.clone(),
        field: field.clone(),
        value,
    };
    super::agent::check_protected_entity(&op, app)?;
    super::material::finish_drag(app);
    let target = (entity, component, field);
    if app
        .editor
        .field_drag
        .as_ref()
        .is_some_and(|drag| drag.target != target)
    {
        finish_drag(app);
    }
    let (_, mut undo) =
        SceneToolExecutor::execute(op, &mut app.world, &app.editor.component_registry)
            .map_err(|error| error.to_string())?;
    if let Some(drag) = &mut app.editor.field_drag {
        drag.undo.commands.append(&mut undo.commands);
    } else {
        app.editor.field_drag = Some(FieldDrag { target, undo });
    }
    Ok(())
}
pub(in crate::application) fn finish_drag(app: &mut Application) {
    if let Some(drag) = app.editor.field_drag.take() {
        app.editor.push_undo(drag.undo);
    }
}
