//! Editor file selection and protection for unsaved scene changes.

use katla_ui::declarative::{
    Alignment, Build, BuildContext, StateId, Widget, WidgetBox, button, empty, hstack, modal,
    scroll, text, textfield, vstack,
};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SceneDialog {
    Open(String),
    SaveAs(String),
    Unsaved,
    Overwrite(PathBuf),
    Error(String),
}

#[derive(Clone)]
pub(crate) struct SceneDialogData {
    pub(crate) dialog: Option<SceneDialog>,
    pub(crate) screen_size: katla_math::Vec2,
}

#[derive(Clone)]
pub(crate) enum SceneDialogAction {
    Submit(StateId),
    Cancel,
    Save,
    Discard,
    Overwrite,
}

pub(crate) struct SceneDialogView;
impl Build for SceneDialogView {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        let screen_size = ctx
            .env::<SceneDialogData>()
            .map(|data| data.screen_size)
            .unwrap_or(katla_math::Vec2::new(800.0, 600.0));
        let width = 560.0f32.min((screen_size.x() - 16.0).max(1.0));
        let height = 240.0f32.min((screen_size.y() - 16.0).max(1.0));
        let content_width = (width - 32.0).max(1.0);
        let dialog = ctx
            .env::<SceneDialogData>()
            .and_then(|data| data.dialog.clone());
        let previous = ctx.state(None::<SceneDialog>);
        let path = ctx.state(String::new());
        let open = ctx.state(false);
        let scroll_id = ctx.state(0.0f32);
        if ctx.get_state::<Option<SceneDialog>>(previous).flatten() != dialog {
            let value = match &dialog {
                Some(SceneDialog::Open(value) | SceneDialog::SaveAs(value)) => value.clone(),
                _ => String::new(),
            };
            ctx.set_state(path, value);
            ctx.set_state(previous, dialog.clone());
        }
        ctx.set_state(open, dialog.is_some());
        let Some(dialog) = dialog else {
            return empty().boxed();
        };
        let cancel = button("Cancel")
            .on_click(ctx.on_click(|actions| actions.emit(SceneDialogAction::Cancel)))
            .boxed();
        let (title, body): (&str, Box<dyn Widget>) = match dialog {
            SceneDialog::Open(_) | SceneDialog::SaveAs(_) => {
                let saving = matches!(dialog, SceneDialog::SaveAs(_));
                let submit =
                    ctx.on_click(move |actions| actions.emit(SceneDialogAction::Submit(path)));
                let submit_button =
                    ctx.on_click(move |actions| actions.emit(SceneDialogAction::Submit(path)));
                (
                    if saving {
                        "Save Scene As"
                    } else {
                        "Open Scene"
                    },
                    vstack([
                        text("Enter the path to a .katla scene file.")
                            .wrap(content_width)
                            .boxed(),
                        textfield("Scene file path", path)
                            .flex_width(content_width)
                            .on_submit(submit)
                            .boxed(),
                        hstack([
                            cancel,
                            button(if saving { "Save" } else { "Open" })
                                .on_click(submit_button)
                                .boxed(),
                        ])
                        .spacing(8.0)
                        .align(Alignment::Trailing)
                        .boxed(),
                    ])
                    .spacing(12.0)
                    .padding_all(16.0)
                    .boxed(),
                )
            }
            SceneDialog::Unsaved => (
                "Unsaved Changes",
                vstack([
                    text("Save your changes before continuing?")
                        .wrap(content_width)
                        .boxed(),
                    hstack([
                        cancel,
                        button("Discard Changes")
                            .on_click(
                                ctx.on_click(|actions| actions.emit(SceneDialogAction::Discard)),
                            )
                            .boxed(),
                        button("Save")
                            .on_click(ctx.on_click(|actions| actions.emit(SceneDialogAction::Save)))
                            .boxed(),
                    ])
                    .spacing(8.0)
                    .align(Alignment::Trailing)
                    .boxed(),
                ])
                .spacing(16.0)
                .padding_all(16.0)
                .boxed(),
            ),
            SceneDialog::Overwrite(path) => (
                "Replace Scene File?",
                vstack([
                    text(format!("{} already exists.", path.display()))
                        .wrap(content_width)
                        .boxed(),
                    hstack([
                        cancel,
                        button("Replace")
                            .on_click(
                                ctx.on_click(|actions| actions.emit(SceneDialogAction::Overwrite)),
                            )
                            .boxed(),
                    ])
                    .spacing(8.0)
                    .align(Alignment::Trailing)
                    .boxed(),
                ])
                .spacing(12.0)
                .padding_all(16.0)
                .boxed(),
            ),
            SceneDialog::Error(message) => (
                "Scene Operation Failed",
                vstack([
                    text(message).wrap(content_width).boxed(),
                    button("OK")
                        .on_click(ctx.on_click(|actions| actions.emit(SceneDialogAction::Cancel)))
                        .boxed(),
                ])
                .spacing(12.0)
                .padding_all(16.0)
                .boxed(),
            ),
        };
        modal(
            width,
            height,
            open,
            scroll(
                vstack([body]).flex_width(width).flex_shrink(0.0).boxed(),
                scroll_id,
            )
            .flex_width(width)
            .flex_height((height - katla_ui::tokens::MODAL_TITLE_HEIGHT).max(1.0))
            .boxed(),
        )
        .title(title)
        .on_close(ctx.on_click(|actions| actions.emit(SceneDialogAction::Cancel)))
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::Vec2;
    use katla_ui::{KeyCode, UiContext, declarative::ViewTree};

