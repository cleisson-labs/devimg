use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use super::{
    hashed_manifest_output, manifest_output_path, operation_hash_matches, output_metadata,
    stable_manifest_output, validate_output_file,
};
use crate::config::{parse_config, CropPosition, FitMode, FormatKind};
use crate::manifest::ManifestOutput;
use crate::pipeline::{
    check_with_options, optimize, CheckOptions, Operation, OptimizeOptions, SourceImage,
};
use crate::plan::legacy_operation_hash;

#[test]
fn validate_output_file_reports_dimension_decode_and_avif_container_issues() {
    let project = TempProject::new("validate-output-file");
    let image_path = project.root.join("out.png");
    write_png(&image_path, 2, 3);
    let mut png_operation = operation(FormatKind::Png, false);
    png_operation.width = 1;
    png_operation.height = 1;

    let stale =
        validate_output_file(&png_operation, &image_path, "public/out.png").expect("stale issue");
    assert_eq!(stale.kind, "stale");
    assert!(stale
        .message
        .contains("output dimensions are 2x3, expected 1x1"));

    let invalid_path = project.root.join("not-image.png");
    fs::write(&invalid_path, b"not an image").expect("invalid image written");
    let invalid = validate_output_file(&png_operation, &invalid_path, "public/not-image.png")
        .expect("invalid");
    assert_eq!(invalid.kind, "invalid_output");

    let avif_operation = operation(FormatKind::Avif, false);
    let missing = project.root.join("missing.avif");
    let missing_issue =
        validate_output_file(&avif_operation, &missing, "public/missing.avif").expect("missing");
    assert_eq!(missing_issue.kind, "invalid_output");

    let bad_ftyp = project.root.join("bad-ftyp.avif");
    fs::write(&bad_ftyp, b"too short").expect("bad avif written");
    let bad_ftyp_issue =
        validate_output_file(&avif_operation, &bad_ftyp, "public/bad-ftyp.avif").expect("bad ftyp");
    assert!(bad_ftyp_issue.message.contains("missing the ftyp box"));

    let bad_brand = project.root.join("bad-brand.avif");
    fs::write(&bad_brand, b"\0\0\0\x18ftypmif1\0\0\0\0mif1").expect("bad brand written");
    let bad_brand_issue =
        validate_output_file(&avif_operation, &bad_brand, "public/bad-brand.avif")
            .expect("bad brand");
    assert!(bad_brand_issue.message.contains("missing the avif brand"));

    let valid_avif = project.root.join("valid.avif");
    fs::write(&valid_avif, b"\0\0\0\x18ftypavif\0\0\0\0mif1").expect("valid avif written");
    assert!(validate_output_file(&avif_operation, &valid_avif, "public/valid.avif").is_none());
}

#[test]
fn stable_manifest_output_accepts_current_and_legacy_hashes_and_reports_stale_entries() {
    let operation = operation(FormatKind::Webp, false);
    let current = manifest_output(
        &operation,
        "public/card.webp",
        "blake3:current",
        &operation.operation_hash,
    );
    let by_output = HashMap::from([(current.output_path.as_str(), &current)]);
    let mut issues = Vec::new();

    let found = stable_manifest_output(&operation, "config-hash", &by_output, &mut issues)
        .expect("current output");
    assert_eq!(found.hash, "blake3:current");
    assert!(issues.is_empty());

    let legacy_hash = legacy_operation_hash(&operation, "config-hash");
    let legacy = manifest_output(
        &operation,
        "public/card.webp",
        "blake3:legacy",
        &legacy_hash,
    );
    let by_output = HashMap::from([(legacy.output_path.as_str(), &legacy)]);
    assert!(stable_manifest_output(&operation, "config-hash", &by_output, &mut issues).is_some());
    assert!(operation_hash_matches(&legacy, &operation, "config-hash"));

    let stale = manifest_output(&operation, "public/card.webp", "blake3:stale", "stale-hash");
    let by_output = HashMap::from([(stale.output_path.as_str(), &stale)]);
    issues.clear();
    assert!(stable_manifest_output(&operation, "config-hash", &by_output, &mut issues).is_some());
    assert_eq!(issues[0].kind, "stale");
    assert!(!operation_hash_matches(&stale, &operation, "config-hash"));

    issues.clear();
    let empty = HashMap::new();
    assert!(stable_manifest_output(&operation, "config-hash", &empty, &mut issues).is_none());
    assert_eq!(issues[0].message, "planned output is missing from manifest");
}

#[test]
fn hashed_manifest_output_reports_missing_duplicate_and_hash_path_drift() {
    let operation = operation(FormatKind::Webp, true);
    let mut issues = Vec::new();
    let missing = HashMap::new();
    assert!(
        hashed_manifest_output(&operation, "config-hash", &missing, &mut issues)
            .expect("missing handled")
            .is_none()
    );
    assert_eq!(
        issues[0].message,
        "planned hashed output is missing from manifest"
    );

    let current = manifest_output(
        &operation,
        "public/card.abcdef123456.webp",
        "blake3:abcdef1234567890",
        &operation.operation_hash,
    );
    let duplicate = manifest_output(
        &operation,
        "public/card.wrong.webp",
        "blake3:duplicate",
        &operation.operation_hash,
    );
    let by_hash = HashMap::from([(
        operation.operation_hash.as_str(),
        vec![&current, &duplicate],
    )]);
    issues.clear();
    let found = hashed_manifest_output(&operation, "config-hash", &by_hash, &mut issues)
        .expect("hashed lookup")
        .expect("output found");
    assert_eq!(found.output_path, "public/card.abcdef123456.webp");
    assert!(issues
        .iter()
        .any(|issue| issue.message.contains("duplicate outputs")));

    let mismatched = manifest_output(
        &operation,
        "public/card.not-the-hash.webp",
        "blake3:abcdef1234567890",
        &operation.operation_hash,
    );
    let by_hash = HashMap::from([(operation.operation_hash.as_str(), vec![&mismatched])]);
    issues.clear();
    assert!(
        hashed_manifest_output(&operation, "config-hash", &by_hash, &mut issues)
            .expect("mismatch handled")
            .is_some()
    );
    assert_eq!(
        issues[0].message,
        "manifest output path does not match its content hash"
    );
}

