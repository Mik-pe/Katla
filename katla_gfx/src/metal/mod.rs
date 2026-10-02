pub(crate) mod argument_buffer;
pub(crate) mod argument_state;
#[cfg(test)]
mod attachment_tests;
pub(crate) mod attachments;
pub(crate) mod binding_schema;
pub(crate) mod blit_encoder;
pub(crate) mod buffer;
mod buffer_history_retirement;
pub(crate) mod command_buffer;
pub(crate) mod compute_encoder;
#[cfg(test)]
mod compute_range_tests;
pub(crate) mod context;
pub(crate) mod diagnostics;
pub(crate) mod encoding_resources;
pub(crate) mod execution_plan;
pub(crate) mod format;
pub(crate) mod frame_lifecycle;
pub(crate) mod frame_render;
#[cfg(test)]
mod frame_slot_tests;
mod graphics_packet;
mod graphics_preflight;
pub(crate) mod material_api;
pub(crate) mod mesh_api;
pub(crate) mod metal_renderer;
pub(crate) mod metal_transient_texture;
#[cfg(test)]
mod picking_tests;
pub(crate) mod pipeline;
pub(crate) mod pipeline_archive;
pub(crate) mod render_encoder;
pub(crate) mod residency;
pub(crate) mod sampler;
pub(crate) mod shader;
pub(crate) mod skeleton_api;
pub(crate) mod submission;
pub(crate) mod surface;
pub(crate) mod sync;
pub(crate) mod texture;
pub(crate) mod texture_api;
pub(crate) mod texture_readback;
pub(crate) mod texture_upload;
pub(crate) mod timestamp_queries;
pub(crate) mod transient_heap;
pub(crate) mod ui_renderer;

pub(crate) use context::MetalBackend;

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod capture_tests;
