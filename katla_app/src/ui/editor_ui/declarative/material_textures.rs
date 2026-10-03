//! Role image previews, explicit sources and independent UV/filter controls.

use crate::{
    material_images::TextureSource,
    rendering::TextureSampling,
    scene::AssetRef,
    ui::editor_ui::{
        ColorScheme,
        types::{EditorAction, MaterialInspectorInfo},
    },
};
use katla_agent::{
    material::MaterialOp,
    material_sampling::{Magnification, Minification, SamplingPatch, TextureRole, TextureWrap},
};
use katla_ecs::EntityId;
use katla_gfx::{AddressMode, FilterMode, MipFilter};
use katla_math::Color;
use katla_ui::{
    FontSize,
    declarative::{
        BuildContext, StateId, Widget, WidgetBox, button, empty, hstack, image, labeled_slider,
        radio, section, text, textfield, vstack,
    },
};

#[derive(Clone, PartialEq)]
struct Baseline {
    entity: Option<EntityId>,
    role: usize,
    value: TextureSampling,
    source: TextureSource,
}
/// Read text state after input has processed the submit frame.
#[derive(Clone)]
pub(crate) enum MaterialTextureAction {
    Assign {
        entity: EntityId,
        role: TextureRole,
        path: StateId,
        root: StateId,
        kind: StateId,
        index: StateId,
    },
    Asset {
        entity: EntityId,
        path: StateId,
        save: bool,
    },
}
pub(super) struct TextureControls {
    baseline: StateId,
    expanded: StateId,
    role: StateId,
    uv_set: StateId,
    channels: [StateId; 6],
    path: StateId,
    root: StateId,
    kind: StateId,
    index: StateId,
    asset_path: StateId,
}
impl TextureControls {
    pub(super) fn reserve(ctx: &mut BuildContext) -> Self {
        Self {
            baseline: ctx.state(Baseline {
                entity: None,
                role: 0,
                value: TextureSampling::default(),
                source: TextureSource::Inherit,
            }),
            expanded: ctx.state(false),
            role: ctx.state(0usize),
            uv_set: ctx.state(0usize),
            channels: std::array::from_fn(|_| ctx.state(0f32)),
            path: ctx.state(String::new()),
            root: ctx.state(0usize),
            kind: ctx.state(0usize),
            index: ctx.state("0".to_owned()),
            asset_path: ctx.state("resources/materials/surface.katmat".to_owned()),
        }
    }
    pub(super) fn build(
        &self,
        ctx: &mut BuildContext,
        selected: Option<(EntityId, &MaterialInspectorInfo)>,
        theme: &ColorScheme,
        width: f32,
    ) -> Option<Box<dyn Widget>> {
        let Some((entity, data)) = selected else {
            let mut base: Baseline = ctx.get_state(self.baseline)?;
            base.entity = None;
            ctx.set_state(self.baseline, base);
            return None;
        };
        let base: Baseline = ctx.get_state(self.baseline)?;
        let index = ctx.get_state::<usize>(self.role).unwrap_or_default().min(4);
        let role = TextureRole::ALL[index];
        let actual = data.sampling.roles()[index];
        let source = &data.sources[index];
        if base.entity == Some(entity) && base.role == index {
            let edited = self
                .channels
                .map(|id| ctx.get_state::<f32>(id).unwrap_or_default());
            let old = channels(base.value);
            let set = ctx.get_state::<usize>(self.uv_set).unwrap_or_default();
            let mut patch = SamplingPatch {
                tex_coord: (set != base.value.uv.tex_coord as usize).then_some(set as u32),
                offset: (edited[..2] != old[..2]).then_some([edited[0], edited[1]]),
                rotation: (edited[2] != old[2]).then_some(edited[2]),
                scale: (edited[3..5] != old[3..5]).then_some([edited[3], edited[4]]),
                anisotropy: (edited[5] != old[5]).then_some(edited[5].round().clamp(1., 16.) as u8),
                ..Default::default()
            };
            if patch.anisotropy.is_some_and(|value| value > 1) {
                patch.minification = Some(linear_minification(actual));
                patch.magnification = Some(Magnification::Linear);
            }
            if !patch.is_empty() {
                ctx.emit(EditorAction::EditMaterial(MaterialOp::SetSampling {
                    entity_ids: vec![entity.id().to_string()],
                    role,
                    patch,
                }));
            }
        }
        if base.entity != Some(entity) || base.role != index || base.source != *source {
            let (root, path, kind, image_index) = source_fields(source);
            ctx.set_state(self.root, root);
            ctx.set_state(self.path, path);
            ctx.set_state(self.kind, kind);
            ctx.set_state(self.index, image_index);
        }
        for (id, value) in self.channels.iter().zip(channels(actual)) {
            ctx.set_state(*id, value);
        }
        ctx.set_state(self.uv_set, actual.uv.tex_coord as usize);
        ctx.set_state(
            self.baseline,
            Baseline {
                entity: Some(entity),
                role: index,
                value: actual,
                source: source.clone(),
            },
        );
        let content_width = (width - 24.).max(1.);
        let mut content = Vec::new();
        for (i, role) in TextureRole::ALL.into_iter().enumerate() {
            let preview = data.previews[i].map_or_else(
                || empty().boxed(),
                |id| image(id, Color::WHITE).image_size(32., 32.).boxed(),
            );
            let state = self.role;
            let callback =
                ctx.on_click(move |actions| actions.emit(TextureRoleAction { state, index: i }));
            let select = text(label(role))
                .color(if i == index {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .truncate((content_width - 40.).max(1.))
                .boxed();
            content.push(
                super::material_drag::ImageDragWidget::new(
                    super::material_drag::DragRole::Target { entity, role },
                    ctx.env::<super::material_drag::MaterialDrag>()
                        .cloned()
                        .unwrap_or_default(),
                    hstack([preview, select]).spacing(8.).boxed(),
                    callback,
                    i == index,
                )
                .boxed(),
            );
        }
        content.push(
            text(source_label(source))
                .truncate(content_width)
                .color(theme.text_muted)
                .font_size(FontSize::Small)
                .boxed(),
        );
        let clear = button("Neutral")
            .flex_width((content_width - 4.) / 2.)
            .tooltip("Clear this role to its neutral image; preserve factors and sampling")
            .on_click(ctx.on_click(move |actions| {
                actions.emit(EditorAction::MaterialPreset(MaterialOp::SetTexture {
                    entity_ids: vec![entity.id().to_string()],
                    role,
                    source: serde_json::json!({"kind":"neutral"}),
                }))
            }))
            .boxed();
        let inherit = button("Original")
            .flex_width((content_width - 4.) / 2.)
            .tooltip("Restore the original imported image for this role")
            .on_click(ctx.on_click(move |actions| {
                actions.emit(EditorAction::MaterialPreset(MaterialOp::SetTexture {
                    entity_ids: vec![entity.id().to_string()],
                    role,
                    source: serde_json::json!({"kind":"inherit"}),
                }))
            }))
            .boxed();
        content.push(hstack([clear, inherit]).spacing(4.).boxed());
        content.push(
            button("Browser image")
                .flex_width(content_width)
                .tooltip("Assign the selected image in the asset browser")
                .on_click(ctx.on_click(move |actions| {
                    actions.emit(EditorAction::UseBrowserMaterialImage { entity, role })
                }))
                .boxed(),
        );
        content.push(choices(
            width < 240.,
            vec![
                radio(self.kind, 0, "Image").boxed(),
                radio(self.kind, 1, "glTF image").boxed(),
            ],
        ));
        content.push(choices(
            width < 260.,
            vec![
                radio(self.root, 0, "Resource").boxed(),
                radio(self.root, 1, "Scene").boxed(),
                radio(self.root, 2, "File").boxed(),
            ],
        ));
        content.push(
            textfield("Image or glTF path", self.path)
                .flex_width(content_width)
                .boxed(),
        );
        if ctx.get_state::<usize>(self.kind) == Some(1) {
            content.push(
                textfield("glTF image index", self.index)
                    .flex_width(content_width)
                    .boxed(),
            );
        }
        let action = MaterialTextureAction::Assign {
            entity,
            role,
            path: self.path,
            root: self.root,
            kind: self.kind,
            index: self.index,
        };
        content.push(
            button("Assign image")
                .on_click(ctx.on_click(move |actions| actions.emit(action.clone())))
                .boxed(),
        );
        content.push(text("Coordinates").color(theme.text_secondary).boxed());
        content.push(
            hstack(
                (0..2)
                    .map(|set| {
                        if data.uv_sets[set] {
                            radio(self.uv_set, set, format!("UV{set}")).boxed()
                        } else {
                            text(format!("UV{set} unavailable"))
                                .color(theme.text_muted)
                                .font_size(FontSize::Small)
                                .boxed()
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(8.)
            .boxed(),
        );
        let values = channels(actual);
        for (i, label) in [
            "Offset U",
            "Offset V",
            "Rotation rad",
            "Scale U",
            "Scale V",
            "Anisotropy",
        ]
        .into_iter()
        .enumerate()
        {
            let range = match i {
                2 => values[i].min(-std::f32::consts::TAU)..=values[i].max(std::f32::consts::TAU),
                5 => 1.0..=16.0,
                _ => values[i].min(-8.)..=values[i].max(8.),
            };
            content.push(
                labeled_slider(label, self.channels[i], range)
                    .label_width(if width < 220. { 60. } else { 84. })
                    .show_value(true)
                    .precision(if i == 5 { 0 } else { 2 })
                    .boxed(),
            );
        }
        let min = minification(actual);
        let next = (min + 1) % 6;
        let filter = MIN[next].0;
        let min_patch = SamplingPatch {
            minification: Some(filter),
            anisotropy: matches!(
                filter,
                Minification::Nearest
                    | Minification::NearestMipmapNearest
                    | Minification::NearestMipmapLinear
            )
            .then_some(1),
            ..Default::default()
        };
        content.push(sampling_button(
            ctx,
            entity,
            role,
            format!("Min: {}", MIN[min].1),
            min_patch,
            content_width,
        ));
        let nearest = actual.sampler.mag_filter == FilterMode::Nearest;
        content.push(sampling_button(
            ctx,
            entity,
            role,
            format!("Mag: {}", if nearest { "Nearest" } else { "Linear" }),
            SamplingPatch {
                magnification: Some(if nearest {
                    Magnification::Linear
                } else {
                    Magnification::Nearest
                }),
                anisotropy: (!nearest).then_some(1),
                ..Default::default()
            },
            content_width,
        ));
        for (axis, address) in [
            ("U", actual.sampler.address_u),
            ("V", actual.sampler.address_v),
        ] {
            let (name, next) = match address {
                AddressMode::Repeat => ("Repeat", TextureWrap::ClampToEdge),
                AddressMode::ClampToEdge => ("Clamp", TextureWrap::MirroredRepeat),
                AddressMode::MirroredRepeat => ("Mirror", TextureWrap::Repeat),
            };
            content.push(sampling_button(
                ctx,
                entity,
                role,
                format!("Wrap {axis}: {name}"),
                SamplingPatch {
                    wrap_u: (axis == "U").then_some(next),
                    wrap_v: (axis == "V").then_some(next),
                    ..Default::default()
                },
                content_width,
            ));
        }
        content.push(
            text("Anisotropy uses linear filtering. Nearest filtering disables anisotropy.")
                .color(theme.text_muted)
                .font_size(FontSize::Small)
                .wrap(content_width)
                .boxed(),
        );
        content.push(
            text("Reusable material")
                .color(theme.text_secondary)
                .boxed(),
        );
        content.push(
            textfield("Project .katmat path", self.asset_path)
                .flex_width(content_width)
                .boxed(),
        );
        content.push(choices(
            width < 260.,
            vec![
                asset_button(ctx, "Save material", entity, self.asset_path, true),
                asset_button(ctx, "Apply material", entity, self.asset_path, false),
            ],
        ));
        let child = if ctx.get_state::<bool>(self.expanded).unwrap_or_default() {
            vstack(content).spacing(6.).flex_shrink(0.).boxed()
        } else {
            empty().boxed()
        };
        Some(section("Texture images and sampling", child, self.expanded).boxed())
    }
}
#[derive(Clone)]
pub(crate) struct TextureRoleAction {
    pub(crate) state: StateId,
    pub(crate) index: usize,
}
fn asset_button(
    ctx: &mut BuildContext,
    name: &str,
    entity: EntityId,
    path: StateId,
    save: bool,
) -> Box<dyn Widget> {
    button(name)
        .on_click(ctx.on_click(move |actions| {
            actions.emit(MaterialTextureAction::Asset { entity, path, save })
        }))
        .boxed()
}
fn sampling_button(
    ctx: &mut BuildContext,
    entity: EntityId,
    role: TextureRole,
    label: String,
    patch: SamplingPatch,
    width: f32,
) -> Box<dyn Widget> {
    button(label)
        .flex_width(width)
        .tooltip("Click to select the next policy")
        .on_click(ctx.on_click(move |actions| {
            actions.emit(EditorAction::MaterialPreset(MaterialOp::SetSampling {
                entity_ids: vec![entity.id().to_string()],
                role,
                patch,
            }))
        }))
        .boxed()
}
fn channels(value: TextureSampling) -> [f32; 6] {
    [
        value.uv.offset[0],
        value.uv.offset[1],
        value.uv.rotation,
        value.uv.scale[0],
        value.uv.scale[1],
        value.sampler.anisotropy as f32,
    ]
}
const MIN: [(Minification, &str); 6] = [
    (Minification::Nearest, "Nearest"),
    (Minification::Linear, "Linear"),
    (Minification::NearestMipmapNearest, "Nearest + nearest mips"),
    (Minification::LinearMipmapNearest, "Linear + nearest mips"),
    (Minification::NearestMipmapLinear, "Nearest + linear mips"),
    (Minification::LinearMipmapLinear, "Linear + linear mips"),
];
fn minification(value: TextureSampling) -> usize {
    match (value.sampler.min_filter, value.sampler.mip_filter) {
        (FilterMode::Nearest, MipFilter::None) => 0,
        (FilterMode::Linear, MipFilter::None) => 1,
        (FilterMode::Nearest, MipFilter::Nearest) => 2,
        (FilterMode::Linear, MipFilter::Nearest) => 3,
        (FilterMode::Nearest, MipFilter::Linear) => 4,
        (FilterMode::Linear, MipFilter::Linear) => 5,
    }
}
fn linear_minification(value: TextureSampling) -> Minification {
    match value.sampler.mip_filter {
        MipFilter::None => Minification::Linear,
        MipFilter::Nearest => Minification::LinearMipmapNearest,
        MipFilter::Linear => Minification::LinearMipmapLinear,
    }
}
pub(crate) fn label(role: TextureRole) -> &'static str {
    match role {
        TextureRole::Albedo => "Albedo",
        TextureRole::Normal => "Normal map",
        TextureRole::MetallicRoughness => "Metallic / roughness",
        TextureRole::Occlusion => "Occlusion map",
        TextureRole::Emission => "Emission map",
    }
}
fn source_fields(source: &TextureSource) -> (usize, String, usize, String) {
    let (asset, kind, index) = match source {
        TextureSource::File { asset } => (Some(asset), 0, 0),
        TextureSource::GltfImage { asset, image_index } => (Some(asset), 1, *image_index),
        _ => (None, 0, 0),
    };
    let (root, path) = match asset {
        Some(AssetRef::Resource(path)) => (0, path.clone()),
        Some(AssetRef::Scene(path)) => (1, path.clone()),
        Some(AssetRef::File(path)) => (2, path.to_string_lossy().into_owned()),
        None => (0, String::new()),
    };
    (root, path, kind, index.to_string())
}
fn source_label(source: &TextureSource) -> String {
    match source {
        TextureSource::Inherit => "Original imported binding".into(),
        TextureSource::Neutral => "Neutral image".into(),
        TextureSource::File { asset } => asset.path().to_string_lossy().into_owned(),
        TextureSource::GltfImage { asset, image_index } => {
            format!("{} · image {image_index}", asset.path().display())
        }
    }
}

fn choices(vertical: bool, children: Vec<Box<dyn Widget>>) -> Box<dyn Widget> {
    if vertical {
        vstack(children).spacing(4.).boxed()
    } else {
        hstack(children).spacing(4.).boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use katla_math::Vec2;
    use katla_ui::{
        UiContext,
        declarative::{Build, ViewTree},
    };

    struct Fixture {
        width: f32,
    }
    impl Build for Fixture {
        fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
            let controls = TextureControls::reserve(ctx);
            ctx.set_state(controls.expanded, true);
            let data = MaterialInspectorInfo {
                values: crate::material_asset::MaterialAsset::default().values,
                sampling: Default::default(),
                sources: std::array::from_fn(|_| TextureSource::Inherit),
                uv_sets: [true, false],
                previews: [None; 5],
            };
            controls
                .build(
                    ctx,
                    Some((EntityId::from_raw(1), &data)),
                    &ColorScheme::default(),
                    self.width,
                )
                .unwrap()
        }
    }
    #[test]
    fn test_texture_controls_fit_narrow_panels_at_larger_font_scales() {
        for width in [180., 240., 400.] {
            for scale in [1., 1.5] {
                let mut ui = UiContext::new();
                ui.set_font_scale(scale);
                let mut tree = ViewTree::new();
                let size = Vec2::new(width, 2200.);
                ui.begin(size, 1.);
                tree.frame(&mut ui, &Fixture { width }, size);
                for (id, _) in tree.iter_nodes() {
                    let bounds = tree.resolved_bounds()[&id];
                    assert!(
                        bounds.min.x() >= -0.5 && bounds.max.x() <= width + 0.5,
                        "width={width} scale={scale} bounds={bounds:?}"
                    );
                }
                assert_eq!(
                    tree.iter_nodes()
                        .filter(|(_, node)| node
                            .widget
                            .as_any()
                            .is::<super::super::material_drag::ImageDragWidget>())
                        .count(),
                    5
                );
                ui.end();
            }
        }
    }
}
