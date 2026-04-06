use super::*;

#[test]
fn test_id_generation() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    // Each call generates a unique ID due to counter increment
    let id1 = ctx.generate_id("test");
    let id2 = ctx.generate_id("test");
    let id3 = ctx.generate_id("other");

    // Same label produces DIFFERENT IDs (counter ensures uniqueness)
    assert_ne!(id1, id2, "same label should get different IDs");
    // Different labels also produce different IDs
    assert_ne!(id1, id3, "different labels should get different IDs");

    ctx.end();
}

#[test]
fn test_id_generation_consistent_across_frames() {
    let mut ctx = UiContext::new();

    // Frame 1
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    let frame1_id1 = ctx.generate_id("button");
    let frame1_id2 = ctx.generate_id("button");
    ctx.end();

    // Frame 2 - same call order should produce same IDs
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    let frame2_id1 = ctx.generate_id("button");
    let frame2_id2 = ctx.generate_id("button");
    ctx.end();

    // IDs should be consistent across frames (important for state persistence)
    assert_eq!(frame1_id1, frame2_id1, "first button ID should be stable");
    assert_eq!(frame1_id2, frame2_id2, "second button ID should be stable");
}

// === Popup Content Bounds Tracking Tests ===

/// Test that track_popup_item correctly expands bounds for a single item.
#[test]
fn test_track_popup_item_single() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    // Initially no bounds tracked
    assert!(ctx.popup_content_bounds.is_none());

    // Track a single item
    let item_bounds = Rect2D::from_origin_size(Vec2::new(100.0, 50.0), Vec2::new(150.0, 24.0));
    ctx.track_popup_item(item_bounds);

    // Bounds should match the item exactly
    let tracked = ctx.popup_content_bounds.unwrap();
    assert_eq!(tracked.min, Vec2::new(100.0, 50.0));
    assert_eq!(tracked.max, Vec2::new(250.0, 74.0));

    ctx.end();
}

/// Test that track_popup_item correctly expands bounds for multiple items.
#[test]
fn test_track_popup_item_multiple() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    // Track first item at (100, 50)
    let item1 = Rect2D::from_origin_size(Vec2::new(100.0, 50.0), Vec2::new(150.0, 24.0));
    ctx.track_popup_item(item1);

    // Track second item below first at (100, 74)
    let item2 = Rect2D::from_origin_size(Vec2::new(100.0, 74.0), Vec2::new(150.0, 24.0));
    ctx.track_popup_item(item2);

    // Track third item below second at (100, 98)
    let item3 = Rect2D::from_origin_size(Vec2::new(100.0, 98.0), Vec2::new(150.0, 24.0));
    ctx.track_popup_item(item3);

    // Bounds should encompass all items
    let tracked = ctx.popup_content_bounds.unwrap();
    assert_eq!(
        tracked.min,
        Vec2::new(100.0, 50.0),
        "Top should be at first item top"
    );
    assert_eq!(
        tracked.max,
        Vec2::new(250.0, 122.0),
        "Bottom should be at last item bottom"
    );

    ctx.end();
}

// === Menu Bar Dropdown Click Tests ===

/// Test that menu_bar_dropdown opens when clicked (press + release).
///
/// This tests the full click cycle:
/// 1. Frame 1: Mouse press on button -> sets active_id
/// 2. Frame 2: Mouse release while over button -> should toggle open state
#[test]
fn test_menu_bar_dropdown_click_opens_dropdown() {
    use crate::input::mouse_button;

    let mut ctx = UiContext::new();
    let mut dropdown_open = false;
    let button_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));

    // Frame 1: Mouse press on the button
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0)); // Center of button
    ctx.input.set_mouse_button(mouse_button::LEFT, true);

    ctx.menu_bar_dropdown(
        "file",
        "File",
        button_bounds,
        &mut dropdown_open,
        |_ui, _open| {
            // Menu content would go here
        },
    );
    ctx.end();

    // Dropdown should NOT be open yet (just pressed, not released)
    assert!(!dropdown_open, "Dropdown should not open on press");

    // Frame 2: Mouse release while still over button
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    // Mouse is still at the same position (still hovering the button)
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, false); // Release

    ctx.menu_bar_dropdown(
        "file",
        "File",
        button_bounds,
        &mut dropdown_open,
        |_ui, _open| {
            // Menu content would go here
        },
    );
    ctx.end();

    // NOW the dropdown should be open!
    assert!(
        dropdown_open,
        "Dropdown should open after click (press + release)"
    );
}

