use katla_agent::MessageRole;
use katla_ui::FontSize;
use katla_ui::declarative::{
    Alignment, Build, BuildContext, DraggablePanelState, DraggablePanelVisibility, StateId,
    ViewDescriptor, button, draggable_panel, hstack, image_button, scroll, text, textfield, vstack,
};

#[derive(Clone)]
pub(crate) struct CoCreatorDrawCtx {
    pub messages: Vec<(MessageRole, String)>,
    pub processing: bool,
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
pub(crate) struct CoCreatorUndoAction;

#[derive(Clone, Debug)]
pub(crate) struct CoCreatorPanelSync {
    pub visibility: DraggablePanelVisibility,
}

pub(crate) struct CoCreatorView;

impl Build for CoCreatorView {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        let draw_ctx = ctx.env::<CoCreatorDrawCtx>().cloned();
        let Some(draw_ctx) = draw_ctx else {
            return ViewDescriptor::Empty;
        };

        let panel_id: StateId = ctx.state(DraggablePanelState::default());
        let mut panel_state: DraggablePanelState = ctx.get_state(panel_id);

        if draw_ctx.is_open && !panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::JustOpened;
            ctx.set_state(panel_id, panel_state);
        } else if !draw_ctx.is_open && panel_state.visibility.is_visible() {
            panel_state.visibility = DraggablePanelVisibility::Hidden;
            ctx.set_state(panel_id, panel_state);
        }

        let current_panel: DraggablePanelState = ctx.get_state(panel_id);
        ctx.emit(CoCreatorPanelSync {
            visibility: current_panel.visibility,
        });

        if !current_panel.visibility.is_visible() {
            return ViewDescriptor::Empty;
        }

        let mut children: Vec<ViewDescriptor> = Vec::new();

        // Undo button
        if draw_ctx.agent_undo_count > 0 {
            children.push(
                image_button(katla_ui::ForkAwesome::UNDO).on_click(ctx.on_click(|actions| {
                    actions.emit(CoCreatorUndoAction);
                })),
            );
        }

        // Message area
        let mut msg_children: Vec<ViewDescriptor> = Vec::new();

        if draw_ctx.messages.is_empty() {
            msg_children.push(
                text(&draw_ctx.status_message)
                    .color(draw_ctx.text_muted)
                    .font_size(FontSize::Small),
            );
        } else {
            for (role, msg_text) in &draw_ctx.messages {
                let (color, prefix) = match role {
                    MessageRole::User => (draw_ctx.user_msg_color, "You: "),
                    MessageRole::Assistant => (draw_ctx.assistant_msg_color, "AI: "),
                    MessageRole::System | MessageRole::Tool => (draw_ctx.system_msg_color, "> "),
                };
                msg_children.push(
                    text(format!("{prefix}{msg_text}"))
                        .color(color)
                        .font_size(FontSize::Small),
                );
            }
        }

        if draw_ctx.processing {
            msg_children.push(
                text("Processing...")
                    .color(draw_ctx.text_muted)
                    .font_size(FontSize::Small),
            );
        }

        let scroll_id: StateId = ctx.state(0.0f32);
        let msg_area = scroll(
            vstack(msg_children)
                .spacing(4.0)
                .padding_all(4.0)
                .align(Alignment::Leading),
            scroll_id,
        );

        // Input area
        let input_id: StateId = ctx.state(String::new());
        let current_input: String = ctx.get_state(input_id);
        let input_clone = current_input.clone();
        let input_field =
            textfield("Ask the AI...", input_id).on_submit(ctx.on_click(move |actions| {
                actions.emit(CoCreatorSubmitAction {
                    text: input_clone.clone(),
                });
            }));

        let send_btn = button("Send").on_click(ctx.on_click(move |actions| {
            actions.emit(CoCreatorSubmitAction {
                text: current_input.clone(),
            });
        }));

        let input_row = hstack([input_field, send_btn]).spacing(4.0);

        children.push(msg_area);
        children.push(input_row);

        draggable_panel(
            "AI Co-Creator",
            400.0,
            500.0,
            vstack(children)
                .spacing(4.0)
                .padding_all(4.0)
                .align(Alignment::Leading),
            panel_id,
        )
        .close_on_outside(false)
    }
}
