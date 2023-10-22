use data_encoding::HEXLOWER;
use ring::digest::{self, Digest, SHA256};
use serde::{Deserialize, Serialize};
use surf::Url;

#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct Download {
    pub url: String,
    pub url_hash: String,
    pub file_name: String,
}

impl Download {
    pub fn from(url: &str) -> Self {
        match Url::parse(url) {
            Ok(u) => u
                .path_segments()
                .map_or_else(Download::default, |segments| {
                    let url_hash = Download::sha256_digest(url.to_string());
                    let file_name = segments.last().unwrap_or("").to_string();

                    Download {
                        url: url.to_string(),
                        url_hash,
                        file_name,
                    }
                }),
            Err(_) => Download::default(),
        }
    }

    fn sha256_digest(data: String) -> String {
        let bytes = data.into_bytes();
        let digest: Digest = digest::digest(&SHA256, &bytes);
        HEXLOWER.encode(digest.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_download() {
        let download = Download::from("https://github.com/example/plugin.wasm");
        assert_eq!(download.url, "https://github.com/example/plugin.wasm");
        assert_eq!(
            download.url_hash,
            "4db716d595a13b42ad1ac6230945cf401b9ad996db3756efc05a2d85b110a81d"
        );
        assert_eq!(download.file_name, "plugin.wasm");
    }

    #[test]
    fn test_empty_download() {
        let d1 = Download::from("https://example.com");
        assert_eq!(d1.url, "https://example.com");
        assert_eq!(
            d1.url_hash,
            "100680ad546ce6a577f42f52df33b4cfdca756859e664b8d7de329b150d09ce9"
        );
        assert_eq!(d1.file_name, "");

        let d2 = Download::from("github.com");
        assert_eq!(d2.url, "");
        assert_eq!(d2.url_hash, "");
        assert_eq!(d2.file_name, "");
    }
}
