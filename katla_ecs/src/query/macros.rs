//! Declarative macros for generating query iterator types and `QueryData`
//! implementations across arities 1–8.
//!
//! Three macro families cover all mutability permutations:
//!
//! - [`impl_query_iter_arity1!`] — arity 1 special case (`&T` / `&mut T`)
//! - [`impl_query_iter_all_ref!`] — all-immutable tuples `(&T1, &T2, …)`
//! - [`impl_query_iter_single_mut!`] — exactly one `&mut` at an arbitrary position
//! - [`impl_query_iter_double_mut!`] — exactly two `&mut` components
//!
//! Adding a 9th component is a one-line macro invocation per desired permutation
//! in `mod.rs`.

// ── Internal helpers ──────────────────────────────────────────────────────

/// Emits `assert_ne!` for every unique pair of the supplied type identifiers.
/// Generates exactly C(N,2) comparisons — each element against all following elements.
macro_rules! assert_all_ne {
    ($only:ident) => {};
    ($first:ident, $second:ident $(, $rest:ident)*) => {
        assert_ne!(TypeId::of::<$first>(), TypeId::of::<$second>(), "Cannot query the same component type twice");
        $(
            assert_ne!(TypeId::of::<$first>(), TypeId::of::<$rest>(), "Cannot query the same component type twice");
        )*
        assert_all_ne!($second $(, $rest)*);
    };
}

/// Builds a `Vec<TypeId>` from the given types.
macro_rules! type_ids_vec {
    ($($Ti:ident),+) => { vec![$(TypeId::of::<$Ti>()),+] };
}

// ── Arity 1 (single component) ────────────────────────────────────────────

macro_rules! impl_query_iter_arity1 {
    () => {
        pub struct QueryIter1Mut<'a, T: Component> {
            iter: std::slice::IterMut<'a, (EntityId, T)>,
        }
        impl<'a, T: Component> Iterator for QueryIter1Mut<'a, T> {
            type Item = (EntityId, &'a mut T);
            fn next(&mut self) -> Option<Self::Item> {
                self.iter.next().map(|(id, c)| (*id, c))
            }
        }
        impl<T: Component + 'static> QueryData for &mut T {
            type Item<'a> = (EntityId, &'a mut T);
            type Iter<'a> = QueryIter1Mut<'a, T>;
            fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
                match storage.get_storage_mut::<T>() {
                    Some(s) => QueryIter1Mut {
                        iter: s.components_vec_mut().iter_mut(),
                    },
                    None => QueryIter1Mut {
                        iter: [].iter_mut(),
                    },
                }
            }
            fn type_ids_for_changed() -> Vec<TypeId> {
                vec![TypeId::of::<T>()]
            }
            fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId {
                item.0
            }
        }

        pub struct QueryIter1<'a, T: Component> {
            iter: std::slice::Iter<'a, (EntityId, T)>,
        }
        impl<'a, T: Component> Iterator for QueryIter1<'a, T> {
            type Item = (EntityId, &'a T);
            fn next(&mut self) -> Option<Self::Item> {
                self.iter.next().map(|(id, c)| (*id, c))
            }
        }
        impl<T: Component + 'static> QueryData for &T {
            type Item<'a> = (EntityId, &'a T);
            type Iter<'a> = QueryIter1<'a, T>;
            fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
                match storage.get_storage::<T>() {
                    Some(s) => QueryIter1 {
                        iter: s.components_vec().iter(),
                    },
                    None => QueryIter1 { iter: [].iter() },
                }
            }
            fn type_ids_for_changed() -> Vec<TypeId> {
                vec![TypeId::of::<T>()]
            }
            fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId {
                item.0
            }
        }
    };
}

// ── Arity N ≥ 2: all-ref iterator ─────────────────────────────────────────
//
// Generates `QueryIterN` struct + `impl QueryData for (&T1, &T2, …)`.
// Iteration is driven by T1's component vec; T2..TN are looked up by EntityId.

