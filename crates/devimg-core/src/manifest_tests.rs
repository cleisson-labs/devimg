use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    export_manifest, manifest_export_to_json, manifest_export_to_typescript,
    manifest_export_to_typescript_with_options, manifest_to_json, read_manifest, write_manifest,
    Manifest, ManifestExportOptions, ManifestOutput, ManifestTypescriptOptions,
};

#[test]
fn manifest_json_round_trips_and_ignores_totals() {
    let manifest = Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "dev\"img.toml".to_string(),
        config_hash: "blake3:abc".to_string(),
        outputs: vec![ManifestOutput {
            source_path: "assets/images/card.png".to_string(),
            source_hash: "blake3:source".to_string(),
            source_width: 800,
            source_height: 450,
            source_bytes: 123,
            output_path: "public/images/card.webp".to_string(),
            preset: "project-card".to_string(),
            fit: "cover".to_string(),
            width: 640,
            height: 360,
            format: "webp".to_string(),
            bytes: 45,
            hash: "blake3:output".to_string(),
            operation_hash: "blake3:operation".to_string(),
        }],
    };

    let json = manifest_to_json(&manifest);
    assert!(json.contains("\"totals\""));
    let parsed: Manifest = serde_json::from_str(&json).expect("manifest parses");

    assert_eq!(parsed.config_path, "dev\"img.toml");
    assert_eq!(parsed.outputs[0].source_path, "assets/images/card.png");
    assert_eq!(parsed.outputs[0].fit, "cover");
    assert_eq!(parsed.source_bytes_total(), 123);
    assert_eq!(parsed.output_bytes_total(), 45);
}

#[test]
fn older_manifest_outputs_without_fit_still_parse() {
    let raw = r#"{
  "version": 1,
  "generated_at": "unix:1",
  "config_path": "devimg.toml",
  "config_hash": "blake3:config",
  "outputs": [
    {
      "source_path": "assets/images/card.png",
      "source_hash": "blake3:source",
      "source_width": 800,
      "source_height": 450,
      "source_bytes": 123,
      "output_path": "public/images/card.webp",
      "preset": "project-card",
      "width": 640,
      "height": 360,
      "format": "webp",
      "bytes": 45,
      "hash": "blake3:output",
      "operation_hash": "blake3:operation"
    }
  ]
}"#;

    let parsed: Manifest = serde_json::from_str(raw).expect("old manifest parses");

    assert_eq!(parsed.outputs[0].fit, "unknown");
}

#[test]
fn older_manifest_exports_with_defaulted_fields() {
    let raw = r#"{
  "config_path": "devimg.toml",
  "config_hash": "blake3:config",
  "outputs": [
    {
      "source_path": "assets/images/card.png",
      "source_hash": "blake3:source",
      "source_width": 800,
      "source_height": 450,
      "source_bytes": 123,
      "output_path": "public/images/card.webp",
      "preset": "project-card",
      "width": 640,
      "height": 360,
      "format": "webp",
      "bytes": 45,
      "hash": "blake3:output",
      "operation_hash": "blake3:operation"
    }
  ]
}"#;
    let parsed: Manifest = serde_json::from_str(raw).expect("older manifest parses");
    let options = ManifestExportOptions {
        strip_prefix: Some("public".to_string()),
        url_prefix: "/".to_string(),
    };

    let exported = export_manifest(&parsed, &options);
    let json = manifest_export_to_json(&parsed, &options);

    assert_eq!(exported.version, 1);
    assert_eq!(exported.generated_at, "unknown");
    assert_eq!(exported.sources[0].variants[0].fit, "unknown");
    assert_eq!(exported.sources[0].variants[0].src, "/images/card.webp");
    assert!(json.contains("\"generated_at\": \"unknown\""));
    assert!(json.contains("\"fit\": \"unknown\""));
}

#[test]
fn manifest_export_groups_sources_and_derives_urls() {
    let manifest = sample_manifest();
    let options = ManifestExportOptions {
        strip_prefix: Some("public".to_string()),
        url_prefix: "/".to_string(),
    };

    let exported = export_manifest(&manifest, &options);

    assert_eq!(exported.sources.len(), 1);
    assert_eq!(exported.sources[0].source_path, "assets/images/card.png");
    assert_eq!(exported.sources[0].variants.len(), 2);
    assert_eq!(exported.sources[0].variants[0].preset, "project-card");
    assert_eq!(exported.sources[0].variants[0].fit, "cover");
    assert_eq!(
        exported.sources[0].variants[0].src,
        "/images/generated/card.project-card.640.webp"
    );
}

#[test]
fn manifest_export_renderers_are_app_consumable() {
    let manifest = sample_manifest();
    let options = ManifestExportOptions {
        strip_prefix: Some("public".to_string()),
        url_prefix: "/assets".to_string(),
    };

    let json = manifest_export_to_json(&manifest, &options);
    let typescript = manifest_export_to_typescript(&manifest, &options);

    assert!(json.contains("\"sources\""));
    assert!(json.contains("\"src\": \"/assets/images/generated/card.project-card.640.webp\""));
    assert!(typescript.starts_with("// Generated by devimg."));
    assert!(typescript.contains("export const DEVIMG_MANIFEST = {"));
    assert!(typescript.contains("src: \"/assets/images/generated/card.project-card.640.webp\""));
    assert!(typescript.ends_with(" as const\n"));
}

