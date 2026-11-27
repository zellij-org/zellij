#[cfg(any(test, target_os = "macos"))]
use std::io::BufWriter;

use anyhow::{Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

#[cfg(any(test, target_os = "macos"))]
pub fn encode(image: &Image) -> Result<Vec<u8>> {
    let mut encoded = Vec::new();
    {
        let writer = BufWriter::new(&mut encoded);
        let mut encoder = png::Encoder::new(writer, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Default);
        let mut writer = encoder
            .write_header()
            .context("failed to write PNG header")?;
        writer
            .write_image_data(&image.pixels)
            .context("failed to write PNG pixels")?;
    }
    Ok(encoded)
}

pub fn decode(encoded: &[u8]) -> Result<Image> {
    let decoder = png::Decoder::new(encoded);
    let mut reader = decoder.read_info().context("failed to read PNG header")?;
    let mut pixels = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut pixels)
        .context("failed to read PNG pixels")?;

    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return Err(anyhow::anyhow!(
            "expected 8-bit RGBA, found {:?} at {:?}",
            info.color_type,
            info.bit_depth
        ));
    }

    pixels.truncate(info.buffer_size());
    Ok(Image {
        width: info.width,
        height: info.height,
        pixels,
    })
}

#[cfg(test)]
pub(crate) mod test_helpers;

#[cfg(test)]
mod tests {
    use super::test_helpers::*;
    use super::*;

    fn image() -> Image {
        Image {
            width: 2,
            height: 2,
            pixels: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        }
    }

    #[test]
    fn an_image_survives_a_round_trip() {
        assert_eq!(decode(&encode(&image()).unwrap()).unwrap(), image());
    }

    #[test]
    fn encoding_is_byte_stable_for_one_image() {
        assert_eq!(encode(&image()).unwrap(), encode(&image()).unwrap());
    }

    #[test]
    fn a_file_round_trip_preserves_the_pixels() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested").join("frame.png");
        write(&path, &image()).unwrap();
        assert_eq!(read(&path).unwrap(), image());
    }

    #[test]
    fn a_non_png_file_is_reported_rather_than_panicking() {
        assert!(decode(b"not a png").is_err());
    }
}