macro_rules! impl_query_iter_all_ref {
    ($N:tt, $T1:ident, $T2:ident $(, $Ti:ident)*) => {
        paste! {
            #[allow(non_snake_case)]
            pub struct [<QueryIter $N>]< 'a,
                $T1: Component, $T2: Component $(, $Ti: Component)*
            > {
                [<storage_ $T2:lower>]: Option<&'a ComponentStorage<$T2>>,
                $(
                    [<storage_ $Ti:lower>]: Option<&'a ComponentStorage<$Ti>>,
                )*
                iter1: std::slice::Iter<'a, (EntityId, $T1)>,
            }

            #[allow(non_snake_case)]
            impl<'a, $T1: Component, $T2: Component $(, $Ti: Component)*> Iterator
                for [<QueryIter $N>]< 'a, $T1, $T2, $($Ti),*>
            {
                type Item = (EntityId, &'a $T1, &'a $T2, $(&'a $Ti),*);

                fn next(&mut self) -> Option<Self::Item> {
                    let [<storage_ $T2:lower>] = self.[<storage_ $T2:lower>].as_ref()?;
                    $(
                        let [<storage_ $Ti:lower>] = self.[<storage_ $Ti:lower>].as_ref()?;
                    )*
                    loop {
                        let (entity_id, component1) = self.iter1.next()?;
                        let Some([<component_ $T2:lower>]) = [<storage_ $T2:lower>].get(*entity_id) else { continue; };
                        $(
                            let Some([<component_ $Ti:lower>]) = [<storage_ $Ti:lower>].get(*entity_id) else { continue; };
                        )*
                        return Some((*entity_id, component1, [<component_ $T2:lower>], $([<component_ $Ti:lower>]),*));
                    }
                }
            }

            impl<$T1: Component + 'static, $T2: Component + 'static $(, $Ti: Component + 'static)*> QueryData
                for (& $T1, & $T2, $(& $Ti),*)
            {
                type Item<'a> = (EntityId, &'a $T1, &'a $T2, $(&'a $Ti),*);
                type Iter<'a> = [<QueryIter $N>]< 'a, $T1, $T2, $($Ti),*>;

                fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
                    assert_all_ne!($T1, $T2 $(, $Ti)*);
                    let s1 = storage.get_storage::<$T1>();
                    let [<s_ $T2:lower>] = storage.get_storage::<$T2>();
                    $(
                        let [<s_rest_ $Ti:lower>] = storage.get_storage::<$Ti>();
                    )*
                    if let (Some(s1), Some([<s_ $T2:lower>]), $(Some([<s_rest_ $Ti:lower>])),*) =
                        (s1, [<s_ $T2:lower>], $([<s_rest_ $Ti:lower>]),*)
                    {
                        [<QueryIter $N>] {
                            [<storage_ $T2:lower>]: Some([<s_ $T2:lower>]),
                            $([<storage_ $Ti:lower>]: Some([<s_rest_ $Ti:lower>]),)*
                            iter1: s1.components_vec().iter(),
                        }
                    } else {
                        [<QueryIter $N>] {
                            [<storage_ $T2:lower>]: None,
                            $([<storage_ $Ti:lower>]: None,)*
                            iter1: [].iter(),
                        }
                    }
                }
                fn type_ids_for_changed() -> Vec<TypeId> {
                    type_ids_vec!($T1, $T2 $(, $Ti)*)
                }
                fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId { item.0 }
            }
        }
    };
}

