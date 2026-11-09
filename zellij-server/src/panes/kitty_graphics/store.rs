use base64::engine::general_purpose::STANDARD as BASE64_ENCODER;
use base64::engine::Engine as _;
use std::borrow::Cow;
use std::io::Write;
use std::sync::Arc;

use super::grid_state::{crop_rgba, scale_rgba};
use super::parser::{DecodedImage, KittyError, KittyErrorCode, KittyFormat};
use crate::panes::sixel::PixelRect;
use std::collections::HashMap;

pub const DEFAULT_KITTY_STORE_QUOTA_BYTES: usize = 335_544_320;
pub(crate) const KITTY_IMAGE_METADATA_BYTES: usize = 512;
// derived entries are evicted down to quota / this much below the quota
const EVICTION_SLACK_DIVISOR: usize = 16;

pub type InternalImageId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScaledImageKey {
    pub source: PixelRect,
    pub size: (usize, usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct KittyImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
struct ScaledVariant {
    bytes: Vec<u8>,
    lru_stamp: u64,
}

#[derive(Debug, Clone)]
struct CachedBase64 {
    value: Arc<str>,
    lru_stamp: u64,
}

/// Derived data cached per image. Both are fully regenerable from the
/// base raster, so they are evicted before image contents when the
/// store is under memory pressure.
#[derive(Debug, Clone, Default)]
struct DerivedImageData {
    scaled_variants: HashMap<ScaledImageKey, ScaledVariant>,
    base64_cache: HashMap<(Option<ScaledImageKey>, bool), CachedBase64>,
}

impl DerivedImageData {
    fn bytes(&self) -> usize {
        self.scaled_variants
            .values()
            .map(|variant| variant.bytes.len())
            .sum::<usize>()
            + self
                .base64_cache
                .values()
                .map(|cached| cached.value.len())
                .sum::<usize>()
    }
}

#[derive(Debug, Clone)]
struct StoredImage {
    image: KittyImage,
    refcount: usize,
    lru_stamp: u64,
    derived: DerivedImageData,
}

/// Identifies a regenerable derived entry for eviction.
#[derive(Debug, Clone, Copy)]
enum DerivedKey {
    ScaledVariant(ScaledImageKey),
    Base64((Option<ScaledImageKey>, bool)),
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
        let admission_bytes = size.saturating_add(KITTY_IMAGE_METADATA_BYTES);
        if admission_bytes > self.quota_bytes {
            return Err(KittyError {
                code: KittyErrorCode::Einval,
                message: "image exceeds storage quota".to_owned(),
                image_id: None,
                image_number: None,
                placement_id: None,
                quiet: 0,
            });
        }
        // evict_to_fit stops when only referenced images remain; without an
        // admission check the insert below would exceed the declared quota.
        if !self.evict_to_fit(admission_bytes, None, u64::MAX) {
            return Err(KittyError {
                code: KittyErrorCode::Einval,
                message: "image storage quota exhausted by referenced images".to_owned(),
                image_id: None,
                image_number: None,
                placement_id: None,
                quiet: 0,
            });
        }
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
                },
                refcount: 0,
                lru_stamp,
                derived: DerivedImageData::default(),
            },
        );
        self.total_bytes += admission_bytes;
        Ok(id)
    }
    pub fn get(&self, id: InternalImageId) -> Option<&KittyImage> {
        self.images.get(&id).map(|stored| &stored.image)
    }
    /// Pure cache lookup: reports whether a scaled variant currently
    /// exists without regenerating it.
    pub fn has_scaled_variant(&self, id: InternalImageId, key: ScaledImageKey) -> bool {
        self.images
            .get(&id)
            .map(|stored| stored.derived.scaled_variants.contains_key(&key))
            .unwrap_or(false)
    }
    pub fn scaled_variant(&self, id: InternalImageId, key: ScaledImageKey) -> Option<&[u8]> {
        self.images
            .get(&id)
            .and_then(|stored| stored.derived.scaled_variants.get(&key))
            .map(|variant| variant.bytes.as_slice())
    }
    /// Compute the bytes of a scaled variant from the base raster without
    /// touching the cache.
    fn render_scaled_variant(&self, id: InternalImageId, key: ScaledImageKey) -> Option<Vec<u8>> {
        let stored = self.images.get(&id)?;
        let (width, height) = (stored.image.width as usize, stored.image.height as usize);
        let source = key.source;
        if source.y < 0 || source.x >= width || source.y as usize >= height {
            return None;
        }
        let cropped = crop_rgba(
            &stored.image.rgba,
            width,
            height,
            source.x,
            source.y as usize,
            source.width,
            source.height,
        );
        let scaled = scale_rgba(
            &cropped,
            source.width,
            source.height,
            key.size.0,
            key.size.1,
        );
        // an empty result (oversized or degenerate target) is not a valid raster
        (!scaled.is_empty()).then_some(scaled)
    }
    /// Fetch a scaled variant, regenerating it from the base raster when it
    /// is not cached. Regeneration makes variant entries safe to evict
    /// under memory pressure; a regenerated result is only retained when it
    /// fits inside the store quota.
    pub fn scaled_variant_or_regenerate(
        &mut self,
        id: InternalImageId,
        key: ScaledImageKey,
    ) -> Option<Cow<'_, [u8]>> {
        let stamp = self.bump_lru();
        if self.has_scaled_variant(id, key) {
            if let Some(variant) = self
                .images
                .get_mut(&id)
                .and_then(|stored| stored.derived.scaled_variants.get_mut(&key))
            {
                variant.lru_stamp = stamp;
            }
            return self.scaled_variant(id, key).map(Cow::Borrowed);
        }
        let scaled = self.render_scaled_variant(id, key)?;
        self.add_scaled_variant(id, key, scaled.clone());
        match self.scaled_variant(id, key) {
            Some(bytes) => Some(Cow::Borrowed(bytes)),
            // The variant could not be retained within the quota; it is
            // still returned uncached so the render stays correct.
            None => Some(Cow::Owned(scaled)),
        }
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
        variant: Option<ScaledImageKey>,
        compressed: bool,
    ) -> Option<Arc<str>> {
        let cache_key = (variant, compressed);
        let stamp = self.bump_lru();
        if let Some(cached) = self
            .images
            .get_mut(&id)
            .and_then(|stored| stored.derived.base64_cache.get_mut(&cache_key))
        {
            cached.lru_stamp = stamp;
            return Some(cached.value.clone());
        }
        let encoded: Arc<str> = {
            let stored = self.images.get(&id)?;
            // An evicted variant is regenerated for the encoding (and
            // cached again below when it fits), like the structured path.
            let regenerated;
            let bytes: &[u8] = match variant {
                Some(key) => match stored.derived.scaled_variants.get(&key) {
                    Some(cached) => &cached.bytes,
                    None => {
                        regenerated = self.render_scaled_variant(id, key)?;
                        &regenerated
                    },
                },
                None => &stored.image.rgba,
            };
            let encoded = if compressed {
                let mut encoder = flate2::write::ZlibEncoder::new(
                    Vec::with_capacity(bytes.len() / 8),
                    flate2::Compression::fast(),
                );
                let deflated = encoder
                    .write_all(bytes)
                    .and_then(|_| encoder.finish())
                    .ok()?;
                BASE64_ENCODER.encode(&deflated)
            } else {
                BASE64_ENCODER.encode(bytes)
            };
            encoded.into()
        };
        let inserted = match self.images.get_mut(&id) {
            Some(stored) => stored
                .derived
                .base64_cache
                .insert(
                    cache_key,
                    CachedBase64 {
                        value: encoded.clone(),
                        lru_stamp: stamp,
                    },
                )
                .map(|old| old.value.len())
                .unwrap_or(0),
            None => 0,
        };
        self.total_bytes = self.total_bytes - inserted + encoded.len();
        // The returned Arc keeps the payload alive for the caller even if
        // this cache entry is dropped to satisfy the quota. The image is
        // protected: evicting the base raster to admit its own encoding
        // would be self-defeating.
        if self.total_bytes > self.quota_bytes {
            self.evict_to_fit(0, Some(id), stamp);
            if self.total_bytes > self.quota_bytes {
                self.remove_derived_entry(id, &DerivedKey::Base64(cache_key));
            }
        }
        Some(encoded)
    }
    pub fn add_scaled_variant(&mut self, id: InternalImageId, key: ScaledImageKey, bytes: Vec<u8>) {
        let lru_stamp = self.bump_lru();
        let inserted = match self.images.get_mut(&id) {
            Some(stored) => {
                let new_len = bytes.len();
                let old_len = stored
                    .derived
                    .scaled_variants
                    .insert(key, ScaledVariant { bytes, lru_stamp })
                    .map(|old| old.bytes.len())
                    .unwrap_or(0);
                // Invalidate encodings of the replaced variant, accounting
                // for their bytes.
                let cache_keys: Vec<_> = stored
                    .derived
                    .base64_cache
                    .keys()
                    .filter(|(cached_variant, _)| *cached_variant == Some(key))
                    .copied()
                    .collect();
                let mut freed = 0usize;
                for cache_key in cache_keys {
                    if let Some(cached) = stored.derived.base64_cache.remove(&cache_key) {
                        freed += cached.value.len();
                    }
                }
                stored.lru_stamp = lru_stamp;
                Some((old_len + freed, new_len))
            },
            None => None,
        };
        if let Some((removed_len, new_len)) = inserted {
            self.total_bytes = self.total_bytes - removed_len + new_len;
            if self.total_bytes > self.quota_bytes {
                self.evict_to_fit(0, Some(id), lru_stamp);
                if self.total_bytes > self.quota_bytes {
                    // Even the new variant cannot be retained; it is
                    // regenerable from the base raster, so drop the cache
                    // entry rather than exceed the declared quota.
                    self.remove_derived_entry(id, &DerivedKey::ScaledVariant(key));
                }
            }
        }
    }
    pub fn reserve_scaled_variant(
        &mut self,
        id: InternalImageId,
        key: ScaledImageKey,
        bytes: usize,
    ) -> bool {
        if bytes > self.quota_bytes {
            return false;
        }
        let existing = self
            .images
            .get(&id)
            .and_then(|stored| stored.image.scaled_variants.get(&key))
            .map(|variant| variant.len())
            .unwrap_or(0);
        let incoming = bytes.saturating_sub(existing);
        self.evict_to_fit(incoming, Some(id));
        self.total_bytes + incoming <= self.quota_bytes
    }
    pub fn clear_scaled_variants(&mut self, id: InternalImageId) {
        if let Some(stored) = self.images.get_mut(&id) {
            let removed: usize = stored
                .derived
                .scaled_variants
                .drain()
                .map(|(_, variant)| variant.bytes.len())
                .sum::<usize>()
                + stored
                    .derived
                    .base64_cache
                    .iter()
                    .filter(|((cached_variant, _), _)| cached_variant.is_some())
                    .map(|(_, cached)| cached.value.len())
                    .sum::<usize>();
            stored
                .derived
                .base64_cache
                .retain(|(cached_variant, _), _| cached_variant.is_none());
            self.total_bytes -= removed;
        }
    }
    pub fn refcount(&self, id: InternalImageId) -> Option<usize> {
        self.images.get(&id).map(|stored| stored.refcount)
    }
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }
    /// Bytes currently retained in cached base64 encodings. Included in
    /// `total_bytes`; exposed so accounting can be verified separately
    /// from image and variant payloads.
    pub fn encoded_cache_bytes(&self) -> usize {
        self.images
            .values()
            .map(|stored| {
                stored
                    .derived
                    .base64_cache
                    .values()
                    .map(|cached| cached.value.len())
                    .sum::<usize>()
            })
            .sum()
    }
    pub fn image_count(&self) -> usize {
        self.images.len()
    }
    fn bump_lru(&mut self) -> u64 {
        self.next_lru_stamp += 1;
        self.next_lru_stamp
    }
    fn stored_image_bytes(stored: &StoredImage) -> usize {
        stored
            .image
            .rgba
            .len()
            .saturating_add(stored.derived.bytes())
            .saturating_add(KITTY_IMAGE_METADATA_BYTES)
    }
    fn remove_entry(&mut self, id: InternalImageId) {
        if let Some(stored) = self.images.remove(&id) {
            self.total_bytes -= Self::stored_image_bytes(&stored);
        }
    }
    fn remove_derived_entry(&mut self, id: InternalImageId, key: &DerivedKey) {
        let freed = match self.images.get_mut(&id) {
            Some(stored) => match key {
                DerivedKey::ScaledVariant(variant_key) => stored
                    .derived
                    .scaled_variants
                    .remove(variant_key)
                    .map(|variant| variant.bytes.len())
                    .unwrap_or(0),
                DerivedKey::Base64(cache_key) => stored
                    .derived
                    .base64_cache
                    .remove(cache_key)
                    .map(|cached| cached.value.len())
                    .unwrap_or(0),
            },
            None => 0,
        };
        self.total_bytes = self.total_bytes.saturating_sub(freed);
    }
    /// All regenerable entries across all images, oldest first, excluding
    /// the entry carrying `protected_stamp` (the one whose admission is in
    /// progress). Built once per eviction pass so that evicting many
    /// entries does not rescan the whole store for each of them.
    fn derived_entries_oldest_first(
        &self,
        protected_stamp: u64,
    ) -> Vec<(u64, InternalImageId, DerivedKey)> {
        let mut entries = Vec::new();
        for (id, stored) in self.images.iter() {
            for (key, variant) in stored.derived.scaled_variants.iter() {
                if variant.lru_stamp != protected_stamp {
                    entries.push((variant.lru_stamp, *id, DerivedKey::ScaledVariant(*key)));
                }
            }
            for (cache_key, cached) in stored.derived.base64_cache.iter() {
                if cached.lru_stamp != protected_stamp {
                    entries.push((cached.lru_stamp, *id, DerivedKey::Base64(*cache_key)));
                }
            }
        }
        entries.sort_by_key(|(stamp, _, _)| *stamp);
        entries
    }
    /// Make room for `incoming_bytes`: evict regenerable derived entries
    /// (scaled variants and cached encodings) least-recently-used first,
    /// then unreferenced whole images. Returns whether the admission can
    /// proceed within the hard limit; a request that cannot be satisfied
    /// evicts nothing. The entry carrying `protected_stamp` and the image
    /// `protected` are exempt from eviction.
    fn evict_to_fit(
        &mut self,
        incoming_bytes: usize,
        protected: Option<InternalImageId>,
        protected_stamp: u64,
    ) -> bool {
        if self.total_bytes + incoming_bytes <= self.quota_bytes {
            return true;
        }
        // Feasibility check mirroring the eviction order below, so a
        // rejected admission does not destroy any retained data. Derived
        // bytes of an evictable image are counted once, inside its
        // image_evictable share via stored_image_bytes — except that
        // whole-image eviction is restricted to unreferenced images, so
        // derived entries of referenced images count only here.
        let image_evictable: usize = self
            .images
            .iter()
            .filter(|(id, stored)| stored.refcount == 0 && Some(**id) != protected)
            .map(|(_, stored)| Self::stored_image_bytes(stored))
            .sum();
        let derived_evictable: usize = self
            .images
            .iter()
            .filter(|(id, stored)| stored.refcount > 0 || Some(**id) == protected)
            .flat_map(|(_, stored)| {
                stored
                    .derived
                    .scaled_variants
                    .values()
                    .filter(|variant| variant.lru_stamp != protected_stamp)
                    .map(|variant| variant.bytes.len())
                    .chain(
                        stored
                            .derived
                            .base64_cache
                            .values()
                            .filter(|cached| cached.lru_stamp != protected_stamp)
                            .map(|cached| cached.value.len()),
                    )
            })
            .sum();
        if self.total_bytes + incoming_bytes - derived_evictable - image_evictable
            > self.quota_bytes
        {
            return false;
        }
        // Derived entries are evicted in a single oldest-first pass down to
        // a low-water mark below the quota, so a long run of small inserts
        // into a full store does not pay for a full scan on every insert.
        // Whole images are only evicted as far as strictly needed.
        let derived_target = self
            .quota_bytes
            .saturating_sub(incoming_bytes)
            .saturating_sub(self.quota_bytes / EVICTION_SLACK_DIVISOR);
        for (_, id, key) in self.derived_entries_oldest_first(protected_stamp) {
            if self.total_bytes <= derived_target {
                break;
            }
            self.remove_derived_entry(id, &key);
        }
        if self.total_bytes + incoming_bytes > self.quota_bytes {
            let mut candidates: Vec<(u64, InternalImageId)> = self
                .images
                .iter()
                .filter(|(id, stored)| stored.refcount == 0 && Some(**id) != protected)
                .map(|(id, stored)| (stored.lru_stamp, *id))
                .collect();
            candidates.sort();
            for (_, id) in candidates {
                if self.total_bytes + incoming_bytes <= self.quota_bytes {
                    break;
                }
                self.remove_entry(id);
            }
        }
        self.total_bytes + incoming_bytes <= self.quota_bytes
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
