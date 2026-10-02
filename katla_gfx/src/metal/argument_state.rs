//! Bindings recorded on one shader stage, including exact dynamic-array sizes.

use super::binding_schema::{ArgumentTableLayout, TableBindingKind};
use std::cell::RefCell;

pub(crate) struct ArgumentState {
    buffers: RefCell<[Option<u64>; 31]>,
    textures: RefCell<[bool; 128]>,
    samplers: RefCell<[bool; 16]>,
}

impl ArgumentState {
    pub(crate) fn buffer(&self, index: usize, size: u64) {
        self.buffers.borrow_mut()[index] = Some(size);
    }
    pub(crate) fn texture(&self, index: usize) {
        self.textures.borrow_mut()[index] = true;
    }
    pub(crate) fn sampler(&self, index: usize) {
        self.samplers.borrow_mut()[index] = true;
    }
    pub(crate) fn validate(&self, layout: &ArgumentTableLayout) -> Result<(), String> {
        for binding in &layout.bindings {
            if binding.kind == TableBindingKind::Buffer
                && let Some(bytes) = self.buffers.borrow()[binding.index]
                && bytes < binding.minimum_buffer_bytes
            {
                return Err(format!(
                    "Shader '{}' buffer {}:{} ('{}') requires {} bytes, but its bound view has {bytes}",
                    layout.entry_point,
                    binding.group,
                    binding.binding,
                    binding.name,
                    binding.minimum_buffer_bytes
                ));
            }
            let bound = match binding.kind {
                TableBindingKind::Buffer => self.buffers.borrow()[binding.index].is_some(),
                TableBindingKind::Texture => self.textures.borrow()[binding.index],
                TableBindingKind::Sampler => self.samplers.borrow()[binding.index],
            };
            if !bound {
                return Err(format!(
                    "Shader '{}' requires missing {:?} binding {}:{} ('{}')",
                    layout.entry_point, binding.kind, binding.group, binding.binding, binding.name
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn sizes(&self, layout: &ArgumentTableLayout) -> Vec<u32> {
        let mut words = vec![0; layout.sizes_word_count];
        for binding in &layout.runtime_array_bindings {
            words[binding.size_index] = self.buffers.borrow()[binding.buffer_index]
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32;
        }
        words
    }
}

impl Default for ArgumentState {
    fn default() -> Self {
        Self {
            buffers: RefCell::new([None; 31]),
            textures: RefCell::new([false; 128]),
            samplers: RefCell::new([false; 16]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reflected_buffer_missing_and_undersized_views_fail_preflight() {
        let module = naga::front::wgsl::parse_str(
            "@group(0) @binding(0) var<uniform> params: vec4<u32>;
             @group(0) @binding(1) var<storage, read_write> out: array<u32, 4>;
             @compute @workgroup_size(1) fn cs_main() { out[0] = params.x; }",
        )
        .unwrap();
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        let options = super::super::binding_schema::options_for_module(
            super::super::binding_schema::ShaderProfile::Graphics,
            &module,
        );
        let layout = super::super::binding_schema::reflect_table_layout(&module, &info, &options)
            .unwrap()
            .remove(0);
        let state = ArgumentState::default();
        assert!(state.validate(&layout).unwrap_err().contains("missing"));
        state.buffer(0, 16);
        state.buffer(1, 15);
        assert!(
            state
                .validate(&layout)
                .unwrap_err()
                .contains("requires 16 bytes")
        );
        state.buffer(1, 16);
        state.validate(&layout).unwrap();
    }
}
