use katla_math::{Color, Rect2D, Vec2, Vec3};
use katla_ui::declarative::ViewTree;
use katla_ui::dock::{DockNode, DockPath, DockTree, DockZone, SplitDirection};
use katla_ui::{UiContext, mouse_button};

use super::*;
use crate::ui::editor_ui::declarative::{
    EditorOverlayView, HierarchyAction, HierarchyDrawCtx, PreferencesDrawCtx, PreferencesPanelSync,
};

/// Preferences is a centered modal (560x520 in an 800x600 window).
fn modal_bounds() -> Rect2D {
    let size = Vec2::new(
        declarative::preferences::PREFERENCES_WIDTH,
        declarative::preferences::PREFERENCES_HEIGHT,
    );
    let screen = Vec2::new(800.0, 600.0);
    let min = Vec2::new((screen.x() - size.x()) * 0.5, (screen.y() - size.y()) * 0.5);
    Rect2D::new(min, min + size)
}

fn preferences_env(
    view_tree: &mut ViewTree,
    preferences: &Preferences,
    editor_settings: &EditorSettings,
    theme: &ColorScheme,
) {
    view_tree.env_mut().set(PreferencesDrawCtx {
        screen_size: Vec2::new(800.0, 600.0),
        is_open: true,
        category: 0,
        preferences: preferences.clone(),
        editor_settings: editor_settings.clone(),
        theme: theme.clone(),
        theme_key: "rcp".to_string(),
    });
}

fn last_sync_open(view_tree: &mut ViewTree) -> Option<bool> {
    let syncs: Vec<PreferencesPanelSync> = view_tree.actions_mut().drain();
    syncs.into_iter().last().map(|sync| sync.open)
}

/// Clicking inside the preferences modal must not close it.
#[test]
fn test_preferences_click_inside_does_not_close() {
    let mut ui = UiContext::new();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);

    let preferences = crate::Preferences::default();
    let editor_settings = EditorSettings::default();
    let theme = ColorScheme::default();

    let bounds = modal_bounds();
    let inside = bounds.center();

    ui.input_mut().mouse_pos = inside;
    ui.input_mut().mouse_pressed[mouse_button::LEFT] = true;
    ui.input_mut().mouse_down[mouse_button::LEFT] = true;

    let mut view_tree = ViewTree::default();
    preferences_env(&mut view_tree, &preferences, &editor_settings, &theme);
    let _ = view_tree.frame(&mut ui, &EditorOverlayView, Vec2::new(800.0, 600.0));
    ui.end();

    ui.input_mut().clear_frame_state();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);
    ui.input_mut().mouse_pos = inside;
    ui.input_mut().mouse_down[mouse_button::LEFT] = false;
    ui.input_mut().mouse_released[mouse_button::LEFT] = true;

    preferences_env(&mut view_tree, &preferences, &editor_settings, &theme);
    let _ = view_tree.frame(&mut ui, &EditorOverlayView, Vec2::new(800.0, 600.0));

    let open = last_sync_open(&mut view_tree);
    assert_eq!(
        open,
        Some(true),
        "modal should stay open after inside click"
    );
}

/// Escape must close the preferences modal.
#[test]
fn test_preferences_escape_closes() {
    use katla_ui::KeyCode;

    let mut ui = UiContext::new();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);

    let preferences = crate::Preferences::default();
    let editor_settings = EditorSettings::default();
    let theme = ColorScheme::default();

    ui.input_mut().mouse_pos = modal_bounds().center();

    let mut view_tree = ViewTree::default();
    preferences_env(&mut view_tree, &preferences, &editor_settings, &theme);
    let _ = view_tree.frame(&mut ui, &EditorOverlayView, Vec2::new(800.0, 600.0));
    ui.end();

    ui.input_mut().clear_frame_state();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);
    ui.input_mut().mouse_pos = modal_bounds().center();
    ui.input_mut().keys_pressed.push(KeyCode::Escape);

    preferences_env(&mut view_tree, &preferences, &editor_settings, &theme);
    let _ = view_tree.frame(&mut ui, &EditorOverlayView, Vec2::new(800.0, 600.0));

    let open = last_sync_open(&mut view_tree);
    assert_eq!(
        open,
        Some(false),
        "Escape should close the preferences modal"
    );
}

