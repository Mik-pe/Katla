//! Window surface and drawable ownership.
//!
//! Thread-affinity model: the surface, its current drawable, and all layer
//! mutations (acquire, present, resize, attachment) are confined to the
//! application's main thread — the thread that owns the `NSView` backing the
//! layer. Enforcement is structural: `MetalSurface` is `!Send`/`!Sync`
//! (compile-time contract in this file's test module), so no code path can
//! move or share it; the renderer methods that touch it
//! (`acquire_frame`/`present`, `resize`) are
//! `&mut self` on the renderer, which the app owns on the main thread.
//! Background pipeline-compilation and upload work therefore never requires
//! the surface — those paths use only `MetalContext` (Send + Sync).

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_foundation::NSSize;
use objc2_metal::{MTL4CommandQueue, MTLDevice, MTLDrawable, MTLPixelFormat, MTLTexture};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use raw_window_handle::RawWindowHandle;

use crate::error::RendererError;
use crate::size::Size2D;

pub(crate) struct MetalSurface {
    pub(crate) layer: Retained<CAMetalLayer>,
    current_drawable: Option<Retained<ProtocolObject<dyn CAMetalDrawable>>>,
    size: Size2D,
}

impl MetalSurface {
    pub(crate) fn new(
        window: &dyn raw_window_handle::HasWindowHandle,
        _display: &dyn raw_window_handle::HasDisplayHandle,
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Result<Self, RendererError> {
        let layer = CAMetalLayer::new();
        layer.setDevice(Some(device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm_sRGB);
        layer.setMaximumDrawableCount(super::metal_renderer::FRAMES_IN_FLIGHT);
        layer.setDisplaySyncEnabled(true);
        layer.setFramebufferOnly(false);

        attach_layer_to_nsview(&layer, window)?;

        Ok(Self {
            layer,
            current_drawable: None,
            size: Size2D::new(0, 0),
        })
    }

    #[cfg(test)]
    pub(crate) fn headless() -> Self {
        let layer = CAMetalLayer::new();
        Self {
            layer,
            current_drawable: None,
            size: Size2D::new(0, 0),
        }
    }

    pub(crate) fn headless_with_device(
        device: &ProtocolObject<dyn MTLDevice>,
        width: u32,
        height: u32,
    ) -> Self {
        let layer = CAMetalLayer::new();
        layer.setDevice(Some(device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm_sRGB);
        layer.setMaximumDrawableCount(super::metal_renderer::FRAMES_IN_FLIGHT);
        layer.setFramebufferOnly(false);
        layer.setDrawableSize(NSSize {
            width: width as f64,
            height: height as f64,
        });
        Self {
            layer,
            current_drawable: None,
            size: Size2D::new(width, height),
        }
    }

    /// Acquire the next drawable, returning `None` when the layer cannot
    /// produce one this tick (minimized/occluded) instead of an error.
    pub(crate) fn try_acquire_next_drawable(
        &mut self,
    ) -> Result<Option<Retained<ProtocolObject<dyn MTLTexture>>>, RendererError> {
        let Some(drawable) = self.layer.nextDrawable() else {
            return Ok(None);
        };
        let texture = drawable.texture();
        self.current_drawable = Some(drawable);
        Ok(Some(texture))
    }

    pub(crate) fn wait_for_drawable(&self, queue: &ProtocolObject<dyn MTL4CommandQueue>) {
        if let Some(drawable) = &self.current_drawable {
            queue.waitForDrawable(drawable.as_ref());
        }
    }

    pub(crate) fn present(&mut self, queue: &ProtocolObject<dyn MTL4CommandQueue>) {
        if let Some(drawable) = self.current_drawable.take() {
            queue.signalDrawable(drawable.as_ref());
            drawable.present();
        }
    }

    pub(crate) fn discard_drawable(&mut self) {
        self.current_drawable = None;
    }

    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        self.size = Size2D::new(width, height);
        let scale = self.layer.contentsScale();
        self.layer.setDrawableSize(NSSize {
            width: width as f64 * scale,
            height: height as f64 * scale,
        });
    }
}

fn attach_layer_to_nsview(
    layer: &CAMetalLayer,
    window: &dyn raw_window_handle::HasWindowHandle,
) -> Result<(), RendererError> {
    let raw_handle = window.window_handle().map_err(|e| {
        RendererError::InitializationFailed(format!("Window handle error: {:?}", e))
    })?;

    match raw_handle.as_raw() {
        RawWindowHandle::AppKit(handle) => {
            let ns_view: &AnyObject = unsafe { &*handle.ns_view.as_ptr().cast::<AnyObject>() };

            unsafe {
                let _: () = objc2::msg_send![ns_view, setWantsLayer: true];
            }

            unsafe {
                let _: () = objc2::msg_send![ns_view, setLayer: layer];
            }

            // Match the layer's contents scale to the screen's backing scale.
            // This must be set unconditionally: if the view's bounds are still
            // zero at attach time (window not yet laid out), leaving
            // contentsScale at its default 1.0 causes every later resize() to
            // size the drawable in logical rather than physical pixels — half
            // resolution on Retina — while the viewport panel rect is computed
            // in physical pixels, making the scene blit land out of bounds.
            unsafe {
                let scale_factor: f64 = objc2::msg_send![ns_view, backingScaleFactor];
                layer.setContentsScale(scale_factor);

                let bounds: objc2_foundation::NSRect = objc2::msg_send![ns_view, bounds];
                if bounds.size.width > 0.0 && bounds.size.height > 0.0 {
                    layer.setDrawableSize(objc2_foundation::NSSize {
                        width: bounds.size.width * scale_factor,
                        height: bounds.size.height * scale_factor,
                    });
                }
            }

            Ok(())
        }
        _ => Err(RendererError::InitializationFailed(
            "Unsupported window handle (expected AppKit)".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    const _: () = {
        trait AmbiguousIfSend<A> {
            fn marker() {}
        }
        impl<T: ?Sized> AmbiguousIfSend<()> for T {}
        impl<T: Send + ?Sized> AmbiguousIfSend<u8> for T {}
        trait AmbiguousIfSync<A> {
            fn marker() {}
        }
        impl<T: ?Sized> AmbiguousIfSync<()> for T {}
        impl<T: Sync + ?Sized> AmbiguousIfSync<u8> for T {}
        let _ = <super::MetalSurface as AmbiguousIfSend<_>>::marker;
        let _ = <super::MetalSurface as AmbiguousIfSync<_>>::marker;
        let _ = <crate::metal::render_encoder::MetalRenderEncoder as AmbiguousIfSend<_>>::marker;
        let _ = <crate::metal::render_encoder::MetalRenderEncoder as AmbiguousIfSync<_>>::marker;
        let _ = <crate::metal::compute_encoder::MetalComputeEncoder as AmbiguousIfSend<_>>::marker;
        let _ = <crate::metal::compute_encoder::MetalComputeEncoder as AmbiguousIfSync<_>>::marker;
        let _ = <crate::metal::blit_encoder::MetalBlitEncoder as AmbiguousIfSend<_>>::marker;
        let _ = <crate::metal::blit_encoder::MetalBlitEncoder as AmbiguousIfSync<_>>::marker;
        const fn assert_send_sync<T: Send + Sync>() {}
        let _ = assert_send_sync::<crate::metal::context::MetalContext>;
        let _ = assert_send_sync::<crate::metal::pipeline::MetalGraphicsPipeline>;
        let _ = assert_send_sync::<crate::metal::pipeline::MetalComputePipeline>;
    };
}
