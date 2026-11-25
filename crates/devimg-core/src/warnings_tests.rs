use std::path::PathBuf;

use super::{
    normalize_warning_path, split_acknowledged_warnings, split_code_and_message, warning_info,
    warning_message, PLAN_NO_VARIANTS, QUALITY_COVER_CROP, QUALITY_OUTPUT_LARGER_THAN_SOURCE,
    QUALITY_SKIPPED_UPSCALE,
};
use crate::config::{
    BudgetConfig, Config, ProjectConfig, WarningAcknowledgementConfig, WarningConfig,
};

#[test]
fn parses_cover_crop_warning_metadata() {
    let info = warning_info(
        "quality:cover-crop: assets/images/accesstrace.png preset project-card width 1200 uses cover and crops about 44% of the resized image",
    );

    assert_eq!(info.code, QUALITY_COVER_CROP);
    assert_eq!(
        info.source.as_deref(),
        Some("assets/images/accesstrace.png")
    );
    assert_eq!(info.preset.as_deref(), Some("project-card"));
    assert_eq!(info.width, Some(1200));

    let no_preset = warning_info("quality:cover-crop: asset cropped without marker");
    assert_eq!(no_preset.source, None);

    let no_width = warning_info("quality:cover-crop: assets/card.png preset card uses cover");
    assert_eq!(no_width.source.as_deref(), Some("assets/card.png"));
    assert_eq!(no_width.preset.as_deref(), Some("card"));
    assert_eq!(no_width.width, None);
}

#[test]
fn parses_manifest_output_warning_metadata() {
    let info = warning_info(
        "quality:output-larger-than-source: public/images/card.webp is larger than its source file (20 KB vs 10 KB)",
    );

    assert_eq!(info.code, QUALITY_OUTPUT_LARGER_THAN_SOURCE);
    assert_eq!(info.output.as_deref(), Some("public/images/card.webp"));
}

#[test]
fn incomplete_warning_messages_keep_unavailable_metadata_empty() {
    let output = warning_info("quality:output-larger-than-source: output unavailable");
    assert_eq!(output.code, QUALITY_OUTPUT_LARGER_THAN_SOURCE);
    assert_eq!(output.output, None);

    for message in [
        "plan:no-variants: source unavailable",
        "plan:no-variants: no variants planned for assets/card.png",
    ] {
        let info = warning_info(message);
        assert_eq!(info.code, PLAN_NO_VARIANTS);
        assert_eq!(info.source, None);
    }

    let preset = warning_info("quality:cover-crop: assets/card.png preset  width 800 uses cover");
    assert_eq!(preset.source.as_deref(), Some("assets/card.png"));
    assert_eq!(preset.preset, None);
    assert_eq!(preset.width, Some(800));
}

#[test]
fn parses_skipped_upscale_and_plan_warning_metadata() {
    let skipped = warning_info(
        "quality:skipped-upscale: skipped ./assets/card.png preset card width 1600 because source is smaller",
    );
    assert_eq!(skipped.code, QUALITY_SKIPPED_UPSCALE);
    assert_eq!(skipped.source.as_deref(), Some("assets/card.png"));
    assert_eq!(skipped.preset.as_deref(), Some("card"));
    assert_eq!(skipped.width, Some(1600));

    let plan = warning_info(
        "plan:no-variants: no variants planned for ./assets/small.png after applying widths",
    );
    assert_eq!(plan.code, PLAN_NO_VARIANTS);
    assert_eq!(plan.source.as_deref(), Some("assets/small.png"));
}

#[test]
fn splits_warning_codes_and_messages() {
    assert_eq!(
        split_code_and_message("plain warning"),
        ("warning", "plain warning")
    );
    assert_eq!(
        split_code_and_message("missing_report: report is missing"),
        ("missing_report", "report is missing")
    );
    assert_eq!(
        split_code_and_message("quality:cover-crop: asset cropped"),
        ("quality:cover-crop", "asset cropped")
    );
    assert_eq!(
        warning_message("budget:failed", "total too large"),
        "budget:failed: total too large"
    );
}

#[test]
fn normalizes_warning_paths() {
    assert_eq!(
        normalize_warning_path(" ./assets\\card.png "),
        "assets/card.png"
    );
    assert_eq!(normalize_warning_path(" ./ "), ".");
}

#[test]
fn split_acknowledged_warnings_respects_filters_and_acknowledgeable_codes() {
    let mut config = config();
    config
        .warnings
        .acknowledgements
        .push(WarningAcknowledgementConfig {
            code: QUALITY_COVER_CROP.to_string(),
            source: Some("assets/card.png".to_string()),
            output: None,
            preset: Some("card".to_string()),
            width: Some(800),
            reason: Some("visually reviewed".to_string()),
        });
    config
        .warnings
        .acknowledgements
        .push(WarningAcknowledgementConfig {
            code: "unknown".to_string(),
            source: None,
            output: None,
            preset: None,
            width: None,
            reason: None,
        });
    config
        .warnings
        .acknowledgements
        .push(WarningAcknowledgementConfig {
            code: QUALITY_OUTPUT_LARGER_THAN_SOURCE.to_string(),
            source: None,
            output: Some("public/images/card.webp".to_string()),
            preset: None,
            width: None,
            reason: Some("accepted file size".to_string()),
        });

    let groups = split_acknowledged_warnings(
        &config,
        vec![
            "quality:cover-crop: assets/card.png preset card width 800 uses cover".to_string(),
            "quality:cover-crop: assets/card.png preset hero width 800 uses cover".to_string(),
            "quality:output-larger-than-source: public/images/card.webp is larger than its source file (20 KB vs 10 KB)".to_string(),
            "unknown: acknowledged config cannot silence this".to_string(),
        ],
    );

    assert_eq!(groups.acknowledged.len(), 2);
    assert_eq!(groups.active.len(), 2);
    assert!(groups.acknowledged[0].contains("assets/card.png preset card"));
    assert!(groups.acknowledged[1].contains("public/images/card.webp"));
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
