use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use crate::config::{
    parse_config, BudgetConfig, Config, CropPosition, FitMode, FormatKind, PresetConfig,
    ProjectConfig, SourceConfig, WarningConfig,
};
use crate::manifest::{
    manifest_export_to_json, manifest_export_to_typescript_with_options, write_manifest, Manifest,
    ManifestExportOptions, ManifestTypescriptOptions,
};
use crate::pipeline::{optimize, CheckIssue, OptimizeOptions, SourceImage};

use super::{
    acknowledged_warning_diagnostic, check_issue_diagnostic, display_path, doctor,
    doctor_report_to_json, inspect_manifest_export, inspect_source_dirs, next_command,
    unique_source_bytes, warning_diagnostic, DoctorDiagnostic, DoctorManifestExportFormat,
    DoctorManifestExportOptions, DoctorOptions,
};

#[test]
fn next_command_prioritizes_regeneration_export_doctor_and_check_paths() {
    let default_config = config("devimg.toml");
    let spaced_config = config("configs/my config.toml");

    assert_eq!(
        next_command(&spaced_config, &[diagnostic("stale")], &[]),
        "devimg optimize --config 'configs/my config.toml' --allow-overwrite"
    );
    assert_eq!(
        next_command(
            &spaced_config,
            &[diagnostic("manifest_export_missing")],
            &[]
        ),
        "devimg manifest export --manifest <manifest> --output <file>"
    );
    assert_eq!(
        next_command(&spaced_config, &[diagnostic("empty_sources")], &[]),
        "devimg doctor --config 'configs/my config.toml'"
    );
    assert_eq!(
        next_command(&spaced_config, &[], &[diagnostic("missing_report")]),
        "devimg check --config 'configs/my config.toml'"
    );
    assert_eq!(next_command(&default_config, &[], &[]), "devimg check");
}

#[test]
fn diagnostic_helpers_map_issue_and_warning_hints() {
    let modified = check_issue_diagnostic(
        &CheckIssue {
            kind: String::from("modified"),
            path: String::from("public/images/card.webp"),
            message: String::from("output changed"),
        },
        "configs/devimg.toml",
    );
    assert!(modified
        .hint
        .contains("devimg optimize --config configs/devimg.toml"));

    let oversized = check_issue_diagnostic(
        &CheckIssue {
            kind: String::from("oversized_total"),
            path: String::from("."),
            message: String::from("budget exceeded"),
        },
        "devimg.toml",
    );
    assert!(oversized.hint.contains("Reduce generated image bytes"));

    let unknown = check_issue_diagnostic(
        &CheckIssue {
            kind: String::from("custom"),
            path: String::from("."),
            message: String::from("custom issue"),
        },
        "devimg.toml",
    );
    assert!(unknown.hint.contains("Review the issue"));

    let quality = warning_diagnostic(
        "quality:cover-crop: ./assets/card.png preset card width 100 uses cover",
        "devimg.toml",
    );
    assert_eq!(quality.path, "assets/card.png");
    assert!(quality.hint.contains("Tune preset quality"));

    let fallback = warning_diagnostic(
        "plan:metadata-not-preserved: strip_metadata=false requested",
        "devimg.toml",
    );
    assert_eq!(fallback.path, "devimg.toml");
    assert!(fallback.hint.contains("Review the preset/source config"));

    let acknowledged = acknowledged_warning_diagnostic(
        "quality:cover-crop: ./assets/card.png preset card width 100 uses cover",
        "devimg.toml",
    );
    assert!(acknowledged.hint.contains("Acknowledged in devimg.toml"));
}

