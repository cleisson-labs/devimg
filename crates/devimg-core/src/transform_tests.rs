use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use crate::config::{CropPosition, FitMode, FormatKind};
use crate::pipeline::{Operation, SourceImage};
use crate::DevimgError;

use super::{
    avif_encode_error, crop_offset, encode_image, execute_operation, final_output_path,
    final_output_project_path, non_empty_encoded, safe_write, transform_image,
};

#[test]
fn webp_quality_changes_encoded_size() {
    let image = detailed_image(128, 128);

    let low = encode_image(&image, FormatKind::Webp, 10).expect("low quality webp encodes");
    let high = encode_image(&image, FormatKind::Webp, 90).expect("high quality webp encodes");

    assert_ne!(low, high);
    assert!(
        high.len() > low.len() + (low.len() / 5),
        "expected high-quality WebP to be materially larger, low={} high={}",
        low.len(),
        high.len()
    );

    let decoded =
        image::load_from_memory_with_format(&high, ImageFormat::WebP).expect("webp decodes");
    assert_eq!((decoded.width(), decoded.height()), (128, 128));
}

#[test]
fn non_empty_encoded_rejects_empty_encoder_output() {
    let error = non_empty_encoded("WebP", Vec::new()).expect_err("empty output rejected");
    assert_eq!(error, "WebP encoder returned empty output");
    assert_eq!(
        non_empty_encoded("AVIF", vec![1, 2, 3]).expect("bytes pass through"),
        vec![1, 2, 3]
    );
}

#[test]
fn avif_encode_error_formats_encoder_errors() {
    assert_eq!(
        avif_encode_error(ravif::Error::TooFewPixels),
        "Provided buffer is smaller than width * height"
    );
}

#[test]
fn jpeg_quality_changes_encoded_size_and_preserves_dimensions() {
    let image = detailed_image(128, 128);

    let low = encode_image(&image, FormatKind::Jpeg, 10).expect("low quality jpeg encodes");
    let high = encode_image(&image, FormatKind::Jpeg, 90).expect("high quality jpeg encodes");

    assert_ne!(low, high);
    assert!(
        high.len() > low.len() + (low.len() / 5),
        "expected high-quality JPEG to be materially larger, low={} high={}",
        low.len(),
        high.len()
    );

    let decoded =
        image::load_from_memory_with_format(&high, ImageFormat::Jpeg).expect("jpeg decodes");
    assert_eq!((decoded.width(), decoded.height()), (128, 128));
}

#[test]
fn png_quality_is_ignored_and_preserves_dimensions() {
    let image = detailed_image(64, 64);

    let low = encode_image(&image, FormatKind::Png, 10).expect("low quality png encodes");
    let high = encode_image(&image, FormatKind::Png, 90).expect("high quality png encodes");

    assert_eq!(low, high);

    let decoded =
        image::load_from_memory_with_format(&high, ImageFormat::Png).expect("png decodes");
    assert_eq!((decoded.width(), decoded.height()), (64, 64));
}

#[test]
fn avif_quality_changes_encoded_size_and_writes_avif_bytes() {
    let image = detailed_image(64, 64);

    let low = encode_image(&image, FormatKind::Avif, 1).expect("low quality avif encodes");
    let high = encode_image(&image, FormatKind::Avif, 90).expect("high quality avif encodes");

    assert_ne!(low, high);
    assert!(
        high.len() > low.len(),
        "expected high-quality AVIF to be larger, low={} high={}",
        low.len(),
        high.len()
    );
    assert_avif_container(&high);
}

#[test]
fn encode_image_reports_empty_dimension_jpeg_and_png_errors() {
    let empty = DynamicImage::ImageRgba8(RgbaImage::new(0, 1));

    for format in [FormatKind::Jpeg, FormatKind::Png] {
        let error =
            encode_image(&empty, format, 80).expect_err("empty dimensions should not encode");
        assert!(
            !error.is_empty(),
            "expected {format:?} encoder to report a non-empty error"
        );
    }
}

#[test]
fn cover_crop_centers_wide_images() {
    let mut source = RgbaImage::new(6, 2);
    for (x, _y, pixel) in source.enumerate_pixels_mut() {
        *pixel = match x {
            0 | 1 => Rgba([255, 0, 0, 255]),
            2 | 3 => Rgba([0, 255, 0, 255]),
            _ => Rgba([0, 0, 255, 255]),
        };
    }

    let cropped = transform_image(
        DynamicImage::ImageRgba8(source),
        FitMode::Cover,
        CropPosition::CENTER,
        2,
        2,
    );

    assert_eq!((cropped.width(), cropped.height()), (2, 2));
    let pixels = cropped.to_rgba8();
    assert!(pixels.pixels().all(|pixel| pixel.0 == [0, 255, 0, 255]));
}

