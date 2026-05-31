use std::path::PathBuf;

use super::{budget_status, evaluate_budgets};
use crate::config::{BudgetConfig, Config, ProjectConfig, WarningConfig};
use crate::manifest::ManifestOutput;
use crate::pipeline::CheckIssue;

#[test]
fn evaluates_file_and_total_budget_failures() {
    let mut config = config();
    config.budgets.max_file_bytes = Some(100);
    config.budgets.max_total_bytes = Some(150);

    let (status, issues) = evaluate_budgets(
        &config,
        &[
            manifest_output("public/a.webp", 120),
            manifest_output("public/b.webp", 50),
        ],
    );

    assert_eq!(status, "fail");
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].kind, "oversized_file");
    assert_eq!(issues[0].path, "public/a.webp");
    assert_eq!(issues[0].message, "120 bytes exceeds max_file_bytes 100");
    assert_eq!(issues[1].kind, "oversized_total");
    assert_eq!(issues[1].path, "public/images/devimg-manifest.json");
    assert_eq!(issues[1].message, "170 bytes exceeds max_total_bytes 150");
}

#[test]
fn budget_status_only_fails_for_oversized_issues() {
    assert_eq!(budget_status(&[]), "pass");
    assert_eq!(
        budget_status(&[CheckIssue {
            kind: "missing".to_string(),
            path: "public/a.webp".to_string(),
            message: "missing".to_string(),
        }]),
        "pass"
    );
    assert_eq!(
        budget_status(&[CheckIssue {
            kind: "oversized_file".to_string(),
            path: "public/a.webp".to_string(),
            message: "too large".to_string(),
        }]),
        "fail"
    );
}

fn config() -> Config {
    Config {
        path: PathBuf::from("devimg.toml"),
        config_hash: "config-hash".to_string(),
        project: ProjectConfig {
            root: PathBuf::from("/project"),
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

fn manifest_output(path: &str, bytes: u64) -> ManifestOutput {
    ManifestOutput {
        source_path: "assets/card.png".to_string(),
        source_hash: "blake3:source".to_string(),
        source_width: 800,
        source_height: 480,
        source_bytes: 80,
        output_path: path.to_string(),
        preset: "card".to_string(),
        fit: "cover".to_string(),
        width: 800,
        height: 480,
        format: "webp".to_string(),
        bytes,
        hash: "blake3:output".to_string(),
        operation_hash: "operation-hash".to_string(),
    }
}
