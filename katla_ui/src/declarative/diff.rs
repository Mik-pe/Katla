use super::descriptor::ViewDescriptor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffAction {
    Update,
    RecurseChildren,
    Replace,
}

pub fn diff_descriptor(old: &ViewDescriptor, new: &ViewDescriptor) -> DiffAction {
    if std::mem::discriminant(old) != std::mem::discriminant(new) {
        return DiffAction::Replace;
    }

    match new {
        ViewDescriptor::HStack(_)
        | ViewDescriptor::VStack(_)
        | ViewDescriptor::ZStack(_)
        | ViewDescriptor::ScrollView(_)
        | ViewDescriptor::Panel(_)
        | ViewDescriptor::Overlay(_)
        | ViewDescriptor::DraggablePanel(_)
        | ViewDescriptor::Selectable { .. }
        | ViewDescriptor::Section { .. }
        | ViewDescriptor::TabBar(_)
        | ViewDescriptor::Grid(_)
        | ViewDescriptor::Modal(_)
        | ViewDescriptor::TransitionContainer { .. }
        | ViewDescriptor::StatusBar(_) => DiffAction::RecurseChildren,

        _ => DiffAction::Update,
    }
}

#[derive(Debug)]
pub enum Patch {
    Insert {
        parent: Option<super::state::ViewId>,
        index: usize,
        descriptor: ViewDescriptor,
    },
    Update {
        node: super::state::ViewId,
        descriptor: ViewDescriptor,
    },
    Remove {
        node: super::state::ViewId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextureId;
    use crate::declarative::StateId;
    use crate::declarative::constructors::*;

    // -- Same variant → Update --

    fn vd(w: Box<dyn crate::declarative::widget::Widget>) -> ViewDescriptor {
        crate::declarative::constructors::into_descriptor(w)
    }

    #[test]
    fn test_diff_same_text_is_update() {
        let a = vd(text("hello"));
        let b = vd(text("world"));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_button_is_update() {
        let a = vd(button("ok"));
        let b = vd(button("cancel"));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_toggle_is_update() {
        let sid = StateId::test_id();
        let a = ViewDescriptor::Toggle {
            label: "a".into(),
            value_id: sid,
        };
        let b = ViewDescriptor::Toggle {
            label: "b".into(),
            value_id: sid,
        };
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_empty_is_update() {
        assert_eq!(
            diff_descriptor(&vd(empty()), &vd(empty())),
            DiffAction::Update
        );
    }

    #[test]
    fn test_diff_same_image_is_update() {
        use katla_math::Color;
        let a = vd(image(TextureId(1), Color::WHITE));
        let b = vd(image(TextureId(2), Color::BLACK));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_separator_is_update() {
        let a = vd(separator_horizontal());
        let b = vd(separator_vertical());
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_icon_is_update() {
        let a = vd(icon('A'));
        let b = vd(icon('B'));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_progress_is_update() {
        let a = vd(progress(0.5, 0.0..=1.0));
        let b = vd(progress(0.8, 0.0..=1.0));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    // -- Same container variant → RecurseChildren --

    #[test]
    fn test_diff_same_hstack_is_recurse() {
        let a = vd(hstack([text("a")]));
        let b = vd(hstack([text("b")]));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_vstack_is_recurse() {
        let a = vd(vstack([text("a")]));
        let b = vd(vstack([text("b")]));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_zstack_is_recurse() {
        use super::super::descriptor::Alignment;
        let a = vd(zstack([(Alignment::Center, text("a"))]));
        let b = vd(zstack([(Alignment::Center, text("b"))]));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_grid_is_recurse() {
        let a = vd(grid(2, katla_math::Vec2::new(100.0, 100.0), [text("a")]));
        let b = vd(grid(2, katla_math::Vec2::new(100.0, 100.0), [text("b")]));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_selectable_is_recurse() {
        let a = vd(selectable(text("a")));
        let b = vd(selectable(text("b")));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_section_is_recurse() {
        let sid = StateId::test_id();
        let a = vd(section("title", vstack([text("a")]), sid));
        let b = vd(section("title", vstack([text("b")]), sid));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_tab_bar_is_recurse() {
        let sid = StateId::test_id();
        let a = vd(tab_bar(vec![tab_item("A"), tab_item("B")], sid, empty()));
        let b = vd(tab_bar(vec![tab_item("C"), tab_item("D")], sid, empty()));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    // -- Self-managed containers → Update --

    #[test]
    fn test_diff_same_menubar_is_update() {
        let a = vd(menubar(vec![]));
        let b = vd(menubar(vec![]));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    // -- Different variants → Replace --

    #[test]
    fn test_diff_text_to_button_is_replace() {
        let a = vd(text("hello"));
        let b = vd(button("hello"));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_diff_hstack_to_vstack_is_replace() {
        let a = vd(hstack([text("a")]));
        let b = vd(vstack([text("a")]));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_diff_empty_to_text_is_replace() {
        let a = vd(empty());
        let b = vd(text("hello"));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_diff_text_to_empty_is_replace() {
        let a = vd(text("hello"));
        let b = vd(empty());
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_diff_image_to_icon_is_replace() {
        use katla_math::Color;
        let a = vd(image(TextureId(1), Color::WHITE));
        let b = vd(icon('X'));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_keyed_child_descriptor_has_key() {
        use super::super::descriptor::ChildDescriptor;
        let cd = super::super::constructors::keyed(42, text("hello"));
        assert_eq!(cd.key, Some(42));
        let desc = super::super::constructors::into_descriptor(text("hello"));
        let cd_nokey: ChildDescriptor = ChildDescriptor::from(desc);
        assert_eq!(cd_nokey.key, None);
    }

    // -- Integration tests: ViewTree reconciliation via diff_descriptor --

    struct StaticDescriptor(ViewDescriptor);

    impl crate::declarative::build::Build for StaticDescriptor {
        fn build(
            &self,
            _ctx: &mut crate::declarative::build::BuildContext,
        ) -> Box<dyn crate::declarative::widget::Widget> {
            crate::declarative::constructors::into_descriptor_owned(self.0.clone())
        }
    }

    fn build_tree(
        tree: &mut crate::declarative::ViewTree,
        widget: Box<dyn crate::declarative::widget::Widget>,
    ) {
        let descriptor = crate::declarative::constructors::into_descriptor(widget);
        tree.build_from(&StaticDescriptor(descriptor));
    }

    fn collect_child_labels(
        tree: &crate::declarative::ViewTree,
        parent: crate::declarative::ViewId,
    ) -> Vec<String> {
        let node = tree.get(parent).expect("parent node");
        let mut labels = Vec::new();
        for &child_id in &node.children {
            if let Some(child) = tree.get(child_id) {
                if let ViewDescriptor::Text { content, .. } = child.descriptor() {
                    labels.push(content.clone());
                } else {
                    labels.push(format!("{:?}", child.descriptor()));
                }
            }
        }
        labels
    }

    fn collect_child_keys(
        tree: &crate::declarative::ViewTree,
        parent: crate::declarative::ViewId,
    ) -> Vec<Option<u64>> {
        let node = tree.get(parent).expect("parent node");
        node.children
            .iter()
            .map(|&id| tree.get(id).and_then(|n| n.key))
            .collect()
    }

    #[test]
    fn test_same_descriptor_is_update() {
        let mut tree = crate::declarative::ViewTree::new();
        let desc = text("hello");
        let desc_vd = crate::declarative::constructors::into_descriptor(desc);
        build_tree(
            &mut tree,
            crate::declarative::constructors::into_descriptor_owned(desc_vd.clone()),
        );
        let root = tree.root().unwrap();
        let v0 = tree.get(root).unwrap().state_version;

        build_tree(
            &mut tree,
            crate::declarative::constructors::into_descriptor_owned(desc_vd.clone()),
        );
        let v1 = tree.get(root).unwrap().state_version;

        assert!(
            v1 > v0,
            "same descriptor should increment state_version (Update)"
        );
    }

    #[test]
    fn test_different_variant_is_replace() {
        let mut tree = crate::declarative::ViewTree::new();
        build_tree(&mut tree, text("hello"));
        let root = tree.root().unwrap();

        build_tree(&mut tree, button("hello"));
        assert!(matches!(
            tree.get(root).unwrap().descriptor(),
            ViewDescriptor::Button { .. }
        ));
        assert!(tree.get(root).unwrap().children.is_empty());
    }

    #[test]
    fn test_vstack_insert_child_at_end() {
        let mut tree = crate::declarative::ViewTree::new();
        build_tree(&mut tree, vstack([text("a"), text("b")]));
        let root = tree.root().unwrap();

        build_tree(&mut tree, vstack([text("a"), text("b"), text("c")]));
        let labels = collect_child_labels(&tree, root);
        assert_eq!(labels, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_vstack_remove_child_from_middle() {
        let mut tree = crate::declarative::ViewTree::new();
        build_tree(&mut tree, vstack([text("a"), text("b"), text("c")]));
        let root = tree.root().unwrap();

        build_tree(&mut tree, vstack([text("a"), text("c")]));
        let labels = collect_child_labels(&tree, root);
        assert_eq!(labels, vec!["a", "c"]);
    }

    #[test]
    fn test_vstack_reorder_children() {
        let mut tree = crate::declarative::ViewTree::new();
        build_tree(&mut tree, vstack([text("a"), text("b"), text("c")]));
        let root = tree.root().unwrap();
        let old_ids: Vec<_> = tree.get(root).unwrap().children.clone();

        build_tree(&mut tree, vstack([text("c"), text("a"), text("b")]));

        let labels = collect_child_labels(&tree, root);
        assert_eq!(labels, vec!["c", "a", "b"]);

        // Unkeyed: nodes are matched by index, not value
        let new_ids: Vec<_> = tree.get(root).unwrap().children.clone();
        assert_eq!(old_ids[0], new_ids[0], "unkeyed: index 0 reused");
        assert_eq!(old_ids[1], new_ids[1], "unkeyed: index 1 reused");
        assert_eq!(old_ids[2], new_ids[2], "unkeyed: index 2 reused");
    }

    #[test]
    fn test_keyed_insert_with_stable_keys() {
        use crate::declarative::constructors::{keyed, vstack_keyed};
        let mut tree = crate::declarative::ViewTree::new();
        build_tree(
            &mut tree,
            vstack_keyed(vec![keyed(1, text("a")), keyed(2, text("b"))]),
        );
        let root = tree.root().unwrap();
        let old_ids: Vec<_> = tree.get(root).unwrap().children.clone();
        assert_eq!(collect_child_keys(&tree, root), vec![Some(1), Some(2)]);

        build_tree(
            &mut tree,
            vstack_keyed(vec![
                keyed(1, text("a")),
                keyed(2, text("b")),
                keyed(3, text("c")),
            ]),
        );

        let new_ids: Vec<_> = tree.get(root).unwrap().children.clone();
        assert_eq!(old_ids[0], new_ids[0], "key=1 should reuse same node");
        assert_eq!(old_ids[1], new_ids[1], "key=2 should reuse same node");
        assert_eq!(collect_child_labels(&tree, root), vec!["a", "b", "c"]);
    }

    #[test]
    fn test_keyed_remove_with_stable_keys() {
        use crate::declarative::constructors::{keyed, vstack_keyed};
        let mut tree = crate::declarative::ViewTree::new();
        build_tree(
            &mut tree,
            vstack_keyed(vec![
                keyed(1, text("a")),
                keyed(2, text("b")),
                keyed(3, text("c")),
            ]),
        );
        let root = tree.root().unwrap();
        let old_ids: Vec<_> = tree.get(root).unwrap().children.clone();

        build_tree(
            &mut tree,
            vstack_keyed(vec![keyed(1, text("a")), keyed(3, text("c"))]),
        );

        let new_ids: Vec<_> = tree.get(root).unwrap().children.clone();
        assert_eq!(old_ids[0], new_ids[0], "key=1 should reuse same node");
        assert_eq!(
            old_ids[2], new_ids[1],
            "key=3 should reuse same node (now at index 1)"
        );
        assert_eq!(collect_child_labels(&tree, root), vec!["a", "c"]);
    }

    #[test]
    fn test_keyed_reorder_matches_by_key_not_index() {
        use crate::declarative::constructors::{keyed, vstack_keyed};
        let mut tree = crate::declarative::ViewTree::new();
        build_tree(
            &mut tree,
            vstack_keyed(vec![
                keyed(1, text("a")),
                keyed(2, text("b")),
                keyed(3, text("c")),
            ]),
        );
        let root = tree.root().unwrap();
        let old_ids: Vec<_> = tree.get(root).unwrap().children.clone();

        build_tree(
            &mut tree,
            vstack_keyed(vec![
                keyed(3, text("c")),
                keyed(1, text("a")),
                keyed(2, text("b")),
            ]),
        );

        let new_ids: Vec<_> = tree.get(root).unwrap().children.clone();
        assert_eq!(
            old_ids[2], new_ids[0],
            "key=3 should be reused at new position 0"
        );
        assert_eq!(
            old_ids[0], new_ids[1],
            "key=1 should be reused at new position 1"
        );
        assert_eq!(
            old_ids[1], new_ids[2],
            "key=2 should be reused at new position 2"
        );
        assert_eq!(collect_child_labels(&tree, root), vec!["c", "a", "b"]);
        assert_eq!(
            collect_child_keys(&tree, root),
            vec![Some(3), Some(1), Some(2)]
        );
    }
}