/// Test that clicking an open dropdown closes it.
#[test]
fn test_menu_bar_dropdown_click_toggles() {
    use crate::input::mouse_button;

    let mut ctx = UiContext::new();
    let mut dropdown_open = false;
    let button_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));

    // --- First click: open the dropdown ---
    // Frame 1: Press
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, true);
    ctx.menu_bar_dropdown(
        "file",
        "File",
        button_bounds,
        &mut dropdown_open,
        |_ui, _open| {},
    );
    ctx.end();

    // Frame 2: Release
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, false);
    ctx.menu_bar_dropdown(
        "file",
        "File",
        button_bounds,
        &mut dropdown_open,
        |_ui, _open| {},
    );
    ctx.end();

    assert!(dropdown_open, "Dropdown should be open after first click");

    // --- Second click: close the dropdown ---
    // Frame 3: Press
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, true);
    ctx.menu_bar_dropdown(
        "file",
        "File",
        button_bounds,
        &mut dropdown_open,
        |_ui, _open| {},
    );
    ctx.end();

    // Frame 4: Release
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, false);
    ctx.menu_bar_dropdown(
        "file",
        "File",
        button_bounds,
        &mut dropdown_open,
        |_ui, _open| {},
    );
    ctx.end();

    assert!(
        !dropdown_open,
        "Dropdown should be closed after second click"
    );
}

/// Test that menu_bar_dropdown click works even when popup bounds overlap button.
/// This tests that the raw input hover check bypasses popup blocking.
#[test]
fn test_menu_bar_dropdown_click_with_open_popup() {
    use crate::input::mouse_button;

    let mut ctx = UiContext::new();
    let mut dropdown_open = true; // Start with dropdown already open
    let button_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));

    // Simulate that a popup was opened in a previous frame (popup state persists)
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    // First, simulate the popup being rendered which sets popup_bounds
    ctx.popup(
        crate::context::Popup::new("test").below_button(button_bounds),
        &mut dropdown_open,
        |_ui, _open| {},
    );
    ctx.end();

    // Now popup_bounds should be set from the previous frame
    // Simulate a click on the button to close the dropdown
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, true);
    ctx.menu_bar_dropdown(
        "test",
        "Test",
        button_bounds,
        &mut dropdown_open,
        |_ui, _open| {},
    );
    ctx.end();

    // Release - should toggle the dropdown closed
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, false);
    ctx.menu_bar_dropdown(
        "test",
        "Test",
        button_bounds,
        &mut dropdown_open,
        |_ui, _open| {},
    );
    ctx.end();

    assert!(
        !dropdown_open,
        "Dropdown should close when clicked with popup open"
    );
}

// === Menu Bar Hover-to-Switch Tests ===

/// Test that hovering over another dropdown while one is open switches to it.
///
/// This is the standard menu bar behavior: when "File" is open and you hover
/// over "Edit", the File menu closes and Edit menu opens automatically.
///
/// Note: In immediate mode UI, hover-to-switch takes 2 frames:
/// - Frame N: Hover detected, close flag set, Edit opens
/// - Frame N+1: File sees close flag and closes
#[test]
fn test_menu_bar_hover_to_switch() {
    use crate::input::mouse_button;

    let mut ctx = UiContext::new();
    let mut file_open = false;
    let mut edit_open = false;
    let file_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));
    let edit_bounds = Rect2D::from_origin_size(Vec2::new(60.0, 0.0), Vec2::new(60.0, 24.0));

    // --- First, open the File dropdown ---
    // Frame 1: Press on File
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0)); // Over File button
    ctx.input.set_mouse_button(mouse_button::LEFT, true);
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.end();

    // Frame 2: Release on File
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, false);
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.end();

    assert!(file_open, "File dropdown should be open");
    assert!(!edit_open, "Edit dropdown should be closed");

    // --- Now hover over Edit (no click, just hover) ---
    // Frame 3: Move mouse to Edit button - hover-to-switch triggered
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(90.0, 12.0)); // Over Edit button
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.end();

    // Edit should be open now
    assert!(edit_open, "Edit dropdown should open when hovering it");

    // Frame 4: File sees the close flag and closes
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(90.0, 12.0)); // Still over Edit
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.end();

    // Now File should be closed and Edit should be open
    assert!(!file_open, "File dropdown should close when hovering Edit");
    assert!(edit_open, "Edit dropdown should remain open");
}