#[test]
fn write_manifest_wraps_parent_directory_creation_errors() {
    let project = TempProject::new("write-manifest-parent-error");
    let nested = project.root.join("nested/manifest.json");
    write_manifest(&nested, &sample_manifest()).expect("nested manifest writes");
    assert!(nested.exists());

    let file_parent = project.root.join("not-a-directory");
    fs::write(&file_parent, b"file").expect("file parent written");

    let error = write_manifest(&file_parent.join("manifest.json"), &sample_manifest())
        .expect_err("file parent should fail directory creation");

    assert!(error.to_string().contains("I/O error"));
}

#[test]
fn read_and_write_manifest_wrap_file_and_json_errors() {
    let project = TempProject::new("manifest-io-json-errors");
    let invalid_json = project.root.join("invalid.json");
    fs::write(&invalid_json, b"{not json").expect("invalid json written");

    let parse_error = read_manifest(&invalid_json).expect_err("invalid json should fail");
    assert!(parse_error.to_string().contains("invalid manifest JSON"));

    let directory_path = project.root.join("manifest-directory");
    fs::create_dir_all(&directory_path).expect("manifest directory created");
    let write_error =
        write_manifest(&directory_path, &sample_manifest()).expect_err("directory write fails");
    assert!(write_error.to_string().contains("I/O error"));
}

#[test]
fn manifest_export_url_helpers_cover_empty_and_exact_prefixes() {
    let manifest = sample_manifest();
    let no_prefix = export_manifest(
        &manifest,
        &ManifestExportOptions {
            strip_prefix: Some(String::new()),
            url_prefix: String::new(),
        },
    );
    assert_eq!(
        no_prefix.sources[0].variants[0].src,
        "public/images/generated/card.project-card.640.webp"
    );

    let exact_manifest = Manifest {
        outputs: vec![ManifestOutput {
            output_path: "public".to_string(),
            ..manifest.outputs[0].clone()
        }],
        ..manifest
    };
    let exact = export_manifest(
        &exact_manifest,
        &ManifestExportOptions {
            strip_prefix: Some("public".to_string()),
            url_prefix: "/assets".to_string(),
        },
    );
    assert_eq!(exact.sources[0].variants[0].src, "/assets");
}

#[test]
fn typescript_export_can_include_lookup_helpers() {
    let manifest = sample_manifest();
    let options = ManifestExportOptions {
        strip_prefix: Some("public".to_string()),
        url_prefix: "/".to_string(),
    };

    let default_typescript = manifest_export_to_typescript(&manifest, &options);
    let helper_typescript = manifest_export_to_typescript_with_options(
        &manifest,
        &options,
        &ManifestTypescriptOptions {
            include_helpers: true,
        },
    );

    assert!(!default_typescript.contains("findDevimgVariant"));
    assert!(helper_typescript.contains("export type DevimgVariantSelector = {"));
    assert!(helper_typescript.contains("export function findDevimgSource"));
    assert!(helper_typescript.contains("export function listDevimgVariants"));
    assert!(helper_typescript.contains("export function findDevimgVariant"));
    assert!(helper_typescript.contains("variant.width < selector.minWidth"));
}

#[test]
fn typescript_export_formats_multiple_sources_and_long_output_paths() {
    let mut manifest = sample_manifest();
    let mut second_source = manifest.outputs[0].clone();
    second_source.source_path = "assets/images/other.png".to_string();
    second_source.output_path = format!(
        "public/images/generated/{}",
        "very-long-output-file-name-that-forces-the-typescript-field-wrapping-branch.project-card.640.webp"
    );
    manifest.outputs.push(second_source);

    let typescript = manifest_export_to_typescript(
        &manifest,
        &ManifestExportOptions {
            strip_prefix: Some("public".to_string()),
            url_prefix: "/".to_string(),
        },
    );

    assert!(typescript.contains("    },\n    {"));
    assert!(typescript.contains("output_path:\n"));
}

fn sample_manifest() -> Manifest {
    Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:config".to_string(),
        outputs: vec![
            ManifestOutput {
                source_path: "assets/images/card.png".to_string(),
                source_hash: "blake3:source".to_string(),
                source_width: 800,
                source_height: 450,
                source_bytes: 123,
                output_path: "public/images/generated/card.project-card.640.webp".to_string(),
                preset: "project-card".to_string(),
                fit: "cover".to_string(),
                width: 640,
                height: 360,
                format: "webp".to_string(),
                bytes: 45,
                hash: "blake3:output-webp".to_string(),
                operation_hash: "blake3:operation-webp".to_string(),
            },
            ManifestOutput {
                source_path: "assets/images/card.png".to_string(),
                source_hash: "blake3:source".to_string(),
                source_width: 800,
                source_height: 450,
                source_bytes: 123,
                output_path: "public/images/generated/card.project-card.640.jpeg".to_string(),
                preset: "project-card".to_string(),
                fit: "cover".to_string(),
                width: 640,
                height: 360,
                format: "jpeg".to_string(),
                bytes: 67,
                hash: "blake3:output-jpeg".to_string(),
                operation_hash: "blake3:operation-jpeg".to_string(),
            },
        ],
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
            "devimg-manifest-tests-{name}-{}-{unique}",
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
