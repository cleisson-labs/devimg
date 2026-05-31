use crate::compare::{
    ManifestCompare, ManifestCompareChange, ManifestCompareMetadataChange, ManifestCompareOutput,
    ManifestCompareSummary,
};
use crate::doctor::{DoctorBudget, DoctorCheck, DoctorDiagnostic, DoctorReport};
use crate::manifest::{Manifest, ManifestOutput};
use crate::pipeline::{CheckIssue, OptimizeResult};

use super::{
    format_signed, render_doctor_report, render_manifest_compare_report, render_manifest_report,
    render_run_report, shell_arg,
};

#[test]
fn shell_arg_leaves_safe_values_and_quotes_unsafe_values() {
    assert_eq!(shell_arg("devimg.toml"), "devimg.toml");
    assert_eq!(
        shell_arg("configs/my config.toml"),
        "'configs/my config.toml'"
    );
    assert_eq!(
        shell_arg("configs/owner's.toml"),
        "'configs/owner'\\''s.toml'"
    );
    assert_eq!(shell_arg(""), "''");
}

#[test]
fn format_signed_marks_positive_values_only() {
    assert_eq!(format_signed(12), "+12");
    assert_eq!(format_signed(0), "0");
    assert_eq!(format_signed(-7), "-7");
}

#[test]
fn run_report_includes_optional_counts_suggestions_and_issues() {
    let result = OptimizeResult {
        mode: String::from("optimize"),
        source_count: 1,
        planned_count: 3,
        generated_count: 1,
        skipped_count: 2,
        stale_count: 1,
        source_bytes: 100,
        output_bytes: 200,
        warnings: vec![String::from("plan:no-variants: no variants planned")],
        acknowledged_warnings: vec![String::from(
            "quality:cover-crop: assets/card.png preset card width 100 uses cover",
        )],
        issues: vec![CheckIssue {
            kind: String::from("missing"),
            path: String::from("public/images/card.webp"),
            message: String::from("output missing"),
        }],
        budget_status: String::from("pass"),
        manifest: Manifest::new(
            String::from("configs/my config.toml"),
            String::from("config-hash"),
            Vec::new(),
        ),
    };

    let rendered = render_run_report(&result);

    assert!(rendered.contains("- Variants skipped: `2`"));
    assert!(rendered.contains("- Variants stale: `1`"));
    assert!(rendered.contains("- Manifest variants: `0`"));
    assert!(rendered.contains("## Suggestions"));
    assert!(rendered.contains("--config 'configs/my config.toml'"));
    assert!(rendered.contains("- `missing`: output missing"));
}

#[test]
fn doctor_report_distinguishes_pass_with_warnings_from_action_required() {
    let clean_report = doctor_report(Vec::new(), Vec::new());
    let rendered_clean = render_doctor_report(&clean_report);
    assert!(rendered_clean.contains("Status: pass"));
    assert!(rendered_clean.contains("- Frameworks: `none`"));
    assert!(rendered_clean.contains("- Manifest helpers: `none`"));

    let mut framework_report = doctor_report(Vec::new(), Vec::new());
    framework_report.frameworks = vec![String::from("next"), String::from("vite")];
    framework_report.manifest_helpers = vec![String::from("lib/devimg.generated.ts")];
    let rendered_framework = render_doctor_report(&framework_report);
    assert!(rendered_framework.contains("- Frameworks: `next, vite`"));
    assert!(rendered_framework.contains("- Manifest helpers: `lib/devimg.generated.ts`"));

    let warning_report = doctor_report(
        vec![DoctorDiagnostic {
            code: String::from("missing_report"),
            path: String::from("devimg-report.md"),
            message: String::from("Markdown report is missing"),
            hint: String::from("Refresh it."),
        }],
        Vec::new(),
    );
    let rendered_warning = render_doctor_report(&warning_report);

    assert!(rendered_warning.contains("Status: pass with warnings"));
    assert!(rendered_warning.contains("Suggestions"));

    let issue_report = doctor_report(
        Vec::new(),
        vec![DoctorDiagnostic {
            code: String::from("missing_manifest"),
            path: String::from("public/images/devimg-manifest.json"),
            message: String::from("Manifest missing"),
            hint: String::from("Run optimize."),
        }],
    );
    let rendered_issue = render_doctor_report(&issue_report);

    assert!(rendered_issue.contains("Status: action required"));
    assert!(rendered_issue.contains("Issues"));

    let mut acknowledged_report = doctor_report(Vec::new(), Vec::new());
    acknowledged_report.acknowledged_warnings = vec![DoctorDiagnostic {
        code: String::from("quality:cover-crop"),
        path: String::from("assets/card.png"),
        message: String::from("Acknowledged crop warning"),
        hint: String::from("Review later."),
    }];
    let rendered_acknowledged = render_doctor_report(&acknowledged_report);
    assert!(rendered_acknowledged.contains("Acknowledged Warnings"));
    assert!(rendered_acknowledged.contains("Acknowledged crop warning"));
}

