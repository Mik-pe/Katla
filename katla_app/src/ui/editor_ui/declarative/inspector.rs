use std::boxed::Box;
use std::path::PathBuf;

use katla_ecs::EntityId;
use katla_math::Rect2D;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, Padding, StateId, Widget, WidgetBox, button, empty, hstack,
    icon, panel_body, property_row, scroll, section, selectable, text, textfield, vstack,
};

use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::{ColliderShapeType, EntityInfo, InspectorEditState};

/// Component types that get an inspector section, in the canonical order used
/// for state-slot reservation. The list must stay stable: sibling views share
/// this view's positional state-slot counter, so reordering or conditionally
/// reserving slots would shift every later view's slots frame-to-frame.
const SECTION_TYPES: [&str; 17] = [
    "Transform",
    "NameComponent",
    "Drawable",
    "PointLight",
    "DirectionalLight",
    "PerspectiveComponent",
    "ScriptComponent",
    "ParticleEmitterComponent",
    "AudioEmitter",
    "AudioSource",
    "AudioListener",
    "VelocityComponent",
    "ReverbZone",
    "ColliderShape",
    "CollisionFilter",
    "RigidBody",
    "PhysicsMaterial",
];

/// Human-readable label for a component type name.
fn component_display_name(type_name: &str) -> &'static str {
    match type_name {
        "NameComponent" => "Name",
        "VelocityComponent" => "Velocity",
        "ParticleEmitterComponent" => "Particle Emitter",
        "PerspectiveComponent" => "Perspective Camera",
        "ScriptComponent" => "Script",
        "PointLight" => "Point Light",
        "DirectionalLight" => "Directional Light",
        "AudioEmitter" => "Audio Emitter",
        "AudioSource" => "Audio Source",
        "AudioListener" => "Audio Listener",
        "ReverbZone" => "Reverb Zone",
        "ColliderShape" => "Collider",
        "CollisionFilter" => "Collision Filter",
        "RigidBody" => "Rigid Body",
        "PhysicsMaterial" => "Physics Material",
        other => match other {
            "Transform" => "Transform",
            "Drawable" => "Drawable",
            _ => "Component",
        },
    }
}

/// Registry types the entity does not already have, matching the filter text
/// against both the type name and the display label (case-insensitive).
/// Sorted by display label so the picker reads alphabetically.
fn addable_components<'a>(available: &[&'a str], owned: &[String], filter: &str) -> Vec<&'a str> {
    let needle = filter.trim().to_lowercase();
    let mut addable: Vec<&'a str> = available
        .iter()
        .copied()
        .filter(|t| !owned.iter().any(|o| o == *t))
        .filter(|t| {
            needle.is_empty()
                || t.to_lowercase().contains(&needle)
                || component_display_name(t).to_lowercase().contains(&needle)
        })
        .collect();
    addable.sort_by_key(|t| component_display_name(t).to_lowercase());
    addable
}

/// Whether the section header shows a remove button: exactly the types the
/// scene tool registry can remove.
fn removable(available_components: &[&str], type_name: &str) -> bool {
    available_components.contains(&type_name)
}

/// Environment data injected before each frame for the inspector panel.
#[derive(Clone)]
pub(crate) struct InspectorDrawCtx {
    pub bounds: Rect2D,
    pub selected_entity: Option<EntityId>,
    pub entities: Vec<EntityInfo>,
    #[expect(dead_code)]
    pub edit: InspectorEditState,
    pub theme: ColorScheme,
    pub available_components: Vec<&'static str>,
    pub add_component_open: bool,
    #[expect(dead_code)]
    pub focus_script_input: bool,
    pub audio_listener_count: usize,
}

pub(crate) struct InspectorView;

/// Actions emitted by the inspector panel.
#[derive(Clone, Debug)]
pub(crate) enum InspectorAction {
    TogglePicker,
    Add { entity: EntityId, component: String },
    Remove { entity: EntityId, component: String },
}