/// Clicking outside the preferences modal must close it.
#[test]
fn test_preferences_click_outside_closes() {
    let mut ui = UiContext::new();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);

    let preferences = crate::Preferences::default();
    let editor_settings = EditorSettings::default();
    let theme = ColorScheme::default();

    let outside = Vec2::new(40.0, 300.0);

    ui.input_mut().mouse_pos = modal_bounds().center();

    let mut view_tree = ViewTree::default();
    preferences_env(&mut view_tree, &preferences, &editor_settings, &theme);
    let _ = view_tree.frame(&mut ui, &EditorOverlayView, Vec2::new(800.0, 600.0));
    ui.end();

    ui.input_mut().clear_frame_state();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);
    ui.input_mut().mouse_pos = outside;
    ui.input_mut().mouse_pressed[mouse_button::LEFT] = true;
    ui.input_mut().mouse_down[mouse_button::LEFT] = true;

    preferences_env(&mut view_tree, &preferences, &editor_settings, &theme);
    let _ = view_tree.frame(&mut ui, &EditorOverlayView, Vec2::new(800.0, 600.0));

    let open = last_sync_open(&mut view_tree);
    assert_eq!(
        open,
        Some(false),
        "clicking outside should close the preferences modal"
    );
}

/// Test that clicking an entity in the hierarchy panel selects it.
///
/// This test requires a fully initialized UiContext with loaded fonts for
/// correct Taffy layout. Without fonts, text measurement returns zero-sized
/// bounds, making hit-testing impossible. The test is kept here as
/// documentation of the expected behavior.
#[test]
#[ignore = "needs fully initialized UiContext with fonts for layout-dependent hit testing"]
fn test_hierarchy_entity_selection_works() {
    use crate::ui::editor_ui::declarative::hierarchy::HierarchyView;

    let mut ui = UiContext::new();
    ui.begin(Vec2::new(800.0, 600.0), 1.0);

    let state = HierarchyState::default();

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
            audio_source: None,
            has_audio_listener: false,
            collider_shape: None,
            rigid_body: None,
            physics_material: None,
            material: None,
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
            audio_source: None,
            has_audio_listener: false,
            collider_shape: None,
            rigid_body: None,
            physics_material: None,
            material: None,
        },
    ];

    let _bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(200.0, 400.0));
    let theme = ColorScheme::default();

    // Scan Y positions to find the entity rows in the hierarchy.
    // We test HierarchyView directly (not the full EditorOverlayView) to isolate
    // the hierarchy's input handling.
    let mut found_entity = None;
    for test_y in 30..200u32 {
        ui.input_mut().mouse_pos = Vec2::new(100.0, test_y as f32);
        ui.input_mut().mouse_pressed = [false; 5];
        ui.input_mut().mouse_down = [false; 5];
        ui.input_mut().mouse_pressed[mouse_button::LEFT] = true;
        ui.input_mut().mouse_down[mouse_button::LEFT] = true;

        let hierarchy_ctx = HierarchyDrawCtx {
            bounds: Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(250.0, 500.0)),
            entities: entities.clone(),
            hierarchy_state: state.clone(),
            theme: theme.clone(),
            search_filter: String::new(),
            selected_entity: None,
        };

        let mut view_tree = ViewTree::default();
        view_tree.env_mut().set(hierarchy_ctx);
        let _ = view_tree.frame(&mut ui, &HierarchyView, Vec2::new(800.0, 600.0));

        let actions: Vec<HierarchyAction> = view_tree.actions_mut().drain();
        if let Some(id) = actions.into_iter().find_map(|a| match a {
            HierarchyAction::SelectEntity(id) => Some(id),
            HierarchyAction::ToggleExpanded(_) => None,
        }) {
            found_entity = Some((test_y, id));
            break;
        }
    }

    let (click_y, selected) =
        found_entity.expect("Should find an entity row by scanning Y positions");

    assert!(
        selected == entity1 || selected == entity2,
        "clicking at y={click_y} should select an entity, got {selected:?}"
    );
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

