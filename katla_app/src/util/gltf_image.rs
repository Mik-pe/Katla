//! Role-aware decoded image normalization and filtered mip-chain descriptors.

use gltf::image::{Data, Format};
use katla_gfx::{ImageFormat, TextureDescriptor};

/// Preserve integer data precision and decode integer color into linear light.
/// Decoded float images already contain linear light and upload as RGBA16F.
/// Nonfinite values and values outside finite half-float range are rejected.
pub(crate) fn texture_upload(
    image: &Data,
    srgb: bool,
) -> Result<(TextureDescriptor, Vec<u8>), String> {
    let (channels, channel_bytes) = match image.format {
        Format::R8 => (1, 1),
        Format::R8G8 => (2, 1),
        Format::R8G8B8 => (3, 1),
        Format::R8G8B8A8 => (4, 1),
        Format::R16 => (1, 2),
        Format::R16G16 => (2, 2),
        Format::R16G16B16 => (3, 2),
        Format::R16G16B16A16 => (4, 2),
        Format::R32G32B32FLOAT => (3, 4),
        Format::R32G32B32A32FLOAT => (4, 4),
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
    let format = match (channel_bytes, srgb) {
        (1, true) => ImageFormat::R8G8B8A8Srgb,
        (1, false) => ImageFormat::R8G8B8A8Unorm,
        (2, false) => ImageFormat::R16G16B16A16Unorm,
        _ => ImageFormat::R16G16B16A16Sfloat,
    };
    let capacity = count
        .checked_mul(format.bytes_per_pixel() as usize)
        .ok_or("normalized image byte count overflow")?;
    let mut rgba = Vec::with_capacity(capacity);
    for (pixel_index, pixel) in image
        .pixels
        .chunks_exact(channels * channel_bytes)
        .enumerate()
    {
        if channel_bytes == 1 {
            rgba.extend_from_slice(&expand_channels(channels, |index| pixel[index], 255));
            continue;
        }
        let integer = |index: usize| {
            let offset = index * 2;
            u16::from_ne_bytes([pixel[offset], pixel[offset + 1]])
        };
        if format == ImageFormat::R16G16B16A16Unorm {
            for value in expand_channels(channels, integer, 65535) {
                rgba.extend_from_slice(&value.to_ne_bytes());
            }
            continue;
        }
        let channel = |index: usize| {
            if channel_bytes == 2 {
                f32::from(integer(index)) / 65535.0
            } else {
                let offset = index * 4;
                f32::from_ne_bytes([
                    pixel[offset],
                    pixel[offset + 1],
                    pixel[offset + 2],
                    pixel[offset + 3],
                ])
            }
        };
        let values = expand_channels(channels, channel, 1.0);
        for (component, value) in values.into_iter().enumerate() {
            let linear = if srgb && channel_bytes == 2 && component < 3 {
                srgb_to_linear(value)
            } else {
                value
            };
            let half = half::f16::from_f32(linear);
            if !linear.is_finite() || !half.is_finite() {
                return Err(format!(
                    "GLTF image pixel {pixel_index} component {component}: {linear} is outside finite RGBA16F range (-65504..65504)"
                ));
            }
            rgba.extend_from_slice(&half.to_bits().to_ne_bytes());
        }
    }
    let mut descriptor = TextureDescriptor::new(image.width, image.height, format);
    descriptor.mip_levels = 32 - image.width.max(image.height).leading_zeros();
    descriptor.generate_mips = descriptor.mip_levels > 1;
    Ok((descriptor, rgba))
}

#[inline]
fn expand_channels<T: Copy>(channels: usize, read: impl Fn(usize) -> T, opaque: T) -> [T; 4] {
    let r = read(0);
    let (g, b) = if channels <= 2 {
        (r, r)
    } else {
        (read(1), read(2))
    };
    let a = match channels {
        2 => read(1),
        4 => read(3),
        _ => opaque,
    };
    [r, g, b, a]
}

#[inline]
fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(format: Format, pixels: Vec<u8>) -> Data {
        Data {
            pixels,
            format,
            width: 1,
            height: 1,
        }
    }

    fn half_values(bytes: &[u8]) -> Vec<f32> {
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|value| half::f16::from_bits(u16::from_ne_bytes(*value)).to_f32())
            .collect()
    }

    #[test]
    fn test_expand_all_integer_channel_layouts_without_losing_data_precision() {
        for (format, values, expected) in [
            (Format::R8, vec![64], [64, 64, 64, 255]),
            (Format::R8G8, vec![64, 128], [64, 64, 64, 128]),
            (Format::R8G8B8, vec![64, 128, 255], [64, 128, 255, 255]),
            (Format::R8G8B8A8, vec![64, 128, 255, 0], [64, 128, 255, 0]),
        ] {
            for srgb in [false, true] {
                let (descriptor, bytes) =
                    texture_upload(&image(format, values.clone()), srgb).unwrap();
                assert_eq!(bytes, expected);
                assert_eq!(
                    descriptor.format,
                    if srgb {
                        ImageFormat::R8G8B8A8Srgb
                    } else {
                        ImageFormat::R8G8B8A8Unorm
                    }
                );
            }
        }
        for (format, values, expected) in [
            (Format::R16, vec![64u16], [64, 64, 64, 65535]),
            (Format::R16G16, vec![64, 129], [64, 64, 64, 129]),
            (
                Format::R16G16B16,
                vec![64, 129, 65535],
                [64, 129, 65535, 65535],
            ),
            (
                Format::R16G16B16A16,
                vec![64, 129, 65535, 0],
                [64, 129, 65535, 0],
            ),
        ] {
            let pixels = values.into_iter().flat_map(u16::to_ne_bytes).collect();
            let (descriptor, bytes) = texture_upload(&image(format, pixels), false).unwrap();
            assert_eq!(descriptor.format, ImageFormat::R16G16B16A16Unorm);
            assert_eq!(
                bytes,
                expected
                    .into_iter()
                    .flat_map(u16::to_ne_bytes)
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn test_decode_16_bit_srgb_rgb_but_keep_alpha_linear() {
        let pixels = [32768u16, 16384, 65535, 32768]
            .into_iter()
            .flat_map(u16::to_ne_bytes)
            .collect();
        let (descriptor, bytes) =
            texture_upload(&image(Format::R16G16B16A16, pixels), true).unwrap();
        assert_eq!(descriptor.format, ImageFormat::R16G16B16A16Sfloat);
        for (actual, expected) in half_values(&bytes)
            .into_iter()
            .zip([0.21405, 0.05088, 1.0, 0.5])
        {
            assert!((actual - expected).abs() < 0.0003, "{actual} != {expected}");
        }
    }

    #[test]
    fn test_float_rgb_and_rgba_remain_linear_hdr_in_color_and_data_roles() {
        for format in [Format::R32G32B32FLOAT, Format::R32G32B32A32FLOAT] {
            let mut values = vec![8.0f32, 0.5, -2.0];
            let mut expected = values.clone();
            if format == Format::R32G32B32A32FLOAT {
                values.push(0.25);
                expected.push(0.25)
            } else {
                expected.push(1.0)
            }
            for srgb in [false, true] {
                let pixels = values.iter().flat_map(|v| v.to_ne_bytes()).collect();
                let (descriptor, bytes) = texture_upload(&image(format, pixels), srgb).unwrap();
                assert_eq!(descriptor.format, ImageFormat::R16G16B16A16Sfloat);
                assert_eq!(half_values(&bytes), expected);
            }
        }
    }

    #[test]
    fn test_mip_chain_covers_rectangular_and_single_pixel_images() {
        let mut source = image(Format::R8G8B8A8, vec![255; 32 * 4]);
        source.width = 8;
        source.height = 4;
        let (descriptor, _) = texture_upload(&source, true).unwrap();
        assert_eq!(descriptor.mip_levels, 4);
        assert!(descriptor.generate_mips);
        let (descriptor, _) = texture_upload(&image(Format::R8, vec![128]), false).unwrap();
        assert_eq!(descriptor.mip_levels, 1);
        assert!(!descriptor.generate_mips);
    }

    #[test]
    fn test_malformed_nonfinite_and_out_of_range_images_fail_precisely() {
        for (format, width, pixels) in [
            (Format::R8G8B8, 1, vec![0, 0]),
            (Format::R16, 1, vec![0]),
            (Format::R8G8B8A8, 0, vec![]),
        ] {
            let mut source = image(format, pixels);
            source.width = width;
            assert!(texture_upload(&source, false).is_err());
        }
        for value in [f32::NAN, f32::INFINITY, -f32::INFINITY, 70000.0] {
            let source = image(
                Format::R32G32B32FLOAT,
                [value, 0.0, 0.0]
                    .into_iter()
                    .flat_map(f32::to_ne_bytes)
                    .collect(),
            );
            let error = texture_upload(&source, true).unwrap_err();
            assert!(error.contains("pixel 0 component 0"), "{error}");
            assert!(error.contains("RGBA16F"), "{error}");
        }
    }
}