impl Build for InspectorView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let draw_ctx = ctx.env::<InspectorDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        let scroll_id: StateId = ctx.state(0.0f32);

        // Expansion slots are reserved unconditionally, in the stable
        // SECTION_TYPES order: every sibling view shares this view's
        // positional state-slot counter, so a slot that appears only while an
        // entity is selected would shift every later view's slots
        // frame-to-frame (type confusion and panics downstream).
        let section_ids: Vec<StateId> = SECTION_TYPES.map(|_| ctx.state(true)).to_vec();
        // Add-component filter text, reserved unconditionally like the slots
        // above (lives entirely in view state, no env round-trip needed).
        let filter_id: StateId = ctx.state(String::new());

        let content = if let Some(entity) = draw_ctx
            .entities
            .iter()
            .find(|e| draw_ctx.selected_entity == Some(e.id))
        {
            let mut sections: Vec<Box<dyn Widget>> = Vec::new();

            // Entity identity header: name carries the hierarchy's display
            // numbering, the type sits underneath as secondary metadata.
            sections.push(
                vstack([
                    text(&entity.name)
                        .color(draw_ctx.theme.text_primary)
                        .font_size(FontSize::Medium)
                        .boxed(),
                    text(&entity.entity_type)
                        .color(draw_ctx.theme.text_secondary)
                        .font_size(FontSize::Small)
                        .boxed(),
                ])
                .spacing(2.0)
                .boxed(),
            );

            for (index, type_name) in SECTION_TYPES.iter().enumerate() {
                if !entity.components.iter().any(|c| c == type_name) {
                    continue;
                }
                let Some(component) = self.build_component_section(
                    ctx,
                    &draw_ctx,
                    entity,
                    type_name,
                    section_ids[index],
                ) else {
                    continue;
                };
                sections.push(component);
            }

            // Add Component action: a quiet expander listing the component
            // types registered with the scene tool registry, minus the ones
            // the entity already has, narrowed by a live filter field.
            if !draw_ctx.available_components.is_empty() {
                let add_open: bool = draw_ctx.add_component_open;
                let mut footer: Vec<Box<dyn Widget>> = Vec::new();
                footer.push(
                    button("+ Add Component")
                        .fill(katla_math::Color::TRANSPARENT)
                        .border(if add_open {
                            draw_ctx.theme.accent
                        } else {
                            draw_ctx.theme.border
                        })
                        .on_click(ctx.on_click(move |actions| {
                            actions.emit(InspectorAction::TogglePicker);
                        }))
                        .boxed(),
                );
                if add_open {
                    let entity_id = entity.id;
                    let filter: String = ctx.get_state(filter_id).unwrap_or_default();
                    let addable = addable_components(
                        &draw_ctx.available_components,
                        &entity.components,
                        &filter,
                    );
                    let inner_width = (draw_ctx.bounds.width() - 24.0).max(0.0);
                    footer.push(
                        textfield("Filter components...", filter_id)
                            .flex_width(inner_width)
                            .boxed(),
                    );
                    if addable.is_empty() {
                        footer.push(
                            text("No matching components")
                                .color(draw_ctx.theme.text_muted)
                                .boxed(),
                        );
                    }
                    for component_type in addable {
                        footer.push(
                            selectable(
                                hstack([text(component_display_name(component_type))
                                    .color(draw_ctx.theme.text_primary)
                                    .boxed()])
                                .padding_all(8.0)
                                .align(Alignment::Leading)
                                .boxed(),
                            )
                            .on_click(ctx.on_click(move |actions| {
                                actions.emit(InspectorAction::Add {
                                    entity: entity_id,
                                    component: component_type.to_string(),
                                });
                            }))
                            .boxed(),
                        );
                    }
                }
                sections.push(vstack(footer).spacing(4.0).boxed());
            }

            vstack(sections)
                .flex_grow(1.0)
                .spacing(12.0)
                .padding(Padding::all(12.0))
                .boxed()
        } else {
            // Intentional, quiet empty state — not an onboarding poster.
            let icon_color = draw_ctx.theme.text_muted;
            vstack([
                icon(katla_ui::ForkAwesome::CUBE)
                    .icon_size(FontSize::XLarge)
                    .color(icon_color)
                    .boxed(),
                text("No entity selected")
                    .color(draw_ctx.theme.text_secondary)
                    .font_size(FontSize::Medium)
                    .boxed(),
                text("Select an entity to inspect it.")
                    .color(draw_ctx.theme.text_muted)
                    .font_size(FontSize::Small)
                    .boxed(),
            ])
            .spacing(8.0)
            .align(Alignment::Center)
            .padding(Padding::all(24.0))
            .boxed()
        };

        let panel_content = scroll(content, scroll_id).flex_grow(1.0).boxed();

        panel_body(panel_content)
            .flex_width(draw_ctx.bounds.width())
            .flex_height(draw_ctx.bounds.height())
            .boxed()
    }
}