    #[test]
    fn test_scene_dialog_fits_narrow_window_and_stretches_path_field() {
        for width in [320.0, 560.0, 800.0] {
            let size = Vec2::new(width, 450.0);
            for dialog in [
                SceneDialog::Open("/long/path/to/a/scene.katla".into()),
                SceneDialog::SaveAs("/long/path/to/a/scene.katla".into()),
                SceneDialog::Unsaved,
                SceneDialog::Overwrite(PathBuf::from("/long/path/to/a/scene.katla")),
                SceneDialog::Error("A scene operation failed with a long explanation.".into()),
            ] {
                let mut tree = ViewTree::default();
                let mut ui = UiContext::new();
                tree.env_mut().set(SceneDialogData {
                    dialog: Some(dialog),
                    screen_size: size,
                });
                ui.begin(size, 1.0);
                tree.frame(&mut ui, &SceneDialogView, size);
                for (id, node) in tree.iter_nodes() {
                    let bounds = tree.resolved_bounds()[&id];
                    assert!(
                        bounds.min.x() >= 0.0 && bounds.max.x() <= width + 0.5,
                        "{bounds:?}"
                    );
                    if node
                        .widget
                        .as_any()
                        .is::<katla_ui::declarative::widgets::textfield::TextField>()
                    {
                        assert!(bounds.width() >= width.min(560.0) - 48.0, "{bounds:?}");
                    }
                }
                ui.end();
            }
        }
    }

    #[test]
    fn test_scene_dialog_escape_emits_cancel_after_dialog_replacement() {
        let mut tree = ViewTree::default();
        let mut ui = UiContext::new();
        let size = Vec2::new(800.0, 600.0);
        for dialog in [
            SceneDialog::Open("scene.katla".into()),
            SceneDialog::Unsaved,
            SceneDialog::SaveAs("chosen.katla".into()),
        ] {
            tree.env_mut().set(SceneDialogData {
                dialog: Some(dialog),
                screen_size: size,
            });
            ui.begin(size, 1.0);
            tree.frame(&mut ui, &SceneDialogView, size);
            ui.end();
            ui.input_mut().clear_frame_state();
            ui.begin(size, 1.0);
            ui.input_mut().keys_pressed.push(KeyCode::Escape);
            tree.frame(&mut ui, &SceneDialogView, size);
            assert!(
                tree.actions_mut()
                    .drain::<SceneDialogAction>()
                    .into_iter()
                    .any(|action| matches!(action, SceneDialogAction::Cancel))
            );
            ui.end();
            ui.input_mut().clear_frame_state();
        }
    }
}