#[test]
fn inspect_manifest_export_reports_unavailable_missing_stale_and_current_exports() {
    let project = TempProject::new("manifest-export");
    let manifest_path = project.root.join("devimg-manifest.json");
    let output_path = project.root.join("devimg.generated.json");
    let mut checks = Vec::new();
    let mut issues = Vec::new();

    inspect_manifest_export(
        &manifest_path,
        export_options(output_path.clone()),
        &mut checks,
        &mut issues,
    )
    .expect("missing manifest is reported");
    assert_eq!(issues[0].code, "manifest_export_unavailable");
    assert_eq!(checks[0].status, "fail");

    let manifest = Manifest::new(
        String::from("devimg.toml"),
        String::from("hash"),
        Vec::new(),
    );
    write_manifest(&manifest_path, &manifest).expect("manifest written");

    checks.clear();
    issues.clear();
    inspect_manifest_export(
        &manifest_path,
        export_options(output_path.clone()),
        &mut checks,
        &mut issues,
    )
    .expect("missing export is reported");
    assert_eq!(issues[0].code, "manifest_export_missing");
    assert_eq!(checks[0].status, "fail");

    fs::write(&output_path, "stale export").expect("stale export written");
    checks.clear();
    issues.clear();
    inspect_manifest_export(
        &manifest_path,
        export_options(output_path.clone()),
        &mut checks,
        &mut issues,
    )
    .expect("stale export is reported");
    assert_eq!(issues[0].code, "manifest_export_stale");
    assert_eq!(checks[0].status, "fail");

    let rendered = manifest_export_to_json(
        &manifest,
        &ManifestExportOptions {
            strip_prefix: None,
            url_prefix: String::from("/"),
        },
    );
    fs::write(&output_path, rendered).expect("current export written");
    checks.clear();
    issues.clear();
    inspect_manifest_export(
        &manifest_path,
        export_options(output_path),
        &mut checks,
        &mut issues,
    )
    .expect("current export is accepted");
    assert!(issues.is_empty());
    assert_eq!(checks[0].status, "pass");

    let ts_output = project.root.join("devimg.generated.ts");
    let ts_rendered = manifest_export_to_typescript_with_options(
        &manifest,
        &ManifestExportOptions {
            strip_prefix: None,
            url_prefix: String::from("/"),
        },
        &ManifestTypescriptOptions {
            include_helpers: true,
        },
    );
    fs::write(&ts_output, ts_rendered).expect("typescript export written");
    checks.clear();
    issues.clear();
    inspect_manifest_export(
        &manifest_path,
        DoctorManifestExportOptions {
            output: ts_output,
            format: DoctorManifestExportFormat::Typescript,
            strip_prefix: None,
            url_prefix: String::from("/"),
            typescript_helpers: true,
        },
        &mut checks,
        &mut issues,
    )
    .expect("typescript export is accepted");
    assert!(issues.is_empty());
    assert_eq!(checks[0].status, "pass");

    let directory_output = project.root.join("export-dir");
    fs::create_dir_all(&directory_output).expect("directory output created");
    let error = inspect_manifest_export(
        &manifest_path,
        export_options(directory_output),
        &mut checks,
        &mut issues,
    )
    .expect_err("directory output should return an I/O error");
    assert!(error.to_string().contains("I/O error"));
}

#[test]
fn display_path_and_unique_source_bytes_handle_empty_and_duplicate_paths() {
    assert_eq!(display_path(Path::new("")), ".");
    assert_eq!(display_path(Path::new("project")), "project");

    let sources = vec![
        source_image("assets/card.png", 100),
        source_image("assets/card.png", 900),
        source_image("assets/other.png", 50),
    ];

    assert_eq!(unique_source_bytes(&sources), 150);
}

#[test]
fn inspect_source_dirs_reports_missing_invalid_and_nested_paths() {
    let missing_project = TempProject::new("missing-source");
    let mut missing_config = config("devimg.toml");
    missing_config.project.root = missing_project.root.clone();
    let mut checks = Vec::new();
    let mut warnings = Vec::new();
    let mut issues = Vec::new();
    inspect_source_dirs(&missing_config, &mut checks, &mut warnings, &mut issues)
        .expect("missing source is reported");
    assert_eq!(checks[0].status, "fail");
    assert_eq!(issues[0].code, "missing_source_dir");

    let invalid_project = TempProject::new("invalid-source");
    let input_file = invalid_project.root.join("assets/images");
    fs::create_dir_all(input_file.parent().expect("input parent")).expect("parent created");
    fs::write(&input_file, b"not a dir").expect("input file written");
    let mut invalid_config = config("devimg.toml");
    invalid_config.project.root = invalid_project.root.clone();
    checks.clear();
    warnings.clear();
    issues.clear();
    inspect_source_dirs(&invalid_config, &mut checks, &mut warnings, &mut issues)
        .expect("invalid source is reported");
    assert_eq!(issues[0].code, "invalid_source_dir");

    let nested_project = TempProject::new("nested-source-output");
    fs::create_dir_all(nested_project.root.join("assets/images/generated"))
        .expect("nested output created");
    let mut output_inside = config("devimg.toml");
    output_inside.project.root = nested_project.root.clone();
    output_inside.sources[0].output = PathBuf::from("assets/images/generated");
    checks.clear();
    warnings.clear();
    issues.clear();
    inspect_source_dirs(&output_inside, &mut checks, &mut warnings, &mut issues)
        .expect("output inside source is warned");
    assert_eq!(checks[0].status, "pass");
    assert_eq!(warnings[0].code, "output_inside_source");

    let reverse_project = TempProject::new("source-inside-output");
    fs::create_dir_all(reverse_project.root.join("public/generated/source"))
        .expect("source inside output created");
    let mut source_inside = config("devimg.toml");
    source_inside.project.root = reverse_project.root.clone();
    source_inside.sources[0].input = PathBuf::from("public/generated/source");
    source_inside.sources[0].output = PathBuf::from("public/generated");
    checks.clear();
    warnings.clear();
    issues.clear();
    inspect_source_dirs(&source_inside, &mut checks, &mut warnings, &mut issues)
        .expect("source inside output is warned");
    assert_eq!(warnings[0].code, "source_inside_output");
}