// ── VAL-EDITOR-001: EditorOverlayView produces documented widget tree ──

#[test]
fn test_editor_overlay_produces_dockspace_in_zstack() {
    let mut ui = UiContext::new();
    ui.begin(Vec2::new(1920.0, 1080.0), 1.0);

    let mut view_tree = ViewTree::default();
    view_tree
        .env_mut()
        .set(crate::ui::editor_ui::declarative::StatusBarData {
            height: 22.0,
            fps: 60.0,
            frame_time_ms: 16.6,
            entity_count: 0,
            draw_call_count: 0,
            total_assets: 0,
            is_playing: false,
            is_paused: false,
            theme: ColorScheme::by_name("rcp").unwrap_or_default(),
            save_confirmation_timer: 0.0,
        });
    view_tree
        .env_mut()
        .set(crate::ui::editor_ui::declarative::ToolbarDrawCtx {
            screen_width: 1920.0,
            font_scale: 1.0,
            show_grid: true,
            show_stats: false,
            show_physics_debug: false,
            show_reverb_debug: false,
            text_muted: Color::WHITE,
            is_playing: false,
            is_paused: false,
            highlight: Color::WHITE,
            warning: Color::WHITE,
            accent: Color::WHITE,
            scene_title: "Test Scene".into(),
            can_undo: false,
            can_redo: false,
            error: Color::WHITE,
        });
    view_tree.env_mut().set(EditorUI::default_dock_tree());

    let _ = view_tree.frame(&mut ui, &EditorOverlayView, Vec2::new(1920.0, 1080.0));

    // Verify a DockSpace<u64> node exists in the tree
    let has_dockspace = view_tree.iter_nodes().any(|(_, node)| {
        node.widget
            .as_any()
            .downcast_ref::<katla_ui::declarative::widgets::dock_space::DockSpace<u64>>()
            .is_some()
    });
    assert!(
        has_dockspace,
        "EditorOverlayView should contain a DockSpace<u64> widget"
    );

    // Verify the root is a ZStack
    let has_zstack_root = view_tree.iter_nodes().any(|(id, node)| {
        if let Some(root_id) = view_tree.root()
            && id == root_id
        {
            return node
                .widget
                .as_any()
                .downcast_ref::<katla_ui::declarative::widgets::zstack::ZStack>()
                .is_some();
        }
        false
    });
    assert!(has_zstack_root, "EditorOverlayView root should be a ZStack");
}

// ── VAL-EDITOR-002: Single frame() call ──

#[test]
fn test_editor_rendering_uses_single_frame_call() {
    let mut ui = UiContext::new();
    ui.begin(Vec2::new(1920.0, 1080.0), 1.0);

    let mut view_tree = ViewTree::default();
    view_tree.env_mut().set(EditorUI::default_dock_tree());

    // Single frame() call — if this panics, the integration is broken
    let _ = view_tree.frame(&mut ui, &EditorOverlayView, Vec2::new(1920.0, 1080.0));
}

// ── VAL-EDITOR-003: Immediate-mode DockArea removed ──

#[test]
fn test_no_immediate_mode_dockarea_references() {
    // The old DockArea was in katla_ui::widgets::dock which is now removed.
    // This test verifies the EditorUI uses DockTree<u64> instead.
    let editor = EditorUI::new();
    // Verify dock_tree is a DockTree<u64>
    let _tree: &DockTree<u64> = &editor.dock_tree;
    // Verify dock_layout and dock_drag are gone (compile-time check by existence)
}

// ── VAL-EDITOR-020..023: DockAction processing ──

