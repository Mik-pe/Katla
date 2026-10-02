//! One preview state machine for toolbar actions, MCP and the co-creator.
use super::super::game_state::{PlayMode, SceneSnapshot};
use crate::application::Application;
use katla_agent::behavior::SimulationOp;
use serde_json::{Value, json};

pub(super) fn execute(app: &mut Application, op: SimulationOp) -> Result<Value, String> {
    let previous = app.play_mode;
    let target = match op {
        SimulationOp::Inspect => previous,
        SimulationOp::Play if previous == PlayMode::Editing => PlayMode::Playing,
        SimulationOp::Play => previous,
        SimulationOp::Pause if previous == PlayMode::Playing => PlayMode::Paused,
        SimulationOp::Pause => previous,
        SimulationOp::Resume if previous == PlayMode::Paused => PlayMode::Playing,
        SimulationOp::Resume => previous,
        SimulationOp::Stop => PlayMode::Editing,
    };
    if target != previous {
        if previous == PlayMode::Editing {
            app.scene_snapshot = Some(SceneSnapshot::capture(app).map_err(|e| e.to_string())?);
        } else if target == PlayMode::Editing {
            let snapshot = app
                .scene_snapshot
                .take()
                .ok_or("Missing authored preview snapshot")?;
            if let Err(error) = snapshot.restore(app) {
                app.scene_snapshot = Some(snapshot);
                return Err(error.to_string());
            }
            // The loader cleared old selection/history and registered restored GPU owners.
        }
        app.play_mode = target;
        if let Some(active) = app.world.get_resource_mut::<katla_script::ScriptsActive>() {
            active.0 = target == PlayMode::Playing;
        }
        if let Some(active) = app.world.get_resource_mut::<katla_physics::PhysicsActive>() {
            active.0 = target == PlayMode::Playing;
        }
        log::info!("Preview mode: {target:?}");
    }
    let mode = match app.play_mode {
        PlayMode::Editing => "editing",
        PlayMode::Playing => "playing",
        PlayMode::Paused => "paused",
    };
    Ok(
        json!({"mode":mode,"changed":previous != target,"runtime_ids_replaced":previous != PlayMode::Editing && target == PlayMode::Editing,
        "particle_gpu":app.scene_features.as_ref().and_then(|features| features.particles.stats()).map(|stats| json!({
            "alive":stats.current_alive_count,
            "submitted_emissions":stats.total_emitted,
            "source_submission":stats.frame_count,
            "scope":"whole scene, latest completed GPU counters; may lag the current frame"
        })),
        "next_step":if app.play_mode == PlayMode::Editing { "Query entities after stop; save or capture authored state." } else { "Inspect trigger/behavior feedback; stop restores authored state." }}),
    )
}
