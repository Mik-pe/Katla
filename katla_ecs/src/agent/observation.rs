use super::session::Observation;
use crate::World;
use crate::scene_tool::ComponentRegistry;

/// Build an observation from the current world state.
pub fn build_observation(world: &World, registry: &ComponentRegistry) -> Observation {
    let entity_count = world.entity_count();
    let type_names = registry.type_names();

    let mut summary_parts = Vec::new();
    for type_name in &type_names {
        let Some(entry) = registry.get(type_name) else {
            continue;
        };
        let count: usize = world
            .entity_ids()
            .filter(|&id| (entry.has_component)(world, id))
            .count();
        if count > 0 {
            summary_parts.push(format!("{type_name}: {count}"));
        }
    }

    let scene_summary = if summary_parts.is_empty() {
        format!("Scene: {entity_count} entities.")
    } else {
        format!(
            "Scene: {entity_count} entities. {}",
            summary_parts.join(", ")
        )
    };

    Observation {
        scene_summary,
        entity_count,
        last_action_result: None,
    }
}
