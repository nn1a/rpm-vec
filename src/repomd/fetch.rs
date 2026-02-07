use crate::error::Result;
use std::io::Read;
use std::path::Path;

pub struct RepoFetcher;

impl RepoFetcher {
    /// Fetch repository metadata from local file
    pub fn fetch_local<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
        let mut file = std::fs::File::open(path)?;
        let mut contents = Vec::new();
        file.read_to_end(&mut contents)?;
        Ok(contents)
    }

    /// Decompress gzip data
    pub fn decompress_gz(data: &[u8]) -> Result<Vec<u8>> {
        use flate2::read::GzDecoder;
        let mut decoder = GzDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;
        Ok(decompressed)
    }

    /// Decompress zstd data
    pub fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>> {
        let decompressed = zstd::decode_all(data)?;
        Ok(decompressed)
    }

    /// Auto-detect compression and decompress
    pub fn auto_decompress<P: AsRef<Path>>(path: P, data: &[u8]) -> Result<Vec<u8>> {
        let extension = path
            .as_ref()
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");

        match extension {
            "gz" => Self::decompress_gz(data),
            "zst" | "zstd" => Self::decompress_zstd(data),
            _ => Ok(data.to_vec()),
        }
    }
}
