//! Normalize decoded glTF images for the application's RGBA8 texture uploads.

use gltf::image::{Data, Format};

/// Expand grayscale and RGB images and quantize 16-bit channels to RGBA8.
/// Floating-point images require a floating-point upload path; reject them
/// rather than interpreting their bytes as 8-bit texels or silently clipping HDR.
pub(crate) fn rgba8_pixels(image: &Data) -> Result<Vec<u8>, String> {
    let (channels, channel_bytes) = match image.format {
        Format::R8 => (1, 1),
        Format::R8G8 => (2, 1),
        Format::R8G8B8 => (3, 1),
        Format::R8G8B8A8 => (4, 1),
        Format::R16 => (1, 2),
        Format::R16G16 => (2, 2),
        Format::R16G16B16 => (3, 2),
        Format::R16G16B16A16 => (4, 2),
        Format::R32G32B32FLOAT | Format::R32G32B32A32FLOAT => {
            return Err("floating-point GLTF images require HDR texture uploads".into());
        }
    };
    let count = (image.width as usize)
        .checked_mul(image.height as usize)
        .filter(|count| *count > 0)
        .ok_or("invalid GLTF image dimensions")?;
    let expected = count
        .checked_mul(channels * channel_bytes)
        .ok_or("GLTF image byte count overflow")?;
    if image.pixels.len() != expected {
        return Err(format!(
            "GLTF image {:?} needs {expected} bytes, received {}",
            image.format,
            image.pixels.len()
        ));
    }
    let capacity = count
        .checked_mul(4)
        .ok_or("RGBA8 image byte count overflow")?;
    let mut rgba = Vec::with_capacity(capacity);
    for pixel in image.pixels.chunks_exact(channels * channel_bytes) {
        let channel = |index: usize| {
            if channel_bytes == 1 {
                pixel[index]
            } else {
                let offset = index * 2;
                let value = u16::from_ne_bytes([pixel[offset], pixel[offset + 1]]);
                ((u32::from(value) * 255 + 32767) / 65535) as u8
            }
        };
        let r = channel(0);
        let (g, b) = if channels <= 2 {
            (r, r)
        } else {
            (channel(1), channel(2))
        };
        let a = match channels {
            2 => channel(1),
            4 => channel(3),
            _ => 255,
        };
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_all_integer_formats() {
        for (format, values, expected) in [
            (Format::R8, vec![64], [64, 64, 64, 255]),
            (Format::R8G8, vec![64, 128], [64, 64, 64, 128]),
            (Format::R8G8B8, vec![64, 128, 255], [64, 128, 255, 255]),
            (Format::R8G8B8A8, vec![64, 128, 255, 0], [64, 128, 255, 0]),
            (Format::R16, vec![64], [64, 64, 64, 255]),
            (Format::R16G16, vec![64, 128], [64, 64, 64, 128]),
            (Format::R16G16B16, vec![64, 128, 255], [64, 128, 255, 255]),
            (
                Format::R16G16B16A16,
                vec![64, 128, 255, 0],
                [64, 128, 255, 0],
            ),
        ] {
            let pixels = if matches!(
                format,
                Format::R8 | Format::R8G8 | Format::R8G8B8 | Format::R8G8B8A8
            ) {
                values
            } else {
                values
                    .into_iter()
                    .flat_map(|v| (u16::from(v) * 257).to_ne_bytes())
                    .collect()
            };
            let data = Data {
                pixels,
                format,
                width: 1,
                height: 1,
            };
            assert_eq!(rgba8_pixels(&data).unwrap(), expected, "{format:?}");
        }
    }

    #[test]
    fn test_invalid_images_fail_without_panicking() {
        for (format, width, pixels) in [
            (Format::R8G8B8, 1, vec![0, 0]),
            (Format::R16, 1, vec![0]),
            (Format::R8G8B8A8, 0, vec![]),
            (Format::R32G32B32FLOAT, 1, vec![0; 12]),
        ] {
            assert!(
                rgba8_pixels(&Data {
                    pixels,
                    format,
                    width,
                    height: 1
                })
                .is_err()
            );
        }
    }
}
