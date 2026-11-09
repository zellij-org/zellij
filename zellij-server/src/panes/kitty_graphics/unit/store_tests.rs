use super::*;

fn rgba_image(width: u32, height: u32) -> DecodedImage {
    DecodedImage {
        bytes: vec![0; (width * height * 4) as usize],
        width,
        height,
        format: KittyFormat::Rgba32,
    }
}

#[test]
fn anonymous_images_receive_distinct_monotonically_increasing_internal_ids() {
    let mut store = KittyImageStore::default();
    let id_a = store.store_image(rgba_image(2, 2)).unwrap();
    let id_b = store.store_image(rgba_image(2, 2)).unwrap();
    assert!(id_b > id_a);
    assert_eq!(id_b, id_a + 1);
    assert_eq!(store.image_count(), 2);
    assert_eq!(store.total_bytes(), 2 * (16 + KITTY_IMAGE_METADATA_BYTES));
}

#[test]
fn lru_zero_refcount_image_is_evicted_first_on_quota_overflow() {
    let mut store = KittyImageStore::with_quota(2000);
    let id_a = store.store_image(rgba_image(10, 10)).unwrap();
    let id_b = store.store_image(rgba_image(10, 10)).unwrap();
    let id_c = store.store_image(rgba_image(10, 10)).unwrap();
    assert!(store.get(id_a).is_none());
    assert!(store.get(id_b).is_some());
    assert!(store.get(id_c).is_some());
    assert_eq!(store.image_count(), 2);
    assert_eq!(store.total_bytes(), 2 * (400 + KITTY_IMAGE_METADATA_BYTES));
}

#[test]
fn touch_updates_lru_order_for_eviction() {
    let mut store = KittyImageStore::with_quota(2000);
    let id_a = store.store_image(rgba_image(10, 10)).unwrap();
    let id_b = store.store_image(rgba_image(10, 10)).unwrap();
    store.touch(id_a);
    let id_c = store.store_image(rgba_image(10, 10)).unwrap();
    assert!(store.get(id_a).is_some());
    assert!(store.get(id_b).is_none());
    assert!(store.get(id_c).is_some());
    assert_eq!(store.image_count(), 2);
    assert_eq!(store.total_bytes(), 2 * (400 + KITTY_IMAGE_METADATA_BYTES));
}

#[test]
fn refcounted_image_is_never_evicted_even_when_oldest() {
    let mut store = KittyImageStore::with_quota(2000);
    let id_a = store.store_image(rgba_image(10, 10)).unwrap();
    store.add_placement_ref(id_a);
    let id_b = store.store_image(rgba_image(10, 10)).unwrap();
    let id_c = store.store_image(rgba_image(10, 10)).unwrap();
    assert!(store.get(id_a).is_some());
    assert!(store.get(id_b).is_none());
    assert!(store.get(id_c).is_some());
    assert_eq!(store.total_bytes(), 2 * (400 + KITTY_IMAGE_METADATA_BYTES));
}

#[test]
fn single_image_exceeding_quota_is_refused_with_einval_and_store_unchanged() {
    let mut store = KittyImageStore::with_quota(1500);
    let id_a = store.store_image(rgba_image(10, 10)).unwrap();
    let result = store.store_image(rgba_image(20, 20));
    match result {
        Err(err) => assert_eq!(err.code, KittyErrorCode::Einval),
        Ok(_) => panic!("expected error"),
    }
    assert_eq!(store.image_count(), 1);
    assert_eq!(store.total_bytes(), 400 + KITTY_IMAGE_METADATA_BYTES);
    assert!(store.get(id_a).is_some());
    let id_small = store.store_image(rgba_image(2, 2)).unwrap();
    assert_eq!(id_small, id_a + 1);
}

#[test]
fn per_image_metadata_counts_toward_the_quota() {
    let mut store = KittyImageStore::with_quota(1000);
    // 122 RGBA pixels are 488 bytes; with the 512 byte metadata charge this
    // image exactly fills the quota.
    let id = store.store_image(rgba_image(122, 1)).unwrap();
    assert_eq!(store.total_bytes(), 1000);

    // One more pixel would fit by payload alone but not with its metadata.
    match store.store_image(rgba_image(123, 1)) {
        Err(err) => assert_eq!(err.code, KittyErrorCode::Einval),
        Ok(_) => panic!("metadata must count toward the quota"),
    }
    assert_eq!(store.image_count(), 1);
    assert!(store.get(id).is_some());
}

