use crate::text::font::WEIGHT_AXIS;
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use swash::{tag_from_bytes, FontRef};

pub(super) const STATIC_FACE: u16 = 0;

pub(super) fn font_file(path: &Path) -> Option<&'static [u8]> {
    static FILES: OnceLock<Mutex<HashMap<PathBuf, &'static [u8]>>> = OnceLock::new();
    let mut files = FILES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(data) = files.get(path) {
        return Some(*data);
    }
    let data = map_font_file(path)
        .map_err(|e| report!("failed to read the font {:?}: {}", path, e))
        .ok()?;
    files.insert(path.to_path_buf(), data);
    Some(data)
}

pub(super) fn map_font_file(path: &Path) -> std::io::Result<&'static [u8]> {
    if !safe_to_map(path) {
        return Ok(Box::leak(std::fs::read(path)?.into_boxed_slice()));
    }
    let file = std::fs::File::open(path)?;
    match unsafe { memmap2::Mmap::map(&file) } {
        Ok(mapped) => Ok(&**Box::leak(Box::new(mapped))),
        Err(_) => Ok(Box::leak(std::fs::read(path)?.into_boxed_slice())),
    }
}

#[cfg(unix)]
pub(super) fn safe_to_map(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    unsafe { libc::access(path.as_ptr(), libc::W_OK) != 0 }
}

#[cfg(not(unix))]
pub(super) fn safe_to_map(_path: &Path) -> bool {
    true
}

pub(super) fn has_weight_axis(font: &FontRef<'_>) -> bool {
    font.variations()
        .find_by_tag(tag_from_bytes(WEIGHT_AXIS))
        .is_some()
}

pub(super) fn weight_coords(font: &FontRef<'_>, weight: u16) -> Vec<i16> {
    let variations = font.variations();
    let Some(axis) = variations.find_by_tag(tag_from_bytes(WEIGHT_AXIS)) else {
        return Vec::new();
    };
    let value = (weight as f32).clamp(axis.min_value(), axis.max_value());
    let coords: Vec<i16> = variations
        .normalized_coords([(tag_from_bytes(WEIGHT_AXIS), value)])
        .collect();
    if coords.iter().all(|coord| *coord == 0) {
        Vec::new()
    } else {
        coords
    }
}

pub(super) fn is_color_capable(font: &FontRef<'static>) -> bool {
    font.color_palettes().count() > 0 || font.color_strikes().count() > 0
}

pub(super) fn face(data: &'static [u8], name: &str) -> Result<FontRef<'static>> {
    FontRef::from_index(data, 0).ok_or_else(|| anyhow!("embedded face {} failed to parse", name))
}
