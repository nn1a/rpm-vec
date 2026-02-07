/// Integration tests for compression support
#[cfg(test)]
mod tests {
    use std::io::Write;

    #[test]
    fn test_gz_compression_support() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let original = b"<?xml version=\"1.0\"?><metadata></metadata>";
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(original).unwrap();
        let compressed = encoder.finish().unwrap();

        // Decompress using our code
        use flate2::read::GzDecoder;
        use std::io::Read;
        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        assert_eq!(original.to_vec(), decompressed);
    }

    #[test]
    fn test_zstd_compression_support() {
        let original = b"<?xml version=\"1.0\"?><metadata></metadata>";

        // Compress
        let compressed = zstd::encode_all(&original[..], 3).unwrap();

        // Decompress
        let decompressed = zstd::decode_all(&compressed[..]).unwrap();

        assert_eq!(original.to_vec(), decompressed);
    }

    #[test]
    fn test_auto_detect_extensions() {
        use std::path::Path;

        let extensions = vec![
            ("test.xml.gz", "gz"),
            ("test.xml.zst", "zst"),
            ("test.xml.zstd", "zstd"),
            ("test.xml", "xml"),
            ("test", ""),
        ];

        for (filename, expected) in extensions {
            let path = Path::new(filename);
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            assert_eq!(ext, expected, "Failed for {}", filename);
        }
    }
}
