use base64::engine::general_purpose::STANDARD as BASE64_ENCODER;
use base64::engine::Engine as _;

use super::parser::{DecodedImage, KittyError, KittyErrorCode, KittyFormat};
use std::collections::HashMap;

pub const DEFAULT_KITTY_STORE_QUOTA_BYTES: usize = 335_544_320;

pub type InternalImageId = u64;

#[derive(Debug, Clone, PartialEq)]
pub struct KittyImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub scaled_variants: HashMap<(u16, u16), Vec<u8>>,
}

#[derive(Debug, Clone)]
struct StoredImage {
    image: KittyImage,
    refcount: usize,
    lru_stamp: u64,
    base64_cache: HashMap<Option<(u16, u16)>, String>,
}

#[derive(Debug, Clone)]
pub struct KittyImageStore {
    images: HashMap<InternalImageId, StoredImage>,
    next_image_id: InternalImageId,
    next_lru_stamp: u64,
    quota_bytes: usize,
    total_bytes: usize,
    next_placement_uid: u64,
}

impl Default for KittyImageStore {
    fn default() -> Self {
        Self::with_quota(DEFAULT_KITTY_STORE_QUOTA_BYTES)
    }
}

impl KittyImageStore {
    pub fn with_quota(quota_bytes: usize) -> Self {
        KittyImageStore {
            images: HashMap::new(),
            next_image_id: 1,
            next_lru_stamp: 0,
            quota_bytes,
            total_bytes: 0,
            next_placement_uid: 1,
        }
    }
    pub fn next_placement_uid(&mut self) -> u64 {
        let uid = self.next_placement_uid;
        self.next_placement_uid += 1;
        uid
    }
    pub fn store_image(&mut self, image: DecodedImage) -> Result<InternalImageId, KittyError> {
        let (rgba, width, height) = Self::normalize_to_rgba(image);
        let size = rgba.len();
        if size > self.quota_bytes {
            return Err(KittyError {
                code: KittyErrorCode::Einval,
                message: "image exceeds storage quota".to_owned(),
                image_id: None,
                image_number: None,
                placement_id: None,
                quiet: 0,
            });
        }
        self.evict_to_fit(size, None);
        let id = self.next_image_id;
        self.next_image_id += 1;
        let lru_stamp = self.bump_lru();
        self.images.insert(
            id,
            StoredImage {
                image: KittyImage {
                    rgba,
                    width,
                    height,
                    scaled_variants: HashMap::new(),
                },
                refcount: 0,
                lru_stamp,
                base64_cache: HashMap::new(),
            },
        );
        self.total_bytes += size;
        Ok(id)
    }
    pub fn get(&self, id: InternalImageId) -> Option<&KittyImage> {
        self.images.get(&id).map(|stored| &stored.image)
    }
    pub fn scaled_variant(&self, id: InternalImageId, cells: (u16, u16)) -> Option<&[u8]> {
        self.images
            .get(&id)
            .and_then(|stored| stored.image.scaled_variants.get(&cells))
            .map(|bytes| bytes.as_slice())
    }
    pub fn touch(&mut self, id: InternalImageId) {
        let lru_stamp = self.bump_lru();
        if let Some(stored) = self.images.get_mut(&id) {
            stored.lru_stamp = lru_stamp;
        }
    }
    pub fn add_placement_ref(&mut self, id: InternalImageId) {
        let lru_stamp = self.bump_lru();
        if let Some(stored) = self.images.get_mut(&id) {
            stored.refcount += 1;
            stored.lru_stamp = lru_stamp;
        }
    }
    pub fn remove_placement_ref(&mut self, id: InternalImageId) {
        if let Some(stored) = self.images.get_mut(&id) {
            stored.refcount = stored.refcount.saturating_sub(1);
        }
    }
    pub fn free(&mut self, id: InternalImageId) {
        let should_remove = match self.images.get_mut(&id) {
            Some(stored) => {
                stored.refcount = stored.refcount.saturating_sub(1);
                stored.refcount == 0
            },
            None => false,
        };
        if should_remove {
            self.remove_entry(id);
        }
    }
    pub fn base64_for(
        &mut self,
        id: InternalImageId,
        variant: Option<(u16, u16)>,
    ) -> Option<String> {
        let stored = self.images.get_mut(&id)?;
        if let Some(cached) = stored.base64_cache.get(&variant) {
            return Some(cached.clone());
        }
        let bytes = match variant {
            Some(cells) => stored.image.scaled_variants.get(&cells)?,
            None => &stored.image.rgba,
        };
        let encoded = BASE64_ENCODER.encode(bytes);
        stored.base64_cache.insert(variant, encoded.clone());
        Some(encoded)
    }
    pub fn add_scaled_variant(&mut self, id: InternalImageId, cells: (u16, u16), bytes: Vec<u8>) {
        let lru_stamp = self.bump_lru();
        let inserted = match self.images.get_mut(&id) {
            Some(stored) => {
                let new_len = bytes.len();
                let old_len = stored
                    .image
                    .scaled_variants
                    .insert(cells, bytes)
                    .map(|old| old.len())
                    .unwrap_or(0);
                stored.base64_cache.remove(&Some(cells));
                stored.lru_stamp = lru_stamp;
                Some((old_len, new_len))
            },
            None => None,
        };
        if let Some((old_len, new_len)) = inserted {
            self.total_bytes = self.total_bytes - old_len + new_len;
            if self.total_bytes > self.quota_bytes {
                self.evict_to_fit(0, Some(id));
            }
        }
    }
    pub fn refcount(&self, id: InternalImageId) -> Option<usize> {
        self.images.get(&id).map(|stored| stored.refcount)
    }
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
    pub fn image_count(&self) -> usize {
        self.images.len()
    }
    fn bump_lru(&mut self) -> u64 {
        self.next_lru_stamp += 1;
        self.next_lru_stamp
    }
    fn stored_image_bytes(stored: &StoredImage) -> usize {
        stored.image.rgba.len()
            + stored
                .image
                .scaled_variants
                .values()
                .map(|bytes| bytes.len())
                .sum::<usize>()
    }
    fn remove_entry(&mut self, id: InternalImageId) {
        if let Some(stored) = self.images.remove(&id) {
            self.total_bytes -= Self::stored_image_bytes(&stored);
        }
    }
    fn evict_to_fit(&mut self, incoming_bytes: usize, protected: Option<InternalImageId>) {
        while self.total_bytes + incoming_bytes > self.quota_bytes {
            let candidate = self
                .images
                .iter()
                .filter(|(id, stored)| stored.refcount == 0 && Some(**id) != protected)
                .min_by_key(|(_, stored)| stored.lru_stamp)
                .map(|(id, _)| *id);
            match candidate {
                Some(id) => self.remove_entry(id),
                None => break,
            }
        }
    }
    fn normalize_to_rgba(image: DecodedImage) -> (Vec<u8>, u32, u32) {
        match image.format {
            KittyFormat::Rgb24 => {
                let mut rgba = Vec::with_capacity(image.bytes.len() / 3 * 4);
                for pixel in image.bytes.chunks_exact(3) {
                    rgba.extend_from_slice(pixel);
                    rgba.push(255);
                }
                (rgba, image.width, image.height)
            },
            KittyFormat::Rgba32 | KittyFormat::Png => (image.bytes, image.width, image.height),
        }
    }
}

#[cfg(test)]
#[path = "./unit/store_tests.rs"]
mod store_tests;
