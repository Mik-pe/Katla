use katla_math::{Color, Rect2D, Vec2, Vec3};
use katla_ui::{widgets::Button, FontSize, ForkAwesome, Response, Widget};

use crate::{
    ui::{editor_ui::Panel, EditorAction, SpawnableModel, Theme},
    Preferences,
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
    pub help_menu_open: bool,
    pub pending_actions: Vec<EditorAction>,
}

pub struct Toolbar<'a> {
    pub screen_size: Vec2,
    pub height: f32,
    /// Menu bar dropdown states
    pub state: &'a mut ToolbarState,
    pub theme: &'a Theme,
    pub preferences: &'a Preferences,
}

impl<'a> Toolbar<'a> {
    pub fn new(
        screen_size: Vec2,
        height: f32,
        state: &'a mut ToolbarState,
        theme: &'a Theme,
        preferences: &'a Preferences,
    ) -> Self {
        Self {
            screen_size,
            height,
            state,
            theme,
            preferences,
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

        // Darker toolbar background
        ui.draw_rect(toolbar_bounds, theme.background_dark);
        ui.draw_line(
            Vec2::new(0.0, self.height),
            Vec2::new(self.screen_size.x(), self.height),
            theme.separator,
            1.0,
        );

        // Make menu bar items not have background by default (only on hover/active)
        let original_button_normal = ui.style.button_normal;
        ui.style.button_normal = Color::TRANSPARENT;

        // Internal padding for non-menu items
        let padding = 4.0;

        // No padding between menu items - menu bar should be tight
        let menu_item_width = 50.0;
        let button_height = self.height;
        let mut cursor = Vec2::new(0.0, 0.0); // Start from left edge

        // === FILE MENU ===
        let file_bounds =
            Rect2D::from_origin_size(cursor, Vec2::new(menu_item_width, button_height));
        ui.menu_bar_dropdown(
            "file_menu",
            "File",
            file_bounds,
            &mut self.state.file_menu_open,
            |ui, open| {
                if ui.menu_item_clicked("New Scene") {
                    // TODO: Implement new scene
                    *open = false;
                }
                if ui.menu_item_clicked("Open...") {
                    // TODO: Implement open scene
                    *open = false;
                }
                if ui.menu_item_clicked("Save") {
                    // TODO: Implement save scene
                    *open = false;
                }
                ui.menu_separator();
                if ui.menu_item_clicked("Quit") {
                    *open = false;
                }
            },
        );
        cursor = Vec2::new(cursor.x() + menu_item_width, cursor.y());

        // === EDIT MENU ===
        let edit_bounds =
            Rect2D::from_origin_size(cursor, Vec2::new(menu_item_width, button_height));
        ui.menu_bar_dropdown(
            "edit_menu",
            "Edit",
            edit_bounds,
            &mut self.state.edit_menu_open,
            |ui, open| {
                if ui.menu_item_clicked("Undo") {
                    // TODO: Implement undo
                    *open = false;
                }
                if ui.menu_item_clicked("Redo") {
                    // TODO: Implement redo
                    *open = false;
                }
                ui.menu_separator();
                if ui.menu_item_clicked("Preferences...") {
                    self.state
                        .pending_actions
                        .push(EditorAction::OpenPanel(Panel::Preferences));
                    *open = false;
                }
            },
        );
        cursor = Vec2::new(cursor.x() + menu_item_width, cursor.y());

        // === VIEW MENU ===
        let view_bounds =
            Rect2D::from_origin_size(cursor, Vec2::new(menu_item_width, button_height));
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
            },
        );
        cursor = Vec2::new(cursor.x() + menu_item_width, cursor.y());

        // === CREATE MENU ===
        let create_bounds = Rect2D::from_origin_size(cursor, Vec2::new(60.0, button_height));
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
        cursor = Vec2::new(cursor.x() + 60.0 + padding, cursor.y());

        // === HELP MENU ===
        let help_bounds =
            Rect2D::from_origin_size(cursor, Vec2::new(menu_item_width, button_height));
        ui.menu_bar_dropdown(
            "help_menu",
            "Help",
            help_bounds,
            &mut self.state.help_menu_open,
            |ui, open| {
                if ui.menu_item_clicked("About") {
                    *open = false;
                }
            },
        );
        cursor = Vec2::new(cursor.x() + menu_item_width, cursor.y());

        // Separator line before play controls
        cursor = Vec2::new(cursor.x() + padding * 2.0, cursor.y());
        ui.draw_line(
            Vec2::new(cursor.x(), padding),
            Vec2::new(cursor.x(), self.height - padding),
            theme.separator,
            1.0,
        );
        cursor = Vec2::new(cursor.x() + padding * 2.0, cursor.y());

        // Title in center (only show if there's enough space)
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

        // Restore original button style
        ui.style.button_normal = original_button_normal;
        Response::default()
    }
}
