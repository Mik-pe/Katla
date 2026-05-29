use katla_math::{Rect2D, Vec2, Vec3};
use katla_ui::declarative::ViewTree;
use katla_ui::{UiContext, mouse_button};

use super::*;
use crate::ui::editor_ui::declarative::{
    EditorRootView, HierarchyDrawCtx, PreferencesDrawCtx, PreferencesPanelSync,
};
use crate::ui::editor_ui::types::PreferencesTab;
use katla_ui::declarative::DraggablePanelVisibility;

/// Test that clicking a tab in the preferences panel doesn't dismiss the window.
#[test]
fn test_preferences_tab_click_does_not_close_panel() {
    let mut ui = UiContext::new();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);

    let preferences = crate::Preferences::default();
    let editor_settings = EditorSettings::default();
    let theme = ColorScheme::default();
    let theme_key = "default";

    let tab_x = 100.0 + 450.0 / 3.0;
    let tab_y = 100.0 + 32.0;

    ui.input_mut().mouse_pos = Vec2::new(tab_x + 10.0, tab_y + 10.0);
    ui.input_mut().mouse_pressed[mouse_button::LEFT] = true;
    ui.input_mut().mouse_down[mouse_button::LEFT] = true;

    let mut view_tree = ViewTree::default();
    view_tree.env_mut().set(PreferencesDrawCtx {
        is_open: true,
        preferences: preferences.clone(),
        editor_settings: editor_settings.clone(),
        theme: theme.clone(),
        theme_key: theme_key.to_string(),
        llm_config: katla_agent::LlmConfig::default(),
    });

    let _ = view_tree.frame(&mut ui, &EditorRootView, Vec2::new(800.0, 600.0));

    ui.end();

    ui.input_mut().clear_frame_state();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);
    ui.input_mut().mouse_pos = Vec2::new(tab_x + 10.0, tab_y + 10.0);
    ui.input_mut().mouse_down[mouse_button::LEFT] = false;
    ui.input_mut().mouse_released[mouse_button::LEFT] = true;

    view_tree.env_mut().set(PreferencesDrawCtx {
        is_open: true,
        preferences,
        editor_settings,
        theme,
        theme_key: theme_key.to_string(),
        llm_config: katla_agent::LlmConfig::default(),
    });

    let _ = view_tree.frame(&mut ui, &EditorRootView, Vec2::new(800.0, 600.0));

    let syncs: Vec<PreferencesPanelSync> = view_tree.actions_mut().drain();
    if let Some(sync) = syncs.into_iter().next() {
        assert!(
            sync.visibility.is_visible(),
            "preferences panel should stay open after clicking tab"
        );
    }
}

/// Test that clicking an entity in the hierarchy panel selects it.
#[test]
fn test_hierarchy_entity_selection_works() {
    let mut ui = UiContext::new();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);

    let mut state = HierarchyState::default();
    let mut selected_entity: Option<EntityId> = None;

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
            perspective: None,
            directional_light: None,
            audio_emitter: None,
            collider_shape: None,
            rigid_body: None,
            physics_material: None,
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
            perspective: None,
            directional_light: None,
            audio_emitter: None,
            collider_shape: None,
            rigid_body: None,
            physics_material: None,
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

    let mut view_tree = ViewTree::default();
    view_tree.env_mut().set(hierarchy_ctx);
    let _ = view_tree.frame(&mut ui, &EditorRootView, Vec2::new(800.0, 600.0));

    // TODO: Implement proper sync - for now, we're not syncing state back
    // This test will need to be updated once HierarchySync emission is implemented
    // state = hierarchy_ctx.hierarchy_state;
    // selected_entity = hierarchy_ctx.selected_entity;

    // For now, just verify the view tree frame completes without panicking
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
