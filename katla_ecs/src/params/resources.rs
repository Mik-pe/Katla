//! Required and optional resource borrows for typed systems.

use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use super::{ParamAccess, ParamContext, SystemParam, sealed};
use crate::{Resource, World};

/// Required shared resource parameter. R must support shared worker access.
pub struct Res<R>(PhantomData<R>);
/// Required mutable resource parameter. R must support worker ownership.
///
/// Caller-thread-only resources remain accessible through exclusive systems:
///
/// ```compile_fail
/// use std::rc::Rc;
/// use katla_ecs::{ResMut, SystemParam, TypedSystem};
/// struct InvalidResourceSystem;
/// impl TypedSystem for InvalidResourceSystem {
///     type Params = ResMut<Rc<()>>;
///     fn run(&mut self, _: <Self::Params as SystemParam>::Item<'_>, _: f32) {}
/// }
/// ```
pub struct ResMut<R>(PhantomData<R>);
/// A resource borrowed for one invocation.
pub struct ResourceRef<'w, R> {
    resource: &'w R,
}
/// A resource exclusively borrowed for one invocation.
pub struct ResourceMut<'w, R> {
    resource: &'w mut R,
}
impl<R> Deref for ResourceRef<'_, R> {
    type Target = R;
    fn deref(&self) -> &R {
        self.resource
    }
}
impl<R> Deref for ResourceMut<'_, R> {
    type Target = R;
    fn deref(&self) -> &R {
        self.resource
    }
}
impl<R> DerefMut for ResourceMut<'_, R> {
    fn deref_mut(&mut self) -> &mut R {
        self.resource
    }
}

impl<R: Resource + Sync> sealed::Sealed for Res<R> {}
impl<R: Resource + Sync> SystemParam for Res<R> {
    type State = ();
    type Item<'w> = ResourceRef<'w, R>;
    fn access(access: &mut ParamAccess) {
        access.resource::<R>(false);
    }
    fn init(_: &mut World) {}
    unsafe fn prepare<'w>(context: &ParamContext<'w>, _: &'w mut ()) -> Self::Item<'w> {
        ResourceRef {
            resource: context.resources.get::<R>().unwrap_or_else(|| {
                panic!(
                    "required resource {} is missing",
                    std::any::type_name::<R>()
                )
            }),
        }
    }
}
impl<R: Resource + Send> sealed::Sealed for ResMut<R> {}
impl<R: Resource + Send> SystemParam for ResMut<R> {
    type State = ();
    type Item<'w> = ResourceMut<'w, R>;
    fn access(access: &mut ParamAccess) {
        access.resource::<R>(true);
    }
    fn init(_: &mut World) {}
    unsafe fn prepare<'w>(context: &ParamContext<'w>, _: &'w mut ()) -> Self::Item<'w> {
        // SAFETY: Scheduler holds an exclusive claim to this resource cell.
        ResourceMut {
            resource: unsafe { context.resources.prepare_ptr::<R>().map(|ptr| &mut *ptr) }
                .unwrap_or_else(|| {
                    panic!(
                        "required resource {} is missing",
                        std::any::type_name::<R>()
                    )
                }),
        }
    }
}
impl<R: Resource + Sync> sealed::Sealed for Option<Res<R>> {}
impl<R: Resource + Sync> SystemParam for Option<Res<R>> {
    type State = ();
    type Item<'w> = Option<ResourceRef<'w, R>>;
    fn access(access: &mut ParamAccess) {
        access.resource::<R>(false);
    }
    fn init(_: &mut World) {}
    unsafe fn prepare<'w>(context: &ParamContext<'w>, _: &'w mut ()) -> Self::Item<'w> {
        context
            .resources
            .get::<R>()
            .map(|resource| ResourceRef { resource })
    }
}
impl<R: Resource + Send> sealed::Sealed for Option<ResMut<R>> {}
impl<R: Resource + Send> SystemParam for Option<ResMut<R>> {
    type State = ();
    type Item<'w> = Option<ResourceMut<'w, R>>;
    fn access(access: &mut ParamAccess) {
        access.resource::<R>(true);
    }
    fn init(_: &mut World) {}
    unsafe fn prepare<'w>(context: &ParamContext<'w>, _: &'w mut ()) -> Self::Item<'w> {
        // SAFETY: Even an absent optional resource retains its exclusive claim.
        unsafe {
            context.resources.prepare_ptr::<R>().map(|ptr| ResourceMut {
                resource: &mut *ptr,
            })
        }
    }
}
