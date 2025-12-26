use katla_ecs::{EntityId, World};

use crate::{
    application::Model,
    components::{DrawableComponent, NameComponent, TransformComponent},
};

pub struct ModelEntity {
    _entity: EntityId,
}

impl ModelEntity {
    pub fn new(world: &mut World, model: Model) -> Self {
        let entity = world.create_entity();

        world.add_component(
            entity,
            TransformComponent::new(katla_math::Transform::default()),
        );
        world.add_component(entity, DrawableComponent(Box::new(model)));
        world.add_component(entity, NameComponent::new("Model"));
        Self { _entity: entity }
    }
}