#[test]
fn doctor_reports_source_directory_errors_before_scanning() {
    let missing_project = TempProject::new("doctor-missing-source");
    let missing_config = parse_config(
        &missing_project.root.join("devimg.toml"),
        r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "card"
widths = [100]
formats = ["webp"]
"#,
    )
    .expect("config parses");

    let report = doctor(&missing_config, DoctorOptions::default()).expect("doctor reports");
    assert_eq!(report.status, "fail");
    assert_eq!(report.issues[0].code, "missing_source_dir");
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.code == "missing_report"));

    let invalid_project = TempProject::new("doctor-invalid-source");
    let input_file = invalid_project.root.join("assets/images");
    fs::create_dir_all(input_file.parent().expect("input parent")).expect("parent created");
    fs::write(&input_file, b"not a dir").expect("input file written");
    let invalid_config = parse_config(
        &invalid_project.root.join("devimg.toml"),
        r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "card"
widths = [100]
formats = ["webp"]
"#,
    )
    .expect("config parses");

    let invalid = doctor(&invalid_config, DoctorOptions::default()).expect("doctor reports");
    assert_eq!(invalid.issues[0].code, "invalid_source_dir");
}

#[test]
fn doctor_reports_framework_warnings_and_check_issues_when_outputs_are_missing() {
    let project = TempProject::new("doctor-framework-check");
    fs::write(project.root.join("next.config.mjs"), "export default {}\n")
        .expect("next config written");
    let input = project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    write_png(&input.join("card.png"));
    let config = parse_config(
        &project.root.join("devimg.toml"),
        r#"
[project]
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"
content_hash_filenames = true

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
    .expect("config parses");

    let report = doctor(
        &config,
        DoctorOptions {
            manifest_export: Some(export_options(project.root.join("lib/devimg.generated.ts"))),
        },
    )
    .expect("doctor reports missing outputs");

    assert_eq!(report.status, "fail");
    assert_eq!(report.frameworks, vec!["next"]);
    assert!(report
        .checks
        .iter()
        .any(|check| check.name == "framework_consumption" && check.status == "pass"));
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.code == "framework_next_image_double_optimization"));
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "missing_manifest"));
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.code == "manifest_export_unavailable"));
    assert!(report
        .checks
        .iter()
        .any(|check| check.name == "outputs" && check.status == "fail"));
}