/// Test that only one menu dropdown can be open at a time.
///
/// Note: Hover-to-switch takes 2 frames in immediate mode:
/// - Frame N: Hover detected, close flag set, new dropdown opens
/// - Frame N+1: Old dropdown sees close flag and closes
#[test]
fn test_menu_bar_only_one_open() {
    use crate::input::mouse_button;

    let mut ctx = UiContext::new();
    let mut file_open = false;
    let mut edit_open = false;
    let mut view_open = false;
    let file_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));
    let edit_bounds = Rect2D::from_origin_size(Vec2::new(60.0, 0.0), Vec2::new(60.0, 24.0));
    let view_bounds = Rect2D::from_origin_size(Vec2::new(120.0, 0.0), Vec2::new(60.0, 24.0));

    // Open File dropdown
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, true);
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
    ctx.end();

    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, false);
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
    ctx.end();

    assert!(
        file_open && !edit_open && !view_open,
        "Only File should be open"
    );

    // Hover over Edit - Frame 1 (hover detected)
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(90.0, 12.0));
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
    ctx.end();

    // Frame 2 (File sees close flag)
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(90.0, 12.0));
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
    ctx.end();

    assert!(
        !file_open && edit_open && !view_open,
        "Only Edit should be open"
    );

    // Hover over View - Frame 1 (hover detected)
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(150.0, 12.0));
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
    ctx.end();

    // Frame 2 (Edit sees close flag)
    ctx.input.clear_frame_state();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(150.0, 12.0));
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("edit", "Edit", edit_bounds, &mut edit_open, |_ui, _open| {});
    ctx.menu_bar_dropdown("view", "View", view_bounds, &mut view_open, |_ui, _open| {});
    ctx.end();

    assert!(
        !file_open && !edit_open && view_open,
        "Only View should be open"
    );
}

/// Test that hover-to-switch only happens when a dropdown is already open.
/// Hovering alone (without clicking first) should NOT open a dropdown.
#[test]
fn test_menu_bar_hover_does_not_open_without_click() {
    let mut ctx = UiContext::new();
    let mut file_open = false;
    let file_bounds = Rect2D::from_origin_size(Vec2::new(0.0, 0.0), Vec2::new(60.0, 24.0));

    // Just hover over File - should NOT open it
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.input.set_mouse_pos(Vec2::new(30.0, 12.0));
    ctx.menu_bar_dropdown("file", "File", file_bounds, &mut file_open, |_ui, _open| {});
    ctx.end();

    assert!(!file_open, "Hovering alone should not open dropdown");
}

// === Modal Bounds Tests ===

/// Test that get_popup_bounds() returns correct bounds for a centered modal.
///
/// This verifies that the modal's bounds match its specified width/height,
/// allowing content to be correctly positioned within the modal.
#[test]
fn test_modal_get_popup_bounds_matches_specified_size() {
    let mut ctx = UiContext::new();
    let mut modal_open = true;
    let modal_width = 320.0;
    let modal_height = 120.0;

    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.modal(
        "test_modal",
        modal_width,
        modal_height,
        &mut modal_open,
        |ui, _open| {
            let bounds = ui.get_popup_bounds();

            // The bounds should have the specified width and height
            assert!(
                (bounds.width() - modal_width).abs() < 1.0,
                "Modal width should be {}, got {}",
                modal_width,
                bounds.width()
            );
            assert!(
                (bounds.height() - modal_height).abs() < 1.0,
                "Modal height should be {}, got {}",
                modal_height,
                bounds.height()
            );

            // The modal should be centered on screen
            let expected_x = (800.0 - modal_width) * 0.5;
            let expected_y = (600.0 - modal_height) * 0.5;
            assert!(
                (bounds.min.x() - expected_x).abs() < 1.0,
                "Modal x should be {}, got {}",
                expected_x,
                bounds.min.x()
            );
            assert!(
                (bounds.min.y() - expected_y).abs() < 1.0,
                "Modal y should be {}, got {}",
                expected_y,
                bounds.min.y()
            );
        },
    );
    ctx.end();
}

/// Test that buttons positioned relative to modal bounds are inside the modal.
///
/// This is the actual bug: when positioning buttons using get_popup_bounds(),
/// they end up outside the modal rectangle because the bounds are wrong.
#[test]
fn test_modal_buttons_within_bounds() {
    let mut ctx = UiContext::new();
    let mut modal_open = true;
    let modal_width = 320.0;
    let modal_height = 120.0;
    let btn_width = 80.0;
    let btn_height = 28.0;
    let btn_margin = 10.0;

    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.modal(
        "test_modal",
        modal_width,
        modal_height,
        &mut modal_open,
        |ui, _open| {
            let bounds = ui.get_popup_bounds();

            // Position "Yes" button at bottom-right of modal
            let yes_btn_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x() + modal_width - btn_width - btn_margin,
                    bounds.min.y() + modal_height - btn_height - btn_margin,
                ),
                Vec2::new(btn_width, btn_height),
            );

            // Position "No" button to the left of "Yes"
            let no_btn_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                    bounds.min.y() + modal_height - btn_height - btn_margin,
                ),
                Vec2::new(btn_width, btn_height),
            );

            // Both buttons should be fully contained within the modal bounds
            assert!(
                bounds.contains_rect(&yes_btn_bounds),
                "Yes button {:?} should be within modal bounds {:?}",
                yes_btn_bounds,
                bounds
            );
            assert!(
                bounds.contains_rect(&no_btn_bounds),
                "No button {:?} should be within modal bounds {:?}",
                no_btn_bounds,
                bounds
            );
        },
    );
    ctx.end();
}

