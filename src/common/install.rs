#[macro_export]
macro_rules! asset_map {
    ($($src:literal => $dst:literal),+) => {
        {
            let mut assets = std::collections::HashMap::new();
            $(
                assets.insert($dst, include_bytes!(concat!("../", $src)).to_vec());
            )+
            assets
        }
    }
}
