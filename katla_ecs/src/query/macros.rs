//! Reference-pattern projection into the common prepared query engine.
macro_rules! impl_legacy_tuple {
    ($($T:ident:$index:tt),+) => {
        impl<$($T:QueryElement),+> sealed::Sealed for ($($T,)+) {}
        impl<$($T:QueryElement),+> QueryData for ($($T,)+) {
            type Item<'a>=(EntityId,$(<$T::Descriptor as QueryDescriptor>::Item<'a>,)+);
            type Iter<'a>=QueryIter<'a,Self>;
            fn fetch(storage:&mut ComponentStorageManager)->Self::Iter<'_> { prepare::<Self>(storage) }
            fn type_ids_for_changed()->Vec<TypeId> {
                let accesses=<( $($T::Descriptor,)+ ) as QueryDescriptor>::accesses();
                accesses.into_iter().map(|a|match a { crate::ComponentAccess::Read(id)|crate::ComponentAccess::Write(id)=>id }).collect()
            }
            fn entity_id_from_item(item:&Self::Item<'_>)->EntityId { item.0 }
        }
        impl<$($T:QueryElement),+> QueryRows for ($($T,)+) {
            type Descriptor=($($T::Descriptor,)+);
            unsafe fn row<'a>(id:EntityId,pointers:<Self::Descriptor as QueryDescriptor>::Pointers)->Self::Item<'a> {
                (id,$(unsafe { $T::Descriptor::borrow(pointers.$index) },)+)
            }
        }
        impl<$($T:Component),+> ImmutableQuery for ($(&$T,)+) {
            fn fetch_ref(storage:&ComponentStorageManager)->Self::Iter<'_> { prepare::<Self>(storage) }
        }
    }
}
