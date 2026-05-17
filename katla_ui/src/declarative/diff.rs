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
        (ViewDescriptor::Slider { .. }, ViewDescriptor::Slider { .. }) => DiffAction::Update,
        (ViewDescriptor::Toggle { .. }, ViewDescriptor::Toggle { .. }) => DiffAction::Update,
        (ViewDescriptor::TextField { .. }, ViewDescriptor::TextField { .. }) => DiffAction::Update,
        (ViewDescriptor::Progress { .. }, ViewDescriptor::Progress { .. }) => DiffAction::Update,
        (ViewDescriptor::ColorPicker { .. }, ViewDescriptor::ColorPicker { .. }) => {
            DiffAction::Update
        }
        (ViewDescriptor::Image { .. }, ViewDescriptor::Image { .. }) => DiffAction::Update,
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