/// Test that button clicks work inside a modal.
///
/// This tests the full click cycle (press + release) for a button
/// positioned inside a modal dialog.
#[test]
fn test_modal_button_click_works() {
    use crate::input::mouse_button;
    use crate::widgets::Button;

    let mut ctx = UiContext::new();
    let mut modal_open = true;
    let modal_width = 320.0;
    let modal_height = 120.0;
    let btn_width = 80.0;
    let btn_height = 28.0;
    let btn_margin = 10.0;
    let mut button_clicked = false;

    // Calculate expected button position (same both frames)
    let modal_x = (800.0 - modal_width) * 0.5;
    let modal_y = (600.0 - modal_height) * 0.5;
    let no_btn_x = modal_x + modal_width - btn_width * 2.0 - btn_margin * 2.0;
    let no_btn_y = modal_y + modal_height - btn_height - btn_margin;
    let btn_center = Vec2::new(no_btn_x + btn_width * 0.5, no_btn_y + btn_height * 0.5);

    // Frame 1: Press on the button inside modal
    ctx.input.set_mouse_pos(btn_center);
    ctx.input.set_mouse_button(mouse_button::LEFT, true);
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.modal(
        "test_modal",
        modal_width,
        modal_height,
        &mut modal_open,
        |ui, _open| {
            let bounds = ui.get_popup_bounds();
            let no_btn_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                    bounds.min.y() + modal_height - btn_height - btn_margin,
                ),
                Vec2::new(btn_width, btn_height),
            );

            let response = ui.add(Button::new("No").bounds(no_btn_bounds));
            if response.clicked {
                button_clicked = true;
            }
        },
    );
    ctx.end();

    // Button should NOT be clicked yet (just pressed)
    assert!(!button_clicked, "Button should not click on press");

    // Frame 2: Release on the button
    ctx.input.clear_frame_state();
    ctx.input.set_mouse_pos(btn_center);
    ctx.input.set_mouse_button(mouse_button::LEFT, false);
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.modal(
        "test_modal",
        modal_width,
        modal_height,
        &mut modal_open,
        |ui, _open| {
            let bounds = ui.get_popup_bounds();
            let no_btn_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                    bounds.min.y() + modal_height - btn_height - btn_margin,
                ),
                Vec2::new(btn_width, btn_height),
            );

            let response = ui.add(Button::new("No").bounds(no_btn_bounds));
            if response.clicked {
                button_clicked = true;
            }
        },
    );
    ctx.end();

    // NOW the button should be clicked!
    assert!(
        button_clicked,
        "Button should be clicked after press+release"
    );
}

/// Test that button hover detection works inside a modal.
#[test]
fn test_modal_button_hover_works() {
    let mut ctx = UiContext::new();
    let mut modal_open = true;
    let modal_width = 320.0;
    let modal_height = 120.0;
    let btn_width = 80.0;
    let btn_height = 28.0;
    let btn_margin = 10.0;

    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.modal(
        "test_modal",
        modal_width,
        modal_height,
        &mut modal_open,
        |ui, _open| {
            let bounds = ui.get_popup_bounds();

            let no_btn_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                    bounds.min.y() + modal_height - btn_height - btn_margin,
                ),
                Vec2::new(btn_width, btn_height),
            );

            // Set mouse position to center of the button
            let btn_center = Vec2::new(
                no_btn_bounds.min.x() + btn_width * 0.5,
                no_btn_bounds.min.y() + btn_height * 0.5,
            );
            ui.input.set_mouse_pos(btn_center);

            // Check if the button would be hovered by checking is_hovered directly
            let hovered = ui.input.is_hovered(no_btn_bounds);
            assert!(
                hovered,
                "Button at {:?} should be hovered when mouse at {:?}",
                no_btn_bounds, btn_center
            );
        },
    );
    ctx.end();
}

