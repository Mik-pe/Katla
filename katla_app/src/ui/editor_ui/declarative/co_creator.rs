use std::cell::RefCell;

use katla_agent::MessageRole;
use katla_math::{Rect2D, Vec2};
use katla_ui::declarative::{Build, BuildContext, ViewDescriptor};
use katla_ui::markdown::{MarkdownColors, draw_markdown_segments, parse_markdown_line, wrap_lines};
use katla_ui::widgets::{Button, ImageButton, TextInput};
use katla_ui::widgets::{DraggablePanel, DraggablePanelConfig};
use katla_ui::{FontSize, ScrollArea, UiContext};

use crate::ui::editor_ui::co_creator::{CoCreatorResponse, CoCreatorState, CoCreatorStyle};

thread_local! {
    static CO_CREATOR_CTX: RefCell<Option<CoCreatorDrawCtx>> = const { RefCell::new(None) };
}

struct CoCreatorDrawCtx {
    state: CoCreatorState,
    style: CoCreatorStyle,
    screen_size: Vec2,
    agent_undo_count: usize,
    response: CoCreatorResponse,
}

pub(crate) fn set_co_creator_ctx(
    state: CoCreatorState,
    style: CoCreatorStyle,
    screen_size: Vec2,
    agent_undo_count: usize,
) {
    CO_CREATOR_CTX.with(|c| {
        *c.borrow_mut() = Some(CoCreatorDrawCtx {
            state,
            style,
            screen_size,
            agent_undo_count,
            response: CoCreatorResponse {
                _submitted: false,
                submitted_text: None,
                undo_clicked: false,
            },
        })
    });
}

pub(crate) fn take_co_creator_ctx() -> Option<(CoCreatorState, CoCreatorResponse)> {
    CO_CREATOR_CTX.with(|c| c.borrow_mut().take().map(|ctx| (ctx.state, ctx.response)))
}

pub(crate) struct CoCreatorView;

impl Build for CoCreatorView {
    fn build(&self, _ctx: &mut BuildContext) -> ViewDescriptor {
        ViewDescriptor::Custom(draw_co_creator)
    }
}

