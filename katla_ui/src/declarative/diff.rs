use super::state::ViewId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffAction {
    Update,
    RecurseChildren,
    Replace,
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