/// Test that Button widget hover state (for visuals) works inside a modal.
/// This tests the actual Button widget's hovered response, not just raw input.
#[test]
fn test_modal_button_widget_hover_visual_works() {
    use crate::widgets::Button;

    let mut ctx = UiContext::new();
    let mut modal_open = true;
    let modal_width = 320.0;
    let modal_height = 120.0;
    let btn_width = 80.0;
    let btn_height = 28.0;
    let btn_margin = 10.0;

    // Calculate expected button position
    let modal_x = (800.0 - modal_width) * 0.5;
    let modal_y = (600.0 - modal_height) * 0.5;
    let no_btn_x = modal_x + modal_width - btn_width * 2.0 - btn_margin * 2.0;
    let no_btn_y = modal_y + modal_height - btn_height - btn_margin;
    let btn_center = Vec2::new(no_btn_x + btn_width * 0.5, no_btn_y + btn_height * 0.5);

    // Set mouse position BEFORE beginning frame
    ctx.input.set_mouse_pos(btn_center);

    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.modal(
        "test_modal",
        modal_width,
        modal_height,
        &mut modal_open,
        |ui, _open| {
            let bounds = ui.get_popup_bounds();
            let no_btn_bounds = Rect2D::from_origin_size(
                Vec2::new(
                    bounds.min.x() + modal_width - btn_width * 2.0 - btn_margin * 2.0,
                    bounds.min.y() + modal_height - btn_height - btn_margin,
                ),
                Vec2::new(btn_width, btn_height),
            );

            // Add the button and check its hover state
            let response = ui.add(Button::new("No").bounds(no_btn_bounds));
            assert!(
                response.hovered,
                "Button widget should report hovered=true when mouse is over it inside modal"
            );
        },
    );
    ctx.end();
}

// === Layout Cursor Tests ===

/// Test that cursor() returns layout cursor when inside a layout.
#[test]
fn test_cursor_returns_layout_cursor_when_in_layout() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    // Set initial cursor position
    ctx.set_cursor(Vec2::new(100.0, 50.0));
    assert_eq!(ctx.cursor(), Vec2::new(100.0, 50.0));

    // Begin a horizontal layout
    ctx.begin_row();

    // Cursor should still return the same position initially
    assert_eq!(ctx.cursor(), Vec2::new(100.0, 50.0));

    ctx.end_row();
    ctx.end();
}

/// Test that layout_item advances cursor in horizontal layout.
#[test]
fn test_layout_item_advances_horizontal_cursor() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    ctx.set_cursor(Vec2::new(100.0, 50.0));
    ctx.begin_row();

    // Get bounds for first item
    let size = Vec2::new(60.0, 24.0);
    let bounds1 = ctx.layout_item(size);

    // First item should be at initial cursor position
    assert_eq!(bounds1.min, Vec2::new(100.0, 50.0));

    // Cursor should have advanced horizontally
    let cursor_after = ctx.cursor();
    assert!(
        cursor_after.x() > 100.0,
        "Cursor should have advanced horizontally, got x={}",
        cursor_after.x()
    );
    assert_eq!(
        cursor_after.y(),
        50.0,
        "Cursor y should stay the same in horizontal layout"
    );

    // Get bounds for second item
    let bounds2 = ctx.layout_item(size);

    // Second item should be at the advanced cursor position
    assert_eq!(bounds2.min.x(), cursor_after.x());
    assert_ne!(bounds1.min.x(), bounds2.min.x(), "Items should not pile up");

    ctx.end_row();
    ctx.end();
}

/// Test that layout_item advances cursor in vertical layout.
#[test]
fn test_layout_item_advances_vertical_cursor() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    ctx.set_cursor(Vec2::new(100.0, 50.0));
    ctx.begin_column();

    // Get bounds for first item
    let size = Vec2::new(60.0, 24.0);
    let bounds1 = ctx.layout_item(size);

    // First item should be at initial cursor position
    assert_eq!(bounds1.min, Vec2::new(100.0, 50.0));

    // Cursor should have advanced vertically
    let cursor_after = ctx.cursor();
    assert_eq!(
        cursor_after.x(),
        100.0,
        "Cursor x should stay the same in vertical layout"
    );
    assert!(
        cursor_after.y() > 50.0,
        "Cursor should have advanced vertically, got y={}",
        cursor_after.y()
    );

    // Get bounds for second item
    let bounds2 = ctx.layout_item(size);

    // Second item should be at the advanced cursor position
    assert_eq!(bounds2.min.y(), cursor_after.y());
    assert_ne!(bounds1.min.y(), bounds2.min.y(), "Items should not pile up");

    ctx.end_column();
    ctx.end();
}

