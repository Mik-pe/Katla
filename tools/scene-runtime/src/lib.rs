//! C ABI for canonical dependency runtimes; Odin owns scene composition and scheduling.
mod physics;
mod script;

use serde_json::{Value, json};
use std::{panic::AssertUnwindSafe, thread::ThreadId};

/// The ABI revision must match the Odin dispatch table.
#[unsafe(no_mangle)]
pub extern "C" fn katla_scene_runtime_abi() -> u32 {
    1
}

pub struct Runtime {
    owner: ThreadId,
    script: script::Scripts,
    physics: physics::Physics,
}
impl Runtime {
    fn new() -> Result<Self, String> {
        Ok(Self {
            owner: std::thread::current().id(),
            script: script::Scripts::new()?,
            physics: physics::Physics::new(),
        })
    }
    fn call(&mut self, input: Value) -> Result<Value, String> {
        if self.owner != std::thread::current().id() {
            return Err("Scene runtime is thread-affine".into());
        }
        if input
            .get("method")
            .and_then(Value::as_str)
            .is_some_and(|method| method.starts_with("physics_"))
        {
            self.physics.call(input)
        } else {
            self.script.call(input)
        }
    }
}

/// Transfers a runtime owner to the calling thread; initialization failure returns null.
#[unsafe(no_mangle)]
pub extern "C" fn katla_scene_runtime_create() -> *mut Runtime {
    match std::panic::catch_unwind(Runtime::new) {
        Ok(Ok(runtime)) => Box::into_raw(Box::new(runtime)),
        _ => std::ptr::null_mut(),
    }
}
/// Destroys a runtime only on its owning thread.
///
/// # Safety
/// The pointer must originate from create and may be destroyed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katla_scene_runtime_destroy(runtime: *mut Runtime) {
    if runtime.is_null() {
        return;
    }
    let owner = unsafe { &*runtime }.owner;
    if owner == std::thread::current().id() {
        drop(unsafe { Box::from_raw(runtime) });
    }
}
/// Releases one response returned by call.
///
/// # Safety
/// The pair must be an unchanged outstanding response allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katla_scene_runtime_free(data: *mut u8, len: usize) {
    if !data.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, len)) });
    }
}
/// Executes bounded JSON input and returns an owned JSON envelope; no exception crosses the ABI.
///
/// # Safety
/// Runtime is a live exclusive owner, input is readable for len bytes, and out_len is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn katla_scene_runtime_call(
    runtime: *mut Runtime,
    data: *const u8,
    len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    if runtime.is_null() || data.is_null() || out_len.is_null() || len > 16 * 1024 * 1024 {
        return std::ptr::null_mut();
    }
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let input: Value = serde_json::from_slice(unsafe { std::slice::from_raw_parts(data, len) })
            .map_err(|e| e.to_string())?;
        unsafe { &mut *runtime }.call(input)
    }));
    let envelope = match result {
        Ok(Ok(value)) => json!({"ok":true,"result":value}),
        Ok(Err(error)) => json!({"ok":false,"error":error}),
        Err(_) => json!({"ok":false,"error":"Scene runtime dependency panicked"}),
    };
    let bytes = match serde_json::to_vec(&envelope) {
        Ok(bytes) => bytes.into_boxed_slice(),
        Err(_) => return std::ptr::null_mut(),
    };
    unsafe { *out_len = bytes.len() };
    Box::into_raw(bytes).cast::<u8>()
}
