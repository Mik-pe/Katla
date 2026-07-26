from pathlib import Path


def replace_once(source: str, old: str, new: str, label: str) -> str:
    if old not in source:
        raise SystemExit(f"expected source fragment not found: {label}")
    return source.replace(old, new, 1)


renderer_path = Path("katla_app/src/application/renderer.rs")
renderer = renderer_path.read_text()
renderer = replace_once(
    renderer,
    '''                katla_gfx::error::RendererError::SwapchainOutOfDate => {
                    log::debug!("Swapchain out of date, triggering recreation on next frame");
                    // Defer recreation to the next frame to avoid complex re-entrancy.
                    // The next RedrawRequested will call recreate_swapchain via a flag.
                    self.needs_swapchain_recreate = true;
                    return;
                }
                _ => {
                    log::error!("Frame render failed, skipping frame: {}", e);
                    return;
                }
''',
    '''                katla_gfx::error::RendererError::SwapchainOutOfDate => {
                    log::debug!("Swapchain out of date, triggering recreation on next frame");
                    // Defer recreation to the next frame to avoid complex re-entrancy.
                    // The next RedrawRequested will call recreate_swapchain via a flag.
                    self.needs_swapchain_recreate = true;
                }
                _ => {
                    log::error!("Frame render failed, skipping frame: {}", e);
                }
''',
    "needless frame-render returns",
)
renderer_path.write_text(renderer)


gltf_path = Path("katla_app/src/animation/gltf_loader.rs")
gltf = gltf_path.read_text()
gltf = gltf.replace("parent_transform.clone() * local_matrix", "*parent_transform * local_matrix", 1)
gltf = replace_once(
    gltf,
    '''                if let Some(joint_index) = joints.iter().position(|&j| j == *node_index) {
                    if joint_index < animated_local.len() {
                        let transform = &animated_local[joint_index];
''',
    '''                if let Some(joint_index) = joints.iter().position(|&j| j == *node_index)
                    && joint_index < animated_local.len()
                {
                    let transform = &animated_local[joint_index];
''',
    "collapsible animation sample guard",
)
gltf = replace_once(
    gltf,
    '''                        animated_local[joint_index] = new_transform;
                    }
                }
''',
    '''                    animated_local[joint_index] = new_transform;
                }
''',
    "collapsed animation sample closing braces",
)
gltf = gltf.replace("let local = animated_local[i].clone();", "let local = animated_local[i];", 1)
gltf = gltf.replace(
    "world_transforms[i] = world_transforms[*parent_idx].clone() * local;",
    "world_transforms[i] = world_transforms[*parent_idx] * local;",
    1,
)
gltf_path.write_text(gltf)


animation_tests_path = Path("katla_app/src/animation/tests.rs")
animation_tests = animation_tests_path.read_text()
prefix = "#[cfg(test)]\nmod tests {\n"
if animation_tests.startswith(prefix):
    body = animation_tests[len(prefix) :]
    if not body.endswith("}\n"):
        raise SystemExit("animation test module closing brace not found")
    body = body[:-2]
    lines = [line[4:] if line.startswith("    ") else line for line in body.splitlines()]
    animation_tests_path.write_text("\n".join(lines).rstrip() + "\n")
elif not animation_tests.startswith("use crate::animation::clips"):
    raise SystemExit("animation tests were neither nested nor already flattened")


memoize_path = Path("katla_ui/src/declarative/widgets/memoize.rs")
memoize = memoize_path.read_text().replace("Arc::new(3.14_f32)", "Arc::new(3.125_f32)", 1)
memoize_path.write_text(memoize)


scene_tests_path = Path("katla_app/src/scene/tests.rs")
scene_tests = scene_tests_path.read_text()
scene_tests = scene_tests.replace(
    '.filter(|e| e.name.as_ref().map_or(false, |n| n.starts_with("Sphere_")))',
    '.filter(|e| e.name.as_ref().is_some_and(|n| n.starts_with("Sphere_")))',
    1,
)
scene_tests = scene_tests.replace("0.92387953", "0.923_879_5")
scene_tests = scene_tests.replace("let original_position = [3.14, 2.71, -1.62];", "let original_position = [3.125, 2.71, -1.62];", 1)
scene_tests = scene_tests.replace(
    "let edited_rotation = [0.0, 0.70710678, 0.0, 0.70710678]; // 90° around Y",
    "let edited_rotation = [\n        0.0,\n        std::f32::consts::FRAC_1_SQRT_2,\n        0.0,\n        std::f32::consts::FRAC_1_SQRT_2,\n    ]; // 90° around Y",
    1,
)
scene_tests_path.write_text(scene_tests)


draw_list_path = Path("katla_ui/src/draw_list.rs")
draw_list = draw_list_path.read_text()
draw_list = draw_list.replace(
    "assert!(list.instances().len() > 0);",
    "assert!(!list.instances().is_empty());",
    1,
)
draw_list = draw_list.replace(
    "assert!(list.vertices().len() > 0);",
    "assert!(!list.vertices().is_empty());",
    1,
)
draw_list_path.write_text(draw_list)