/// Test that advance_cursor works in horizontal layout.
#[test]
fn test_advance_cursor_in_layout() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    ctx.set_cursor(Vec2::new(100.0, 50.0));
    ctx.begin_row();

    // Cursor starts at layout start
    assert_eq!(ctx.cursor(), Vec2::new(100.0, 50.0));

    // Advance cursor manually
    ctx.advance_cursor(Vec2::new(60.0, 24.0));

    // Cursor should have advanced
    let cursor_after = ctx.cursor();
    assert!(
        cursor_after.x() > 100.0,
        "Cursor should have advanced horizontally"
    );

    // Advance again
    ctx.advance_cursor(Vec2::new(60.0, 24.0));

    // Cursor should have advanced more
    let cursor_final = ctx.cursor();
    assert!(
        cursor_final.x() > cursor_after.x(),
        "Cursor should have advanced again"
    );

    ctx.end_row();
    ctx.end();
}

/// Test that set_cursor works inside a layout.
#[test]
fn test_set_cursor_in_layout() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    // Set main cursor before layout
    ctx.set_cursor(Vec2::new(100.0, 50.0));

    ctx.begin_row();

    // set_cursor inside layout should update layout cursor
    ctx.set_cursor(Vec2::new(200.0, 100.0));

    // cursor() should return the updated layout cursor
    assert_eq!(ctx.cursor(), Vec2::new(200.0, 100.0));

    ctx.end_row();
    ctx.end();
}

/// Test that layouts are independent between panels.
#[test]
fn test_layouts_dont_interfere() {
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    // First "panel" - left side
    ctx.begin_column();
    ctx.set_cursor(Vec2::new(10.0, 10.0));
    let bounds1 = ctx.layout_item(Vec2::new(100.0, 20.0));
    assert_eq!(bounds1.min, Vec2::new(10.0, 10.0));
    ctx.end_column();

    // After end_column, main cursor should be updated
    let cursor_after_first = ctx.cursor();
    assert!(
        cursor_after_first.y() > 10.0,
        "Cursor should have moved down"
    );

    // Second "panel" - right side (simulating inspector after hierarchy)
    ctx.begin_column();
    ctx.set_cursor(Vec2::new(500.0, 10.0)); // Different X position
    let bounds2 = ctx.layout_item(Vec2::new(100.0, 20.0));
    // Item should be at the new position, not affected by first panel
    assert_eq!(bounds2.min, Vec2::new(500.0, 10.0));
    ctx.end_column();

    ctx.end();
}

// === Click Behavior Tests ===

/// Test that pressing inside a widget and releasing outside returns Released (not Clicked).
///
/// This verifies the press-then-drag-off behavior: if the user presses inside a button
/// but moves the mouse outside before releasing, the button should NOT register a click.
#[test]
fn test_click_behavior_press_inside_release_outside() {
    use crate::input::mouse_button;
    use crate::widgets::Button;

    let mut ctx = UiContext::new();
    let button_bounds = Rect2D::from_origin_size(Vec2::new(100.0, 100.0), Vec2::new(80.0, 30.0));
    let mut button_clicked = false;

    // Frame 1: Press mouse inside button bounds
    ctx.input.set_mouse_pos(Vec2::new(140.0, 115.0)); // Center of button
    ctx.input.set_mouse_button(mouse_button::LEFT, true);
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    {
        let response = ctx.add(Button::new("Test").bounds(button_bounds));
        assert!(response.hovered, "Button should be hovered");
        assert!(!response.clicked, "Button should not be clicked on press");
        // active_id is set during this frame's click processing, but the response
        // captures the state before the click handler ran
    }
    ctx.end();

    // Frame 2: Move mouse outside button, then release
    ctx.input.clear_frame_state();
    ctx.input.set_mouse_pos(Vec2::new(300.0, 400.0)); // Outside button
    ctx.input.set_mouse_button(mouse_button::LEFT, false);
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    {
        let response = ctx.add(Button::new("Test").bounds(button_bounds));
        if response.clicked {
            button_clicked = true;
        }
    }
    ctx.end();

    assert!(
        !button_clicked,
        "Button should NOT be clicked when mouse is released outside its bounds"
    );
}

// === Popup Blocking Tests ===