#[test]
fn manifest_output_path_rejects_paths_outside_project_root() {
    let project = TempProject::new("manifest-output-path");
    let config = project_config(&project.root);
    let mut output = manifest_output(
        &operation(FormatKind::Webp, false),
        "../escape.webp",
        "blake3:hash",
        "operation-hash",
    );

    let error = manifest_output_path(&config, &output).expect_err("escaping path rejected");
    assert!(error
        .to_string()
        .contains("manifest output path escapes project root"));

    output.output_path = "public/card.webp".to_string();
    assert_eq!(
        manifest_output_path(&config, &output).expect("path resolves"),
        project.root.join("public/card.webp")
    );
}

#[test]
fn output_metadata_wraps_io_errors() {
    let project = TempProject::new("metadata-error");
    let missing = project.root.join("missing.webp");

    let error = output_metadata(&missing).expect_err("missing metadata should fail");

    assert!(error.to_string().contains("I/O error"));
}

#[test]
fn check_reports_missing_manifest_and_modified_outputs_without_writing_report() {
    let missing_project = TempProject::new("missing-manifest");
    let input = missing_project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    write_png(&input.join("card.png"), 2, 2);
    let missing_config = project_config(&missing_project.root);

    let missing = check_with_options(
        &missing_config,
        CheckOptions {
            write_report: false,
        },
    )
    .expect("missing manifest is a check issue");
    assert!(!missing.passed);
    assert!(missing
        .result
        .issues
        .iter()
        .any(|issue| issue.kind == "missing_manifest"));
    assert!(!missing_project.root.join("devimg-report.md").exists());

    let modified_project = TempProject::new("modified-output");
    let input = modified_project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    write_png(&input.join("card.png"), 2, 2);
    let modified_config = project_config(&modified_project.root);
    optimize(&modified_config, OptimizeOptions::default()).expect("optimize succeeds");
    let output_path = modified_project
        .root
        .join("public/images/generated/card.card.2.png");
    fs::write(&output_path, b"changed").expect("output changed");

    let modified = check_with_options(
        &modified_config,
        CheckOptions {
            write_report: false,
        },
    )
    .expect("modified output is a check issue");
    assert!(!modified.passed);
    assert!(modified
        .result
        .issues
        .iter()
        .any(|issue| issue.kind == "modified"));
}

fn operation(format: FormatKind, content_hash_filenames: bool) -> Operation {
    Operation {
        source: SourceImage {
            source_name: "portfolio".to_string(),
            path: PathBuf::from("assets/card.png"),
            project_path: "assets/card.png".to_string(),
            relative_path: PathBuf::from("card.png"),
            output_root: PathBuf::from("public"),
            output_root_project_path: "public".to_string(),
            width: 100,
            height: 100,
            bytes: 10,
            hash: "blake3:source".to_string(),
            format: FormatKind::Png,
        },
        preset: "card".to_string(),
        fit: FitMode::Cover,
        crop: CropPosition::CENTER,
        quality: 80,
        format,
        width: 100,
        height: 100,
        strip_metadata: true,
        content_hash_filenames,
        output_path: PathBuf::from("public/card.webp"),
        output_project_path: "public/card.webp".to_string(),
        operation_hash: "operation-hash".to_string(),
    }
}

fn manifest_output(
    operation: &Operation,
    output_path: &str,
    hash: &str,
    operation_hash: &str,
) -> ManifestOutput {
    ManifestOutput {
        source_path: operation.source.project_path.clone(),
        source_hash: operation.source.hash.clone(),
        source_width: operation.source.width,
        source_height: operation.source.height,
        source_bytes: operation.source.bytes,
        output_path: output_path.to_string(),
        preset: operation.preset.clone(),
        fit: operation.fit.label().to_string(),
        width: operation.width,
        height: operation.height,
        format: operation.format.label().to_string(),
        bytes: 20,
        hash: hash.to_string(),
        operation_hash: operation_hash.to_string(),
    }
}

fn write_png(path: &Path, width: u32, height: u32) {
    let mut image = RgbaImage::new(width, height);
    for pixel in image.pixels_mut() {
        *pixel = Rgba([10, 20, 30, 255]);
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("png encoded");
    fs::write(path, bytes.into_inner()).expect("png written");
}

fn project_config(root: &Path) -> crate::Config {
    parse_config(
        &root.join("devimg.toml"),
        r#"
[project]
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "card"
widths = [2]
formats = ["png"]
"#,
    )
    .expect("config parses")
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
            "devimg-check-tests-{name}-{}-{unique}",
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
