use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{current_output, operation_hash_matches, output_metadata, CurrentManifestCache};
use crate::config::{
    BudgetConfig, Config, CropPosition, FitMode, FormatKind, ProjectConfig, WarningConfig,
};
use crate::hash::hash_bytes;
use crate::manifest::{Manifest, ManifestOutput};
use crate::pipeline::{Operation, SourceImage};
use crate::plan::legacy_operation_hash;

#[test]
fn operation_hash_matches_current_or_legacy_hash() {
    let operation = operation(false);

    let mut output = manifest_output("public/card.webp", "operation-hash");
    assert!(operation_hash_matches(&output, &operation, "config-hash"));

    output.operation_hash = legacy_operation_hash(&operation, "config-hash");
    assert!(operation_hash_matches(&output, &operation, "config-hash"));

    output.operation_hash = "other-hash".to_string();
    assert!(!operation_hash_matches(&output, &operation, "config-hash"));
}

#[test]
fn non_hashed_manifest_lookup_uses_output_path_and_operation_hash() {
    let operation = operation(false);
    let empty_cache = CurrentManifestCache::new(manifest(Vec::new()));
    assert!(empty_cache
        .manifest_output(&operation)
        .expect("lookup succeeds")
        .is_none());

    let cache = CurrentManifestCache::new(manifest(vec![manifest_output(
        "public/card.webp",
        "operation-hash",
    )]));
    assert_eq!(
        cache
            .manifest_output(&operation)
            .expect("lookup succeeds")
            .map(|output| output.output_path.as_str()),
        Some("public/card.webp")
    );

    let stale_cache = CurrentManifestCache::new(manifest(vec![manifest_output(
        "public/card.webp",
        "stale-hash",
    )]));
    assert!(stale_cache
        .manifest_output(&operation)
        .expect("lookup succeeds")
        .is_none());
}

#[test]
fn current_output_rejects_missing_changed_and_invalid_files() {
    let project = TempProject::new("current-output");
    let config = config(&project.root);
    let operation = operation(false);
    let missing = manifest_output("public/missing.webp", "operation-hash");
    assert!(current_output(&config, &operation, &missing)
        .expect("missing lookup succeeds")
        .is_none());
    let escaping = manifest_output("../escape.webp", "operation-hash");
    assert!(current_output(&config, &operation, &escaping).is_err());

    fs::create_dir_all(project.root.join("public")).expect("public directory created");
    let changed_path = project.root.join("public/changed.webp");
    fs::write(&changed_path, b"changed").expect("changed output written");
    let changed = manifest_output("public/changed.webp", "operation-hash");
    assert!(current_output(&config, &operation, &changed)
        .expect("changed hash lookup succeeds")
        .is_none());

    let invalid_path = project.root.join("public/invalid.webp");
    fs::write(&invalid_path, b"not an image").expect("invalid output written");
    let mut invalid = manifest_output("public/invalid.webp", "operation-hash");
    invalid.hash = hash_bytes(b"not an image");
    assert!(current_output(&config, &operation, &invalid)
        .expect("invalid image lookup succeeds")
        .is_none());
}

#[test]
fn output_metadata_wraps_io_errors() {
    let project = TempProject::new("metadata-error");
    let missing = project.root.join("missing.webp");

    let error = output_metadata(&missing).expect_err("missing metadata should fail");

    assert!(error.to_string().contains("I/O error"));
}

#[test]
fn hashed_manifest_lookup_requires_unique_hash_and_expected_path() {
    let operation = operation(true);

    let empty_cache = CurrentManifestCache::new(manifest(Vec::new()));
    assert!(empty_cache
        .hashed_manifest_output(&operation)
        .expect("lookup succeeds")
        .is_none());

    let duplicate_cache = CurrentManifestCache::new(manifest(vec![
        manifest_output("public/card.abcdef123456.webp", "operation-hash"),
        manifest_output("public/card.abcdef123456.webp", "operation-hash"),
    ]));
    assert!(duplicate_cache
        .hashed_manifest_output(&operation)
        .expect("lookup succeeds")
        .is_none());

    let wrong_path_cache = CurrentManifestCache::new(manifest(vec![manifest_output(
        "public/card.webp",
        "operation-hash",
    )]));
    assert!(wrong_path_cache
        .hashed_manifest_output(&operation)
        .expect("lookup succeeds")
        .is_none());

    let current_cache = CurrentManifestCache::new(manifest(vec![manifest_output(
        "public/card.abcdef123456.webp",
        "operation-hash",
    )]));
    assert_eq!(
        current_cache
            .hashed_manifest_output(&operation)
            .expect("lookup succeeds")
            .map(|output| output.output_path.as_str()),
        Some("public/card.abcdef123456.webp")
    );

    let legacy_cache = CurrentManifestCache::new(manifest(vec![manifest_output(
        "public/card.abcdef123456.webp",
        &legacy_operation_hash(&operation, "config-hash"),
    )]));
    assert_eq!(
        legacy_cache
            .hashed_manifest_output(&operation)
            .expect("lookup succeeds")
            .map(|output| output.output_path.as_str()),
        Some("public/card.abcdef123456.webp")
    );
}

fn manifest(outputs: Vec<ManifestOutput>) -> Manifest {
    Manifest {
        version: 1,
        generated_at: "now".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "config-hash".to_string(),
        outputs,
    }
}

fn manifest_output(path: &str, operation_hash: &str) -> ManifestOutput {
    ManifestOutput {
        source_path: "assets/card.png".to_string(),
        source_hash: "blake3:source".to_string(),
        source_width: 1000,
        source_height: 600,
        source_bytes: 123,
        output_path: path.to_string(),
        preset: "card".to_string(),
        fit: "cover".to_string(),
        width: 800,
        height: 480,
        format: "webp".to_string(),
        bytes: 90,
        hash: "blake3:abcdef1234567890".to_string(),
        operation_hash: operation_hash.to_string(),
    }
}

fn operation(content_hash_filenames: bool) -> Operation {
    Operation {
        source: SourceImage {
            source_name: "images".to_string(),
            path: PathBuf::from("/project/assets/card.png"),
            project_path: "assets/card.png".to_string(),
            relative_path: PathBuf::from("card.png"),
            output_root: PathBuf::from("/project/public"),
            output_root_project_path: "public".to_string(),
            width: 1000,
            height: 600,
            bytes: 123,
            hash: "blake3:source".to_string(),
            format: FormatKind::Png,
        },
        preset: "card".to_string(),
        fit: FitMode::Cover,
        crop: CropPosition { x: 0.5, y: 0.5 },
        quality: 82,
        format: FormatKind::Webp,
        width: 800,
        height: 480,
        strip_metadata: true,
        content_hash_filenames,
        output_path: PathBuf::from("/project/public/card.webp"),
        output_project_path: "public/card.webp".to_string(),
        operation_hash: "operation-hash".to_string(),
    }
}

fn config(root: &Path) -> Config {
    Config {
        path: root.join("devimg.toml"),
        config_hash: "config-hash".to_string(),
        project: ProjectConfig {
            root: root.to_path_buf(),
            manifest: PathBuf::from("public/images/devimg-manifest.json"),
            report: PathBuf::from("devimg-report.md"),
            overwrite: false,
            strip_metadata: true,
            content_hash_filenames: false,
        },
        sources: Vec::new(),
        presets: Vec::new(),
        overrides: Vec::new(),
        warnings: WarningConfig::default(),
        budgets: BudgetConfig::default(),
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
            "devimg-incremental-tests-{name}-{}-{unique}",
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