/// Test that an open popup blocks clicks on widgets underneath.
///
/// When a popup is open, clicking on a widget that is visually behind the popup
/// should not register. The popup consumes the click event.
#[test]
fn test_popup_blocks_click_underneath() {
    use crate::input::mouse_button;
    use crate::widgets::Button;

    let mut ctx = UiContext::new();
    let mut popup_open = true;
    let mut button_clicked = false;

    let button_bounds = Rect2D::from_origin_size(Vec2::new(100.0, 100.0), Vec2::new(80.0, 30.0));
    let popup_bounds_config =
        Rect2D::from_origin_size(Vec2::new(80.0, 80.0), Vec2::new(200.0, 200.0));

    // popup_bounds covers the button_bounds, so the button is "underneath" the popup.

    // Frame 1: Press on button area (which is inside popup)
    ctx.input.set_mouse_pos(Vec2::new(140.0, 115.0)); // Inside both popup and button
    ctx.input.set_mouse_button(mouse_button::LEFT, true);
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    // Render popup first (it sets popup_bounds and blocks hover for widgets underneath)
    ctx.popup(
        Popup::new("test_popup").fixed(popup_bounds_config),
        &mut popup_open,
        |_ui, _open| {},
    );

    // Try to click the button underneath the popup
    {
        let response = ctx.add(Button::new("Test").bounds(button_bounds));
        if response.clicked {
            button_clicked = true;
        }
        assert!(
            !response.hovered,
            "Button should not be hovered when popup covers it"
        );
    }
    ctx.end();

    // Frame 2: Release on button area (still inside popup)
    ctx.input.clear_frame_state();
    ctx.input.set_mouse_pos(Vec2::new(140.0, 115.0));
    ctx.input.set_mouse_button(mouse_button::LEFT, false);
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);

    ctx.popup(
        Popup::new("test_popup").fixed(popup_bounds_config),
        &mut popup_open,
        |_ui, _open| {},
    );

    {
        let response = ctx.add(Button::new("Test").bounds(button_bounds));
        if response.clicked {
            button_clicked = true;
        }
        assert!(
            !response.hovered,
            "Button should not be hovered when popup covers it on release"
        );
    }
    ctx.end();

    assert!(
        !button_clicked,
        "Button underneath popup should NOT receive click"
    );
}

// === Cursor Advancement Consistency Tests (VAL-UI-004) ===

/// Test that ui.add() advances the cursor after rendering a widget.
///
/// All widgets drawn via ui.add() should advance the cursor so that
/// subsequent widgets don't overlap.
#[test]
fn test_add_advances_cursor() {
    use crate::widgets::Button;

    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.set_cursor(Vec2::new(10.0, 10.0));

    let cursor_before = ctx.cursor();
    let _response = ctx.add(Button::new("Test").bounds(Rect2D::from_origin_size(
        Vec2::new(10.0, 10.0),
        Vec2::new(100.0, 30.0),
    )));
    let cursor_after = ctx.cursor();

    assert!(
        cursor_after.y() > cursor_before.y(),
        "Cursor should advance vertically after ui.add(Button): before.y()={}, after.y()={}",
        cursor_before.y(),
        cursor_after.y()
    );

    ctx.end();
}

/// Test that multiple widgets added via ui.add() stack vertically.
#[test]
fn test_add_stacks_widgets_vertically() {
    use crate::widgets::Button;

    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.set_cursor(Vec2::new(10.0, 10.0));

    let h = 30.0;
    let _r1 = ctx.add(Button::new("A").bounds(Rect2D::from_origin_size(
        Vec2::new(10.0, 10.0),
        Vec2::new(100.0, h),
    )));
    let cursor_after_first = ctx.cursor();

    let _r2 = ctx.add(Button::new("B").bounds(Rect2D::from_origin_size(
        cursor_after_first,
        Vec2::new(100.0, h),
    )));
    let cursor_after_second = ctx.cursor();

    assert!(
        cursor_after_second.y() > cursor_after_first.y(),
        "Second widget should be below first: first.y()={}, second.y()={}",
        cursor_after_first.y(),
        cursor_after_second.y()
    );

    ctx.end();
}

/// Test that Label advances cursor after ui.add().
#[test]
fn test_label_advances_cursor_via_add() {
    use crate::widgets::Label;

    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.set_cursor(Vec2::new(10.0, 10.0));

    let cursor_before = ctx.cursor();
    let response = ctx.add(Label::new("Hello").bounds(Rect2D::from_origin_size(
        Vec2::new(10.0, 10.0),
        Vec2::new(50.0, 20.0),
    )));
    let cursor_after = ctx.cursor();

    assert_eq!(response.bounds.height(), 20.0);
    assert!(
        cursor_after.y() > cursor_before.y(),
        "Cursor should advance after Label: before.y()={}, after.y()={}",
        cursor_before.y(),
        cursor_after.y()
    );

    ctx.end();
}

/// Test that cursor advancement works correctly inside a column layout.
#[test]
fn test_add_advances_cursor_in_column_layout() {
    use crate::widgets::Button;

    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.set_cursor(Vec2::new(10.0, 10.0));
    ctx.begin_column();

    let cursor_before = ctx.cursor();
    let _r1 = ctx.add(Button::new("A").bounds(Rect2D::from_origin_size(
        cursor_before,
        Vec2::new(100.0, 30.0),
    )));
    let cursor_after_first = ctx.cursor();

    let _r2 = ctx.add(Button::new("B").bounds(Rect2D::from_origin_size(
        cursor_after_first,
        Vec2::new(100.0, 30.0),
    )));
    let cursor_after_second = ctx.cursor();

    assert!(
        cursor_after_second.y() > cursor_after_first.y(),
        "Widgets should stack vertically in column layout"
    );

    ctx.end_column();
    ctx.end();
}

