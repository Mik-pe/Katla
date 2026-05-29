use katla_ecs::EntityId;
use katla_math::{Color, Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, Padding, StateId, ViewDescriptor};
use katla_ui::{
    FontSize, ForkAwesome, ScrollArea, ScrollAreaState,
    widgets::{Button, ColorPickerButton, LabeledSlider, Vec3Slider},
};

use crate::ui::editor_ui::ColorScheme;
use crate::ui::editor_ui::types::{
    ColliderShapeType, EditorAction, EntityInfo, InspectorEditState, RigidBodyType,
};

/// Environment data injected before each frame for the inspector panel.
#[derive(Clone)]
pub(crate) struct InspectorDrawCtx {
    pub bounds: Rect2D,
    pub selected_entity: Option<EntityId>,
    pub entities: Vec<EntityInfo>,
    pub edit: InspectorEditState,
    pub scroll_state: ScrollAreaState,
    pub add_component_scroll_state: ScrollAreaState,
    pub theme: ColorScheme,
    pub pending_actions: Vec<EditorAction>,
    pub available_components: Vec<&'static str>,
    pub add_component_open: bool,
    pub add_component_filter: String,
    pub focus_script_input: bool,
}

/// Actions emitted by the inspector panel to sync state back to the application.
#[derive(Clone, Debug)]
pub(crate) struct InspectorSync {
    pub edit: InspectorEditState,
    pub scroll_state: ScrollAreaState,
    pub add_component_scroll_state: ScrollAreaState,
    pub pending_actions: Vec<EditorAction>,
    pub add_component_open: bool,
    pub add_component_filter: String,
    pub focus_script_input: bool,
}

pub(crate) struct InspectorView;

impl Build for InspectorView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        use katla_ui::declarative::{
            button, hstack, panel, scroll, separator_horizontal, text, textfield, vstack,
        };

        let draw_ctx = ctx.env::<InspectorDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return ViewDescriptor::Empty;
        };

        let scroll_id: StateId = ctx.state(0.0f32);
        let add_scroll_id: StateId = ctx.state(0.0f32);

        let header_text = if let Some(entity_id) = draw_ctx.selected_entity {
            format!("Inspector: Entity {}", entity_id.id())
        } else {
            "Inspector".to_string()
        };

        // Build component editor UI
        let content = if let Some(entity) = draw_ctx
            .entities
            .iter()
            .find(|e| draw_ctx.selected_entity == Some(e.id))
        {
            let mut children = Vec::new();

            // Transform
            children.push(text("Transform").font_size(FontSize::Small));
            children.push(separator_horizontal());
            children.push(text(format!("Position: {:?}", entity.position)));
            children.push(text(format!("Rotation: {:?}", entity.rotation)));
            children.push(text(format!("Scale: {:?}", entity.scale)));
            children.push(separator_horizontal());

            // Type
            children.push(text("Type").font_size(FontSize::Small));
            children.push(text(&entity.entity_type));

            vstack(children)
        } else {
            text("No entity selected").color(draw_ctx.theme.text_muted)
        };

        let panel_content = scroll(content, scroll_id);

        panel(header_text, panel_content).header_height(24.0)
    }
}
