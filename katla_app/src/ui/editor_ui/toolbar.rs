use katla_math::{Color, Rect2D, Vec2, Vec3};
use katla_ui::{FontSize, Response, Widget};

use crate::gizmo::GizmoMode;
use crate::{
    Preferences,
    ui::{EditorAction, SpawnableModel, Theme, editor_ui::Panel},
};

impl SpawnableModel {
    pub fn name(&self) -> &'static str {
        match self {
            SpawnableModel::Cube => "Cube",
            SpawnableModel::Sphere => "Sphere",
            SpawnableModel::Cylinder => "Cylinder",
            SpawnableModel::Plane => "Plane",
            SpawnableModel::Torus => "Torus",
        }
    }

    pub fn all() -> &'static [SpawnableModel] {
        &[
            SpawnableModel::Cube,
            SpawnableModel::Sphere,
            SpawnableModel::Cylinder,
            SpawnableModel::Plane,
            SpawnableModel::Torus,
        ]
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolbarState {
    pub file_menu_open: bool,
    pub edit_menu_open: bool,
    pub view_menu_open: bool,
    pub create_menu_open: bool,
    pub pending_actions: Vec<EditorAction>,
}

pub struct Toolbar<'a> {
    pub screen_size: Vec2,
    pub height: f32,
    /// Menu bar dropdown states
    pub state: &'a mut ToolbarState,
    pub theme: &'a Theme,
    pub preferences: &'a Preferences,
    /// Current gizmo mode (for highlighting the active button).
    pub gizmo_mode: GizmoMode,
}

impl<'a> Toolbar<'a> {
    pub fn new(
        screen_size: Vec2,
        height: f32,
        state: &'a mut ToolbarState,
        theme: &'a Theme,
        preferences: &'a Preferences,
        gizmo_mode: GizmoMode,
    ) -> Self {
        Self {
            screen_size,
            height,
            state,
            theme,
            preferences,
            gizmo_mode,
        }
    }
}

