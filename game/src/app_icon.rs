//! Katla's embedded desktop identity, independent of the working directory.

use winit::window::Icon;

const ICON_PNG: &[u8] = include_bytes!("../../assets/katla-icon.png");

pub(crate) fn window_icon() -> Result<Icon, Box<dyn std::error::Error>> {
    let image =
        image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png)?.into_rgba8();
    let (width, height) = image.dimensions();
    Ok(Icon::from_rgba(image.into_raw(), width, height)?)
}

#[cfg(target_os = "macos")]
pub(crate) fn install_dock_icon() -> Result<(), &'static str> {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;

    let main_thread = MainThreadMarker::new().ok_or("Dock icon requires the main thread")?;
    let data = NSData::with_bytes(include_bytes!("../../assets/katla-icon.icns"));
    let image = NSImage::initWithData(NSImage::alloc(), &data).ok_or("Invalid embedded ICNS")?;
    if !image.isValid() {
        return Err("Embedded ICNS has no valid image representation");
    }
    let app = NSApplication::sharedApplication(main_thread);
    // SAFETY: Called on the main thread with a valid, retained image.
    unsafe { app.setApplicationIconImage(Some(&image)) };
    log::info!("Installed Katla Dock icon");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_icon_has_transparency_and_window_representation() {
        let image = image::load_from_memory(ICON_PNG)
            .expect("valid embedded app icon")
            .into_rgba8();
        assert_eq!(image.dimensions(), (1024, 1024));
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert!(image.get_pixel(512, 512)[3] >= 250);
        assert!(window_icon().is_ok());
    }
}
