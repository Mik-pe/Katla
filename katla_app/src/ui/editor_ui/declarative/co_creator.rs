use std::boxed::Box;

use katla_agent::MessageRole;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, DraggablePanelState, DraggablePanelVisibility, StateId, Widget,
    WidgetBox, button, draggable_panel, empty, hstack, image_button, scroll, text, textfield,
    vstack,
};

#[derive(Clone)]
pub(crate) struct CoCreatorDrawCtx {
    pub messages: Vec<(MessageRole, String)>,
    pub processing: bool,
    pub host_name: Option<String>,
    pub input_epoch: u64,
    pub status_message: String,
    pub user_msg_color: katla_math::Color,
    pub assistant_msg_color: katla_math::Color,
    pub system_msg_color: katla_math::Color,
    pub text_muted: katla_math::Color,
    pub agent_undo_count: usize,
    pub is_open: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CoCreatorSubmitAction {
    pub text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CoCreatorConnectAction {
    pub socket: String,
    pub thread_id: String,
}

#[derive(Clone, Debug)]
pub(crate) struct CoCreatorUndoAction;

#[derive(Clone, Debug)]
pub(crate) struct CoCreatorConnectionSettingsAction;

#[derive(Clone, Debug)]
pub(crate) struct CoCreatorPanelSync {
    pub visibility: DraggablePanelVisibility,
}

pub(crate) struct CoCreatorView;

impl Build for CoCreatorView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let draw_ctx = ctx.env::<CoCreatorDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return empty().boxed();
        };

        let panel_id: StateId = ctx.state(DraggablePanelState::default());
        let mut panel_state: DraggablePanelState = ctx.get_state(panel_id).unwrap_or_default();

