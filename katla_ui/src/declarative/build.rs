use std::any::Any;
use std::collections::HashMap;

use super::actions::ActionStream;
use super::descriptor::Callback;
use super::state::{StateArena, StateId, ViewId};
use super::widget::Widget;

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

type ActionCallback = dyn FnMut(&mut ActionStream);

pub struct CallbackTable {
    callbacks: Vec<Box<ActionCallback>>,
}

impl CallbackTable {
    pub fn new() -> Self {
        Self {
            callbacks: Vec::new(),
        }
    }

    pub fn push<F: FnMut(&mut ActionStream) + 'static>(&mut self, f: F) -> Callback {
        let index = self.callbacks.len() as u32;
        self.callbacks.push(Box::new(f) as Box<ActionCallback>);
        Callback(index)
    }

    pub fn invoke(&mut self, callback: &Callback, actions: &mut ActionStream) {
        if let Some(f) = self.callbacks.get_mut(callback.0 as usize) {
            f(actions);
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
/// - `ctx.get_state(id)` / `ctx.set_state(id, value)` — read/write state by [`StateId`] (get returns `Option`)
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

    /// Read a value from the state arena by its [`StateId`].
    pub fn get_state<T: Clone + 'static>(&self, id: StateId) -> Option<T> {
        self.state_arena.get(id)
    }

    /// Write a value to the state arena by its [`StateId`].
    /// Returns `true` if the value was updated, `false` if the ID was not found or the type didn't match.
    pub fn set_state<T: PartialEq + 'static>(&mut self, id: StateId, value: T) -> bool {
        self.state_arena.set(id, value)
    }

    pub fn env<T: Clone + 'static>(&self) -> Option<&T> {
        self.env.get::<T>()
    }

    pub fn on_click<F: FnMut(&mut ActionStream) + 'static>(&mut self, f: F) -> Callback {
        self.callbacks.push(f)
    }

    pub fn emit<A: 'static>(&mut self, action: A) {
        self.actions.emit(action);
    }
}

/// Trait for types that produce a [`Box<dyn Widget>`](super::widget::Widget) tree.
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
///     fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
///         let count = ctx.state(0u32);
///         button(format!("Count: {}", count))
///             .on_click(ctx.on_click(|| println!("clicked")))
///     }
/// }
/// ```
pub trait Build {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget>;
}

impl<F: Fn(&mut BuildContext) -> Box<dyn Widget> + 'static> Build for F {
    fn build(&self, ctx: &mut BuildContext) -> Box<dyn Widget> {
        self(ctx)
    }
}