#[test]
fn free_with_positive_refcount_retains_data_and_removes_at_zero() {
    let mut store = KittyImageStore::default();
    let id_a = store.store_image(rgba_image(10, 10)).unwrap();
    store.add_placement_ref(id_a);
    store.add_placement_ref(id_a);
    assert_eq!(store.refcount(id_a), Some(2));
    store.free(id_a);
    assert_eq!(store.refcount(id_a), Some(1));
    assert!(store.get(id_a).is_some());
    assert_eq!(store.total_bytes(), 400 + KITTY_IMAGE_METADATA_BYTES);
    store.free(id_a);
    assert!(store.get(id_a).is_none());
    assert_eq!(store.image_count(), 0);
    assert_eq!(store.total_bytes(), 0);
}

#[test]
fn byte_accounting_matches_hand_computed_total_with_scaled_variants() {
    let mut store = KittyImageStore::default();
    let id_a = store.store_image(rgba_image(10, 10)).unwrap();
    let id_b = store.store_image(rgba_image(4, 5)).unwrap();
    let first = super::ScaledImageKey {
        source: crate::panes::sixel::PixelRect::new(0, 0, 10, 10),
        size: (3, 2),
    };
    let second = super::ScaledImageKey {
        size: (5, 4),
        ..first
    };
    store.add_scaled_variant(id_a, first, vec![0; 120]);
    store.add_scaled_variant(id_a, second, vec![0; 60]);
    store.add_scaled_variant(id_a, first, vec![0; 100]);
    assert_eq!(
        store.total_bytes(),
        2 * KITTY_IMAGE_METADATA_BYTES + 400 + 80 + 100 + 60
    );
    assert_eq!(store.scaled_variant(id_a, first).unwrap().len(), 100);
    store.free(id_a);
    assert_eq!(store.total_bytes(), KITTY_IMAGE_METADATA_BYTES + 80);
    store.free(id_b);
    assert_eq!(store.total_bytes(), 0);
}

#[test]
fn remove_placement_ref_keeps_data_but_makes_image_evictable() {
    let mut store = KittyImageStore::with_quota(2000);
    let id_a = store.store_image(rgba_image(10, 10)).unwrap();
    store.add_placement_ref(id_a);
    store.remove_placement_ref(id_a);
    assert!(store.get(id_a).is_some());
    assert_eq!(store.refcount(id_a), Some(0));
    let id_b = store.store_image(rgba_image(10, 10)).unwrap();
    store.add_placement_ref(id_b);
    let id_c = store.store_image(rgba_image(10, 10)).unwrap();
    assert!(store.get(id_a).is_none());
    assert!(store.get(id_b).is_some());
    assert!(store.get(id_c).is_some());
    store.add_placement_ref(id_c);
    let result = store.store_image(rgba_image(10, 10));
    match result {
        Err(err) => assert_eq!(err.code, KittyErrorCode::Einval),
        Ok(_) => panic!("admission must fail when only referenced images remain"),
    }
    assert_eq!(store.total_bytes(), 2 * (400 + KITTY_IMAGE_METADATA_BYTES));
}

#[test]
fn scaled_variant_reservation_respects_quota_when_images_are_pinned() {
    let mut store = KittyImageStore::with_quota(2000);
    let id_a = store.store_image(rgba_image(10, 10)).unwrap();
    store.add_placement_ref(id_a);
    let key = super::ScaledImageKey {
        source: crate::panes::sixel::PixelRect::new(0, 0, 10, 10),
        size: (12, 12),
    };
    assert!(store.reserve_scaled_variant(id_a, key, 576));
    assert!(!store.reserve_scaled_variant(id_a, key, 1200));
    assert!(!store.reserve_scaled_variant(id_a, key, usize::MAX));
}

