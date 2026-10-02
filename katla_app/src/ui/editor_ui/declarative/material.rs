//! Live surface controls using the same PBR vocabulary as agent tools.

use crate::ui::editor_ui::{ColorScheme, types::EditorAction};
use katla_agent::material::{MaterialOp, MaterialPreset, MaterialValues};
use katla_ecs::EntityId;
use katla_math::Color;
use katla_ui::declarative::{
    BuildContext, StateId, Widget, WidgetBox, button, empty, hstack, icon, labeled_slider, section,
    text, vstack,
};
use katla_ui::{FontSize, ForkAwesome};

#[derive(Clone, Copy, PartialEq)]
struct Baseline {
    entity: Option<EntityId>,
    values: MaterialValues,
}

pub(super) struct MaterialControls {
    baseline: StateId,
    channels: [StateId; 7],
    expanded: StateId,
}

impl MaterialControls {
    pub(super) fn reserve(ctx: &mut BuildContext) -> Self {
        Self {
            baseline: ctx.state(Baseline {
                entity: None,
                values: MaterialPreset::Plaster.values(),
            }),
            channels: std::array::from_fn(|_| ctx.state(0.0f32)),
            expanded: ctx.state(true),
        }
    }

    pub(super) fn build(
        &self,
        ctx: &mut BuildContext,
        selected: Option<(EntityId, MaterialValues)>,
        theme: &ColorScheme,
        width: f32,
    ) -> Option<Box<dyn Widget>> {
        let Some((entity, values)) = selected else {
            let mut baseline: Baseline = ctx.get_state(self.baseline)?;
            baseline.entity = None;
            ctx.set_state(self.baseline, baseline);
            return None;
        };
        let baseline: Baseline = ctx.get_state(self.baseline)?;
        let actual = channels(values);
        if baseline.entity == Some(entity) {
            let edited: [f32; 7] = self
                .channels
                .map(|id| ctx.get_state(id).unwrap_or_default());
            let old = channels(baseline.values);
            if edited != old {
                ctx.emit(EditorAction::EditMaterial(MaterialOp::Set {
                    entity_ids: vec![entity.id().to_string()],
                    preset: None,
                    base_color: (edited[..4] != old[..4])
                        .then_some([edited[0], edited[1], edited[2], edited[3]]),
                    metallic: (edited[4] != old[4]).then_some(edited[4]),
                    roughness: (edited[5] != old[5]).then_some(edited[5]),
                    ao: (edited[6] != old[6]).then_some(edited[6]),
                }));
            }
        }
        for (id, value) in self.channels.iter().zip(actual) {
            ctx.set_state(*id, value);
        }
        ctx.set_state(
            self.baseline,
            Baseline {
                entity: Some(entity),
                values,
            },
        );

        let c = values.base_color;
        let mut content = vec![
            hstack([
                hstack([
                    icon(ForkAwesome::SQUARE)
                        .color(Color::new(c[0], c[1], c[2], 1.0))
                        .icon_size(FontSize::Large)
                        .boxed(),
                    text(format!(
                        "#{:02X}{:02X}{:02X}",
                        (c[0] * 255.0).round() as u8,
                        (c[1] * 255.0).round() as u8,
                        (c[2] * 255.0).round() as u8
                    ))
                    .color(theme.text_primary)
                    .boxed(),
                ])
                .spacing(4.0)
                .boxed(),
                vstack([
                    text("Surface material").color(theme.text_primary).boxed(),
                    text("Live on this object")
                        .color(theme.text_muted)
                        .font_size(FontSize::Small)
                        .boxed(),
                ])
                .spacing(2.0)
                .boxed(),
            ])
            .spacing(10.0)
            .boxed(),
        ];
        for presets in MaterialPreset::ALL.chunks(2) {
            let buttons = presets
                .iter()
                .copied()
                .map(|preset| {
                    button(preset.label())
                        .fill(theme.panel_bg)
                        .border(Color::TRANSPARENT)
                        .on_click(ctx.on_click(move |actions| {
                            actions.emit(EditorAction::MaterialPreset(MaterialOp::Set {
                                entity_ids: vec![entity.id().to_string()],
                                preset: Some(preset),
                                base_color: None,
                                metallic: None,
                                roughness: None,
                                ao: None,
                            }));
                        }))
                        .boxed()
                })
                .collect::<Vec<_>>();
            content.push(hstack(buttons).spacing(6.0).boxed());
        }
        for (index, label) in [
            "Red",
            "Green",
            "Blue",
            "Alpha",
            "Metallic",
            "Roughness",
            "Occlusion",
        ]
        .iter()
        .enumerate()
        {
            content.push(
                labeled_slider(*label, self.channels[index], 0.0..=1.0)
                    .label_width(76.0)
                    .show_value(true)
                    .precision(2)
                    .boxed(),
            );
        }
        content.push(
            text("Presets are PBR tints. Model textures stay attached.")
                .color(theme.text_muted)
                .font_size(FontSize::Small)
                .wrap((width - 24.0).max(120.0))
                .boxed(),
        );
        let child = if ctx.get_state::<bool>(self.expanded).unwrap_or_default() {
            vstack(content).spacing(6.0).boxed()
        } else {
            empty().boxed()
        };
        Some(section("Material", child, self.expanded).boxed())
    }
}

fn channels(v: MaterialValues) -> [f32; 7] {
    [
        v.base_color[0],
        v.base_color[1],
        v.base_color[2],
        v.base_color[3],
        v.metallic,
        v.roughness,
        v.ao,
    ]
}
