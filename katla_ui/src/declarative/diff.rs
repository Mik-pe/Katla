use super::descriptor::ViewDescriptor;
use super::state::ViewId;

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

pub enum Patch {
    Insert {
        parent: Option<ViewId>,
        index: usize,
        widget: Box<dyn super::widget::Widget>,
    },
    Update {
        node: ViewId,
        widget: Box<dyn super::widget::Widget>,
    },
    Remove {
        node: ViewId,
    },
}
