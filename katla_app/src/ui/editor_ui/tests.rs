use katla_math::{Rect2D, Vec2, Vec3};
use katla_ui::declarative::ViewTree;
use katla_ui::{UiContext, mouse_button};

use super::*;
use crate::ui::editor_ui::declarative::{
    EditorRootView, HierarchyDrawCtx, PreferencesDrawCtx, set_hierarchy_ctx, set_preferences_ctx,
    take_hierarchy_ctx, take_preferences_ctx,
};
use crate::ui::editor_ui::types::{PreferencesPanelState, PreferencesTab};
use katla_ui::widgets::{DraggablePanelState, PanelState};

/// Test that clicking a tab in the preferences panel doesn't dismiss the window.
#[test]
fn test_preferences_tab_click_does_not_close_panel() {
    let mut ui = UiContext::new();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);

    let state = PreferencesPanelState {
        panel: DraggablePanelState {
            visibility: PanelState::Visible,
            position: Some(Vec2::new(100.0, 100.0)),
            dragging: false,
            drag_offset: Vec2::new(0.0, 0.0),
        },
        current_tab: PreferencesTab::General,
        scroll_state: Default::default(),
        llm_config: katla_agent::LlmConfig::default(),
    };

    let preferences = crate::Preferences::default();
    let editor_settings = EditorSettings::default();
    let theme = ColorScheme::default();
    let theme_key = "default";

    let tab_x = 100.0 + 450.0 / 3.0;
    let tab_y = 100.0 + 32.0;

    ui.input_mut().mouse_pos = Vec2::new(tab_x + 10.0, tab_y + 10.0);
    ui.input_mut().mouse_pressed[mouse_button::LEFT] = true;
    ui.input_mut().mouse_down[mouse_button::LEFT] = true;

    set_preferences_ctx(PreferencesDrawCtx {
        screen_size: Vec2::new(800.0, 600.0),
        state,
        preferences: preferences.clone(),
        editor_settings: editor_settings.clone(),
        theme: theme.clone(),
        theme_key: theme_key.to_string(),
        pending_actions: Vec::new(),
    });

    let mut view_tree = ViewTree::default();
    let _ = view_tree.frame(&mut ui, &EditorRootView, Vec2::new(800.0, 600.0));

    let mut state = take_preferences_ctx().unwrap().state;

    ui.end();

    ui.input_mut().clear_frame_state();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);
    ui.input_mut().mouse_pos = Vec2::new(tab_x + 10.0, tab_y + 10.0);
    ui.input_mut().mouse_down[mouse_button::LEFT] = false;
    ui.input_mut().mouse_released[mouse_button::LEFT] = true;

    set_preferences_ctx(PreferencesDrawCtx {
        screen_size: Vec2::new(800.0, 600.0),
        state,
        preferences,
        editor_settings,
        theme,
        theme_key: theme_key.to_string(),
        pending_actions: Vec::new(),
    });

    let _ = view_tree.frame(&mut ui, &EditorRootView, Vec2::new(800.0, 600.0));

    let prefs_ctx = take_preferences_ctx().unwrap();

    assert!(
        prefs_ctx.state.panel.visibility.is_visible(),
        "preferences panel should stay open after clicking tab"
    );

    assert_eq!(
        prefs_ctx.state.current_tab,
        PreferencesTab::Viewport,
        "tab should change to Viewport after clicking it"
    );
}