impl InspectorView {
    /// Build the section for one component type present on the entity, or
    /// `None` for structural relations that get no section.
    #[allow(clippy::too_many_lines)]
    fn build_component_section(
        &self,
        ctx: &mut BuildContext,
        draw_ctx: &InspectorDrawCtx,
        entity: &EntityInfo,
        type_name: &str,
        expanded_id: StateId,
    ) -> Option<Box<dyn Widget>> {
        let theme = &draw_ctx.theme;
        let on_remove = removable(&draw_ctx.available_components, type_name).then(|| {
            let entity = entity.id;
            let component = type_name.to_string();
            ctx.on_click(move |actions| {
                actions.emit(InspectorAction::Remove {
                    entity,
                    component: component.clone(),
                })
            })
        });

        let mut rows: Vec<Box<dyn Widget>> = Vec::new();
        match type_name {
            "Transform" => {
                rows.push(
                    property_row(
                        "Position",
                        format!(
                            "{:.2}, {:.2}, {:.2}",
                            entity.position.x(),
                            entity.position.y(),
                            entity.position.z()
                        ),
                    )
                    .boxed(),
                );
                rows.push(
                    property_row(
                        "Rotation",
                        format!(
                            "{:.2}, {:.2}, {:.2}",
                            entity.rotation.x(),
                            entity.rotation.y(),
                            entity.rotation.z()
                        ),
                    )
                    .boxed(),
                );
                rows.push(
                    property_row(
                        "Scale",
                        format!(
                            "{:.2}, {:.2}, {:.2}",
                            entity.scale.x(),
                            entity.scale.y(),
                            entity.scale.z()
                        ),
                    )
                    .boxed(),
                );
            }
            "NameComponent" => {
                rows.push(property_row("Name", entity.name.clone()).boxed());
            }
            "Drawable" => {
                rows.push(property_row("Type", entity.entity_type.clone()).boxed());
            }
            "PointLight" => {
                if let Some(light) = &entity.point_light {
                    rows.push(property_row("Color", format_rgb(light.color)).boxed());
                    rows.push(property_row("Intensity", format!("{:.2}", light.intensity)).boxed());
                    rows.push(property_row("Range", format!("{:.2}", light.range)).boxed());
                }
            }
            "DirectionalLight" => {
                if let Some(light) = &entity.directional_light {
                    rows.push(property_row("Direction", format_vec3(light.direction)).boxed());
                    rows.push(property_row("Color", format_rgb(light.color)).boxed());
                    rows.push(property_row("Intensity", format!("{:.2}", light.intensity)).boxed());
                }
            }
            "PerspectiveComponent" => {
                if let Some(cam) = &entity.perspective {
                    rows.push(property_row("FOV", format!("{:.1}°", cam.fov)).boxed());
                    rows.push(property_row("Near", format!("{:.3}", cam.near)).boxed());
                    rows.push(property_row("Aspect", format!("{:.2}", cam.aspect_ratio)).boxed());
                }
            }
            "ScriptComponent" => {
                let path = entity.script_path.as_deref().unwrap_or("(no script)");
                rows.push(property_row("Path", path).boxed());
            }
            "ParticleEmitterComponent" => {
                if let Some(pe) = &entity.particle_emitter {
                    rows.push(property_row("Emit Rate", format!("{:.1}/s", pe.emit_rate)).boxed());
                    rows.push(
                        property_row("Velocity", format!("{:.2}", pe.velocity_magnitude)).boxed(),
                    );
                    rows.push(
                        property_row("Lifetime", format!("{:.2}s", pe.base_lifetime)).boxed(),
                    );
                    rows.push(property_row("Gravity", format!("{:.2}", pe.gravity)).boxed());
                    rows.push(property_row("Scale", format!("{:.3}", pe.base_scale)).boxed());
                }
            }
            "AudioEmitter" => {
                if let Some(ae) = &entity.audio_emitter {
                    rows.push(property_row("Path", ae.source_path.clone()).boxed());
                    rows.push(property_row("Volume", format!("{:.2}", ae.volume)).boxed());
                    rows.push(property_row("Looping", on_off(ae.looping)).boxed());
                    rows.push(property_row("Spatial", on_off(ae.spatial)).boxed());
                    rows.push(
                        property_row(
                            "Distances",
                            format!("{:.1} - {:.1}", ae.min_distance, ae.max_distance),
                        )
                        .boxed(),
                    );
                }
            }
            "AudioSource" => {
                if let Some(src) = &entity.audio_source {
                    rows.push(property_row("Path", &src.path).boxed());
                    if let Some(sr) = src.sample_rate {
                        rows.push(property_row("Sample Rate", format!("{} Hz", sr)).boxed());
                    }
                    if let Some(ch) = src.channels {
                        rows.push(property_row("Channels", ch.to_string()).boxed());
                    }
                    if let Some(dur) = src.duration_secs {
                        rows.push(property_row("Duration", format!("{:.2}s", dur)).boxed());
                    }
                    let path_clone = src.path.clone();
                    rows.push(
                        button("▶ Play Preview")
                            .on_click(ctx.on_click(move |actions| {
                                actions.emit(
                                    crate::ui::editor_ui::types::EditorAction::AudioPreviewToggle {
                                        path: PathBuf::from(&path_clone),
                                    },
                                );
                            }))
                            .boxed(),
                    );
                }
            }
            "AudioListener" => {
                rows.push(text("Active listener").boxed());
                if draw_ctx.audio_listener_count > 1 {
                    rows.push(
                        text(format!(
                            "⚠ {} listeners in scene",
                            draw_ctx.audio_listener_count
                        ))
                        .color(theme.warning)
                        .boxed(),
                    );
                }
            }
            "VelocityComponent" => {
                rows.push(muted_note("Adds constant velocity each frame", theme));
            }
            "ReverbZone" => {
                rows.push(muted_note("Applies reverb inside its volume", theme));
            }
            "ColliderShape" => {
                if let Some(collider) = &entity.collider_shape {
                    match collider.shape_type {
                        ColliderShapeType::Sphere => {
                            rows.push(property_row("Shape", "Sphere".to_string()).boxed());
                            rows.push(
                                property_row("Radius", format!("{:.3}", collider.sphere_radius))
                                    .boxed(),
                            );
                        }
                        ColliderShapeType::Box => {
                            rows.push(property_row("Shape", "Box".to_string()).boxed());
                            rows.push(
                                property_row(
                                    "Half Extents",
                                    format_vec3(collider.box_half_extents),
                                )
                                .boxed(),
                            );
                        }
                        ColliderShapeType::Capsule => {
                            rows.push(property_row("Shape", "Capsule".to_string()).boxed());
                            rows.push(
                                property_row(
                                    "Half Height",
                                    format!("{:.3}", collider.capsule_half_height),
                                )
                                .boxed(),
                            );
                            rows.push(
                                property_row("Radius", format!("{:.3}", collider.capsule_radius))
                                    .boxed(),
                            );
                        }
                    }
                }
            }
            "CollisionFilter" => {
                rows.push(muted_note("Filters collision pairs by layer", theme));
            }
            "RigidBody" => {
                if let Some(rb) = &entity.rigid_body {
                    rows.push(property_row("Type", rb.body_type.label()).boxed());
                    rows.push(
                        property_row("Gravity Scale", format!("{:.2}", rb.gravity_scale)).boxed(),
                    );
                    rows.push(
                        property_row("Linear Velocity", format_vec3(rb.linear_velocity)).boxed(),
                    );
                }
            }
            "PhysicsMaterial" => {
                if let Some(pm) = &entity.physics_material {
                    rows.push(property_row("Friction", format!("{:.2}", pm.friction)).boxed());
                    rows.push(
                        property_row("Restitution", format!("{:.2}", pm.restitution)).boxed(),
                    );
                    rows.push(property_row("Density", format!("{:.2}", pm.density)).boxed());
                }
            }
            "Parent" | "Children" => return None,
            _ => return None,
        }

        let content = vstack(rows).spacing(4.0).boxed();
        let mut section_widget = section(component_display_name(type_name), content, expanded_id);
        if let Some(cb) = on_remove {
            section_widget = section_widget.on_remove(cb);
        }
        Some(section_widget.boxed())
    }
}

