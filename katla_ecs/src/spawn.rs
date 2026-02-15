//! Spawn API for ergonomic entity creation.
//!
//! This module provides the `Spawnable` trait and implementations for tuples
//! of components, allowing batch entity creation with a single call.
//!
//! # Example
//!
//! ```ignore
//! use katla_ecs::{World, Component, Spawnable};
//!
//! #[derive(Component, Default)]
//! struct Transform { position: [f32; 3] }
//!
//! #[derive(Component, Default)]
//! struct Velocity { value: [f32; 3] }
//!
//! #[derive(Component)]
//! struct Health { value: f32 }
//!
//! let mut world = World::new();
//!
//! // Spawn entity with multiple components
//! let player = world.spawn((
//!     Transform::default(),
//!     Velocity::default(),
//!     Health { value: 100.0 },
//! ));
//! ```

use crate::components::Component;
use crate::entity::EntityId;
use crate::world::World;

/// Trait for types that can spawn an entity with components.
///
/// This is implemented for tuples of components up to size 8.
pub trait Spawnable {
    /// Spawns an entity in the world and returns its ID.
    fn spawn(self, world: &mut World) -> EntityId;
}

// Implement for 1-tuple (need to handle specially due to Rust's tuple rules)
impl<T: Component + 'static> Spawnable for (T,) {
    fn spawn(self, world: &mut World) -> EntityId {
        let id = world.create_entity();
        world.add_component(id, self.0);
        id
    }
}

macro_rules! impl_spawnable_tuple {
    ($($T:ident),*) => {
        impl<$($T: Component + 'static),*> Spawnable for ($($T),*) {
            #[allow(non_snake_case)]
            fn spawn(self, world: &mut World) -> EntityId {
                let id = world.create_entity();
                let ($($T),*) = self;
                $(world.add_component(id, $T);)*
                id
            }
        }
    };
}

// Generate implementations for tuples of size 2-8
impl_spawnable_tuple!(T1, T2);
impl_spawnable_tuple!(T1, T2, T3);
impl_spawnable_tuple!(T1, T2, T3, T4);
impl_spawnable_tuple!(T1, T2, T3, T4, T5);
impl_spawnable_tuple!(T1, T2, T3, T4, T5, T6);
impl_spawnable_tuple!(T1, T2, T3, T4, T5, T6, T7);
impl_spawnable_tuple!(T1, T2, T3, T4, T5, T6, T7, T8);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Component;

    #[derive(Component, Debug, PartialEq)]
    struct A(i32);

    #[derive(Component, Debug, PartialEq)]
    struct B(f32);

    #[derive(Component, Debug, PartialEq)]
    struct C(String);

    #[derive(Component, Debug, PartialEq)]
    struct D(bool);

    #[derive(Component, Debug, PartialEq)]
    struct E(i64);

    #[derive(Component, Debug, PartialEq)]
    struct F(i32);

    #[derive(Component, Debug, PartialEq)]
    struct G(i32);

    #[derive(Component, Debug, PartialEq)]
    struct H(i32);

    #[test]
    fn test_spawn_single() {
        let mut world = World::new();
        let id = (A(42),).spawn(&mut world);

        assert!(world.entity_exists(id));
        assert_eq!(world.get_component::<A>(id), Some(&A(42)));
    }

    #[test]
    fn test_spawn_two() {
        let mut world = World::new();
        let id = (A(1), B(2.0)).spawn(&mut world);

        assert!(world.entity_exists(id));
        assert_eq!(world.get_component::<A>(id), Some(&A(1)));
        assert_eq!(world.get_component::<B>(id), Some(&B(2.0)));
    }

    #[test]
    fn test_spawn_three() {
        let mut world = World::new();
        let id = (A(1), B(2.0), C("hello".to_string())).spawn(&mut world);

        assert!(world.entity_exists(id));
        assert_eq!(world.get_component::<A>(id), Some(&A(1)));
        assert_eq!(world.get_component::<B>(id), Some(&B(2.0)));
        assert_eq!(world.get_component::<C>(id), Some(&C("hello".to_string())));
    }

    #[test]
    fn test_spawn_four() {
        let mut world = World::new();
        let id = (A(1), B(2.0), C("test".to_string()), D(true)).spawn(&mut world);

        assert!(world.entity_exists(id));
        assert_eq!(world.get_component::<A>(id), Some(&A(1)));
        assert_eq!(world.get_component::<B>(id), Some(&B(2.0)));
        assert_eq!(world.get_component::<C>(id), Some(&C("test".to_string())));
        assert_eq!(world.get_component::<D>(id), Some(&D(true)));
    }

    #[test]
    fn test_spawn_eight() {
        let mut world = World::new();
        let id = (
            A(1),
            B(2.0),
            C("test".to_string()),
            D(true),
            E(5),
            F(6),
            G(7),
            H(8),
        )
            .spawn(&mut world);

        assert!(world.entity_exists(id));
        assert_eq!(world.get_component::<A>(id), Some(&A(1)));
        assert_eq!(world.get_component::<B>(id), Some(&B(2.0)));
        assert_eq!(world.get_component::<C>(id), Some(&C("test".to_string())));
        assert_eq!(world.get_component::<D>(id), Some(&D(true)));
        assert_eq!(world.get_component::<E>(id), Some(&E(5)));
        assert_eq!(world.get_component::<F>(id), Some(&F(6)));
        assert_eq!(world.get_component::<G>(id), Some(&G(7)));
        assert_eq!(world.get_component::<H>(id), Some(&H(8)));
    }

    #[test]
    fn test_spawn_multiple_entities() {
        let mut world = World::new();

        let id1 = (A(1), B(1.0)).spawn(&mut world);
        let id2 = (A(2), B(2.0)).spawn(&mut world);
        let id3 = (A(3), B(3.0)).spawn(&mut world);

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);

        assert_eq!(world.get_component::<A>(id1), Some(&A(1)));
        assert_eq!(world.get_component::<A>(id2), Some(&A(2)));
        assert_eq!(world.get_component::<A>(id3), Some(&A(3)));
    }
}
