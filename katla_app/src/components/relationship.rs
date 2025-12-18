use katla_ecs::{Component, EntityId};

#[derive(Component, Default)]
pub struct Children {
    pub children: Vec<EntityId>,
}

impl Children {
    pub fn new(children: Vec<EntityId>) -> Self {
        Children { children: children }
    }
}

#[derive(Component, Default)]
pub struct Parent {
    pub parent: EntityId,
}

impl Parent {
    pub fn new(parent: EntityId) -> Self {
        Parent { parent: parent }
    }
}