// ── Arity N ≥ 2: single-mut iterator (generic) ───────────────────────────
//
// Generates a query iterator with exactly one `&mut` component at an arbitrary
// position. The mutable component drives iteration; all other components are
// immutable lookups by EntityId.
//
// # Invocation
//
// ```text
// // (&mut T1, &T2, &T3) — mut at position 0
// impl_query_iter_single_mut!(3, T1Mut, [], T1, [T2, T3]);
//
// // (&T1, &mut T2, &T3) — mut at position 1
// impl_query_iter_single_mut!(3, T2Mut, [T1], T2, [T3]);
//
// // (&T1, &T2, &mut T3) — mut at position 2
// impl_query_iter_single_mut!(3, T3Mut, [T1, T2], T3, []);
// ```
//
// Parameters:
// - `$N`         — arity (token, e.g. `3`)
// - `$suffix`    — suffix for the struct name (e.g. `T2Mut`)
// - `[$($pre),*]` — types before the mut position (all immutable)
// - `$mut_type`  — the mutable type (drives iteration)
// - `[$($post),*]`— types after the mut position (all immutable)
//
// All types are listed in their original user-facing order.

macro_rules! impl_query_iter_single_mut {
    ($N:tt, $suffix:ident, [$($pre:ident),*], $mut_type:ident, [$($post:ident),*]) => {
        paste! {
            // Build the full type-parameter list using paste to avoid trailing-comma
            // issues when either $pre or $post is empty.
            #[allow(non_snake_case)]
            pub struct [<QueryIter $N $suffix>]< 'a,
                $($pre: Component,)*
                $mut_type: Component
                $(, $post: Component)*
            > {
                $(
                    [<storage_ $pre:lower>]: Option<&'a ComponentStorage<$pre>>,
                )*
                $(
                    [<storage_ $post:lower>]: Option<&'a ComponentStorage<$post>>,
                )*
                iter_driver: std::slice::IterMut<'a, (EntityId, $mut_type)>,
            }

            #[allow(non_snake_case)]
            impl<'a, $($pre: Component,)* $mut_type: Component $(, $post: Component)*> Iterator
                for [<QueryIter $N $suffix>]< 'a, $($pre,)* $mut_type $(, $post)*>
            {
                type Item = (
                    EntityId,
                    $(&'a $pre,)*
                    &'a mut $mut_type
                    $(, &'a $post)*
                );

                fn next(&mut self) -> Option<Self::Item> {
                    $(
                        let [<storage_ $pre:lower>] = self.[<storage_ $pre:lower>].as_ref()?;
                    )*
                    $(
                        let [<storage_ $post:lower>] = self.[<storage_ $post:lower>].as_ref()?;
                    )*
                    loop {
                        let (entity_id, driver_component) = self.iter_driver.next()?;
                        $(
                            let Some([<component_ $pre:lower>]) =
                                [<storage_ $pre:lower>].get(*entity_id)
                            else { continue; };
                        )*
                        $(
                            let Some([<component_ $post:lower>]) =
                                [<storage_ $post:lower>].get(*entity_id)
                            else { continue; };
                        )*
                        return Some((
                            *entity_id,
                            $([<component_ $pre:lower>],)*
                            driver_component
                            $(, [<component_ $post:lower>])*
                        ));
                    }
                }
            }

            impl<
                $($pre: Component + 'static,)*
                $mut_type: Component + 'static
                $(, $post: Component + 'static)*
            > QueryData
                for ($(& $pre,)* &mut $mut_type $(, & $post)*)
            {
                type Item<'a> = (
                    EntityId,
                    $(&'a $pre,)*
                    &'a mut $mut_type
                    $(, &'a $post)*
                );
                type Iter<'a> = [<QueryIter $N $suffix>]< 'a, $($pre,)* $mut_type $(, $post)*>;

                fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
                    assert_all_ne!($($pre,)* $mut_type $(, $post)*);
                    unsafe {
                        let ptr = storage.borrow_ptr();
                        $(
                            let [<s_ $pre:lower>] = (*ptr).get_storage::<$pre>();
                        )*
                        let [<s_ $mut_type:lower>] = (*ptr).get_storage_mut::<$mut_type>();
                        $(
                            let [<s_ $post:lower>] = (*ptr).get_storage::<$post>();
                        )*
                        if let (
                            $(Some([<s_ $pre:lower>]),)*
                            Some([<s_ $mut_type:lower>])
                            $(, Some([<s_ $post:lower>]))*
                        ) = (
                            $([<s_ $pre:lower>],)*
                            [<s_ $mut_type:lower>]
                            $(, [<s_ $post:lower>])*
                        ) {
                            [<QueryIter $N $suffix>] {
                                $(
                                    [<storage_ $pre:lower>]: Some([<s_ $pre:lower>]),
                                )*
                                $(
                                    [<storage_ $post:lower>]: Some([<s_ $post:lower>]),
                                )*
                                iter_driver: [<s_ $mut_type:lower>].components_vec_mut().iter_mut(),
                            }
                        } else {
                            [<QueryIter $N $suffix>] {
                                $(
                                    [<storage_ $pre:lower>]: None,
                                )*
                                $(
                                    [<storage_ $post:lower>]: None,
                                )*
                                iter_driver: [].iter_mut(),
                            }
                        }
                    }
                }
                fn type_ids_for_changed() -> Vec<TypeId> {
                    type_ids_vec!($($pre,)* $mut_type $(, $post)*)
                }
                fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId { item.0 }
            }
        }
    };
}

// ── Arity N ≥ 2: double-mut iterator (generic) ───────────────────────────
//
// Generates a query iterator with exactly two `&mut` components. Uses an
// index-based approach with raw pointers to safely create two simultaneous
// mutable references into disjoint storages.
//
// # Invocation
//
// ```text
// // (&mut T1, &mut T2, &T3) — two leading muts
// impl_query_iter_double_mut!(3, T1T2Mut, [T1, T2], [T3]);
//
// // (&T1, &mut T2, &mut T3) — trailing muts
// impl_query_iter_double_mut!(3, T2T3Mut, [T1], [T2, T3]);
// ```
//
// Parameters:
// - `$N`           — arity
// - `$suffix`      — suffix for the struct name
// - `[$m1, $m2 $(, $mi)*]` — the mutable types (first two drive via index-based iteration)
// - `[$($refi),*]` — the immutable types (looked up by EntityId)

macro_rules! impl_query_iter_double_mut {
    ($N:tt, $suffix:ident, [$m1:ident, $m2:ident $(, $mi:ident)*], [$($refi:ident),*]) => {
        paste! {
            #[allow(non_snake_case)]
            pub struct [<QueryIter $N $suffix>]< 'a,
                $m1: Component,
                $m2: Component
                $(, $mi: Component)*
                $(, $refi: Component)*
            > {
                storage_m1: Option<&'a mut ComponentStorage<$m1>>,
                storage_m2_vec: Option<&'a mut Vec<(EntityId, $m2)>>,
                index: usize,
                $(
                    [<storage_ $mi:lower>]: Option<&'a mut ComponentStorage<$mi>>,
                )*
                $(
                    [<storage_ $refi:lower>]: Option<&'a ComponentStorage<$refi>>,
                )*
            }

            #[allow(non_snake_case)]
            impl<'a,
                $m1: Component,
                $m2: Component
                $(, $mi: Component)*
                $(, $refi: Component)*
            > Iterator
                for [<QueryIter $N $suffix>]< 'a, $m1, $m2 $(, $mi)* $(, $refi)*>
            {
                type Item = (
                    EntityId,
                    &'a mut $m1,
                    &'a mut $m2,
                    $(&'a mut $mi,)*
                    $(&'a $refi,)*
                );

                fn next(&mut self) -> Option<Self::Item> {
                    let storage_m1 = self.storage_m1.as_mut()?;
                    let storage_m2_vec = self.storage_m2_vec.as_mut()?;
                    $(
                        let [<storage_ $mi:lower>] = self.[<storage_ $mi:lower>].as_ref()?;
                    )*
                    $(
                        let [<storage_ $refi:lower>] = self.[<storage_ $refi:lower>].as_ref()?;
                    )*
                    while self.index < storage_m2_vec.len() {
                        let idx = self.index;
                        self.index += 1;
                        let entity_id = storage_m2_vec[idx].0;
                        if let Some(component_m1) = storage_m1.get_mut(entity_id) {
                            $(
                                let Some([<component_ $mi:lower>]) =
                                    [<storage_ $mi:lower>].get_mut(entity_id)
                                else { continue; };
                            )*
                            $(
                                let Some([<component_ $refi:lower>]) =
                                    [<storage_ $refi:lower>].get(entity_id)
                                else { continue; };
                            )*
                            // SAFETY: All mutable types have distinct TypeIds
                            // (disjoint storages). Each element is accessed at
                            // most once via the monotonically increasing index.
                            let c_m1_ptr = component_m1 as *mut $m1;
                            let c_m2_ptr = &mut storage_m2_vec[idx].1 as *mut $m2;
                            $(
                                let [<c_ $mi:lower>_ptr>] =
                                    [<component_ $mi:lower>] as *mut $mi;
                            )*
                            unsafe {
                                return Some((
                                    entity_id,
                                    &mut *c_m1_ptr,
                                    &mut *c_m2_ptr,
                                    $(&mut *[<c_ $mi:lower>_ptr>],)*
                                    $([<component_ $refi:lower>],)*
                                ));
                            }
                        }
                    }
                    None
                }
            }

            impl<
                $m1: Component + 'static,
                $m2: Component + 'static
                $(, $mi: Component + 'static)*
                $(, $refi: Component + 'static)*
            > QueryData
                for (&mut $m1, &mut $m2 $(, &mut $mi)* $(, & $refi)*)
            {
                type Item<'a> = (
                    EntityId,
                    &'a mut $m1,
                    &'a mut $m2,
                    $(&'a mut $mi,)*
                    $(&'a $refi,)*
                );
                type Iter<'a> = [<QueryIter $N $suffix>]< 'a, $m1, $m2 $(, $mi)* $(, $refi)*>;

                fn fetch(storage: &mut ComponentStorageManager) -> Self::Iter<'_> {
                    assert_all_ne!($m1, $m2 $(, $mi)* $(, $refi)*);
                    unsafe {
                        let ptr = storage.borrow_ptr();
                        let (s_m1, s_m2) =
                            ComponentStorageManager::get_two_storage_mut::<$m1, $m2>(ptr);
                        $(
                            let [<s_ $mi:lower>] = (*ptr).get_storage_mut::<$mi>();
                        )*
                        $(
                            let [<s_ $refi:lower>] = (*ptr).get_storage::<$refi>();
                        )*
                        if let (
                            Some(s_m1),
                            Some(s_m2)
                            $(, Some([<s_ $mi:lower>]))*
                            $(, Some([<s_ $refi:lower>]))*
                        ) = (
                            s_m1,
                            s_m2
                            $(, [<s_ $mi:lower>])*
                            $(, [<s_ $refi:lower>])*
                        ) {
                            [<QueryIter $N $suffix>] {
                                storage_m1: Some(s_m1),
                                storage_m2_vec: Some(s_m2.components_vec_mut()),
                                index: 0,
                                $(
                                    [<storage_ $mi:lower>]: Some([<s_ $mi:lower>]),
                                )*
                                $(
                                    [<storage_ $refi:lower>]: Some([<s_ $refi:lower>]),
                                )*
                            }
                        } else {
                            [<QueryIter $N $suffix>] {
                                storage_m1: None,
                                storage_m2_vec: None,
                                index: 0,
                                $(
                                    [<storage_ $mi:lower>]: None,
                                )*
                                $(
                                    [<storage_ $refi:lower>]: None,
                                )*
                            }
                        }
                    }
                }
                fn type_ids_for_changed() -> Vec<TypeId> {
                    type_ids_vec!($m1, $m2 $(, $mi)* $(, $refi)*)
                }
                fn entity_id_from_item(item: &Self::Item<'_>) -> EntityId { item.0 }
            }
        }
    };
}
