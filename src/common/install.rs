#[macro_export]
macro_rules! asset_map {
    ($($path:literal),+) => {
        {
            let mut assets = std::collections::HashMap::new();
            $(
                assets.insert($path, include_bytes!(concat!("../", $path)).to_vec());
            )+
            assets
        }
    }
}