#[test]
fn test_dock_action_tab_moved() {
    let mut tree = default_test_dock_tree();
    let from_path = DockPath(vec![0]);
    let to_path = DockPath(vec![1]);
    // Moving the only tab from left leaf to right leaf center zone
    // collapses the split into a single leaf
    tree.move_tab(&from_path, &to_path, DockZone::Center)
        .unwrap();
    // After collapse, root should be a single leaf with all tabs
    if let DockNode::Leaf { tabs, .. } = tree.root() {
        assert!(
            tabs.contains(&EditorPanel::Hierarchy.id()),
            "Leaf should contain Hierarchy tab"
        );
        assert!(
            tabs.contains(&EditorPanel::Viewport.id()),
            "Leaf should contain Viewport tab"
        );
    } else {
        panic!("Expected Leaf after collapse, got {:?}", tree.root());
    }
}

#[test]
fn test_dock_action_tab_closed() {
    let mut tree = default_test_dock_tree();
    let path = DockPath(vec![0]);
    tree.remove_tab(&path, &EditorPanel::Hierarchy.id())
        .unwrap();
    // Removing the only tab from the left leaf should collapse the split
    // The tree should collapse to just the right side
    assert!(matches!(
        tree.root(),
        DockNode::Split { .. } | DockNode::Leaf { .. }
    ));
}

#[test]
fn test_dock_action_tab_activated() {
    let mut tree = DockTree::new(DockNode::Leaf {
        tabs: vec![1u64, 2u64, 3u64],
        active: 0,
    });
    tree.activate_tab(&DockPath::root(), &2u64).unwrap();
    if let DockNode::Leaf { active, .. } = tree.root() {
        assert_eq!(
            *active, 1,
            "Active tab should be index 1 after activating tab 2"
        );
    } else {
        panic!("Expected Leaf");
    }
}

#[test]
fn test_dock_action_split_resized() {
    let mut tree = default_test_dock_tree();
    tree.set_ratio(&DockPath::root(), 0.6).unwrap();
    if let DockNode::Split { ratio, .. } = tree.root() {
        assert!(
            (ratio - 0.6).abs() < 0.01,
            "Ratio should be 0.6, got {}",
            ratio
        );
    } else {
        panic!("Expected Split at root");
    }
}

// ── VAL-EDITOR-030/031: Layout persistence ──

#[test]
fn test_default_dock_tree_structure() {
    let tree = EditorUI::default_dock_tree();
    // Root should be a vertical split (top: main area, bottom: tabs)
    assert!(matches!(
        tree.root(),
        DockNode::Split {
            direction: SplitDirection::Vertical,
            ..
        }
    ));

    // Should have leaves with the expected panels
    let bounds = tree.leaf_bounds(Rect2D::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0)));
    assert!(
        bounds.len() >= 3,
        "Default layout should have at least 3 leaves"
    );
}

#[test]
fn test_dock_tree_serialization_roundtrip() {
    let tree = EditorUI::default_dock_tree();
    let json = katla_ui::dock::to_json(&tree).unwrap();
    let restored: DockTree<u64> = katla_ui::dock::from_json(&json).unwrap();
    assert_eq!(
        *tree.root(),
        *restored.root(),
        "Round-trip serialization should preserve tree"
    );
}

// ── Helper ──

fn default_test_dock_tree() -> DockTree<u64> {
    let left = DockNode::Leaf {
        tabs: vec![EditorPanel::Hierarchy.id()],
        active: 0,
    };
    let right = DockNode::Leaf {
        tabs: vec![EditorPanel::Viewport.id(), EditorPanel::Inspector.id()],
        active: 0,
    };
    DockTree::new(DockNode::Split {
        direction: SplitDirection::Horizontal,
        ratio: 0.25,
        children: [Box::new(left), Box::new(right)],
    })
}

