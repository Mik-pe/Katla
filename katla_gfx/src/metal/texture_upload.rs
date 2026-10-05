//! Staged texture uploads for Metal.
//!
//! Initial texture data never lives in a Shared texture: bytes go into a pooled
//! Shared staging buffer, a blit pass copies them into the Private destination
//! before any consumer pass of the frame, and staging slots are recycled only
//! after the consuming submission completes.

use std::time::Instant;

use crate::backend::resource::GpuBuffer;
use crate::error::RendererError;
use crate::metal::buffer::MetalBuffer;
use crate::metal::context::MetalContext;
use crate::metal::texture::MetalTexture;
use crate::texture::{
    ImageFormat, TextureDescriptor, TextureUploadBudget, TextureUploadMetrics, TextureUploadRegion,
};

#[cfg(test)]
pub(crate) fn validate_upload(
    format: ImageFormat,
    width: u32,
    height: u32,
    len: usize,
) -> Result<(), RendererError> {
    let desc = TextureDescriptor::new(width, height, format);
    let layout = TextureUploadRegion::base(&desc).validate(&desc, len)?;
    if layout.required_bytes != len {
        return Err(RendererError::UploadFailed {
            resource: "texture".into(),
            expected_bytes: layout.required_bytes,
            actual_bytes: len,
            detail: format!(
                "{format:?} {width}x{height}, row pitch {}",
                layout.bytes_per_row
            ),
        });
    }
    Ok(())
}

struct PendingTextureUpload {
    staging: MetalBuffer,
    dst: MetalTexture,
    region: TextureUploadRegion,
    bytes_per_row: usize,
    bytes_per_image: usize,
    generate_mips: bool,
}

struct SubmittedTextureUploads {
    submission_id: u64,
    submitted: bool,
    started: Instant,
    uploads: Vec<PendingTextureUpload>,
}

/// Upload bytes are admitted atomically and belong to their exact submission until completion.
#[derive(Default)]
pub(crate) struct TextureUploadQueue {
    pending: Vec<PendingTextureUpload>,
    in_flight: Vec<SubmittedTextureUploads>,
    free: Vec<MetalBuffer>,
    budget: TextureUploadBudget,
    metrics: TextureUploadMetrics,
}

impl TextureUploadQueue {
    pub(crate) fn record_failure(&mut self) {
        self.metrics.failure_count += 1;
    }