fn draw_co_creator(ui: &mut UiContext, _bounds: Rect2D) {
    let ctx = CO_CREATOR_CTX.with(|c| c.borrow_mut().take());
    let Some(mut ctx) = ctx else {
        return;
    };

    if !ctx.state.is_open() {
        ctx.state.panel.mark_shown();
        CO_CREATOR_CTX.with(|c| *c.borrow_mut() = Some(ctx));
        return;
    }

    let status_message = ctx.state.status_message.clone();
    let messages: Vec<(MessageRole, String)> = ctx
        .state
        .messages
        .iter()
        .map(|m| (m.role.clone(), m.text.clone()))
        .collect();
    let processing = ctx.state.processing;
    let mut input_text = ctx.state.input_text.clone();
    let mut scroll_state = ctx.state.scroll_state;
    let mut send_clicked = false;
    let mut enter_pressed = false;
    let mut undo_clicked = false;

    let md_colors = MarkdownColors::from_style(ui.style());

    DraggablePanel::show(
        ui,
        &mut ctx.state.panel,
        DraggablePanelConfig::new("co_creator", "AI Co-Creator")
            .size(400.0, 500.0)
            .screen_size(ctx.screen_size)
            .close_on_outside_click(false),
        |ui, frame| {
            let panel_padding = ui.style().panel_padding;
            let content_x = frame.panel_bounds.min.x() + panel_padding;
            let content_width = frame.panel_bounds.width() - 2.0 * panel_padding;
            let header_height = 32.0;
            let bottom_padding = panel_padding;

            let line_count = input_text.lines().count().max(1);
            let input_height = (line_count.min(5) as f32) * 28.0;

            let msg_area_top = frame.panel_bounds.min.y() + header_height + panel_padding;
            let msg_area_bottom = frame.panel_bounds.max.y() - input_height - bottom_padding;

            let font_size = ui.scaled_font_size(FontSize::Small);

            if ctx.agent_undo_count > 0 {
                let undo_bounds = Rect2D::from_origin_size(
                    Vec2::new(
                        frame.panel_bounds.max.x() - 40.0,
                        frame.panel_bounds.min.y() + 2.0,
                    ),
                    Vec2::new(28.0, 28.0),
                );
                let response = ui.add(
                    ImageButton::new(katla_ui::ForkAwesome::UNDO)
                        .bounds(undo_bounds)
                        .id("co_creator_undo"),
                );
                if response.clicked {
                    undo_clicked = true;
                }
            }

            let msg_area_bounds = Rect2D::from_origin_size(
                Vec2::new(content_x, msg_area_top),
                Vec2::new(content_width, msg_area_bottom - msg_area_top),
            );

            let scroll_config = ScrollArea::new("co_creator_msgs")
                .max_height(msg_area_bounds.height())
                .stick_to_bottom(true);

            let style = &ctx.style;
            scroll_state = ui.scroll_area(scroll_config, scroll_state, msg_area_bounds, |ui| {
                let scroll_off = ui.scroll_offset();
                let mut y = msg_area_top - scroll_off;

                if messages.is_empty() {
                    ui.draw_text(
                        &status_message,
                        Vec2::new(content_x, y),
                        style.text_muted,
                        font_size,
                    );
                    y += font_size + 2.0;
                } else {
                    for (role, text) in &messages {
                        let color = match role {
                            MessageRole::User => style.user_msg_color,
                            MessageRole::Assistant => style.assistant_msg_color,
                            MessageRole::System => style.system_msg_color,
                            MessageRole::Tool => style.system_msg_color,
                        };

                        let prefix = match role {
                            MessageRole::User => "You: ",
                            MessageRole::Assistant => "AI: ",
                            MessageRole::System => "> ",
                            MessageRole::Tool => "> ",
                        };

                        let full_text = format!("{prefix}{text}");

                        for line in wrap_lines(&full_text, content_width, font_size, ui) {
                            let segments = parse_markdown_line(&line);
                            draw_markdown_segments(
                                ui,
                                &segments,
                                Vec2::new(content_x, y),
                                color,
                                font_size,
                                &md_colors,
                            );
                            y += font_size + 2.0;
                        }
                        y += 4.0;
                    }
                }

                if processing {
                    ui.draw_text(
                        "Processing...",
                        Vec2::new(content_x, y),
                        style.text_muted,
                        font_size,
                    );
                    y += font_size + 2.0;
                }

                y - msg_area_top + scroll_off
            });

            let input_y = msg_area_bottom + ui.style().item_inner_spacing;
            let input_bounds = Rect2D::from_origin_size(
                Vec2::new(content_x, input_y),
                Vec2::new(content_width - 70.0, input_height),
            );

            let send_x = content_x + content_width - 60.0;
            let send_bounds =
                Rect2D::from_origin_size(Vec2::new(send_x, input_y), Vec2::new(60.0, 28.0));

            let input_response = ui.add(
                TextInput::new("co_creator_input", &mut input_text)
                    .bounds(input_bounds)
                    .placeholder("Ask the AI...")
                    .multiline(true)
                    .id("co_creator_input"),
            );
            enter_pressed = input_response.enter_pressed;

            let response = ui.add(
                Button::new("Send")
                    .bounds(send_bounds)
                    .id("co_creator_send"),
            );
            send_clicked = response.clicked;
        },
    );

    ctx.state.input_text = input_text;
    ctx.state.scroll_state = scroll_state;

    let mut submitted_text: Option<String> = None;
    if send_clicked || enter_pressed {
        let text = ctx.state.input_text.clone();
        ctx.state.submit_message(&text);
        if !text.trim().is_empty() {
            submitted_text = Some(text);
        }
    }

    ctx.state.panel.mark_shown();

    ctx.response = CoCreatorResponse {
        _submitted: submitted_text.is_some(),
        submitted_text,
        undo_clicked,
    };

    CO_CREATOR_CTX.with(|c| *c.borrow_mut() = Some(ctx));
}
