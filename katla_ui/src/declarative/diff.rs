use super::descriptor::ViewDescriptor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffAction {
    Update,
    RecurseChildren,
    Replace,
}

pub fn diff_descriptor(old: &ViewDescriptor, new: &ViewDescriptor) -> DiffAction {
    match (old, new) {
        // Same leaf variants -> Update
        (ViewDescriptor::Empty, ViewDescriptor::Empty) => DiffAction::Update,
        (ViewDescriptor::Text { .. }, ViewDescriptor::Text { .. }) => DiffAction::Update,
        (ViewDescriptor::Button { .. }, ViewDescriptor::Button { .. }) => DiffAction::Update,
        (ViewDescriptor::LabeledSlider { .. }, ViewDescriptor::LabeledSlider { .. }) => {
            DiffAction::Update
        }
        (ViewDescriptor::Slider { .. }, ViewDescriptor::Slider { .. }) => DiffAction::Update,
        (ViewDescriptor::Vec3Slider { .. }, ViewDescriptor::Vec3Slider { .. }) => {
            DiffAction::Update
        }
        (ViewDescriptor::Toggle { .. }, ViewDescriptor::Toggle { .. }) => DiffAction::Update,
        (ViewDescriptor::TextField { .. }, ViewDescriptor::TextField { .. }) => DiffAction::Update,
        (ViewDescriptor::Progress { .. }, ViewDescriptor::Progress { .. }) => DiffAction::Update,
        (ViewDescriptor::ColorPicker { .. }, ViewDescriptor::ColorPicker { .. }) => {
            DiffAction::Update
        }
        (ViewDescriptor::ImageButton { .. }, ViewDescriptor::ImageButton { .. }) => {
            DiffAction::Update
        }
        (ViewDescriptor::RadioButton { .. }, ViewDescriptor::RadioButton { .. }) => {
            DiffAction::Update
        }
        (ViewDescriptor::Image { .. }, ViewDescriptor::Image { .. }) => DiffAction::Update,
        (ViewDescriptor::PropertyRow { .. }, ViewDescriptor::PropertyRow { .. }) => {
            DiffAction::Update
        }
        (ViewDescriptor::Separator { .. }, ViewDescriptor::Separator { .. }) => DiffAction::Update,
        (ViewDescriptor::Icon { .. }, ViewDescriptor::Icon { .. }) => DiffAction::Update,
        (ViewDescriptor::Section { .. }, ViewDescriptor::Section { .. }) => {
            DiffAction::RecurseChildren
        }
        (ViewDescriptor::Custom(_), ViewDescriptor::Custom(_)) => DiffAction::Update,

        // TransitionContainer -> RecurseChildren (has single child)
        (
            ViewDescriptor::TransitionContainer { .. },
            ViewDescriptor::TransitionContainer { .. },
        ) => DiffAction::RecurseChildren,

        // Container variants -> RecurseChildren
        (ViewDescriptor::HStack(_), ViewDescriptor::HStack(_)) => DiffAction::RecurseChildren,
        (ViewDescriptor::VStack(_), ViewDescriptor::VStack(_)) => DiffAction::RecurseChildren,
        (ViewDescriptor::ZStack(_), ViewDescriptor::ZStack(_)) => DiffAction::RecurseChildren,
        (ViewDescriptor::ScrollView(_), ViewDescriptor::ScrollView(_)) => {
            DiffAction::RecurseChildren
        }
        (ViewDescriptor::Panel(_), ViewDescriptor::Panel(_)) => DiffAction::RecurseChildren,
        (ViewDescriptor::Overlay(_), ViewDescriptor::Overlay(_)) => DiffAction::RecurseChildren,
        (ViewDescriptor::StatusBar(_), ViewDescriptor::StatusBar(_)) => DiffAction::RecurseChildren,
        (ViewDescriptor::DraggablePanel(_), ViewDescriptor::DraggablePanel(_)) => {
            DiffAction::RecurseChildren
        }
        (ViewDescriptor::Selectable { .. }, ViewDescriptor::Selectable { .. }) => {
            DiffAction::RecurseChildren
        }

        // Self-managed rendering -> Update
        (ViewDescriptor::MenuBar(_), ViewDescriptor::MenuBar(_)) => DiffAction::Update,
        (ViewDescriptor::TreeView(_), ViewDescriptor::TreeView(_)) => DiffAction::Update,
        (ViewDescriptor::ContextMenu(_), ViewDescriptor::ContextMenu(_)) => DiffAction::Update,
        (ViewDescriptor::TabBar(_), ViewDescriptor::TabBar(_)) => DiffAction::RecurseChildren,
        (ViewDescriptor::Grid(_), ViewDescriptor::Grid(_)) => DiffAction::RecurseChildren,

        // Container with single child -> RecurseChildren
        (ViewDescriptor::Modal(_), ViewDescriptor::Modal(_)) => DiffAction::RecurseChildren,

        // Different variants -> Replace
        _ => DiffAction::Replace,
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

    #[test]
    fn test_diff_same_text_is_update() {
        let a = text("hello");
        let b = text("world");
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_button_is_update() {
        let a = button("ok");
        let b = button("cancel");
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
        assert_eq!(diff_descriptor(&empty(), &empty()), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_image_is_update() {
        use katla_math::Color;
        let a = image(TextureId(1), Color::WHITE);
        let b = image(TextureId(2), Color::BLACK);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_separator_is_update() {
        let a = separator_horizontal();
        let b = separator_vertical();
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_icon_is_update() {
        let a = icon('A');
        let b = icon('B');
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_progress_is_update() {
        let a = progress(0.5, 0.0..=1.0);
        let b = progress(0.8, 0.0..=1.0);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    #[test]
    fn test_diff_same_custom_is_update() {
        fn noop(_: &mut crate::UiContext, _: katla_math::Rect2D) {}
        let a = ViewDescriptor::Custom(noop);
        let b = ViewDescriptor::Custom(noop);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    // -- Same container variant → RecurseChildren --

    #[test]
    fn test_diff_same_hstack_is_recurse() {
        let a = hstack([text("a")]);
        let b = hstack([text("b")]);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_vstack_is_recurse() {
        let a = vstack([text("a")]);
        let b = vstack([text("b")]);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_zstack_is_recurse() {
        use super::super::descriptor::Alignment;
        let a = zstack([(Alignment::Center, text("a"))]);
        let b = zstack([(Alignment::Center, text("b"))]);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_grid_is_recurse() {
        let a = grid(2, katla_math::Vec2::new(100.0, 100.0), [text("a")]);
        let b = grid(2, katla_math::Vec2::new(100.0, 100.0), [text("b")]);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_selectable_is_recurse() {
        let a = selectable(text("a"));
        let b = selectable(text("b"));
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_section_is_recurse() {
        let sid = StateId::test_id();
        let a = section("title", vstack([text("a")]), sid);
        let b = section("title", vstack([text("b")]), sid);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    #[test]
    fn test_diff_same_tab_bar_is_recurse() {
        let sid = StateId::test_id();
        let a = tab_bar(vec![tab_item("A"), tab_item("B")], sid, empty());
        let b = tab_bar(vec![tab_item("C"), tab_item("D")], sid, empty());
        assert_eq!(diff_descriptor(&a, &b), DiffAction::RecurseChildren);
    }

    // -- Self-managed containers → Update --

    #[test]
    fn test_diff_same_menubar_is_update() {
        let a = menubar(vec![]);
        let b = menubar(vec![]);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Update);
    }

    // -- Different variants → Replace --

    #[test]
    fn test_diff_text_to_button_is_replace() {
        let a = text("hello");
        let b = button("hello");
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_diff_hstack_to_vstack_is_replace() {
        let a = hstack([text("a")]);
        let b = vstack([text("a")]);
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_diff_empty_to_text_is_replace() {
        let a = empty();
        let b = text("hello");
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_diff_text_to_empty_is_replace() {
        let a = text("hello");
        let b = empty();
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_diff_image_to_icon_is_replace() {
        use katla_math::Color;
        let a = image(TextureId(1), Color::WHITE);
        let b = icon('X');
        assert_eq!(diff_descriptor(&a, &b), DiffAction::Replace);
    }

    #[test]
    fn test_keyed_child_descriptor_has_key() {
        use super::super::descriptor::ChildDescriptor;
        let cd = super::super::constructors::keyed(42, text("hello"));
        assert_eq!(cd.key, Some(42));
        let cd_nokey: ChildDescriptor = ChildDescriptor::from(text("hello"));
        assert_eq!(cd_nokey.key, None);
    }
}
