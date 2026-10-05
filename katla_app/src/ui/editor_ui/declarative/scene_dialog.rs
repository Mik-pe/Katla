//! Editor file selection and protection for unsaved scene changes.

use katla_ui::declarative::{
    Alignment, Build, BuildContext, StateId, Widget, WidgetBox, button, empty, hstack, modal, text,
    textfield, vstack,
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
        let dialog = ctx
            .env::<SceneDialogData>()
            .and_then(|data| data.dialog.clone());
        let previous = ctx.state(None::<SceneDialog>);
        let path = ctx.state(String::new());
        let open = ctx.state(false);
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
                        text("Enter the path to a .katla scene file.").boxed(),
                        textfield("Scene file path", path).on_submit(submit).boxed(),
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
                    text("Save your changes before continuing?").boxed(),
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
                    text(format!("{} already exists.", path.display())).boxed(),
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
                    text(message).boxed(),
                    button("OK")
                        .on_click(ctx.on_click(|actions| actions.emit(SceneDialogAction::Cancel)))
                        .boxed(),
                ])
                .spacing(12.0)
                .padding_all(16.0)
                .boxed(),
            ),
        };
        modal(560.0, 240.0, open, body)
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