/// Test that cursor advancement works correctly inside a row layout.
#[test]
fn test_add_advances_cursor_in_row_layout() {
    use crate::widgets::Button;

    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.set_cursor(Vec2::new(10.0, 10.0));
    ctx.begin_row();

    let cursor_before = ctx.cursor();
    let _r1 = ctx.add(Button::new("A").bounds(Rect2D::from_origin_size(
        cursor_before,
        Vec2::new(80.0, 30.0),
    )));
    let cursor_after_first = ctx.cursor();

    assert!(
        cursor_after_first.x() > cursor_before.x(),
        "Widget should advance cursor horizontally in row layout"
    );
    assert!(
        cursor_after_first.y() == cursor_before.y(),
        "Cursor y should stay the same in row layout"
    );

    ctx.end_row();
    ctx.end();
}

/// Test that at_cursor() + ui.add() positions and advances correctly.
#[test]
fn test_at_cursor_positions_and_advances() {
    use crate::widgets::{Button, Label};

    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.set_cursor(Vec2::new(50.0, 50.0));

    // Label at_cursor should position at cursor and advance
    let r1 = ctx.add(Label::new("Test Label").at_cursor(&ctx));
    assert_eq!(r1.bounds.min.x(), 50.0);
    assert_eq!(r1.bounds.min.y(), 50.0);

    let cursor_after_label = ctx.cursor();
    assert!(
        cursor_after_label.y() > 50.0,
        "Cursor should advance after Label at_cursor"
    );

    // Button at_cursor should position at the new cursor position
    let r2 = ctx.add(Button::new("Click").at_cursor(&ctx));
    assert_eq!(r2.bounds.min.x(), cursor_after_label.x());
    assert_eq!(r2.bounds.min.y(), cursor_after_label.y());

    let cursor_after_button = ctx.cursor();
    assert!(
        cursor_after_button.y() > cursor_after_label.y(),
        "Cursor should advance after Button at_cursor"
    );

    ctx.end();
}

// === VAL-UI-005: Border drawing centralized in button_with_colors ===

/// Test that button border drawing is handled entirely by button_with_colors().
///
/// A Button with border_color set should produce more draw primitives than one
/// without, confirming the border is drawn inside the internal method, not split
/// across the builder and the internal method.
#[test]
fn test_button_border_drawn_in_button_with_colors() {
    use crate::widgets::Button;

    let button_bounds = Rect2D::from_origin_size(Vec2::new(10.0, 10.0), Vec2::new(80.0, 30.0));
    let border_color = Color::from_rgb_hex(0xFF0000);

    // Button without border
    let mut ctx_no_border = UiContext::new();
    ctx_no_border.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx_no_border.add(Button::new("NoBorder").bounds(button_bounds));
    let draw_list_no_border = ctx_no_border.end();

    // Button with border
    let mut ctx_with_border = UiContext::new();
    ctx_with_border.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx_with_border.add(
        Button::new("WithBorder")
            .bounds(button_bounds)
            .border(border_color),
    );
    let draw_list_with_border = ctx_with_border.end();

    // Border adds 4 rectangles (top, bottom, left, right edges)
    // The bordered button should have more vertices and indices
    assert!(
        draw_list_with_border.vertex_count() > draw_list_no_border.vertex_count(),
        "Button with border should produce more vertices than without border"
    );
    assert!(
        draw_list_with_border.index_count() > draw_list_no_border.index_count(),
        "Button with border should produce more indices than without border"
    );
}

/// Test that Button::border() is properly forwarded through Widget::ui() to
/// button_with_colors(), confirming no border drawing happens in the builder layer.
#[test]
fn test_button_border_forwarded_to_internal_method() {
    use crate::widgets::Button;

    let button_bounds = Rect2D::from_origin_size(Vec2::new(10.0, 10.0), Vec2::new(80.0, 30.0));
    let custom_border = Color::from_rgb_hex(0x00FF00);

    // Render button with border via the public builder API
    let mut ctx = UiContext::new();
    ctx.begin(Vec2::new(800.0, 600.0), 1.0);
    ctx.add(
        Button::new("Test")
            .bounds(button_bounds)
            .border(custom_border),
    );
    let draw_list = ctx.end();

    // Verify the draw list has border primitives (more than just bg + text)
    // Background rect: 6 indices, border: 4 rects * 6 = 24 indices, text may add more
    assert!(
        draw_list.index_count() >= 30,
        "Button with border should have at least 30 indices (bg + 4 border edges)"
    );
}