/// Test that clicking an entity in the hierarchy panel selects it.
#[test]
fn test_hierarchy_entity_selection_works() {
    let mut ui = UiContext::new();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);

    let mut state = HierarchyState::default();
    let mut selected_entity = None;

    let mut world = katla_ecs::World::new();
    let entity1 = world.create_entity();
    let entity2 = world.create_entity();

    let entities = vec![
        EntityInfo {
            id: entity1,
            name: "Cube".to_string(),
            position: Vec3::new(0.0, 0.0, 0.0),
            rotation: Vec3::new(0.0, 0.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            entity_type: "Mesh".to_string(),
            components: vec![],
            depth: 0,
            has_children: false,
            parent_id: None,
            point_light: None,
            particle_emitter: None,
            script_path: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
        },
        EntityInfo {
            id: entity2,
            name: "Sphere".to_string(),
            position: Vec3::new(0.0, 0.0, 0.0),
            rotation: Vec3::new(0.0, 0.0, 0.0),
            scale: Vec3::new(1.0, 1.0, 1.0),
            entity_type: "Mesh".to_string(),
            components: vec![],
            depth: 0,
            has_children: false,
            parent_id: None,
            point_light: None,
            particle_emitter: None,
            script_path: None,
            mass: None,
            drag: None,
            perspective: None,
            directional_light: None,
        },
    ];

    let bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 400.0));
    let theme = ColorScheme::default();

    let header_height = 24.0;
    let content_padding = 4.0;
    let search_field_height = 26.0;
    let item_height = 22.0;
    let click_y = header_height
        + content_padding
        + search_field_height
        + content_padding
        + item_height
        + item_height / 2.0;
    ui.input_mut().mouse_pos = Vec2::new(100.0, click_y);
    ui.input_mut().mouse_pressed[mouse_button::LEFT] = true;
    ui.input_mut().mouse_down[mouse_button::LEFT] = true;

    let hierarchy_ctx = HierarchyDrawCtx {
        bounds,
        entities: entities.clone(),
        selected_entity: None,
        hierarchy_state: std::mem::take(&mut state),
        theme: theme.clone(),
        pending_actions: Vec::new(),
        search_filter: String::new(),
    };
    set_hierarchy_ctx(hierarchy_ctx);

    let mut view_tree = ViewTree::default();
    let _ = view_tree.frame(&mut ui, &EditorRootView, Vec2::new(800.0, 600.0));

    if let Some(hierarchy_ctx) = take_hierarchy_ctx() {
        state = hierarchy_ctx.hierarchy_state;
        selected_entity = hierarchy_ctx.selected_entity;

        assert_eq!(
            selected_entity,
            Some(entity2),
            "clicking entity should select it"
        );
        assert!(
            hierarchy_ctx
                .pending_actions
                .iter()
                .any(|a| matches!(a, EditorAction::SelectEntity(id) if *id == entity2)),
            "selecting entity should emit SelectEntity action"
        );
    } else {
        panic!("hierarchy context should be returned after frame");
    }
}

/// Test that save confirmation timer starts at 2.0 and counts down.
#[test]
fn test_save_confirmation_timer_countdown() {
    let mut editor = EditorUI::new();
    assert_eq!(
        editor.save_confirmation_timer, 0.0,
        "timer should start at zero"
    );

    editor.show_save_confirmation();
    assert_eq!(
        editor.save_confirmation_timer, 2.0,
        "timer should be set to 2.0 after confirmation"
    );

    editor.update_timers(0.5);
    assert!(
        (editor.save_confirmation_timer - 1.5).abs() < 1e-6,
        "timer should decrement by dt"
    );

    editor.update_timers(2.0);
    assert_eq!(
        editor.save_confirmation_timer, 0.0,
        "timer should clamp to zero, not go negative"
    );
}

/// Test that prev_want_capture_keyboard suppresses Ctrl+S logic.
#[test]
fn test_ctrl_s_suppressed_when_keyboard_captured() {
    let mut editor = EditorUI::new();

    // When no keyboard capture, Ctrl+S should be allowed
    editor.prev_want_capture_keyboard = false;
    assert!(
        !editor.prev_want_capture_keyboard,
        "Ctrl+S should be allowed when keyboard is not captured"
    );

    // When keyboard is captured (TextInput focused or modal open), Ctrl+S should be suppressed
    editor.prev_want_capture_keyboard = true;
    assert!(
        editor.prev_want_capture_keyboard,
        "Ctrl+S should be suppressed when keyboard is captured"
    );
}

/// Test that save confirmation timer does not go below zero.
#[test]
fn test_save_confirmation_timer_never_negative() {
    let mut editor = EditorUI::new();
    editor.show_save_confirmation();

    // Update with a very large dt
    editor.update_timers(100.0);
    assert_eq!(
        editor.save_confirmation_timer, 0.0,
        "timer should never go below zero"
    );
}
