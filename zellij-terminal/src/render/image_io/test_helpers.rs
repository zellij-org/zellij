use super::*;
use std::path::Path;

impl Image {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[offset],
            self.pixels[offset + 1],
            self.pixels[offset + 2],
            self.pixels[offset + 3],
        ]
    }
}

pub fn write(path: &Path, image: &Image) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {:?}", parent))?;
    }
    std::fs::write(path, encode(image)?).with_context(|| format!("failed to write {:?}", path))
}

pub fn read(path: &Path) -> Result<Image> {
    let bytes = std::fs::read(path).with_context(|| format!("failed to read {:?}", path))?;
    decode(&bytes).with_context(|| format!("{:?} is not a readable PNG", path))
}