fn format_rgb(c: [f32; 3]) -> String {
    format!("{:.2}, {:.2}, {:.2}", c[0], c[1], c[2])
}

fn format_vec3(v: [f32; 3]) -> String {
    format!("{:.2}, {:.2}, {:.2}", v[0], v[1], v[2])
}

fn on_off(value: bool) -> &'static str {
    if value { "On" } else { "Off" }
}

fn muted_note(message: &str, theme: &ColorScheme) -> Box<dyn Widget> {
    text(message.to_string()).color(theme.text_muted).boxed()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_names_are_human_readable() {
        assert_eq!(component_display_name("NameComponent"), "Name");
        assert_eq!(
            component_display_name("ParticleEmitterComponent"),
            "Particle Emitter"
        );
        assert_eq!(
            component_display_name("PerspectiveComponent"),
            "Perspective Camera"
        );
        assert_eq!(component_display_name("ColliderShape"), "Collider");
        assert_eq!(component_display_name("Transform"), "Transform");
    }

    #[test]
    fn test_addable_excludes_owned_components() {
        let available = vec!["PointLight", "ScriptComponent", "RigidBody"];
        let owned = vec!["PointLight".to_string()];
        let addable = addable_components(&available, &owned, "");
        assert_eq!(addable, vec!["RigidBody", "ScriptComponent"]);
    }

    #[test]
    fn test_addable_keeps_all_when_nothing_owned() {
        let available = vec!["PointLight", "ScriptComponent"];
        let addable = addable_components(&available, &[], "");
        assert_eq!(addable, vec!["PointLight", "ScriptComponent"]);
    }

    #[test]
    fn test_addable_filter_matches_display_name() {
        let available = vec!["PointLight", "DirectionalLight", "ScriptComponent"];
        let addable = addable_components(&available, &[], "light");
        assert_eq!(addable, vec!["DirectionalLight", "PointLight"]);
    }

    #[test]
    fn test_addable_filter_matches_type_name() {
        let available = vec!["PointLight", "ScriptComponent"];
        let addable = addable_components(&available, &[], "script");
        assert_eq!(addable, vec!["ScriptComponent"]);
    }

    #[test]
    fn test_addable_filter_is_trimmed() {
        let available = vec!["PointLight", "ScriptComponent"];
        let addable = addable_components(&available, &[], "  point  ");
        assert_eq!(addable, vec!["PointLight"]);
    }

    #[test]
    fn test_removable_follows_registry() {
        let available = vec!["PointLight", "NameComponent"];
        assert!(removable(&available, "PointLight"));
        assert!(!removable(&available, "Transform"));
        assert!(!removable(&available, "Drawable"));
    }
}
