use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{hash_bytes, hash_file};
use crate::DevimgError;

#[test]
fn hashes_bytes_with_blake3_prefix() {
    let expected = format!("blake3:{}", blake3::hash(b"devimg").to_hex());
    assert_eq!(hash_bytes(b"devimg"), expected);
}

#[test]
fn hashes_files_and_wraps_read_errors() {
    let dir = temp_dir("hash_file");
    let path = dir.join("asset.bin");
    fs::write(&path, b"devimg").expect("test file writes");

    assert_eq!(
        hash_file(&path).expect("file hashes"),
        hash_bytes(b"devimg")
    );

    let missing = dir.join("missing.bin");
    let error = hash_file(&missing).expect_err("missing file errors");
    assert!(matches!(error, DevimgError::Io { path, .. } if path == missing));

    let _ = fs::remove_file(path);
    let _ = fs::remove_dir_all(dir);
}

fn temp_dir(label: &str) -> std::path::PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time after epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("devimg_{label}_{}_{}", std::process::id(), now));
    fs::create_dir_all(&path).expect("temp dir creates");
    path
}