#[test]
fn store_image_rejects_when_referenced_images_fill_the_quota() {
    // Pin enough images to fill the quota: nothing is evictable, so the
    // next admission must fail and the retained bytes must stay within
    // the declared hard limit rather than silently exceeding it.
    let mut store = KittyImageStore::with_quota(2500);
    let id_a = store.store_image(rgba_image(10, 10)).unwrap();
    let id_b = store.store_image(rgba_image(10, 10)).unwrap();
    store.add_placement_ref(id_a);
    store.add_placement_ref(id_b);
    assert_eq!(store.total_bytes(), 2 * (400 + KITTY_IMAGE_METADATA_BYTES));

    let result = store.store_image(rgba_image(10, 10));
    match result {
        Err(err) => assert_eq!(err.code, KittyErrorCode::Einval),
        Ok(_) => panic!("admission must fail when the quota is pinned"),
    }

    // The store stays within quota and existing placements remain valid.
    assert_eq!(store.total_bytes(), 2 * (400 + KITTY_IMAGE_METADATA_BYTES));
    assert!(store.get(id_a).is_some());
    assert!(store.get(id_b).is_some());

    // A smaller image still fits within the quota without evicting
    // anything; a full-size one cannot be admitted while nothing is
    // evictable. Once a placement is released, its image becomes
    // evictable and admission succeeds again.
    assert!(store.store_image(rgba_image(5, 5)).is_ok());
    let with_small = 2 * (400 + KITTY_IMAGE_METADATA_BYTES) + (100 + KITTY_IMAGE_METADATA_BYTES);
    assert_eq!(store.total_bytes(), with_small);
    assert!(store.store_image(rgba_image(10, 10)).is_err());
    assert_eq!(store.total_bytes(), with_small);
    store.remove_placement_ref(id_a);
    let id_c = store.store_image(rgba_image(10, 10)).unwrap();
    assert!(store.get(id_a).is_none());
    assert!(store.get(id_b).is_some());
    assert!(store.get(id_c).is_some());
    assert_eq!(
        store.total_bytes(),
        2 * (400 + KITTY_IMAGE_METADATA_BYTES) + (100 + KITTY_IMAGE_METADATA_BYTES)
    );
}

#[test]
fn variant_growth_on_referenced_image_is_bounded_and_regenerable() {
    // Variants of a referenced image used to grow without bound because
    // only whole unreferenced images were evictable. Now the oldest
    // derived entries are evicted to stay inside the quota, and an
    // evicted variant is regenerated from the base raster on demand.
    let mut store = KittyImageStore::with_quota(3000);
    let id = store.store_image(rgba_image(10, 10)).unwrap();
    store.add_placement_ref(id);
    assert_eq!(store.total_bytes(), 400 + KITTY_IMAGE_METADATA_BYTES);

    let key = |n: usize| super::ScaledImageKey {
        source: crate::panes::sixel::PixelRect::new(0, 0, 10, 10),
        size: (n, n),
    };
    for n in 1..=20usize {
        store.add_scaled_variant(id, key(n), vec![0; n * n * 4]);
    }
    assert!(
        store.total_bytes() <= 3000,
        "derived data must stay within the quota, got {}",
        store.total_bytes()
    );

    // Older variants were evicted but remain correct: a miss regenerates
    // the exact same raster that was cached.
    let evicted_key = key(1);
    assert!(!store.has_scaled_variant(id, evicted_key));
    let regenerated = store
        .scaled_variant_or_regenerate(id, evicted_key)
        .expect("evicted variants regenerate");
    assert_eq!(regenerated.len(), 4);

    // A regeneration that cannot be retained still returns the bytes so
    // the render stays correct while the quota holds.
    assert!(store.total_bytes() <= 3000);
}

#[test]
fn base64_encodings_are_accounted_and_shared() {
    let mut store = KittyImageStore::with_quota(100000);
    let id = store.store_image(rgba_image(10, 10)).unwrap();
    let before = store.total_bytes();

    let first = store.base64_for(id, None, false).unwrap();
    let accounted = store.total_bytes() - before;
    assert_eq!(
        accounted,
        first.len(),
        "cached encodings count toward the quota"
    );

    // Cache hits return the same shared payload rather than a clone.
    let second = store.base64_for(id, None, false).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &second));
    assert_eq!(store.total_bytes(), before + first.len());

    // Encoding payloads are evicted ahead of image bytes under pressure.
    let tight = store.total_bytes() - 1;
    let mut store2 = KittyImageStore::with_quota(tight);
    let id2 = store2.store_image(rgba_image(10, 10)).unwrap();
    store2.base64_for(id2, None, false).unwrap();
    assert!(store2.total_bytes() <= tight);
    assert!(store2.get(id2).is_some());
}

