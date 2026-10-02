//! Direct queries and change-filtered iteration.

use super::World;
use crate::EntityId;
use std::collections::HashSet;

impl World {
    /// Creates a query for iterating over entities with specific components.
    ///
    /// Queries provide efficient iteration over entities with specific component combinations.
    ///
    /// # Example
    ///
    /// ```
    /// use katla_ecs::{World, Component};
    ///
    /// #[derive(Component, Default)]
    /// struct Position { x: f32, y: f32 }
    ///
    /// #[derive(Component, Default)]
    /// struct Velocity { dx: f32, dy: f32 }
    ///
    /// let mut world = World::new();
    /// let id = world.spawn((Position::default(), Velocity::default()));
    ///
    /// // Query and modify entities
    /// for (_entity, pos, vel) in world.query::<(&mut Position, &Velocity)>() {
    ///     pos.x += vel.dx;
    ///     pos.y += vel.dy;
    /// }
    /// ```
    pub fn query<Q: crate::query::QueryData>(&mut self) -> Q::Iter<'_> {
        self.storage.get_mut().query::<Q>()
    }

    /// Creates a typed query view whose references are lent for each access.
    /// The exclusive World borrow freezes structure until the view is dropped.
    pub fn query_typed<D, F>(&mut self) -> crate::typed_query::QueryView<'_, D, F>
    where
        D: crate::typed_query::QueryDescriptor,
        F: crate::query::QueryFilter + 'static,
    {
        crate::typed_query::Query::<D, F>::accesses();
        let mut cache = crate::typed_query::QueryCache::default();
        // SAFETY: The exclusive World borrow holds every component claim and
        // prevents structural mutation for the entire returned view's lifetime.
        unsafe {
            crate::typed_query::PreparedQuery::<D, F>::prepare(
                &*self.storage.get(),
                self.structural_epoch,
                &mut cache,
            )
            .into_view()
        }
    }

    /// Read-only query for iterating over entities with specific components.
    ///
    /// Unlike [`query`](Self::query), this takes `&self` and only supports
    /// immutable access patterns. Use this when you need to iterate components
    /// from a shared reference to the world (e.g., in UI callbacks or
    /// serialization that take `&World` or `&Application`).
    ///
    /// The `Q` type parameter must implement [`ImmutableQuery`](crate::query::ImmutableQuery),
    /// which is a sealed trait implemented only for patterns that yield shared
    /// references (`&T`, `(&T, &U)`, etc.). Calling `query_ref::<&mut T>()` will
    /// fail to compile because `&mut T` does not implement `ImmutableQuery`.
    pub fn query_ref<Q>(&self) -> Q::Iter<'_>
    where
        Q: crate::query::QueryData + crate::query::ImmutableQuery,
    {
        // SAFETY: The sealed immutable query cannot write any component column.
        Q::fetch_ref(unsafe { &*self.storage.get() })
    }

    /// Read-only parallel query using rayon for concurrent iteration.
    ///
    /// Takes `&self` and returns a `rayon::iter::ParallelIterator` over entities
    /// with the specified component combination. Only supports immutable
    /// access patterns for soundness in parallel contexts.
    ///
    /// # Example
    ///
    /// ```
    /// use katla_ecs::{World, Component};
    /// use rayon::iter::ParallelIterator;
    ///
    /// #[derive(Component, Default)]
    /// struct Position { x: f32, y: f32 }
    ///
    /// #[derive(Component, Default)]
    /// struct Velocity { dx: f32, dy: f32 }
    ///
    /// let mut world = World::new();
    /// world.spawn((Position { x: 1.0, y: 2.0 }, Velocity { dx: 0.1, dy: 0.2 }));
    /// world.spawn((Position { x: 3.0, y: 4.0 }, Velocity { dx: 0.3, dy: 0.4 }));
    ///
    /// let count = world.par_query::<(&Position, &Velocity)>().count();
    /// assert_eq!(count, 2);
    /// ```
    pub fn par_query<Q>(&self) -> impl rayon::iter::ParallelIterator<Item = Q::Item<'_>>
    where
        Q: crate::query::ParQueryData,
    {
        // SAFETY: Parallel query patterns only produce shared component references.
        Q::par_fetch(unsafe { &*self.storage.get() })
    }

    /// Queries only entities whose components have changed since the last `clear_changed()` call.
    ///
    /// Uses the same query syntax as [`query`](Self::query) but filters results to only
    /// include entities where at least one queried component type was mutated (via
    /// `add_component` or `get_component_mut`) since the last frame.
    ///
    /// Change detection is automatically reset at the end of each [`update`](Self::update) call.
    ///
    /// # Example
    ///
    /// ```
    /// use katla_ecs::{World, Component};
    ///
    /// #[derive(Component, Default)]
    /// struct Transform { x: f32, y: f32 }
    ///
    /// let mut world = World::new();
    /// let id = world.spawn((Transform::default(),));
    ///
    /// // Only process entities whose Transform was added or mutably accessed
    /// world.clear_changed();
    /// world.get_component_mut::<Transform>(id);
    ///
    /// let changed: Vec<_> = world.query_changed::<&Transform>().collect();
    /// assert_eq!(changed.len(), 1);
    /// ```
    pub fn query_changed<Q>(&mut self) -> QueryChangedIter<'_, Q>
    where
        Q: crate::query::QueryData,
    {
        let type_ids = Q::type_ids_for_changed();
        let mut changed_ids = HashSet::new();
        self.storage
            .get_mut()
            .collect_changed_entity_ids_into(&type_ids, &mut changed_ids);

        QueryChangedIter {
            inner: self.storage.get_mut().query::<Q>(),
            changed_ids,
        }
    }

    /// Query entities with additional filter conditions.
    ///
    /// Like [`query`](Self::query), but applies [`With<T>`](crate::query::With) and
    /// [`Without<T>`](crate::query::Without) filters to exclude entities that don't match.
    /// Filter types produce no output data — they only control which entities appear.
    ///
    /// # Example
    ///
    /// ```
    /// use katla_ecs::{World, Component, Without};
    ///
    /// #[derive(Component, Default)]
    /// struct Position { x: f32, y: f32 }
    ///
    /// #[derive(Component, Default)]
    /// struct Velocity { dx: f32, dy: f32 }
    ///
    /// #[derive(Component, Default)]
    /// struct Static;
    ///
    /// let mut world = World::new();
    /// world.spawn((Position { x: 1.0, y: 0.0 }, Velocity { dx: 1.0, dy: 0.0 }));
    /// world.spawn((Position { x: 5.0, y: 0.0 }, Static));
    ///
    /// for (_id, pos) in world.query_filtered::<&Position, Without<Static>>() {
    ///     assert_eq!(pos.x, 1.0);
    /// }
    /// ```
    pub fn query_filtered<Q, F>(&mut self) -> crate::query::FilteredQueryIter<'_, Q, F>
    where
        Q: crate::query::QueryData,
        F: crate::query::QueryFilter,
    {
        crate::query::assert_filter_query_disjoint::<Q, F>();
        let storage_ptr = self.storage.get() as *const _;
        let inner = self.storage.get_mut().query::<Q>();
        crate::query::FilteredQueryIter {
            inner,
            storage_ptr,
            _filter: std::marker::PhantomData,
        }
    }

    /// Resets change detection tracking for all component types.
    ///
    /// After this call, `query_changed` will return an empty iterator until
    /// components are next mutated. This is called automatically at the end
    /// of each [`update`](Self::update) call.
    pub fn clear_changed(&mut self) {
        self.storage.get_mut().clear_changed();
    }
}

/// Iterator for `query_changed` that filters query results to only include
/// entities whose components have changed since the last `clear_changed()` call.
///
/// For multi-component queries, an entity is included if **any** of its queried
/// component types have been mutated (union semantics).
pub struct QueryChangedIter<'a, Q: crate::query::QueryData> {
    inner: Q::Iter<'a>,
    changed_ids: HashSet<EntityId>,
}

impl<'a, Q: crate::query::QueryData> Iterator for QueryChangedIter<'a, Q> {
    type Item = Q::Item<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.inner.next()?;
            let entity_id = Q::entity_id_from_item(&item);
            if self.changed_ids.contains(&entity_id) {
                return Some(item);
            }
        }
    }
}
