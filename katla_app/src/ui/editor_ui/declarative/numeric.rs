//! Scalar authoring rows synchronized with the current scene snapshot.
use crate::ui::editor_ui::{EditorAction, types::EntityInfo};
use katla_ecs::EntityId;
use katla_math::Color;
use katla_ui::declarative::widgets::number_input::{NumberInput, NumberState};
use katla_ui::{
    FontSize,
    declarative::{Alignment, BuildContext, StateId, Widget, WidgetBox, hstack, text, vstack},
};

const FIELDS: [(&str, &str); 15] = [
    ("TransformComponent", "x"),
    ("TransformComponent", "y"),
    ("TransformComponent", "z"),
    ("TransformComponent", "rot_x"),
    ("TransformComponent", "rot_y"),
    ("TransformComponent", "rot_z"),
    ("TransformComponent", "scale_x"),
    ("TransformComponent", "scale_y"),
    ("TransformComponent", "scale_z"),
    ("PointLight", "intensity"),
    ("PointLight", "range"),
    ("DirectionalLight", "intensity"),
    ("PerspectiveComponent", "fov"),
    ("PerspectiveComponent", "near"),
    ("PerspectiveComponent", "aspect_ratio"),
];
pub(super) struct NumericControls {
    baseline: StateId,
    fields: [StateId; 15],
}
impl NumericControls {
    pub(super) fn reserve(ctx: &mut BuildContext) -> Self {
        Self {
            baseline: ctx.state((None::<EntityId>, [0.0f32; 15])),
            fields: std::array::from_fn(|_| ctx.state(NumberState::new(0.0))),
        }
    }
    pub(super) fn sync(&self, ctx: &mut BuildContext, selected: Option<&EntityInfo>) {
        let Some(entity) = selected else {
            ctx.set_state(self.baseline, (None::<EntityId>, [0.0f32; 15]));
            return;
        };
        let mut actual = [0.0; 15];
        actual[..3].copy_from_slice(&entity.position.to_array());
        actual[3..6].copy_from_slice(&entity.rotation.to_array().map(f32::to_degrees));
        actual[6..9].copy_from_slice(&entity.scale.to_array());
        if let Some(light) = &entity.point_light {
            actual[9] = light.intensity;
            actual[10] = light.range;
        }
        if let Some(light) = &entity.directional_light {
            actual[11] = light.intensity;
        }
        if let Some(camera) = &entity.perspective {
            actual[12..15].copy_from_slice(&[camera.fov, camera.near, camera.aspect_ratio]);
        }
        let (previous_entity, previous): (Option<EntityId>, [f32; 15]) =
            ctx.get_state(self.baseline).unwrap_or((None, [0.0; 15]));
        for (index, id) in self.fields.iter().enumerate() {
            let mut state: NumberState = ctx
                .get_state(*id)
                .unwrap_or_else(|| NumberState::new(actual[index]));
            if previous_entity == Some(entity.id) && state.value != previous[index] {
                let value = if (3..6).contains(&index) {
                    state.value.to_radians()
                } else {
                    state.value
                };
                ctx.emit(EditorAction::EditField {
                    entity: entity.id,
                    component: FIELDS[index].0.into(),
                    field: FIELDS[index].1.into(),
                    value: serde_json::json!(value),
                });
            }
            if previous_entity != Some(entity.id) {
                state = NumberState::new(actual[index]);
            } else {
                state.value = actual[index];
            }
            ctx.set_state(*id, state);
        }
        ctx.set_state(self.baseline, (Some(entity.id), actual));
    }
    pub(super) fn transform(&self, width: f32) -> Vec<Box<dyn Widget>> {
        ["Position", "Rotation", "Scale"]
            .into_iter()
            .enumerate()
            .map(|(row, label)| {
                let compact = width < 212.0;
                let mut children = Vec::new();
                if !compact {
                    children.push(
                        vstack([text(label).font_size(FontSize::Small).boxed()])
                            .flex_width(54.0)
                            .boxed(),
                    );
                }
                for axis in 0..3 {
                    let index = row * 3 + axis;
                    let range = if row == 2 {
                        0.001..=10000.0
                    } else {
                        -1000000.0..=1000000.0
                    };
                    let step = if row == 1 { 0.5 } else { 0.01 };
                    let tint = [
                        Color::new(0.85, 0.45, 0.45, 1.0),
                        Color::new(0.5, 0.75, 0.5, 1.0),
                        Color::new(0.45, 0.65, 0.9, 1.0),
                    ][axis];
                    children.push(
                        NumberInput::new(
                            format!("{label} {}", ["X", "Y", "Z"][axis]),
                            self.fields[index],
                            range,
                            step,
                        )
                        .axis(["X", "Y", "Z"][axis], tint)
                        .boxed(),
                    );
                }
                let inputs = hstack(children)
                    .spacing(4.0)
                    .align(Alignment::Middle)
                    .flex_width(width.max(140.0))
                    .boxed();
                if compact {
                    vstack([text(label).font_size(FontSize::Small).boxed(), inputs])
                        .spacing(4.0)
                        .boxed()
                } else {
                    inputs
                }
            })
            .collect()
    }
    pub(super) fn scalar(
        &self,
        index: usize,
        label: &str,
        min: f32,
        max: f32,
        step: f32,
        width: f32,
    ) -> Box<dyn Widget> {
        hstack([
            vstack([text(label).font_size(FontSize::Small).boxed()])
                .flex_width(72.0)
                .boxed(),
            NumberInput::new(label, self.fields[index], min..=max, step).boxed(),
        ])
        .spacing(8.0)
        .align(Alignment::Middle)
        .flex_width(width.max(124.0))
        .boxed()
    }
}
