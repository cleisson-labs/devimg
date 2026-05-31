use std::path::PathBuf;

use super::DevimgError;

#[test]
fn devimg_error_io_accepts_owned_paths() {
    let error = DevimgError::io(
        PathBuf::from("public/card.webp"),
        std::io::Error::other("disk full"),
    );

    assert!(error.to_string().contains("public/card.webp"));
    assert!(error.to_string().contains("disk full"));
}