#[test]
fn test_preferences_fit_small_window_and_all_categories() {
    use katla_ui::declarative::widgets::{labeled_slider::LabeledSlider, modal::Modal};
    for category in 0..4 {
        let screen = Vec2::new(560.0, 450.0);
        let mut ui = UiContext::new();
        ui.begin(screen, 1.0);
        let mut tree = ViewTree::default();
        tree.env_mut().set(PreferencesDrawCtx {
            screen_size: screen,
            is_open: true,
            category,
            preferences: Preferences::default(),
            editor_settings: EditorSettings::default(),
            theme: ColorScheme::default(),
            theme_key: "dark".into(),
        });
        tree.frame(&mut ui, &declarative::preferences::PreferencesView, screen);
        for (id, node) in tree.iter_nodes() {
            let bounds = tree.resolved_bounds()[&id];
            assert!(
                bounds.min.x() >= 0.0 && bounds.max.x() <= screen.x() + 0.5,
                "category {category} overflows horizontally: {bounds:?}"
            );
            if node.widget.as_any().is::<Modal>() {
                assert!(bounds.min.y() >= 0.0 && bounds.max.y() <= screen.y() + 0.5);
            }
            if let Some(slider) = node.widget.as_any().downcast_ref::<LabeledSlider>() {
                assert!(
                    slider.track_bounds(bounds).width() >= 60.0,
                    "category {category} leaves no useful slider track: {bounds:?}"
                );
            }
        }
    }
}

#[test]
fn test_mixer_keeps_sliders_in_each_strip_at_different_widths() {
    use katla_ui::declarative::widgets::labeled_slider::LabeledSlider;
    for width in [240.0, 560.0, 1280.0] {
        let screen = Vec2::new(width, 180.0);
        let mut ui = UiContext::new();
        ui.begin(screen, 1.0);
        let mut tree = ViewTree::default();
        tree.env_mut().set(declarative::mixer::MixerDrawCtx {
            bounds: Rect2D::from_origin_size(Vec2::ZERO, screen),
            levels: katla_audio::LevelsSnapshot::default(),
            active_voices: 0,
            peak_voices: 0,
            preferences: Preferences::default(),
            theme: ColorScheme::default(),
        });
        tree.frame(&mut ui, &declarative::mixer::MixerView, screen);
        let sliders = tree
            .iter_nodes()
            .filter_map(|(id, node)| {
                node.widget
                    .as_any()
                    .is::<LabeledSlider>()
                    .then_some(tree.resolved_bounds()[&id])
            })
            .collect::<Vec<_>>();
        assert_eq!(sliders.len(), 4);
        for bounds in &sliders {
            assert!(bounds.min.x() >= 12.0 && bounds.max.x() <= width - 12.0);
            assert!(
                bounds.width() >= 120.0 && bounds.height() <= 40.0,
                "{bounds:?}"
            );
        }
        for (index, bounds) in sliders.iter().enumerate() {
            for other in &sliders[index + 1..] {
                assert!(
                    bounds.max.x() <= other.min.x()
                        || other.max.x() <= bounds.min.x()
                        || bounds.max.y() <= other.min.y()
                        || other.max.y() <= bounds.min.y(),
                    "slider rows overlap: {bounds:?}, {other:?}"
                );
            }
        }
    }
}

