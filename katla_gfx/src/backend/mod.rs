// Backend abstraction scaffolding for Metal/Vulkan unification (Phase E).
// Items here define the unified GPU backend API. Some trait methods are
// implemented by the Metal backend but not yet called from the engine.
#[allow(dead_code)]
pub mod command;
#[allow(dead_code)]
pub mod resource;
#[allow(dead_code)]
pub mod traits;
