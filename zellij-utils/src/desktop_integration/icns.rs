use anyhow::{bail, Context, Result};

const MAGIC: &[u8; 4] = b"icns";
const HEADER: usize = 8;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

// 1024px, so the icon stays sharp at 512pt on Retina screens.
const LARGE_ICON_PNG: &[u8] = include_bytes!("../../assets/icons/logo1024.png");

pub fn zellij() -> Result<Vec<u8>> {
    from_pngs(&[super::ICON_PNG, LARGE_ICON_PNG])
}

pub fn from_pngs(pngs: &[&[u8]]) -> Result<Vec<u8>> {
    if pngs.is_empty() {
        bail!("an icns file needs at least one image");
    }
    let mut chunks: Vec<(&[u8; 4], &[u8])> = Vec::with_capacity(pngs.len());
    for png in pngs {
        let (width, height) = png_size(png)?;
        if width != height {
            bail!("an icns image must be square, found {width}x{height}");
        }
        let chunk_type = chunk_type(width)
            .with_context(|| format!("{width}x{height} is not an icns image size"))?;
        if chunks.iter().any(|(existing, _)| *existing == chunk_type) {
            bail!("two images of {width}x{height} were given, and icns holds one of each size");
        }
        chunks.push((chunk_type, png));
    }

    let total = HEADER
        + chunks
            .iter()
            .map(|(_, png)| HEADER + png.len())
            .sum::<usize>();
    let total = u32::try_from(total).context("the images are too large for an icns file")?;

    let mut icns = Vec::with_capacity(total as usize);
    icns.extend_from_slice(MAGIC);
    icns.extend_from_slice(&total.to_be_bytes());
    for (chunk_type, png) in chunks {
        let length =
            u32::try_from(HEADER + png.len()).context("an image is too large for an icns chunk")?;
        icns.extend_from_slice(chunk_type);
        icns.extend_from_slice(&length.to_be_bytes());
        icns.extend_from_slice(png);
    }
    Ok(icns)
}

fn chunk_type(size: u32) -> Option<&'static [u8; 4]> {
    match size {
        16 => Some(b"icp4"),
        32 => Some(b"icp5"),
        64 => Some(b"icp6"),
        128 => Some(b"ic07"),
        256 => Some(b"ic08"),
        512 => Some(b"ic09"),
        1024 => Some(b"ic10"),
        _ => None,
    }
}

fn png_size(png: &[u8]) -> Result<(u32, u32)> {
    if png.len() < 24 || &png[..8] != PNG_SIGNATURE || &png[12..16] != b"IHDR" {
        bail!("the image does not begin with a PNG header");
    }
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    Ok((width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ICON_PNG: &[u8] = super::super::ICON_PNG;

    fn declared_length(icns: &[u8]) -> u32 {
        u32::from_be_bytes([icns[4], icns[5], icns[6], icns[7]])
    }

    #[test]
    fn the_embedded_icons_become_an_icns_file_of_two_chunks() {
        let icns = zellij().unwrap();

        assert_eq!(&icns[..4], MAGIC);
        assert_eq!(declared_length(&icns) as usize, icns.len());
        assert_eq!(
            icns.len(),
            HEADER + HEADER + ICON_PNG.len() + HEADER + LARGE_ICON_PNG.len()
        );
    }

    #[test]
    fn every_chunk_is_walkable_from_the_header_alone() {
        let icns = zellij().unwrap();
        let mut offset = HEADER;
        let mut seen = Vec::new();
        while offset < icns.len() {
            let chunk_type = &icns[offset..offset + 4];
            let length = u32::from_be_bytes([
                icns[offset + 4],
                icns[offset + 5],
                icns[offset + 6],
                icns[offset + 7],
            ]) as usize;
            assert!(length >= HEADER && offset + length <= icns.len());
            seen.push((
                String::from_utf8(chunk_type.to_vec()).unwrap(),
                icns[offset + HEADER..offset + length].to_vec(),
            ));
            offset += length;
        }
        assert_eq!(offset, icns.len());
        assert_eq!(
            seen,
            vec![
                ("ic07".to_string(), ICON_PNG.to_vec()),
                ("ic10".to_string(), LARGE_ICON_PNG.to_vec()),
            ]
        );
    }

    #[test]
    fn the_declared_size_of_each_image_picks_its_chunk_type() {
        assert_eq!(png_size(ICON_PNG).unwrap(), (128, 128));
        assert_eq!(png_size(LARGE_ICON_PNG).unwrap(), (1024, 1024));
        assert_eq!(chunk_type(256), Some(b"ic08"));
        assert_eq!(chunk_type(48), None);
    }

    #[test]
    fn a_non_square_or_odd_sized_or_non_png_image_is_refused() {
        let mut oblong = ICON_PNG.to_vec();
        oblong[23] = 64;
        assert!(from_pngs(&[&oblong]).is_err());

        let mut odd = ICON_PNG.to_vec();
        odd[19] = 48;
        odd[23] = 48;
        assert!(from_pngs(&[&odd]).is_err());

        assert!(from_pngs(&[b"not a png"]).is_err());
        assert!(from_pngs(&[]).is_err());
        assert!(from_pngs(&[ICON_PNG, ICON_PNG]).is_err());
    }
}