#[test]
fn cover_crop_honors_vertical_anchors() {
    let source = vertical_stripes();

    let top = transform_image(
        DynamicImage::ImageRgba8(source.clone()),
        FitMode::Cover,
        CropPosition { x: 0.5, y: 0.0 },
        2,
        2,
    )
    .to_rgba8();
    let bottom = transform_image(
        DynamicImage::ImageRgba8(source),
        FitMode::Cover,
        CropPosition { x: 0.5, y: 1.0 },
        2,
        2,
    )
    .to_rgba8();

    assert!(top.pixels().all(|pixel| pixel.0 == [255, 0, 0, 255]));
    assert!(bottom.pixels().all(|pixel| pixel.0 == [0, 0, 255, 255]));
}

#[test]
fn cover_crop_honors_horizontal_anchors() {
    let source = horizontal_stripes();

    let left = transform_image(
        DynamicImage::ImageRgba8(source.clone()),
        FitMode::Cover,
        CropPosition { x: 0.0, y: 0.5 },
        2,
        2,
    )
    .to_rgba8();
    let right = transform_image(
        DynamicImage::ImageRgba8(source),
        FitMode::Cover,
        CropPosition { x: 1.0, y: 0.5 },
        2,
        2,
    )
    .to_rgba8();

    assert!(left.pixels().all(|pixel| pixel.0 == [255, 0, 0, 255]));
    assert!(right.pixels().all(|pixel| pixel.0 == [0, 0, 255, 255]));
}