#[test]
fn rgb24_images_are_normalized_to_rgba_at_store_time() {
    let mut store = KittyImageStore::default();
    let id = store
        .store_image(DecodedImage {
            bytes: (1..=12).collect(),
            width: 2,
            height: 2,
            format: KittyFormat::Rgb24,
        })
        .unwrap();
    let image = store.get(id).unwrap();
    assert_eq!(
        image.rgba,
        vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
    );
    assert_eq!(store.total_bytes(), 16 + KITTY_IMAGE_METADATA_BYTES);
}

#[test]
fn a_large_insert_over_many_small_variants_evicts_in_a_single_pass() {
    // 40k distinct tiny variants; admitting one image that needs most of
    // the quota back evicts all of them. Rescanning every entry for each
    // eviction would take 40k * 40k entry visits.
    let quota = 4_000_000;
    let mut store = KittyImageStore::with_quota(quota);
    let id = store.store_image(rgba_image(2, 2)).unwrap();
    store.add_placement_ref(id);
    let key = |n: usize| super::ScaledImageKey {
        source: crate::panes::sixel::PixelRect::new(0, 0, 2, 2),
        size: (n, 1),
    };
    for n in 1..=40_000usize {
        store.add_scaled_variant(id, key(n), vec![0; 16]);
    }
    assert!(store.total_bytes() <= quota);
    let started = std::time::Instant::now();
    let big = store
        .store_image(rgba_image(1000, 900))
        .expect("derived entries make room");
    assert!(store.get(big).is_some());
    assert!(store.total_bytes() <= quota);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "eviction took {:?}",
        started.elapsed()
    );
}

#[test]
fn eviction_of_derived_entries_removes_the_least_recently_used_first() {
    let mut store = KittyImageStore::with_quota(3700);
    let id = store.store_image(rgba_image(10, 10)).unwrap();
    store.add_placement_ref(id);
    let key = |n: usize| super::ScaledImageKey {
        source: crate::panes::sixel::PixelRect::new(0, 0, 10, 10),
        size: (n, n),
    };
    store.add_scaled_variant(id, key(10), vec![0; 400]);
    store.add_scaled_variant(id, key(11), vec![0; 400]);
    // a hit makes key(10) newer than key(11)
    store.scaled_variant_or_regenerate(id, key(10)).unwrap();
    store.add_scaled_variant(id, key(12), vec![0; 2100]);
    assert!(store.total_bytes() <= 3700);
    assert!(store.has_scaled_variant(id, key(10)));
    assert!(store.has_scaled_variant(id, key(12)));
    assert!(!store.has_scaled_variant(id, key(11)));
}

#[test]
fn base64_of_an_evicted_variant_is_regenerated() {
    let mut store = KittyImageStore::with_quota(100_000);
    let id = store.store_image(rgba_image(4, 4)).unwrap();
    let key = super::ScaledImageKey {
        source: crate::panes::sixel::PixelRect::new(0, 0, 4, 4),
        size: (8, 8),
    };
    assert!(!store.has_scaled_variant(id, key));
    let encoded = store
        .base64_for(id, Some(key), false)
        .expect("an evicted variant is regenerated for its encoding");
    assert_eq!(encoded.len(), 8 * 8 * 4 / 3 * 4 + 4);
    // an unknown image or an invalid variant still yields nothing
    assert!(store.base64_for(id + 1000, Some(key), false).is_none());
    let outside = super::ScaledImageKey {
        source: crate::panes::sixel::PixelRect::new(100, 0, 4, 4),
        size: (8, 8),
    };
    assert!(store.base64_for(id, Some(outside), false).is_none());
}

#[test]
fn an_oversized_variant_is_never_regenerated_as_an_empty_raster() {
    let mut store = KittyImageStore::with_quota(100_000);
    let id = store.store_image(rgba_image(4, 4)).unwrap();
    let key = super::ScaledImageKey {
        source: crate::panes::sixel::PixelRect::new(0, 0, 4, 4),
        size: (20_000, 20_000),
    };
    assert!(store.scaled_variant_or_regenerate(id, key).is_none());
    assert!(store.base64_for(id, Some(key), false).is_none());
}
