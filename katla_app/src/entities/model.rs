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
        // Extract the transform data to create TransformComponent
        let transform_ref = &model.transform;
        // Create a new Transform with the same values (Transform has no Clone, but we can reconstruct)
        let position = transform_ref.position;
        let rotation = transform_ref.rotation;
        let scale = transform_ref.scale;
        let transform = katla_math::Transform {
            position,
            rotation,
            scale,
        };

        world.add_component(entity, TransformComponent::new(transform));
        // Keep the original model with its transform intact for rendering
        world.add_component(entity, DrawableComponent(Box::new(model)));
        world.add_component(entity, NameComponent::new("Model"));
        Self { _entity: entity }
    }

    pub fn new_with_transform(
        world: &mut World,
        model: Model,
        transform: katla_math::Transform,
    ) -> Self {
        let entity = world.create_entity();

        world.add_component(entity, DrawableComponent(Box::new(model)));
        world.add_component(entity, TransformComponent::new(transform));
        world.add_component(entity, NameComponent::new("Model"));
        Self { _entity: entity }
    }
}