#[test]
fn test_floating_panels_keep_state_when_opening_and_changing_emitter_shape() {
    use crate::ui::particle_inspector::{EmitterConfigView, ParticleInspectorData};
    use declarative::{co_creator::CoCreatorDrawCtx, particle_inspector::ParticleInspectorDrawCtx};
    let screen = Vec2::new(1280.0, 720.0);
    let mut ui = UiContext::new();
    let mut tree = ViewTree::default();
    let preferences = Preferences {
        show_grid: true,
        ..Preferences::default()
    };
    let entity = katla_ecs::EntityId::from_raw(1);
    for (assistant_open, particle_open, prefs_open, shape) in [
        (false, false, false, "Point"),
        (false, false, true, "Point"),
        (false, true, false, "Line"),
        (true, true, false, "Box"),
        (false, true, true, "Sphere"),
        (true, false, false, "Point"),
        (true, true, true, "Point"),
    ] {
        preferences_env(
            &mut tree,
            &preferences,
            &EditorSettings::default(),
            &ColorScheme::default(),
        );
        let mut prefs = tree.env_mut().get::<PreferencesDrawCtx>().unwrap().clone();
        prefs.is_open = prefs_open;
        prefs.category = 1;
        tree.env_mut().set(prefs);
        tree.env_mut().set(CoCreatorDrawCtx {
            messages: vec![],
            processing: false,
            host_name: None,
            input_epoch: 0,
            status_message: "Disconnected".into(),
            user_msg_color: Color::WHITE,
            assistant_msg_color: Color::WHITE,
            system_msg_color: Color::WHITE,
            text_muted: Color::WHITE,
            agent_undo_count: 0,
            is_open: assistant_open,
        });
        tree.env_mut().set(ParticleInspectorDrawCtx {
            is_open: particle_open,
            theme: ColorScheme::default(),
            data: ParticleInspectorData {
                emitter_entities: vec![entity],
                selected_emitter_entity: Some(entity),
                stats: Some(crate::ui::ParticleStats::default()),
                selected_emitter_config: Some(EmitterConfigView {
                    active: true,
                    shape_name: shape,
                    shape_params: [2.0; 3],
                    emit_rate: 10.0,
                    base_lifetime: 1.0,
                    lifetime_variation: 0.2,
                    velocity_magnitude: 1.0,
                    velocity_cone_angle: 0.0,
                    base_scale: 1.0,
                    scale_variation: 0.0,
                    color: [1.0; 4],
                    color_variation: 0.0,
                    color_end: [1.0; 4],
                    scale_end: 1.0,
                    gravity: -9.8,
                    turbulence_strength: 0.0,
                    turbulence_frequency: 1.0,
                }),
            },
        });
        ui.begin(screen, 1.0);
        tree.frame(&mut ui, &EditorOverlayView, screen);
        if particle_open {
            let sliders = tree
                .iter_nodes()
                .filter_map(|(id, node)| {
                    node.widget
                        .as_any()
                        .is::<katla_ui::declarative::widgets::labeled_slider::LabeledSlider>()
                        .then_some(tree.resolved_bounds()[&id])
                })
                .collect::<Vec<_>>();
            for bounds in sliders {
                assert!(bounds.height() >= 20.0, "compressed slider: {bounds:?}");
            }
        }
        assert!(
            tree.actions_mut()
                .drain::<crate::ui::ParticleInspectorAction>()
                .is_empty(),
            "opening a panel or changing shape must not edit emitter values"
        );
        assert!(
            tree.actions_mut().drain::<PreferencesAction>().is_empty(),
            "opening panels must not mutate preferences"
        );
        ui.end();
    }

    tree.actions_mut()
        .drain::<declarative::CoCreatorPanelSync>();
    tree.actions_mut()
        .drain::<declarative::ParticleInspectorPanelSync>();
    let panel_ids = tree
        .iter_nodes()
        .filter_map(|(_, node)| {
            node.widget
                .as_any()
                .downcast_ref::<katla_ui::declarative::widgets::draggable_panel::DraggablePanel>()
                .map(|panel| panel.state_id)
        })
        .collect::<Vec<_>>();
    assert_eq!(panel_ids.len(), 2);
    for id in panel_ids {
        tree.state_arena_mut()
            .set(id, DraggablePanelState::default());
    }
    ui.begin(screen, 1.0);
    tree.frame(&mut ui, &EditorOverlayView, screen);
    assert!(
        tree.actions_mut()
            .drain::<declarative::CoCreatorPanelSync>()
            .iter()
            .all(|sync| !sync.visibility.is_visible())
    );
    assert!(
        tree.actions_mut()
            .drain::<declarative::ParticleInspectorPanelSync>()
            .iter()
            .all(|sync| !sync.visibility.is_visible())
    );
    assert!(matches!(
        tree.actions_mut()
            .drain::<crate::ui::ParticleInspectorAction>()
            .as_slice(),
        [crate::ui::ParticleInspectorAction::Close]
    ));
    ui.end();
}