#[test]
fn manifest_compare_report_renders_empty_sections_and_signed_deltas() {
    let compare = ManifestCompare {
        base_generated_at: String::from("base-time"),
        head_generated_at: String::from("head-time"),
        base_config_hash: String::from("base-hash"),
        head_config_hash: String::from("head-hash"),
        summary: ManifestCompareSummary {
            base_variant_count: 2,
            head_variant_count: 5,
            variant_count_delta: 3,
            base_output_bytes: 20,
            head_output_bytes: 10,
            output_bytes_delta: -10,
            added_count: 0,
            removed_count: 0,
            changed_count: 0,
            metadata_changed_count: 0,
            unchanged_count: 0,
        },
        added: Vec::new(),
        removed: Vec::new(),
        changed: Vec::new(),
        metadata_changed: Vec::new(),
        top_byte_contributors: Vec::new(),
    };

    let rendered = render_manifest_compare_report(&compare);

    assert!(rendered.contains("- Variants: `2` -> `5` (`+3`)"));
    assert!(rendered.contains("- Output bytes: `20` -> `10` (`-10`)"));
    assert!(rendered.contains("No added outputs."));
    assert!(rendered.contains("No removed outputs."));
    assert!(rendered.contains("No changed outputs."));
    assert!(rendered.contains("No metadata-only output changes."));
    assert!(rendered.contains("No head outputs."));
}

#[test]
fn manifest_report_lists_outputs_and_totals() {
    let manifest = Manifest {
        version: 1,
        generated_at: "unix:123".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:config".to_string(),
        outputs: vec![ManifestOutput {
            source_path: "assets/card.png".to_string(),
            source_hash: "blake3:source".to_string(),
            source_width: 100,
            source_height: 50,
            source_bytes: 75,
            output_path: "public/card.webp".to_string(),
            preset: "card".to_string(),
            fit: "cover".to_string(),
            width: 80,
            height: 40,
            format: "webp".to_string(),
            bytes: 25,
            hash: "blake3:output".to_string(),
            operation_hash: "operation-hash".to_string(),
        }],
    };

    let rendered = render_manifest_report(&manifest);

    assert!(rendered.contains("- Manifest version: `1`"));
    assert!(rendered.contains("- Source bytes: `75`"));
    assert!(rendered.contains("- Output bytes: `25`"));
    assert!(rendered.contains("- `assets/card.png` -> `public/card.webp` (80x40 webp, 25 bytes)"));
}

