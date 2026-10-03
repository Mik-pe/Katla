//! Native inspector input acceptance for texture slots, sampling and reusable surfaces.

use super::Application;
use crate::components::{DrawableComponent, NameComponent};
use crate::ui::editor_ui::declarative::material_drag::{DragRole, ImageDragWidget};
use katla_agent::material::MaterialValues;
use katla_math::Vec2;
use katla_ui::{
    declarative::widgets::{
        button::Button, labeled_slider::LabeledSlider, section::Section, textfield::TextField,
    },
    input::mouse_button,
};

#[derive(Default)]
pub(super) struct MaterialUiAcceptance {
    entity: Option<katla_ecs::EntityId>,
    before: Option<MaterialValues>,
    history: usize,
    asset: String,
}
impl MaterialUiAcceptance {
    pub(super) fn begin(
        &mut self,
        app: &mut Application,
        frame: usize,
        output: &str,
    ) -> Result<(), String> {
        match frame {
            180 => {
                let id = app.spawn_sphere([2., 1., 0.], 0.5, 16, 16);
                app.world
                    .add_component(id, NameComponent::new("Inspector material fixture"));
                app.editor.editor_ui.selected_entity = Some(id);
                self.entity = Some(id);
                self.before = Some(super::editor::material::values(
                    app.world
                        .get_component::<DrawableComponent>(id)
                        .ok_or("Native UI fixture has no drawable")?,
                ));
                self.history = app.editor.undo_stack.len();
                let directory = std::path::Path::new(output).join("images");
                std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
                image::save_buffer(
                    directory.join("checker.png"),
                    &[40u8, 180, 230, 255, 240, 240, 240, 0].repeat(8),
                    4,
                    4,
                    image::ColorType::Rgba8,
                )
                .map_err(|error| error.to_string())?;
                app.editor
                    .editor_ui
                    .asset_browser
                    .navigate_to(&directory, &app.editor.thumbnail_texture_handles);
                self.asset = format!(
                    "resources/materials/__material-ui-acceptance-{}.katmat",
                    std::process::id()
                );
            }
            183 => {
                let ids: Vec<_> = app
                    .editor
                    .editor_ui
                    .view_tree()
                    .iter_nodes()
                    .filter_map(|(_, node)| {
                        node.widget
                            .as_any()
                            .downcast_ref::<Section>()
                            .map(|section| section.expanded_id)
                    })
                    .collect();
                let tree = app.editor.editor_ui.view_tree_mut();
                for id in ids {
                    tree.state_arena_mut().set(id, false);
                }
            }
            186 => Self::click(app, "section", "Texture images and sampling"),
            187 | 195 | 205 | 212 | 223 | 232 | 249 | 256 => Self::release(app),
            193 => {
                if let Some(point) = Self::drag_position(app, true) {
                    Self::press(app, point);
                }
            }
            194 => {
                if let Some(point) = Self::drag_position(app, false) {
                    app.ui_context
                        .input_mut()
                        .set_mouse_pos(Vec2::new(point.0, point.1));
                }
            }
            201 => Self::scroll_to(app, "button", "Neutral"),
            204 => Self::click(app, "button", "Neutral"),
            211 => Self::click(app, "button", "Original"),
            216 => Self::scroll_to(app, "button", "Browser image"),
            217 => Self::click(app, "button", "Browser image"),
            218 => Self::release(app),
            220 => Self::scroll_to(app, "button", "Min:"),
            222 => Self::click(app, "button", "Min:"),
            227 => Self::scroll_to(app, "slider", "Scale U"),
            230 | 231 => {
                let tree = app.editor.editor_ui.view_tree();
                let point = tree.iter_nodes().find_map(|(id, node)| {
                    let slider = node.widget.as_any().downcast_ref::<LabeledSlider>()?;
                    if slider.label != "Scale U" {
                        return None;
                    }
                    let bounds = slider.track_bounds(*tree.resolved_bounds().get(&id)?);
                    Some((
                        bounds.min.x() + bounds.width() * 0.75,
                        bounds.center().y() + if frame == 231 { 32. } else { 0. },
                    ))
                });
                if let Some(point) = point {
                    Self::press(app, point);
                }
            }
            238 => Self::scroll_to(app, "field", "Project .katmat path"),
            241 => {
                let field = app
                    .editor
                    .editor_ui
                    .view_tree()
                    .iter_nodes()
                    .find_map(|(_, node)| {
                        node.widget
                            .as_any()
                            .downcast_ref::<TextField>()
                            .filter(|field| field.placeholder == "Project .katmat path")
                            .map(|field| field.value_id)
                    });
                if let Some(field) = field {
                    app.editor
                        .editor_ui
                        .view_tree_mut()
                        .state_arena_mut()
                        .set(field, self.asset.clone());
                }
            }
            248 => Self::click(app, "button", "Save material"),
            254 => Self::scroll_to(app, "button", "Apply material"),
            255 => Self::click(app, "button", "Apply material"),
            261 => {
                let root = app
                    .resources
                    .root
                    .parent()
                    .ok_or("Native UI fixture has no project root")?;
                let path = root.join(&self.asset);
                if path.exists() {
                    std::fs::remove_file(path).map_err(|error| error.to_string())?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(super) fn end(
        &self,
        app: &Application,
        frame: usize,
    ) -> Option<(&'static str, bool, String, &'static str)> {
        let d = app.world.get_component::<DrawableComponent>(self.entity?)?;
        let values = super::editor::material::values(d);
        match frame {
            199 => Some((
                "image_drag_assigns_one_role_and_preserves_factors",
                d.texture_bindings.0[0]
                    .as_ref()
                    .is_some_and(|binding| binding.has_image())
                    && Some(values) == self.before
                    && d.texture_bindings.0[1..].iter().all(Option::is_none)
                    && app.editor.undo_stack.len() == self.history + 1,
                format!(
                    "source={:?}, history={}",
                    d.texture_bindings.assignments(),
                    app.editor.undo_stack.len() - self.history
                ),
                "21_texture_drag",
            )),
            208 => Some((
                "neutral_image_button_preserves_factors",
                d.texture_bindings.0[0]
                    .as_ref()
                    .is_some_and(|binding| !binding.has_image())
                    && Some(values) == self.before,
                format!("sources={:?}", d.texture_bindings.assignments()),
                "22_texture_neutral",
            )),
            215 => Some((
                "original_image_button_restores_inheritance",
                d.texture_bindings.0[0].is_none(),
                format!("sources={:?}", d.texture_bindings.assignments()),
                "23_texture_original",
            )),
            225 => Some((
                "image_browser_assignment_and_filter_button",
                d.texture_bindings.0[0]
                    .as_ref()
                    .is_some_and(|binding| binding.has_image())
                    && d.sampling.albedo.sampler.min_filter == katla_gfx::FilterMode::Nearest
                    && d.sampling.albedo.sampler.mip_filter == katla_gfx::MipFilter::None,
                format!("sampling={:?}", d.sampling.albedo),
                "24_texture_filters",
            )),
            235 => Some((
                "uv_drag_is_one_scoped_undo",
                (d.sampling.albedo.uv.scale[0] - 4.).abs() < 0.001
                    && d.sampling.normal == crate::rendering::TextureSampling::default()
                    && Some(values) == self.before
                    && app.editor.undo_stack.len() == self.history + 6,
                format!(
                    "sampling={:?}, history={}",
                    d.sampling,
                    app.editor.undo_stack.len() - self.history
                ),
                "25_texture_uv",
            )),
            252 => {
                let path = app.resources.root.parent()?.join(&self.asset);
                let saved = crate::material_asset::MaterialAsset::load(&path);
                Some((
                    "save_material_button_captures_effective_surface",
                    saved.as_ref().is_ok_and(|asset| {
                        asset.values == values
                            && asset.sampling == d.sampling
                            && !matches!(
                                asset.textures.albedo,
                                crate::material_images::TextureSource::Inherit
                            )
                    }),
                    format!("saved={saved:?}"),
                    "26_material_saved",
                ))
            }
            259 => Some((
                "apply_material_button_is_one_complete_edit",
                app.editor.undo_stack.len() == self.history + 7
                    && self.before.is_some_and(|before| {
                        let mut expected = before;
                        expected.base_color = values.base_color;
                        values == expected
                            && values
                                .base_color
                                .iter()
                                .zip(before.base_color)
                                .all(|(actual, old)| (*actual - old).abs() < 0.000001)
                    })
                    && (d.sampling.albedo.uv.scale[0] - 4.).abs() < 0.001,
                format!("history={}", app.editor.undo_stack.len() - self.history),
                "27_material_applied",
            )),
            _ => None,
        }
    }
    fn bounds(app: &Application, kind: &str, label: &str) -> Option<katla_math::Rect2D> {
        let tree = app.editor.editor_ui.view_tree();
        tree.iter_nodes().find_map(|(id, node)| {
            let any = node.widget.as_any();
            let matches = match kind {
                "button" => any
                    .downcast_ref::<Button>()
                    .is_some_and(|button| button.label.starts_with(label)),
                "section" => any
                    .downcast_ref::<Section>()
                    .is_some_and(|section| section.title == label),
                "slider" => any
                    .downcast_ref::<LabeledSlider>()
                    .is_some_and(|slider| slider.label == label),
                "field" => any
                    .downcast_ref::<TextField>()
                    .is_some_and(|field| field.placeholder == label),
                _ => false,
            };
            matches
                .then(|| tree.resolved_bounds().get(&id).copied())
                .flatten()
        })
    }
    fn click(app: &mut Application, kind: &str, label: &str) {
        if let Some(bounds) = Self::bounds(app, kind, label) {
            let point = if kind == "section" {
                (bounds.center().x(), bounds.min.y() + 12.)
            } else {
                (bounds.center().x(), bounds.center().y())
            };
            Self::press(app, point);
        } else {
            log::error!("Missing material UI target {label}");
        }
    }
    fn scroll_to(app: &mut Application, kind: &str, label: &str) {
        if let Some(bounds) = Self::bounds(app, kind, label) {
            let input = app.ui_context.input_mut();
            input.set_mouse_pos(Vec2::new(1140., 280.));
            input.scroll_delta = Vec2::new(0., (280. - bounds.center().y()) / 30.);
        }
    }
    fn drag_position(app: &Application, source: bool) -> Option<(f32, f32)> {
        let tree = app.editor.editor_ui.view_tree();
        tree.iter_nodes().find_map(|(id, node)| {
            let widget = node.widget.as_any().downcast_ref::<ImageDragWidget>()?;
            let matches = match &widget.role {
                DragRole::Source(path) => {
                    source && path.file_name().and_then(|name| name.to_str()) == Some("checker.png")
                }
                DragRole::Target { role, .. } => {
                    !source && *role == katla_agent::material_sampling::TextureRole::Albedo
                }
            };
            matches
                .then(|| {
                    tree.resolved_bounds()
                        .get(&id)
                        .map(|bounds| (bounds.center().x(), bounds.center().y()))
                })
                .flatten()
        })
    }
    fn press(app: &mut Application, point: (f32, f32)) {
        let input = app.ui_context.input_mut();
        input.set_mouse_pos(Vec2::new(point.0, point.1));
        let time = input.last_click_time[mouse_button::LEFT] + 0.1;
        input.set_mouse_button_with_time(mouse_button::LEFT, true, time);
    }
    fn release(app: &mut Application) {
        app.ui_context
            .input_mut()
            .set_mouse_button(mouse_button::LEFT, false);
    }
}