#[test]
fn doctor_reports_scan_and_plan_failures_after_source_directories_pass() {
    let scan_project = TempProject::new("doctor-scan-failed");
    let input = scan_project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    write_png(&input.join("not-jpeg.jpg"));
    let scan_config = parse_config(
        &scan_project.root.join("devimg.toml"),
        r#"
[project]
manifest = "public/images/devimg-manifest.json"

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
    .expect("config parses");

    let scan_report = doctor(&scan_config, DoctorOptions::default()).expect("doctor reports");
    assert!(scan_report
        .issues
        .iter()
        .any(|issue| issue.code == "source_scan_failed"));

    let plan_project = TempProject::new("doctor-plan-failed");
    let input = plan_project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    write_png(&input.join("card.png"));
    let plan_config = parse_config(
        &plan_project.root.join("devimg.toml"),
        r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "card"
widths = [2]
formats = ["png"]

[[overrides]]
include = ["["]
"#,
    )
    .expect("config parses");

    let plan_report = doctor(&plan_config, DoctorOptions::default()).expect("doctor reports");
    assert!(plan_report
        .issues
        .iter()
        .any(|issue| issue.code == "plan_failed"));
}

#[test]
fn doctor_collects_active_check_warnings_and_check_runtime_errors() {
    let warning_project = TempProject::new("doctor-active-warning");
    let input = warning_project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    write_png_with_size(&input.join("banner.png"), 640, 360);
    let warning_config = parse_config(
        &warning_project.root.join("devimg.toml"),
        r#"
[project]
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "hero"
widths = [2]
formats = ["png"]
fit = "cover"
aspect_ratio = "1:1"
"#,
    )
    .expect("config parses");
    optimize(&warning_config, OptimizeOptions::default()).expect("optimize succeeds");

    let warning_report = doctor(&warning_config, DoctorOptions::default()).expect("doctor reports");
    assert!(warning_report
        .warnings
        .iter()
        .any(|warning| warning.code == "quality:cover-crop"));

    let error_project = TempProject::new("doctor-check-error");
    let input = error_project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    write_png(&input.join("card.png"));
    let error_config = parse_config(
        &error_project.root.join("devimg.toml"),
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
    .expect("config parses");
    optimize(&error_config, OptimizeOptions::default()).expect("optimize succeeds");
    let output_path = error_project
        .root
        .join("public/images/generated/card.card.2.png");
    fs::remove_file(&output_path).expect("output removed");
    fs::create_dir_all(&output_path).expect("directory replaces output");

    let error_report = doctor(&error_config, DoctorOptions::default()).expect("doctor reports");
    assert!(error_report
        .issues
        .iter()
        .any(|issue| issue.code == "check_failed"));
}

#[test]
fn doctor_passes_after_optimize_and_serializes_budget_summary() {
    let project = TempProject::new("doctor-pass");
    let input = project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    write_png(&input.join("card.png"));
    let config = parse_config(
        &project.root.join("devimg.toml"),
        r#"
[project]
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"

[budgets]
max_total_bytes = 1000000

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
    .expect("config parses");
    optimize(&config, OptimizeOptions::default()).expect("optimize succeeds");

    let report = doctor(&config, DoctorOptions::default()).expect("doctor passes");
    let json = doctor_report_to_json(&report);

    assert!(report.passed());
    assert_eq!(report.source_image_count, 1);
    assert_eq!(report.planned_variant_count, 1);
    assert_eq!(report.generated_variant_count, 1);
    assert_eq!(report.budget.status, "pass");
    assert!(report.budget.total_headroom_bytes.expect("headroom") > 0);
    assert!(json.ends_with('\n'));
    assert!(json.contains("\"status\": \"pass\""));
}

fn export_options(output: PathBuf) -> DoctorManifestExportOptions {
    DoctorManifestExportOptions {
        output,
        format: DoctorManifestExportFormat::Json,
        strip_prefix: None,
        url_prefix: String::from("/"),
        typescript_helpers: false,
    }
}

fn diagnostic(code: &str) -> DoctorDiagnostic {
    DoctorDiagnostic {
        code: code.to_string(),
        path: String::from("."),
        message: String::new(),
        hint: String::new(),
    }
}

fn source_image(project_path: &str, bytes: u64) -> SourceImage {
    SourceImage {
        source_name: String::from("portfolio"),
        path: PathBuf::from(project_path),
        project_path: project_path.to_string(),
        relative_path: PathBuf::from(project_path),
        output_root: PathBuf::from("public/images/generated"),
        output_root_project_path: String::from("public/images/generated"),
        width: 100,
        height: 100,
        bytes,
        hash: format!("blake3:{project_path}"),
        format: FormatKind::Png,
    }
}

fn write_png(path: &Path) {
    write_png_with_size(path, 2, 2);
}

fn write_png_with_size(path: &Path, width: u32, height: u32) {
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

fn config(path: &str) -> Config {
    Config {
        path: PathBuf::from(path),
        config_hash: String::from("config-hash"),
        project: ProjectConfig {
            root: PathBuf::from("."),
            manifest: PathBuf::from("public/images/devimg-manifest.json"),
            report: PathBuf::from("devimg-report.md"),
            overwrite: false,
            strip_metadata: true,
            content_hash_filenames: false,
        },
        sources: vec![SourceConfig {
            name: String::from("portfolio"),
            input: PathBuf::from("assets/images"),
            output: PathBuf::from("public/images/generated"),
            include: Vec::new(),
            exclude: Vec::new(),
        }],
        presets: vec![PresetConfig {
            name: String::from("card"),
            widths: vec![100],
            formats: vec![FormatKind::Webp],
            quality: 80,
            fit: FitMode::Cover,
            crop: CropPosition::CENTER,
            aspect_ratio: None,
            allow_upscale: false,
        }],
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
            "devimg-doctor-tests-{name}-{}-{unique}",
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