#[test]
fn cover_crop_honors_custom_normalized_position() {
    let source = vertical_stripes();

    let cropped = transform_image(
        DynamicImage::ImageRgba8(source),
        FitMode::Cover,
        CropPosition { x: 0.5, y: 0.25 },
        2,
        2,
    )
    .to_rgba8();

    assert_eq!(cropped.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(cropped.get_pixel(0, 1).0, [0, 255, 0, 255]);
}

#[test]
fn crop_offset_rounds_fractional_positions() {
    assert_eq!(crop_offset(3, 0.0), 0);
    assert_eq!(crop_offset(3, 0.5), 2);
    assert_eq!(crop_offset(3, 1.0), 3);
}

#[test]
fn final_output_project_path_adds_hash_fragment_or_preserves_plain_paths() {
    let mut operation = operation(true);

    assert_eq!(
        final_output_project_path(
            "public/images/card.webp",
            "blake3:abcdef1234567890",
            &operation
        )
        .expect("hashed path renders"),
        "public/images/card.abcdef123456.webp"
    );
    assert_eq!(
        final_output_project_path("card.webp", "fedcba9876543210", &operation)
            .expect("hash without prefix renders"),
        "card.fedcba987654.webp"
    );

    let error = final_output_project_path("public/images/card", "blake3:abcdef", &operation)
        .expect_err("hashed output without extension should fail");
    assert!(error
        .to_string()
        .contains("could not derive extension for hashed output filename"));

    operation.content_hash_filenames = false;
    assert_eq!(
        final_output_project_path("public/images/card", "blake3:abcdef", &operation)
            .expect("plain path is preserved"),
        "public/images/card"
    );
}

#[test]
fn safe_write_creates_parent_replaces_when_allowed_and_rejects_missing_filename() {
    let project = TempProject::new("safe-write");
    let output = project.root.join("nested/card.webp");

    safe_write(&output, b"first", false).expect("initial write succeeds");
    assert_eq!(fs::read(&output).expect("output readable"), b"first");

    safe_write(&output, b"second", true).expect("replace succeeds");
    assert_eq!(fs::read(&output).expect("output readable"), b"second");

    let error = safe_write(Path::new(""), b"bytes", false).expect_err("empty path should fail");
    assert!(error.to_string().contains("missing filename"));
}

#[test]
fn safe_write_reports_parent_creation_errors() {
    let project = TempProject::new("safe-write-parent-error");
    let blocker = project.root.join("blocker");
    fs::write(&blocker, b"not a directory").expect("blocker file written");

    let error = safe_write(&blocker.join("card.webp"), b"bytes", false)
        .expect_err("file parent should block directory creation");

    assert_io_error_path(error, &blocker);
}

#[test]
fn safe_write_reports_tmp_write_errors() {
    let project = TempProject::new("safe-write-tmp-error");
    let output = project.root.join("public/card.webp");
    fs::create_dir_all(output.parent().expect("output parent")).expect("output parent created");
    let tmp_path = expected_tmp_path(&output);
    fs::create_dir(&tmp_path).expect("tmp path directory created");

    let error = safe_write(&output, b"bytes", false)
        .expect_err("directory tmp path should reject byte writes");

    assert_io_error_path(error, &tmp_path);
}

#[test]
fn safe_write_reports_remove_errors_when_allowed_target_is_directory() {
    let project = TempProject::new("safe-write-remove-error");
    let output = project.root.join("public/card.webp");
    fs::create_dir_all(&output).expect("directory output created");

    let error = safe_write(&output, b"bytes", true)
        .expect_err("directory target should not be removable as a file");

    assert_io_error_path(error, &output);
}

#[test]
fn safe_write_reports_rename_errors_for_existing_directory_targets() {
    let project = TempProject::new("safe-write-rename-error");
    let output = project.root.join("public/card.webp");
    fs::create_dir_all(&output).expect("directory output created");

    let error = safe_write(&output, b"bytes", false)
        .expect_err("directory target should reject file rename");

    assert_io_error_path(error, &output);
}

#[test]
fn execute_operation_writes_reuses_and_refuses_changed_outputs() {
    let project = TempProject::new("execute-operation");
    let source_path = project.root.join("assets/card.png");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("source parent created");
    write_image(&source_path, 8, 8);

    let output_path = project.root.join("public/card.webp");
    let mut operation = operation(false);
    operation.source.path = source_path;
    operation.source.width = 8;
    operation.source.height = 8;
    operation.output_path = output_path.clone();
    operation.output_project_path = "public/card.webp".to_string();
    operation.width = 4;
    operation.height = 4;

    let first = execute_operation(&operation, false).expect("operation writes output");
    assert_eq!(first.output_path, "public/card.webp");
    assert!(output_path.exists());

    let second = execute_operation(&operation, false).expect("matching output is reused");
    assert_eq!(second.hash, first.hash);
    assert_eq!(
        second.bytes,
        fs::metadata(&output_path).expect("metadata").len()
    );

    fs::write(&output_path, b"changed output").expect("output changed");
    let error = execute_operation(&operation, false).expect_err("changed output needs overwrite");
    assert!(error
        .to_string()
        .contains("refusing to overwrite unmanaged output"));

    let replaced = execute_operation(&operation, true).expect("changed output replaced");
    assert_eq!(replaced.hash, first.hash);
}

#[test]
fn execute_operation_reports_encode_errors() {
    let project = TempProject::new("execute-encode-error");
    let source_path = project.root.join("assets/card.png");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("source parent created");
    write_image(&source_path, 8, 8);

    let mut operation = operation(false);
    operation.source.path = source_path;
    operation.source.width = 8;
    operation.source.height = 8;
    operation.fit = FitMode::Fill;
    operation.format = FormatKind::Png;
    operation.output_path = project.root.join("public/card.png");
    operation.output_project_path = "public/card.png".to_string();
    operation.width = 0;
    operation.height = 1;

    let error = execute_operation(&operation, false).expect_err("encoding should fail");

    assert!(error.to_string().contains("image error at"));
    assert!(error.to_string().contains("public/card.png"));
}

#[cfg(unix)]
#[test]
fn execute_operation_reports_metadata_errors_for_reused_output() {
    use std::io::Write;

    let project = TempProject::new("execute-metadata-error");
    let source_path = project.root.join("assets/card.png");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("source parent created");
    write_image(&source_path, 8, 8);

    let output_path = project.root.join("public/card.webp");
    fs::create_dir_all(output_path.parent().expect("output parent"))
        .expect("output parent created");
    let mut operation = operation(false);
    operation.source.path = source_path.clone();
    operation.source.width = 8;
    operation.source.height = 8;
    operation.output_path = output_path.clone();
    operation.output_project_path = "public/card.webp".to_string();
    operation.width = 4;
    operation.height = 4;

    let processed = transform_image(
        image::open(&source_path).expect("source image opens"),
        operation.fit,
        operation.crop,
        operation.width,
        operation.height,
    );
    let encoded =
        encode_image(&processed, operation.format, operation.quality).expect("output encodes");

    let status = std::process::Command::new("mkfifo")
        .arg(&output_path)
        .status()
        .expect("mkfifo runs");
    assert!(status.success(), "mkfifo should create output fifo");

    let writer_path = output_path.clone();
    let writer = std::thread::spawn(move || {
        let mut fifo = fs::OpenOptions::new()
            .write(true)
            .open(&writer_path)
            .expect("fifo opens for writing");
        fs::remove_file(&writer_path).expect("fifo path removed");
        fifo.write_all(&encoded).expect("encoded bytes written");
    });

    let error =
        execute_operation(&operation, false).expect_err("removed reused output should fail");
    writer.join().expect("fifo writer joined");

    assert_io_error_path(error, &output_path);
}

#[test]
fn execute_operation_reports_source_decode_errors() {
    let project = TempProject::new("execute-decode-error");
    let source_path = project.root.join("assets/not-image.png");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("source parent created");
    fs::write(&source_path, b"not an image").expect("invalid source written");

    let mut operation = operation(false);
    operation.source.path = source_path;
    operation.output_path = project.root.join("public/card.webp");

    let error = execute_operation(&operation, false).expect_err("invalid source should fail");

    assert!(error.to_string().contains("could not decode image"));
}

#[test]
fn transform_image_supports_fill_and_contain_modes() {
    let image = detailed_image(8, 4);

    let filled = transform_image(image.clone(), FitMode::Fill, CropPosition::CENTER, 2, 7);
    assert_eq!((filled.width(), filled.height()), (2, 7));

    let contained = transform_image(image, FitMode::Contain, CropPosition::CENTER, 4, 4);
    assert_eq!((contained.width(), contained.height()), (4, 2));
}

#[test]
fn final_output_path_reports_missing_stem_or_extension_for_hashed_outputs() {
    let operation = operation(true);

    assert_eq!(
        final_output_path(
            Path::new("public/images/card.webp"),
            "blake3:abcdef1234567890",
            &operation
        )
        .expect("hashed filesystem path renders"),
        PathBuf::from("public/images/card.abcdef123456.webp")
    );

    let missing_stem = final_output_path(Path::new(""), "blake3:abcdef", &operation)
        .expect_err("missing stem should fail");
    assert!(missing_stem
        .to_string()
        .contains("could not derive file stem"));

    let missing_extension = final_output_path(Path::new("card"), "blake3:abcdef", &operation)
        .expect_err("missing extension should fail");
    assert!(missing_extension
        .to_string()
        .contains("could not derive file extension"));
}

fn horizontal_stripes() -> RgbaImage {
    let mut source = RgbaImage::new(6, 2);
    for (x, _y, pixel) in source.enumerate_pixels_mut() {
        *pixel = match x {
            0 | 1 => Rgba([255, 0, 0, 255]),
            2 | 3 => Rgba([0, 255, 0, 255]),
            _ => Rgba([0, 0, 255, 255]),
        };
    }
    source
}

fn vertical_stripes() -> RgbaImage {
    let mut source = RgbaImage::new(2, 6);
    for (_x, y, pixel) in source.enumerate_pixels_mut() {
        *pixel = match y {
            0 | 1 => Rgba([255, 0, 0, 255]),
            2 | 3 => Rgba([0, 255, 0, 255]),
            _ => Rgba([0, 0, 255, 255]),
        };
    }
    source
}

fn detailed_image(width: u32, height: u32) -> DynamicImage {
    let mut image = RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let mixed = x.wrapping_mul(31) ^ y.wrapping_mul(17);
        *pixel = Rgba([
            ((x * 3 + y * 5 + mixed) % 256) as u8,
            ((x * 11 + y * 7) % 256) as u8,
            ((x * y + mixed * 13) % 256) as u8,
            255,
        ]);
    }
    DynamicImage::ImageRgba8(image)
}