impl<'a> Widget for Toolbar<'a> {
    fn ui(self, ui: &mut katla_ui::UiContext) -> katla_ui::Response {
        let theme = &self.theme;
        let toolbar_bounds = Rect2D::from_origin_size(
            Vec2::new(0.0, 0.0),
            Vec2::new(self.screen_size.x(), self.height),
        );

        ui.draw_rect(toolbar_bounds, theme.background_dark);
        ui.draw_line(
            Vec2::new(0.0, self.height),
            Vec2::new(self.screen_size.x(), self.height),
            theme.separator,
            1.0,
        );

        let original_button_normal = ui.style.button_normal;
        ui.style.button_normal = Color::TRANSPARENT;

        let padding = 4.0;
        let menu_item_width = 50.0;
        let button_height = self.height;

        ui.begin_row();
        ui.set_cursor(Vec2::new(0.0, 0.0));

        let file_bounds =
            Rect2D::from_origin_size(ui.cursor(), Vec2::new(menu_item_width, button_height));
        ui.menu_bar_dropdown(
            "file_menu",
            "File",
            file_bounds,
            &mut self.state.file_menu_open,
            |ui, open| {
                if ui.menu_item_clicked("New Scene") {
                    self.state.pending_actions.push(EditorAction::NewScene);
                    *open = false;
                }
                if ui.menu_item_clicked("Open...") {
                    self.state.pending_actions.push(EditorAction::OpenScene);
                    *open = false;
                }
                if ui.menu_item_clicked("Save") {
                    self.state.pending_actions.push(EditorAction::SaveScene);
                    *open = false;
                }
                ui.menu_separator();
                if ui.menu_item_clicked("Quit") {
                    self.state.pending_actions.push(EditorAction::Quit);
                    *open = false;
                }
            },
        );
        ui.spacing(menu_item_width);

        let edit_bounds =
            Rect2D::from_origin_size(ui.cursor(), Vec2::new(menu_item_width, button_height));
        ui.menu_bar_dropdown(
            "edit_menu",
            "Edit",
            edit_bounds,
            &mut self.state.edit_menu_open,
            |ui, open| {
                if ui.menu_item_clicked("Preferences...") {
                    self.state
                        .pending_actions
                        .push(EditorAction::OpenPanel(Panel::Preferences));
                    *open = false;
                }
            },
        );
        ui.spacing(menu_item_width);

        let view_bounds =
            Rect2D::from_origin_size(ui.cursor(), Vec2::new(menu_item_width, button_height));
        let show_grid = self.preferences.show_grid;
        let show_stats = self.preferences.show_stats;
        ui.menu_bar_dropdown(
            "view_menu",
            "View",
            view_bounds,
            &mut self.state.view_menu_open,
            |ui, open| {
                if ui.toggle_menu_item_clicked("Grid", show_grid) {
                    self.state.pending_actions.push(EditorAction::ToggleGrid);
                    *open = false;
                }
                if ui.toggle_menu_item_clicked("Stats", show_stats) {
                    self.state.pending_actions.push(EditorAction::ToggleStats);
                    *open = false;
                }
                ui.menu_separator();
                if ui.menu_item_clicked("Particle Inspector") {
                    self.state
                        .pending_actions
                        .push(EditorAction::OpenPanel(Panel::ParticleInspector));
                    *open = false;
                }
            },
        );
        ui.spacing(menu_item_width);

        let create_bounds = Rect2D::from_origin_size(ui.cursor(), Vec2::new(60.0, button_height));
        ui.menu_bar_dropdown(
            "create_menu",
            "Create",
            create_bounds,
            &mut self.state.create_menu_open,
            |ui, open| {
                for model in SpawnableModel::all() {
                    if ui.menu_item_clicked(model.name()) {
                        self.state
                            .pending_actions
                            .push(EditorAction::SpawnModel(*model, Vec3::new(0.0, 0.0, 0.0)));
                        *open = false;
                    }
                }
            },
        );
        ui.spacing(60.0 + padding);

        ui.end_row();

        // Gizmo mode buttons
        let gizmo_modes: &[(u8, &str, &str)] = &[
            (0, "W:Move", "translate"),
            (1, "E:Rotate", "rotate"),
            (2, "R:Scale", "scale"),
        ];

        let gizmo_button_width = 65.0;
        let gizmo_start_x = ui.cursor().x() + padding;
        ui.draw_line(
            Vec2::new(gizmo_start_x - padding, padding),
            Vec2::new(gizmo_start_x - padding, self.height - padding),
            theme.separator,
            1.0,
        );

        ui.begin_row();
        ui.set_cursor(Vec2::new(gizmo_start_x, 0.0));

        for &(mode_id, label, _icon) in gizmo_modes {
            let is_active = self.gizmo_mode as u8 == mode_id;
            let btn_bounds =
                Rect2D::from_origin_size(ui.cursor(), Vec2::new(gizmo_button_width, button_height));

            let bg = if is_active {
                theme.highlight
            } else {
                Color::TRANSPARENT
            };

            let text_color = if is_active {
                theme.text_primary
            } else {
                theme.text_muted
            };

            if ui.mouse_clicked(katla_ui::input::mouse_button::LEFT)
                && btn_bounds.contains(ui.mouse_pos())
            {
                self.state
                    .pending_actions
                    .push(EditorAction::SetGizmoMode(mode_id));
            }

            if btn_bounds.contains(ui.mouse_pos()) && !is_active {
                ui.draw_rect(btn_bounds, theme.button_hover);
            }

            ui.draw_rect(btn_bounds, bg);
            let font_size = ui.scaled_font_size(FontSize::Small);
            let text_size = ui.measure_text(label, font_size);
            let text_pos = Vec2::new(
                btn_bounds.min.x() + (btn_bounds.width() - text_size.x()) * 0.5,
                btn_bounds.min.y() + (btn_bounds.height() - text_size.y()) * 0.5,
            );
            ui.draw_text(label, text_pos, text_color, font_size);

            ui.spacing(gizmo_button_width);
        }

        ui.end_row();

        let current_x = ui.cursor().x();
        ui.draw_line(
            Vec2::new(current_x + padding, padding),
            Vec2::new(current_x + padding, self.height - padding),
            theme.separator,
            1.0,
        );

        let title = "Katla Engine";
        let title_size = ui.measure_text(title, ui.scaled_font_size(FontSize::Medium));
        let title_pos = Vec2::new(
            self.screen_size.x() * 0.5 - title_size.x() * 0.5,
            self.height * 0.5 - title_size.y() * 0.5,
        );
        ui.draw_text(
            title,
            title_pos,
            theme.text_muted,
            ui.scaled_font_size(FontSize::Medium),
        );

        ui.style.button_normal = original_button_normal;
        Response::default()
    }
}