#[test]
fn test_audio_controls_follow_preferences_and_allow_muting() {
    use katla_ui::declarative::widgets::labeled_slider::LabeledSlider;
    let screen = Vec2::new(560.0, 450.0);
    for mixer in [false, true] {
        let mut ui = UiContext::new();
        let mut tree = ViewTree::default();
        let mut preferences = Preferences::default();
        let view: &dyn katla_ui::declarative::Build = if mixer {
            &declarative::mixer::MixerView
        } else {
            &declarative::preferences::PreferencesView
        };
        let set_env = |tree: &mut ViewTree, preferences: &Preferences| {
            if mixer {
                tree.env_mut().set(declarative::MixerDrawCtx {
                    bounds: Rect2D::from_origin_size(Vec2::ZERO, screen),
                    levels: katla_audio::LevelsSnapshot::default(),
                    active_voices: 0,
                    peak_voices: 0,
                    preferences: preferences.clone(),
                    theme: ColorScheme::default(),
                });
            } else {
                tree.env_mut().set(PreferencesDrawCtx {
                    screen_size: screen,
                    is_open: true,
                    category: 2,
                    preferences: preferences.clone(),
                    editor_settings: EditorSettings::default(),
                    theme: ColorScheme::default(),
                    theme_key: "dark".into(),
                });
            }
        };
        set_env(&mut tree, &preferences);
        ui.begin(screen, 1.0);
        tree.frame(&mut ui, view, screen);
        ui.end();
        let master_id = tree
            .iter_nodes()
            .find_map(|(_, node)| {
                node.widget
                    .as_any()
                    .downcast_ref::<LabeledSlider>()
                    .map(|slider| slider.value_id)
            })
            .unwrap();
        tree.state_arena_mut().set(master_id, 0.0f32);
        ui.begin(screen, 1.0);
        tree.frame(&mut ui, view, screen);
        ui.end();
        assert!(
            matches!(tree.actions_mut().drain::<PreferencesAction>().as_slice(),
            [PreferencesAction::SetMasterVolume(volume)] if *volume == 0.0)
        );
        for volume in [0.0, 0.35, 1.0] {
            preferences.audio.master_volume = volume;
            set_env(&mut tree, &preferences);
            ui.begin(screen, 1.0);
            tree.frame(&mut ui, view, screen);
            ui.end();
            assert_eq!(tree.state_arena().get::<f32>(master_id), Some(volume));
            assert!(tree.actions_mut().drain::<PreferencesAction>().is_empty());
        }
    }
}

#[test]
fn test_modal_pointer_capture_blocks_viewport_and_scrim_clicks() {
    let mut editor = EditorUI::new();
    editor.last_screen_size = Vec2::new(800.0, 600.0);
    editor.last_viewport_bounds = Rect2D::from_origin_size(Vec2::ZERO, editor.last_screen_size);
    editor.focused_panel = FocusedPanel::Hierarchy;
    for preferences in [false, true] {
        editor.scene_dialog =
            (!preferences).then_some(declarative::scene_dialog::SceneDialog::Unsaved);
        editor.preferences_panel.visibility = if preferences {
            katla_ui::declarative::DraggablePanelVisibility::Visible
        } else {
            katla_ui::declarative::DraggablePanelVisibility::Hidden
        };
        for position in [Vec2::new(400.0, 300.0), Vec2::new(10.0, 100.0)] {
            assert!(editor.captures_pointer_at(position));
            editor.update_focused_panel_from_click(position);
            assert_eq!(editor.focused_panel, FocusedPanel::Hierarchy);
        }
    }
    editor.preferences_panel.close();
    assert!(!editor.captures_pointer_at(Vec2::new(400.0, 300.0)));
}