text_path = Path("katla_ui/src/text/mod.rs")
text = text_path.read_text()
text = text.replace(
    "alpha >= 0.0 && alpha <= 1.0,",
    "(0.0..=1.0).contains(&alpha),",
    1,
)
text = replace_once(
    text,
    '''        for i in 0..char_advances.len() {
            assert_eq!(
                char_advances[i], char_advances[i],
                "Advance width should be consistent"
            );
        }
''',
    '''        assert!(
            char_advances.iter().all(|advance| *advance > 0.0),
            "Every character advance should be positive"
        );
''',
    "meaningful character-advance assertion",
)
text = text.replace(
    '''        assert!(
            0.0 >= 0.0 && 0.0 <= 1.0,
            "UV min X should be normalized to [0,1]"
        );
        assert!(
            0.0 >= 0.0 && 0.0 <= 1.0,
            "UV min Y should be normalized to [0,1]"
        );
''',
    "",
    1,
)
text = text.replace(
    "white_pixel_uv_max_x >= 0.0 && white_pixel_uv_max_x <= 1.0,",
    "(0.0..=1.0).contains(&white_pixel_uv_max_x),",
    1,
)
text = text.replace(
    "white_pixel_uv_max_y >= 0.0 && white_pixel_uv_max_y <= 1.0,",
    "(0.0..=1.0).contains(&white_pixel_uv_max_y),",
    1,
)
text = text.replace('        assert!(0.0 >= 0.0, "UV min X must be >= 0.0");\n', "", 1)
text = text.replace('        assert!(0.0 >= 0.0, "UV min Y must be >= 0.0");\n', "", 1)
text = text.replace("let atlas_y = 0 + padding;", "let atlas_y = padding;", 1)
for name in ("uv_min_x", "uv_min_y", "uv_max_x", "uv_max_y"):
    text = text.replace(
        f"assert!({name} >= 0.0 && {name} <= 1.0);",
        f"assert!((0.0..=1.0).contains(&{name}));",
        1,
    )
text = text.replace("            assert!(0.0 >= 0.0 && 0.0 <= 1.0);\n", "", 2)
text = text.replace(
    "assert!(uv_max_x >= 0.0 && uv_max_x <= 1.0);",
    "assert!((0.0..=1.0).contains(&uv_max_x));",
    1,
)
text = text.replace(
    "assert!(uv_max_y >= 0.0 && uv_max_y <= 1.0);",
    "assert!((0.0..=1.0).contains(&uv_max_y));",
    1,
)
text = text.replace(
    'assert!(runs.len() >= 1, "CJK text should be laid out");',
    'assert!(!runs.is_empty(), "CJK text should be laid out");',
    1,
)
text_path.write_text(text)


editor_tests_path = Path("katla_app/src/ui/editor_ui/tests.rs")
editor_tests = editor_tests_path.read_text()
editor_tests = replace_once(
    editor_tests,
    '''        if let Some(id) = actions.into_iter().find_map(|a| match a {
            HierarchyAction::SelectEntity(id) => Some(id),
        }) {
''',
    '''        if let Some(HierarchyAction::SelectEntity(id)) = actions.into_iter().next() {
''',
    "hierarchy action selection",
)
editor_tests = replace_once(
    editor_tests,
    '''        if let Some(root_id) = view_tree.root() {
            if id == root_id {
                return node
                    .widget
                    .as_any()
                    .downcast_ref::<katla_ui::declarative::widgets::zstack::ZStack>()
                    .is_some();
            }
        }
''',
    '''        if let Some(root_id) = view_tree.root()
            && id == root_id
        {
            return node
                .widget
                .as_any()
                .downcast_ref::<katla_ui::declarative::widgets::zstack::ZStack>()
                .is_some();
        }
''',
    "collapsed editor root check",
)
editor_tests_path.write_text(editor_tests)


dock_tree_path = Path("katla_ui/src/dock/tree.rs")
dock_tree = dock_tree_path.read_text()
dock_tree = dock_tree.replace(
    "let active = if tabs.is_empty() { 0 } else { 0 };",
    "let active = 0;",
    1,
)
dock_tree = dock_tree.replace(
    "assert!(matches!(tree.get(&path(&[2])), None));",
    "assert!(tree.get(&path(&[2])).is_none());",
    1,
)
dock_tree = replace_once(
    dock_tree,
    '''            match node {
                DockNode::Split { children, .. } => {
                    // Neither child should be Empty or empty Leaf
                    for child in children {
                        match &**child {
                            DockNode::Empty => {
                                panic!("collapsed tree should not have Empty children in splits")
                            }
                            DockNode::Leaf { tabs, .. } if tabs.is_empty() => {
                                panic!("collapsed tree should not have empty Leaf children")
                            }
                            _ => assert_no_empty_splits(child),
                        }
                    }
                }
                _ => {}
            }
''',
    '''            if let DockNode::Split { children, .. } = node {
                // Neither child should be Empty or empty Leaf
                for child in children {
                    match &**child {
                        DockNode::Empty => {
                            panic!("collapsed tree should not have Empty children in splits")
                        }
                        DockNode::Leaf { tabs, .. } if tabs.is_empty() => {
                            panic!("collapsed tree should not have empty Leaf children")
                        }
                        _ => assert_no_empty_splits(child),
                    }
                }
            }
''',
    "single-pattern dock-tree validation",
)
dock_tree_path.write_text(dock_tree)