        if draw_ctx.is_open && !panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::JustOpened;
            ctx.set_state(panel_id, panel_state);
        } else if !draw_ctx.is_open && panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::Hidden;
            ctx.set_state(panel_id, panel_state);
        }

        let current_panel: DraggablePanelState = ctx.get_state(panel_id).unwrap_or_default();
        ctx.emit(CoCreatorPanelSync {
            visibility: current_panel.visibility,
        });

        if !current_panel.visibility.is_visible() {
            return empty().boxed();
        }

        let mut children: Vec<Box<dyn Widget>> = Vec::new();
        if let Some(name) = &draw_ctx.host_name {
            children.push(text(name).wrap(392.0).font_size(FontSize::Small).boxed());
        }
        children.push(
            button("Connection settings")
                .on_click(ctx.on_click(|actions| {
                    actions.emit(CoCreatorConnectionSettingsAction);
                }))
                .boxed(),
        );
        children.push(
            text(&draw_ctx.status_message)
                .wrap(392.0)
                .color(draw_ctx.text_muted)
                .font_size(FontSize::Small)
                .boxed(),
        );

        // Undo button
        if draw_ctx.agent_undo_count > 0 {
            children.push(
                image_button(katla_ui::ForkAwesome::UNDO)
                    .on_click(ctx.on_click(|actions| {
                        actions.emit(CoCreatorUndoAction);
                    }))
                    .boxed(),
            );
        }

        // Message area
        let mut msg_children: Vec<Box<dyn Widget>> = Vec::new();

        for (role, msg_text) in &draw_ctx.messages {
            let (color, prefix) = match role {
                MessageRole::User => (draw_ctx.user_msg_color, "You: "),
                MessageRole::Assistant => (draw_ctx.assistant_msg_color, ""),
                MessageRole::System | MessageRole::Tool => (draw_ctx.system_msg_color, ""),
            };
            msg_children.push(
                text(format!("{prefix}{msg_text}"))
                    .wrap(380.0)
                    .color(color)
                    .font_size(FontSize::Small)
                    .boxed(),
            );
        }

        if draw_ctx.processing {
            msg_children.push(
                text("Processing...")
                    .color(draw_ctx.text_muted)
                    .font_size(FontSize::Small)
                    .boxed(),
            );
        }

        let scroll_id: StateId = ctx.state(0.0f32);
        let pin_id = ctx.state(true);
        let msg_area = scroll(
            vstack(msg_children)
                .spacing(4.0)
                .padding_all(4.0)
                .align(Alignment::Leading)
                .boxed(),
            scroll_id,
        )
        .auto_scroll(pin_id)
        .flex_grow(1.0)
        .flex_width(392.0);

        // Input area
        let input_id: StateId = ctx.state(String::new());
        let epoch_id = ctx.state(draw_ctx.input_epoch);
        if ctx.get_state::<u64>(epoch_id) != Some(draw_ctx.input_epoch) {
            ctx.set_state(input_id, String::new());
            ctx.set_state(epoch_id, draw_ctx.input_epoch);
        }
        let current_input: String = ctx.get_state(input_id).unwrap_or_default();
        let input_clone = current_input.clone();
        let input_field = textfield("Ask about this view...", input_id)
            .flex_grow(1.0)
            .on_submit(ctx.on_click(move |actions| {
                actions.emit(CoCreatorSubmitAction {
                    text: input_clone.clone(),
                });
            }))
            .boxed();

        let send_btn = button("Send")
            .on_click(ctx.on_click(move |actions| {
                actions.emit(CoCreatorSubmitAction {
                    text: current_input.clone(),
                });
            }))
            .boxed();

        let input_row = hstack([input_field, send_btn])
            .spacing(4.0)
            .flex_width(392.0)
            .boxed();

        children.push(msg_area.boxed());
        children.push(input_row);

        draggable_panel(
            "Scene assistant",
            400.0,
            500.0,
            vstack(children)
                .flex_width(400.0)
                .flex_height(500.0 - katla_ui::declarative::widgets::draggable_panel::DraggablePanel::TITLE_BAR_HEIGHT)
                .spacing(4.0)
                .padding_all(4.0)
                .align(Alignment::Leading)
                .boxed(),
            panel_id,
        )
        .close_on_outside(false)
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::{Color, Vec2};
    use katla_ui::declarative::widgets::textfield::TextField;
    use katla_ui::{UiContext, declarative::ViewTree};

    fn context(epoch: u64) -> CoCreatorDrawCtx {
        CoCreatorDrawCtx {
            messages: vec![],
            processing: false,
            host_name: Some("My room".into()),
            input_epoch: epoch,
            status_message: "Connected".into(),
            user_msg_color: Color::WHITE,
            assistant_msg_color: Color::WHITE,
            system_msg_color: Color::WHITE,
            text_muted: Color::WHITE,
            agent_undo_count: 0,
            is_open: true,
        }
    }

    #[test]
    fn test_long_reply_keeps_question_outside_scrolling_content() {
        use katla_ui::declarative::widgets::{draggable_panel::DraggablePanel, scroll::ScrollView};
        let mut ui = UiContext::new();
        let mut tree = ViewTree::default();
        let size = Vec2::new(900.0, 700.0);
        let mut env = context(0);
        env.messages = vec![(
            MessageRole::Assistant,
            "A long reply about the room.\n".repeat(100),
        )];
        ui.begin(size, 1.0);
        tree.env_mut().set(env);
        tree.frame(&mut ui, &CoCreatorView, size);
        let bounds_of = |predicate: fn(&dyn std::any::Any) -> bool| {
            let (id, _) = tree
                .iter_nodes()
                .find(|(_, node)| predicate(node.widget.as_any()))
                .unwrap();
            tree.resolved_bounds()[&id]
        };
        let panel = bounds_of(|w| w.is::<DraggablePanel>());
        let input = bounds_of(|w| w.is::<TextField>());
        let messages = bounds_of(|w| w.is::<ScrollView>());
        assert!(input.max.y() <= panel.max.y());
        assert!(input.min.y() >= messages.max.y());
        assert!(messages.height() > 200.0 && messages.height() < panel.height());
        assert!(input.width() > 300.0);
    }

    #[test]
    fn test_question_panel_has_only_question_field_and_resets_submitted_input() {
        let mut ui = UiContext::new();
        let mut tree = ViewTree::default();
        let size = Vec2::new(900.0, 700.0);
        ui.begin(size, 1.0);
        tree.env_mut().set(context(0));
        tree.frame(&mut ui, &CoCreatorView, size);
        let fields: Vec<_> = tree
            .iter_nodes()
            .filter_map(|(_, node)| {
                node.widget
                    .as_any()
                    .downcast_ref::<TextField>()
                    .map(|f| f.value_id)
            })
            .collect();
        assert_eq!(
            fields.len(),
            1,
            "Technical connection fields belong in settings"
        );
        let input = fields[0];
        tree.state_arena_mut()
            .set(input, "Furnish this room".to_string());
        ui.end();
        ui.begin(size, 1.0);
        tree.env_mut().set(context(0));
        tree.frame(&mut ui, &CoCreatorView, size);
        assert_eq!(
            tree.state_arena().get::<String>(input).unwrap(),
            "Furnish this room"
        );
        ui.end();
        ui.begin(size, 1.0);
        tree.env_mut().set(context(1));
        tree.frame(&mut ui, &CoCreatorView, size);
        assert_eq!(tree.state_arena().get::<String>(input).unwrap(), "");
    }
}
