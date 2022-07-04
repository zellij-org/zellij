pub const DEFAULT_CACHE_FILE_PATH: &str = "/tmp/status-bar-tips.cache";
pub const MAX_CACHE_HITS: usize = 20; // this should be 10, but right now there's a bug where the plugin load function is called twice, and sot he cache is hit twice
