use std::any::Any;
use std::collections::HashMap;

use super::actions::ActionStream;
use super::descriptor::{Callback, ViewDescriptor};
use super::state::{StateArena, StateId, ViewId};

#[derive(Default)]
pub struct Environment {
    values: HashMap<std::any::TypeId, Box<dyn Any>>,
}

impl Environment {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set<T: Clone + 'static>(&mut self, value: T) {
        self.values
            .insert(std::any::TypeId::of::<T>(), Box::new(value));
    }

    pub fn get<T: Clone + 'static>(&self) -> Option<&T> {
        self.values
            .get(&std::any::TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
    }
}

pub struct CallbackTable {
    callbacks: Vec<Box<dyn FnMut()>>,
}

impl CallbackTable {
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
        }
    }

    pub fn push<F: FnMut() + 'static>(&mut self, f: F) -> Callback {
        let index = self.callbacks.len() as u32;
        self.callbacks.push(Box::new(f));
        Callback(index)
    }

    pub fn invoke(&mut self, callback: &Callback) {
        if let Some(f) = self.callbacks.get_mut(callback.0 as usize) {
            f();
        }
    }

    pub fn clear(&mut self) {
        self.callbacks.clear();
    }
}

impl Default for CallbackTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Context provided during [`Build::build()`] for accessing state, callbacks, and environment data.
///
/// - `ctx.state(initial)` — get or create persistent state scoped to this view node
/// - `ctx.env::<T>()` — read typed data injected by the application before the frame
/// - `ctx.on_click(f)` — register a click callback, returns a `Callback` handle
/// - `ctx.emit(action)` — emit a typed action that can be drained after the frame
pub struct BuildContext<'a> {
    node_id: ViewId,
    state_arena: &'a mut StateArena,
    callbacks: &'a mut CallbackTable,
    actions: &'a mut ActionStream,
    env: &'a Environment,
}

impl<'a> BuildContext<'a> {
    pub(crate) fn new(
        node_id: ViewId,
        state_arena: &'a mut StateArena,
        callbacks: &'a mut CallbackTable,
        actions: &'a mut ActionStream,
        env: &'a Environment,
    ) -> Self {
        Self {
            node_id,
            state_arena,
            callbacks,
            actions,
            env,
        }
    }

    pub fn state<T: Clone + PartialEq + 'static>(&mut self, initial: T) -> StateId {
        self.state_arena.get_or_create(self.node_id, initial)
    }

    pub fn env<T: Clone + 'static>(&self) -> Option<&T> {
        self.env.get::<T>()
    }

    pub fn on_click<F: FnMut() + 'static>(&mut self, f: F) -> Callback {
        self.callbacks.push(f)
    }

    pub fn emit<A: 'static>(&mut self, action: A) {
        self.actions.emit(action);
    }
}

/// Trait for types that produce a [`ViewDescriptor`](super::descriptor::ViewDescriptor) tree.
///
/// Implement this on a struct (unit struct is fine) and use [`BuildContext`]
/// to access state, environment data, register callbacks, and emit actions.
///
/// # Example
///
/// ```ignore
/// struct MyView;
///
/// impl Build for MyView {
///     fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
///         let count = ctx.state(0u32);
///         ViewDescriptor::Button {
///             label: format!("Count: {}", count),
///             fill_color: None,
///             hover_color: None,
///             border_color: None,
///             on_click: Some(ctx.on_click(|| println!("clicked"))),
///         }
///     }
/// }
/// ```
pub trait Build {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor;
}

impl<F: Fn(&mut BuildContext) -> ViewDescriptor + 'static> Build for F {
    fn build(&self, ctx: &mut BuildContext) -> ViewDescriptor {
        self(ctx)
    }
}