#[test]
fn manifest_compare_report_renders_changed_metadata_and_top_output_sections() {
    let compare = ManifestCompare {
        base_generated_at: "base-time".to_string(),
        head_generated_at: "head-time".to_string(),
        base_config_hash: "base-hash".to_string(),
        head_config_hash: "head-hash".to_string(),
        summary: ManifestCompareSummary {
            base_variant_count: 2,
            head_variant_count: 3,
            variant_count_delta: 1,
            base_output_bytes: 100,
            head_output_bytes: 170,
            output_bytes_delta: 70,
            added_count: 1,
            removed_count: 1,
            changed_count: 2,
            metadata_changed_count: 1,
            unchanged_count: 0,
        },
        added: vec![compare_output("assets/added.png", "public/added.webp", 70)],
        removed: vec![compare_output(
            "assets/removed.png",
            "public/removed.webp",
            20,
        )],
        changed: vec![
            ManifestCompareChange {
                source_path: "assets/card.png".to_string(),
                preset: "card".to_string(),
                width: 80,
                height: 40,
                format: "webp".to_string(),
                base_fit: "cover".to_string(),
                head_fit: "cover".to_string(),
                base_output_path: "public/card.webp".to_string(),
                head_output_path: "public/card.webp".to_string(),
                base_bytes: 25,
                head_bytes: 30,
                byte_delta: 5,
                base_hash: "base".to_string(),
                head_hash: "head".to_string(),
            },
            ManifestCompareChange {
                source_path: "assets/hero.png".to_string(),
                preset: "hero".to_string(),
                width: 1200,
                height: 630,
                format: "jpeg".to_string(),
                base_fit: "cover".to_string(),
                head_fit: "contain".to_string(),
                base_output_path: "public/hero-old.jpeg".to_string(),
                head_output_path: "public/hero-new.jpeg".to_string(),
                base_bytes: 100,
                head_bytes: 80,
                byte_delta: -20,
                base_hash: "base".to_string(),
                head_hash: "head".to_string(),
            },
        ],
        metadata_changed: vec![ManifestCompareMetadataChange {
            source_path: "assets/meta.png".to_string(),
            preset: "card".to_string(),
            width: 80,
            height: 40,
            format: "webp".to_string(),
            output_path: "public/meta.webp".to_string(),
            bytes: 33,
            hash: "same-hash".to_string(),
            base_operation_hash: "old-operation".to_string(),
            head_operation_hash: "new-operation".to_string(),
        }],
        top_byte_contributors: vec![compare_output("assets/top.png", "public/top.webp", 90)],
    };

    let rendered = render_manifest_compare_report(&compare);

    assert!(rendered.contains("- `public/added.webp` (source: `assets/added.png`"));
    assert!(rendered.contains("- `public/removed.webp` (source: `assets/removed.png`"));
    assert!(rendered.contains("fit `cover`, output `public/card.webp`"));
    assert!(rendered.contains(
        "fit `cover` -> `contain`, output `public/hero-old.jpeg` -> `public/hero-new.jpeg`"
    ));
    assert!(rendered
        .contains("- `assets/meta.png` preset `card` 80x40 webp: `public/meta.webp` (33 bytes)"));
    assert!(rendered.contains("- `public/top.webp` (source: `assets/top.png`"));
}

fn doctor_report(warnings: Vec<DoctorDiagnostic>, issues: Vec<DoctorDiagnostic>) -> DoctorReport {
    DoctorReport {
        status: if issues.is_empty() {
            String::from("pass")
        } else {
            String::from("fail")
        },
        config_path: String::from("devimg.toml"),
        project_root: String::from("."),
        manifest_path: String::from("public/images/devimg-manifest.json"),
        report_path: String::from("devimg-report.md"),
        frameworks: Vec::new(),
        manifest_helpers: Vec::new(),
        source_image_count: 0,
        planned_variant_count: 0,
        generated_variant_count: 0,
        source_bytes: 0,
        output_bytes: 0,
        budget: DoctorBudget {
            status: String::from("not evaluated"),
            max_total_bytes: None,
            max_file_bytes: None,
            total_output_bytes: 0,
            total_headroom_bytes: None,
        },
        checks: vec![DoctorCheck {
            name: String::from("config"),
            status: String::from("pass"),
            message: String::from("loaded devimg.toml"),
        }],
        warnings,
        acknowledged_warnings: Vec::new(),
        issues,
        next_command: String::from("devimg check"),
    }
}

fn compare_output(source_path: &str, output_path: &str, bytes: u64) -> ManifestCompareOutput {
    ManifestCompareOutput {
        source_path: source_path.to_string(),
        output_path: output_path.to_string(),
        preset: "card".to_string(),
        fit: "cover".to_string(),
        width: 80,
        height: 40,
        format: "webp".to_string(),
        bytes,
        hash: format!("blake3:{bytes}"),
    }
}
