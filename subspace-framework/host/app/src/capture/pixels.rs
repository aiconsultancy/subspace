//! X11 packed TrueColor pixels to opaque RGBA, independent of host byte order.

const BYTE_BITS: usize = u8::BITS as usize;
const RGBA_CHANNELS: usize = 4;
const X11_REPLY_ALIGNMENT: usize = 4;

#[derive(Clone, Copy)]
pub(super) struct PixelFormat {
    pub bits: u8,
    pub pad: u8,
    pub little_endian: bool,
    pub masks: [u32; 3],
}

pub(super) fn rgba(
    data: &[u8],
    width: u32,
    height: u32,
    format: PixelFormat,
) -> Result<Vec<u8>, &'static str> {
    if !matches!(format.bits, 24 | 32) || !matches!(format.pad, 8 | 16 | 32) {
        return Err("unsupported X11 capture pixel format");
    }
    let bytes = usize::from(format.bits) / BYTE_BITS;
    let row_bits = width as usize * usize::from(format.bits);
    let stride = row_bits.div_ceil(usize::from(format.pad)) * usize::from(format.pad) / BYTE_BITS;
    let required = stride * height as usize;
    if width == 0
        || height == 0
        || !(required..=required.next_multiple_of(X11_REPLY_ALIGNMENT)).contains(&data.len())
    {
        return Err("invalid X11 capture pixel data");
    }
    let mut result = Vec::with_capacity(width as usize * height as usize * RGBA_CHANNELS);
    for row in data[..required].chunks_exact(stride) {
        for pixel in row[..width as usize * bytes].chunks_exact(bytes) {
            let value = packed(pixel, format.little_endian);
            for mask in format.masks {
                result.push(channel(value, mask)?);
            }
            result.push(u8::MAX);
        }
    }
    Ok(result)
}

fn packed(bytes: &[u8], little_endian: bool) -> u32 {
    if little_endian {
        bytes
            .iter()
            .rev()
            .fold(0, |value, byte| (value << u8::BITS) | u32::from(*byte))
    } else {
        bytes
            .iter()
            .fold(0, |value, byte| (value << u8::BITS) | u32::from(*byte))
    }
}

fn channel(value: u32, mask: u32) -> Result<u8, &'static str> {
    if mask == 0 || mask.count_ones() > u16::BITS {
        return Err("unsupported X11 capture colour mask");
    }
    let maximum = mask >> mask.trailing_zeros();
    if !(maximum + 1).is_power_of_two() {
        return Err("non-contiguous X11 capture colour mask");
    }
    Ok((((value & mask) >> mask.trailing_zeros()) * u32::from(u8::MAX) / maximum) as u8)
}

#[cfg(test)]
mod tests {
    use super::{PixelFormat, rgba};

    fn format(bits: u8, little_endian: bool) -> PixelFormat {
        PixelFormat {
            bits,
            pad: 32,
            little_endian,
            masks: [0xff0000, 0xff00, 0xff],
        }
    }

    #[test]
    fn little_endian_bgrx_becomes_opaque_rgba() {
        assert_eq!(
            rgba(&[3, 2, 1, 0, 6, 5, 4, 0], 2, 1, format(32, true)),
            Ok(vec![1, 2, 3, 255, 4, 5, 6, 255])
        );
    }

    #[test]
    fn packed_rgb_skips_scanline_padding() {
        assert_eq!(
            rgba(&[1, 2, 3, 99, 4, 5, 6, 99], 1, 2, format(24, false)),
            Ok(vec![1, 2, 3, 255, 4, 5, 6, 255])
        );
    }

    #[test]
    fn packed_rgb_accepts_the_x11_replies_final_word_padding() {
        let byte_rows = PixelFormat {
            pad: 8,
            ..format(24, false)
        };
        assert_eq!(
            rgba(&[1, 2, 3, 99], 1, 1, byte_rows),
            Ok(vec![1, 2, 3, 255])
        );
        let halfword_rows = PixelFormat {
            pad: 16,
            ..format(24, false)
        };
        assert_eq!(
            rgba(&[1, 2, 3, 4, 5, 6, 99, 99], 2, 1, halfword_rows),
            Ok(vec![1, 2, 3, 255, 4, 5, 6, 255])
        );
    }

    #[test]
    fn ten_bit_colour_scales_to_eight_bits() {
        let format = PixelFormat {
            masks: [0x3ff00000, 0xffc00, 0x3ff],
            ..format(32, true)
        };
        assert_eq!(
            rgba(&0x3ff00000_u32.to_le_bytes(), 1, 1, format),
            Ok(vec![255, 0, 0, 255])
        );
    }

    #[test]
    fn malformed_data_and_unsupported_masks_fail() {
        assert!(rgba(&[], 1, 1, format(32, true)).is_err());
        assert!(rgba(&[0; 4], 1, 1, format(16, true)).is_err());
        let missing_mask = PixelFormat {
            masks: [0, 0xff00, 0xff],
            ..format(32, true)
        };
        assert!(rgba(&[0; 4], 1, 1, missing_mask).is_err());
    }
}