    pub(crate) fn stage(
        &mut self,
        context: &MetalContext,
        dst: MetalTexture,
        format: ImageFormat,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> Result<(), RendererError> {
        if format != dst.descriptor().format
            || width != dst.descriptor().width
            || height != dst.descriptor().height
        {
            self.record_failure();
            return Err(RendererError::InvalidDescriptor {
                resource: "texture upload".into(),
                reason: "upload format or extent differs from destination texture".into(),
            });
        }
        let desc = dst.descriptor();
        self.stage_region(context, dst, &desc, TextureUploadRegion::base(&desc), data)
    }

    pub(crate) fn stage_region(
        &mut self,
        context: &MetalContext,
        dst: MetalTexture,
        desc: &TextureDescriptor,
        region: TextureUploadRegion,
        data: &[u8],
    ) -> Result<(), RendererError> {
        let result = self.stage_validated(context, dst, desc, region, data);
        if result.is_err() {
            self.record_failure();
        }
        result
    }

    fn stage_validated(
        &mut self,
        context: &MetalContext,
        dst: MetalTexture,
        desc: &TextureDescriptor,
        region: TextureUploadRegion,
        data: &[u8],
    ) -> Result<(), RendererError> {
        let layout = region.validate(desc, data.len())?;
        let fail = |detail: &str| RendererError::UploadFailed {
            resource: desc.label.unwrap_or("texture upload").into(),
            expected_bytes: layout.required_bytes,
            actual_bytes: data.len(),
            detail: format!(
                "{:?} {:?}, mip {} layer {}: {detail}",
                desc.format, region.extent, region.mip_level, region.array_layer
            ),
        };
        let generate_mips = desc.generate_mips && desc.mip_levels > 1 && region.mip_level == 0;
        if generate_mips
            && (region.origin != [0; 3]
                || region.extent != [desc.width, desc.height, desc.depth]
                || desc.array_layers != 1
                || desc.format.block_extent() != [1, 1]
                || !desc.format.supports_mip_generation())
        {
            return Err(fail(
                "mip generation requires a full uncompressed filterable base mip in a single-layer texture",
            ));
        }
        // A common 256-byte row alignment works for all supported uncompressed and BC layouts.
        let row_pitch = layout
            .row_bytes
            .checked_add(255)
            .map(|v| v & !255)
            .ok_or_else(|| fail("aligned row pitch overflow"))?;
        let image_pitch = row_pitch
            .checked_mul(layout.block_rows)
            .ok_or_else(|| fail("aligned image pitch overflow"))?;
        let bytes = image_pitch
            .checked_mul(region.extent[2] as usize)
            .ok_or_else(|| fail("staging size overflow"))?;
        let queued = self
            .metrics
            .queued_bytes
            .checked_add(bytes)
            .ok_or_else(|| fail("queued size overflow"))?;
        let upload_count = self
            .in_flight
            .iter()
            .filter(|batch| !batch.submitted)
            .fold(self.pending.len(), |count, batch| {
                count.saturating_add(batch.uploads.len())
            });
        if queued > self.budget.max_queued_bytes
            || queued > self.budget.max_bytes_per_submission
            || upload_count >= self.budget.max_uploads_per_submission
        {
            return Err(fail(
                "upload batch budget exhausted; retry after submission",
            ));
        }
        let free_index = self
            .free
            .iter()
            .enumerate()
            .filter(|(_, b)| b.size() as usize >= bytes)
            .min_by_key(|(_, b)| b.size())
            .map(|(i, _)| i);
        let staging = if let Some(index) = free_index {
            self.free.swap_remove(index)
        } else {
            while self.metrics.staging_bytes.saturating_add(bytes) > self.budget.max_staging_bytes
                && !self.free.is_empty()
            {
                if let Some(buffer) = self.free.pop() {
                    self.metrics.staging_bytes -= buffer.size() as usize;
                }
            }
            if self.metrics.staging_bytes.saturating_add(bytes) > self.budget.max_staging_bytes {
                return Err(fail(
                    "in-flight staging budget exhausted; wait for completion",
                ));
            }
            let buffer = context.create_buffer(bytes as u64, true)?;
            self.metrics.staging_bytes += bytes;
            self.metrics.staging_high_water_mark = self
                .metrics
                .staging_high_water_mark
                .max(self.metrics.staging_bytes);
            buffer
        };
        let map = staging.map();
        if map.is_null() {
            self.free.push(staging);
            return Err(fail("staging buffer is not CPU mapped"));
        }
        unsafe {
            std::ptr::write_bytes(map, 0, bytes);
            for z in 0..region.extent[2] as usize {
                for row in 0..layout.block_rows {
                    std::ptr::copy_nonoverlapping(
                        data.as_ptr()
                            .add(z * layout.bytes_per_image + row * layout.bytes_per_row),
                        map.add(z * image_pitch + row * row_pitch),
                        layout.row_bytes,
                    );
                }
            }
        }
        staging.unmap();
        self.pending.push(PendingTextureUpload {
            staging,
            dst,
            region,
            bytes_per_row: row_pitch,
            bytes_per_image: image_pitch,
            generate_mips,
        });
        self.metrics.queued_bytes = queued;
        Ok(())
    }

    pub(crate) fn encode_into(
        &mut self,
        encoder: &mut crate::metal::blit_encoder::MetalBlitEncoder,
        submission_id: u64,
    ) {
        if self.pending.is_empty() {
            return;
        }
        use objc2_metal::MTL4CommandEncoder;
        encoder
            .inner
            .barrierAfterQueueStages_beforeStages_visibilityOptions(
                objc2_metal::MTLStages::All,
                objc2_metal::MTLStages::Blit,
                objc2_metal::MTL4VisibilityOptions::Device,
            );
        let uploads = std::mem::take(&mut self.pending);
        let mut written = std::collections::HashSet::new();
        for upload in &uploads {
            use objc2_metal::MTLTexture;
            let id = upload.dst.inner.gpuResourceID().to_raw();
            let mip_end = if upload.generate_mips {
                upload.dst.descriptor().mip_levels
            } else {
                upload.region.mip_level + 1
            };
            if (upload.region.mip_level..mip_end)
                .any(|mip| written.contains(&(id, mip, upload.region.array_layer)))
            {
                encoder.barrier_transfers();
            }
            for mip in upload.region.mip_level..mip_end {
                written.insert((id, mip, upload.region.array_layer));
            }
            encoder.copy_buffer_to_texture_region(
                &upload.staging,
                &upload.dst,
                TextureUploadRegion {
                    bytes_per_row: upload.bytes_per_row,
                    bytes_per_image: upload.bytes_per_image,
                    ..upload.region
                },
            );
            if upload.generate_mips {
                encoder.generate_mipmaps(&upload.dst);
            }
        }
        self.in_flight.push(SubmittedTextureUploads {
            submission_id,
            submitted: false,
            started: Instant::now(),
            uploads,
        });
    }

    /// Publish staging ownership only when its command buffer is committed.
    pub(crate) fn mark_submitted(&mut self, submission_id: u64) {
        for batch in &mut self.in_flight {
            if batch.submission_id == submission_id && !batch.submitted {
                batch.submitted = true;
                batch.started = Instant::now();
                let bytes: usize = batch
                    .uploads
                    .iter()
                    .map(|upload| upload.bytes_per_image * upload.region.extent[2] as usize)
                    .sum();
                self.metrics.queued_bytes -= bytes;
                self.metrics.submitted_bytes += bytes as u64;
            }
        }
    }

    /// Replay staged bytes after an encoded command buffer is abandoned before submission.
    pub(crate) fn cancel_unsubmitted(&mut self, submission_id: u64) {
        if !self
            .in_flight
            .iter()
            .any(|batch| batch.submission_id == submission_id && !batch.submitted)
        {
            return;
        }
        let mut restored = Vec::new();
        let mut index = 0;
        while index < self.in_flight.len() {
            if self.in_flight[index].submission_id != submission_id
                || self.in_flight[index].submitted
            {
                index += 1;
                continue;
            }
            restored.extend(self.in_flight.remove(index).uploads);
        }
        restored.append(&mut self.pending);
        self.pending = restored;
    }

    /// Completion of one submission cannot retire bytes owned by any other submission.
    pub(crate) fn retire_completed(&mut self, submission_id: u64) {
        let mut index = 0;
        while index < self.in_flight.len() {
            if self.in_flight[index].submission_id != submission_id
                || !self.in_flight[index].submitted
            {
                index += 1;
                continue;
            }
            let batch = self.in_flight.remove(index);
            self.metrics.completed_batches += 1;
            self.metrics.completion_latency_ns =
                batch.started.elapsed().as_nanos().min(u64::MAX as u128) as u64;
            self.free
                .extend(batch.uploads.into_iter().map(|upload| upload.staging));
        }
    }

    /// A terminal failed submission still completes ownership; its texture contents remain invalid.
    pub(crate) fn retire_failed_submission(&mut self, submission_id: u64) {
        if self
            .in_flight
            .iter()
            .any(|batch| batch.submission_id == submission_id && batch.submitted)
        {
            self.record_failure();
            self.retire_completed(submission_id);
        }
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
    pub(crate) fn pending_transfers(&self) -> Vec<(u64, TextureUploadRegion)> {
        use objc2_metal::MTLTexture;
        let mut transfers = Vec::new();
        for upload in &self.pending {
            let id = upload.dst.inner.gpuResourceID().to_raw();
            transfers.push((id, upload.region));
            if upload.generate_mips {
                let desc = upload.dst.descriptor();
                for mip_level in 1..desc.mip_levels {
                    transfers.push((
                        id,
                        TextureUploadRegion {
                            mip_level,
                            array_layer: 0,
                            origin: [0; 3],
                            extent: [desc.width, desc.height, desc.depth]
                                .map(|value| (value >> mip_level).max(1)),
                            bytes_per_row: 0,
                            bytes_per_image: 0,
                        },
                    ));
                }
            }
        }
        transfers
    }

    pub(crate) fn metrics(&self) -> TextureUploadMetrics {
        self.metrics
    }
}

#[cfg(test)]
#[path = "texture_upload_tests.rs"]
mod tests;