fn write_image(path: &Path, width: u32, height: u32) {
    let image = detailed_image(width, height);
    let mut bytes = Vec::new();
    image
        .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("png encoded");
    fs::write(path, bytes).expect("image written");
}

fn assert_avif_container(bytes: &[u8]) {
    assert!(bytes.len() > 16, "AVIF output should not be empty");
    assert_eq!(&bytes[4..8], b"ftyp");
    assert!(
        bytes.windows(4).any(|window| window == b"avif"),
        "AVIF output should contain the avif brand"
    );
}

fn expected_tmp_path(path: &Path) -> PathBuf {
    let file_name = path.file_name().expect("path has filename");
    path.with_file_name(format!(
        "{}.tmp-{}",
        file_name.to_string_lossy(),
        std::process::id()
    ))
}

fn assert_io_error_path(error: DevimgError, expected_path: &Path) {
    match error {
        DevimgError::Io { path, .. } => assert_eq!(path, expected_path),
        other => panic!(
            "expected I/O error at {}, got {other}",
            expected_path.display()
        ),
    }
}

fn operation(content_hash_filenames: bool) -> Operation {
    Operation {
        source: SourceImage {
            source_name: String::from("portfolio"),
            path: PathBuf::from("assets/images/card.png"),
            project_path: String::from("assets/images/card.png"),
            relative_path: PathBuf::from("card.png"),
            output_root: PathBuf::from("public/images/generated"),
            output_root_project_path: String::from("public/images/generated"),
            width: 100,
            height: 100,
            bytes: 100,
            hash: String::from("blake3:source"),
            format: FormatKind::Png,
        },
        preset: String::from("card"),
        fit: FitMode::Cover,
        crop: CropPosition::CENTER,
        quality: 80,
        format: FormatKind::Webp,
        width: 100,
        height: 100,
        strip_metadata: true,
        content_hash_filenames,
        output_path: PathBuf::from("public/images/generated/card.webp"),
        output_project_path: String::from("public/images/generated/card.webp"),
        operation_hash: String::from("operation-hash"),
    }
}

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "devimg-transform-tests-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temp project created");
        Self { root }
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
