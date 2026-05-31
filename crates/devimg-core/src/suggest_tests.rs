use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde_json::json;

use super::*;
use crate::config::{
    load_config, BudgetConfig, Config, CropPosition, FitMode, FormatKind, ProjectConfig,
    WarningConfig,
};
use crate::doctor::DoctorDiagnostic;
use crate::pipeline::{optimize, Operation, OptimizeOptions, SourceImage};

#[test]
fn suggest_requires_metadata_only_and_summarizes_doctor_diagnostics() {
    let config = config("configs/dev img.toml");

    let error = suggest(
        &config,
        SuggestOptions {
            metadata_only: false,
        },
    )
    .expect_err("non metadata-only suggest should fail");
    assert!(error
        .to_string()
        .contains("devimg suggest currently requires --metadata-only"));

    let report = suggest(&config, SuggestOptions::default()).expect("suggestion report builds");

    assert_eq!(report.version, 1);
    assert_eq!(report.mode, "metadata-only");
    assert_eq!(report.summary.suggestion_count, report.items.len());
    assert_eq!(report.summary.error_count, 1);
    assert_eq!(report.summary.warning_count, 1);
    assert!(report
        .items
        .iter()
        .any(|item| item.warning_code == "empty_sources"
            && item.severity == "error"
            && item.next_command == "devimg doctor --config 'configs/dev img.toml'"));
    assert!(report
        .items
        .iter()
        .any(|item| item.warning_code == "missing_report"
            && item.severity == "warning"
            && item.next_command == "devimg check --config 'configs/dev img.toml'"));
}

#[test]
fn suggest_includes_acknowledged_warnings_from_doctor() {
    let project = TempProject::new("suggest-acknowledged");
    let input = project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input directory created");
    write_png(&input.join("accesstrace.png"), 640, 360);
    fs::write(
        project.root.join("devimg.toml"),
        r#"[project]
root = "."
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
include = ["**/*.png"]

[[preset]]
name = "project-card"
widths = [120]
formats = ["webp"]
quality = 90
fit = "cover"
aspect_ratio = "1:1"
crop = "top"

[[warnings.acknowledge]]
code = "quality:cover-crop"
source = "assets/images/accesstrace.png"
preset = "project-card"
width = 120
reason = "Intentional top crop after review."
"#,
    )
    .expect("config written");
    let config = load_config(project.root.join("devimg.toml")).expect("config loads");
    optimize(&config, OptimizeOptions::default()).expect("outputs generated");

    let report = suggest(&config, SuggestOptions::default()).expect("suggestion report builds");
    let acknowledged = report
        .items
        .iter()
        .find(|item| item.acknowledged && item.warning_code == QUALITY_COVER_CROP)
        .expect("acknowledged crop suggestion exists");

    assert_eq!(acknowledged.severity, "advisory");
    assert_eq!(
        acknowledged.source_path.as_deref(),
        Some("assets/images/accesstrace.png")
    );
    assert_eq!(acknowledged.preset.as_deref(), Some("project-card"));
    assert_eq!(acknowledged.width, Some(120));
    assert!(report.summary.advisory_count >= 1);
}

#[test]
fn parses_suggested_quality_from_lossy_warning() {
    assert_eq!(
        suggested_min_quality_from_message(
            "asset.png preset hero uses webp quality 74 below the suggested minimum 82 for screenshot assets"
        ),
        Some(82)
    );
}

#[test]
fn renders_empty_and_detailed_suggestion_markdown() {
    let empty_report = SuggestionReport {
        version: 1,
        config_path: "devimg.toml".to_string(),
        mode: "metadata-only".to_string(),
        summary: SuggestionSummary {
            suggestion_count: 0,
            error_count: 0,
            warning_count: 0,
            advisory_count: 0,
        },
        items: Vec::new(),
    };

    let empty_markdown = render_suggestion_markdown(&empty_report);
    assert!(empty_markdown.contains("No suggestions."));
    assert!(suggestion_report_to_json(&empty_report).ends_with('\n'));

    let mut changes = BTreeMap::new();
    changes.insert("quality".to_string(), json!(82));
    let report = SuggestionReport {
        summary: SuggestionSummary {
            suggestion_count: 1,
            error_count: 0,
            warning_count: 0,
            advisory_count: 1,
        },
        items: vec![SuggestionItem {
            id: "advisory:quality".to_string(),
            source_path: Some("assets/card.png".to_string()),
            source_kind: Some("source".to_string()),
            output_path: Some("public/card.webp".to_string()),
            affected_path: "public/card.webp".to_string(),
            preset: Some("card".to_string()),
            width: Some(800),
            format: Some("webp".to_string()),
            warning_code: QUALITY_LOW_LOSSY.to_string(),
            severity: "advisory".to_string(),
            acknowledged: false,
            rationale: "quality:low-lossy-quality: assets/card.png preset card uses webp quality 72 below the suggested minimum 82".to_string(),
            suggested_config: Some(SuggestedConfigPatch {
                target: "preset".to_string(),
                source_path: Some("assets/card.png".to_string()),
                preset: Some("card".to_string()),
                changes,
                toml_hint: "Raise quality.".to_string(),
            }),
            commands: vec![
                "devimg optimize --allow-overwrite".to_string(),
                "devimg doctor".to_string(),
            ],
            next_command: "devimg optimize --allow-overwrite".to_string(),
        }],
        ..empty_report
    };

    let markdown = render_suggestion_markdown(&report);
    assert!(markdown.contains("Suggested config: target `preset`"));
    assert!(markdown.contains("Commands:"));
    assert!(markdown.contains("Next command: `devimg optimize --allow-overwrite`"));

    let manual_report = SuggestionReport {
        summary: SuggestionSummary {
            suggestion_count: 1,
            error_count: 1,
            warning_count: 0,
            advisory_count: 0,
        },
        items: vec![SuggestionItem {
            id: "error:manual".to_string(),
            source_path: None,
            source_kind: Some("project".to_string()),
            output_path: None,
            affected_path: "devimg.toml".to_string(),
            preset: None,
            width: None,
            format: None,
            warning_code: "custom".to_string(),
            severity: "error".to_string(),
            acknowledged: false,
            rationale: "custom issue".to_string(),
            suggested_config: None,
            commands: Vec::new(),
            next_command: String::new(),
        }],
        ..report
    };
    assert!(render_suggestion_markdown(&manual_report)
        .contains("manual review or regeneration required"));
}

#[test]
fn context_enriches_fields_from_outputs_and_operations() {
    let mut context = SuggestionContext::default();
    context.outputs_by_output.insert(
        "public/card.webp".to_string(),
        metadata("assets/card.png", "public/card.webp", "card", 800, "webp"),
    );

    let mut from_output = SuggestionFields {
        output_path: Some("public/card.webp".to_string()),
        ..SuggestionFields::default()
    };
    context.enrich(&mut from_output);
    assert_eq!(from_output.source_path.as_deref(), Some("assets/card.png"));
    assert_eq!(from_output.preset.as_deref(), Some("card"));
    assert_eq!(from_output.width, Some(800));
    assert_eq!(from_output.format.as_deref(), Some("webp"));

    context.operations_by_output.insert(
        "public/from-operation.webp".to_string(),
        metadata(
            "assets/from-operation.png",
            "public/from-operation.webp",
            "operation-card",
            640,
            "webp",
        ),
    );
    let mut from_operation_output = SuggestionFields {
        output_path: Some("public/from-operation.webp".to_string()),
        ..SuggestionFields::default()
    };
    context.enrich(&mut from_operation_output);
    assert_eq!(
        from_operation_output.source_path.as_deref(),
        Some("assets/from-operation.png")
    );
    assert_eq!(
        from_operation_output.preset.as_deref(),
        Some("operation-card")
    );

    let mut direct_apply = SuggestionFields::default();
    direct_apply.apply(&metadata(
        "assets/direct.png",
        "public/direct.webp",
        "direct",
        320,
        "webp",
    ));
    assert_eq!(
        direct_apply.output_path.as_deref(),
        Some("public/direct.webp")
    );

    context.operations_by_source_preset_width.insert(
        ("assets/hero.png".to_string(), "hero".to_string(), 1200),
        vec![metadata(
            "assets/hero.png",
            "public/hero.webp",
            "hero",
            1200,
            "webp",
        )],
    );
    let mut single_operation = SuggestionFields {
        source_path: Some("assets/hero.png".to_string()),
        preset: Some("hero".to_string()),
        width: Some(1200),
        ..SuggestionFields::default()
    };
    context.enrich(&mut single_operation);
    assert_eq!(
        single_operation.output_path.as_deref(),
        Some("public/hero.webp")
    );
    assert_eq!(single_operation.format.as_deref(), Some("webp"));

    context.operations_by_source_preset_width.insert(
        ("assets/gallery.png".to_string(), "thumb".to_string(), 400),
        vec![
            metadata(
                "assets/gallery.png",
                "public/gallery-1.webp",
                "thumb",
                400,
                "webp",
            ),
            metadata(
                "assets/gallery.png",
                "public/gallery-2.webp",
                "thumb",
                400,
                "webp",
            ),
        ],
    );
    let mut same_format = SuggestionFields {
        source_path: Some("assets/gallery.png".to_string()),
        preset: Some("thumb".to_string()),
        width: Some(400),
        ..SuggestionFields::default()
    };
    context.enrich(&mut same_format);
    assert_eq!(same_format.output_path, None);
    assert_eq!(same_format.format.as_deref(), Some("webp"));

    context.operations_by_source_preset_width.insert(
        ("assets/logo.png".to_string(), "logo".to_string(), 200),
        vec![
            metadata("assets/logo.png", "public/logo.webp", "logo", 200, "webp"),
            metadata("assets/logo.png", "public/logo.avif", "logo", 200, "avif"),
        ],
    );
    let mut mixed_formats = SuggestionFields {
        source_path: Some("assets/logo.png".to_string()),
        preset: Some("logo".to_string()),
        width: Some(200),
        ..SuggestionFields::default()
    };
    context.enrich(&mut mixed_formats);
    assert_eq!(mixed_formats.output_path, None);
    assert_eq!(mixed_formats.format, None);

    let mut missing_source = SuggestionFields::default();
    context.enrich(&mut missing_source);
    assert_eq!(missing_source.source_path, None);

    let mut missing_preset = SuggestionFields {
        source_path: Some("assets/card.png".to_string()),
        ..SuggestionFields::default()
    };
    context.enrich(&mut missing_preset);
    assert_eq!(missing_preset.preset, None);

    let mut missing_width = SuggestionFields {
        source_path: Some("assets/card.png".to_string()),
        preset: Some("card".to_string()),
        ..SuggestionFields::default()
    };
    context.enrich(&mut missing_width);
    assert_eq!(missing_width.width, None);
}

#[test]
fn context_loads_sources_plan_operations_and_manifest_outputs() {
    let project = TempProject::new("suggest-context");
    let input = project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input directory created");
    write_png(&input.join("card.png"), 320, 180);
    fs::write(
        project.root.join("devimg.toml"),
        r#"[project]
root = "."
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
include = ["**/*.png"]

[[preset]]
name = "card"
widths = [160]
formats = ["png"]
"#,
    )
    .expect("config written");
    let config = load_config(project.root.join("devimg.toml")).expect("config loads");
    optimize(&config, OptimizeOptions::default()).expect("outputs generated");

    let context = SuggestionContext::new(&config);
    assert!(context.is_source_path("assets/images/card.png"));

    let output_path = "public/images/generated/card.card.160.png";
    let mut by_output = SuggestionFields {
        output_path: Some(output_path.to_string()),
        ..SuggestionFields::default()
    };
    context.enrich(&mut by_output);

    assert_eq!(
        by_output.source_path.as_deref(),
        Some("assets/images/card.png")
    );
    assert_eq!(by_output.preset.as_deref(), Some("card"));
    assert_eq!(by_output.width, Some(160));
    assert_eq!(by_output.format.as_deref(), Some("png"));

    let mut by_source_preset_width = SuggestionFields {
        source_path: Some("assets/images/card.png".to_string()),
        preset: Some("card".to_string()),
        width: Some(160),
        ..SuggestionFields::default()
    };
    context.enrich(&mut by_source_preset_width);
    assert_eq!(
        by_source_preset_width.output_path.as_deref(),
        Some(output_path)
    );
}

#[test]
fn context_keeps_source_paths_when_plan_or_manifest_are_unavailable() {
    let project = TempProject::new("suggest-context-errors");
    let input = project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input directory created");
    write_png(&input.join("card.png"), 320, 180);
    fs::write(
        project.root.join("devimg.toml"),
        r#"[project]
root = "."
manifest = "public/images/missing-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
include = ["**/*.png"]

[[preset]]
name = "card"
widths = [160]
formats = ["png"]

[[overrides]]
include = ["["]
"#,
    )
    .expect("config written");
    let config = load_config(project.root.join("devimg.toml")).expect("config loads");

    let context = SuggestionContext::new(&config);

    assert!(context.is_source_path("assets/images/card.png"));
    assert!(context.operations_by_output.is_empty());
    assert!(context.outputs_by_output.is_empty());
}

#[test]
fn context_tolerates_scan_failures_and_manifest_path_rejections() {
    let missing_input = TempProject::new("suggest-context-scan-error");
    fs::write(
        missing_input.root.join("devimg.toml"),
        r#"[project]
root = "."
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/missing"
output = "public/images/generated"

[[preset]]
name = "card"
widths = [160]
formats = ["png"]
"#,
    )
    .expect("config written");
    let config = load_config(missing_input.root.join("devimg.toml")).expect("config loads");
    let context = SuggestionContext::new(&config);
    assert!(context.source_paths.is_empty());
    assert!(context.operations_by_output.is_empty());

    let escaping_manifest = TempProject::new("suggest-context-manifest-escape");
    fs::write(
        escaping_manifest.root.join("devimg.toml"),
        r#"[project]
root = "."
manifest = "../outside-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "card"
widths = [160]
formats = ["png"]
"#,
    )
    .expect("config written");
    fs::create_dir_all(escaping_manifest.root.join("assets/images")).expect("input created");
    write_png(
        &escaping_manifest.root.join("assets/images/card.png"),
        320,
        180,
    );
    let config = load_config(escaping_manifest.root.join("devimg.toml")).expect("config loads");
    let context = SuggestionContext::new(&config);
    assert!(context.is_source_path("assets/images/card.png"));
    assert!(context.outputs_by_output.is_empty());
}

#[test]
fn metadata_extractors_preserve_operation_and_manifest_fields() {
    let operation = operation(false);
    let from_operation = SuggestionMetadata::from_operation(&operation);
    assert_eq!(from_operation.source_path, "assets/card.png");
    assert_eq!(from_operation.output_path, "public/card.webp");
    assert_eq!(from_operation.preset, "card");
    assert_eq!(from_operation.width, 800);
    assert_eq!(from_operation.format, "webp");

    let output = manifest_output("public/card.webp", "webp", 321);
    let from_output = SuggestionMetadata::from_output(&output);
    assert_eq!(from_output.source_path, "assets/card.png");
    assert_eq!(from_output.output_path, "public/card.webp");
    assert_eq!(from_output.preset, "card");
    assert_eq!(from_output.width, 800);
    assert_eq!(from_output.format, "webp");
}

#[test]
fn fields_from_diagnostic_prefers_matching_warning_metadata_then_context() {
    let mut context = SuggestionContext::default();
    context
        .source_paths
        .insert("assets/source-only.png".to_string());
    context.operations_by_output.insert(
        "public/from-operation.webp".to_string(),
        metadata(
            "assets/from-operation.png",
            "public/from-operation.webp",
            "card",
            800,
            "webp",
        ),
    );
    context.outputs_by_output.insert(
        "public/from-manifest.webp".to_string(),
        metadata(
            "assets/from-manifest.png",
            "public/from-manifest.webp",
            "card",
            800,
            "webp",
        ),
    );

    let parsed = fields_from_diagnostic(
        &context,
        &diagnostic(
            QUALITY_COVER_CROP,
            "ignored",
            "quality:cover-crop: ./assets/card.png preset card width 800 uses cover and crops about 44% of the resized image",
        ),
    );
    assert_eq!(parsed.source_path.as_deref(), Some("assets/card.png"));
    assert_eq!(parsed.preset.as_deref(), Some("card"));
    assert_eq!(parsed.width, Some(800));

    let mismatched = fields_from_diagnostic(
        &context,
        &diagnostic(
            "missing",
            "public/missing.webp",
            "quality:cover-crop: assets/card.png preset card width 800 uses cover",
        ),
    );
    assert_eq!(mismatched.source_path, None);
    assert_eq!(
        mismatched.output_path.as_deref(),
        Some("public/missing.webp")
    );

    let source_path = fields_from_diagnostic(
        &context,
        &diagnostic("unknown", "assets/source-only.png", "unknown: check source"),
    );
    assert_eq!(
        source_path.source_path.as_deref(),
        Some("assets/source-only.png")
    );

    let operation_output = fields_from_diagnostic(
        &context,
        &diagnostic(
            "unknown",
            "public/from-operation.webp",
            "unknown: check output",
        ),
    );
    assert_eq!(
        operation_output.output_path.as_deref(),
        Some("public/from-operation.webp")
    );

    let manifest_output = fields_from_diagnostic(
        &context,
        &diagnostic(
            "unknown",
            "public/from-manifest.webp",
            "unknown: check output",
        ),
    );
    assert_eq!(
        manifest_output.output_path.as_deref(),
        Some("public/from-manifest.webp")
    );
}

#[test]
fn classifies_warning_severity_and_source_kind() {
    assert_eq!(severity_for_warning("missing_report"), "warning");
    assert_eq!(severity_for_warning(BUDGET_FAILED), "warning");
    assert_eq!(severity_for_warning("framework_nextjs"), "warning");
    assert_eq!(severity_for_warning(QUALITY_LOW_LOSSY), "advisory");

    let output_fields = SuggestionFields {
        output_path: Some("public/card.webp".to_string()),
        ..SuggestionFields::default()
    };
    assert_eq!(
        source_kind("anything", &output_fields, "public/card.webp").as_deref(),
        Some("generated")
    );

    let source_fields = SuggestionFields {
        source_path: Some("assets/card.png".to_string()),
        ..SuggestionFields::default()
    };
    assert_eq!(
        source_kind("anything", &source_fields, "assets/card.png").as_deref(),
        Some("source")
    );

    assert_eq!(
        source_kind(
            "missing_manifest",
            &SuggestionFields::default(),
            "devimg.toml"
        )
        .as_deref(),
        Some("manifest")
    );
    assert_eq!(
        source_kind(
            "framework_nextjs",
            &SuggestionFields::default(),
            "next.config.mjs"
        )
        .as_deref(),
        Some("framework")
    );
    assert_eq!(
        source_kind(
            "oversized_file",
            &SuggestionFields::default(),
            "public/card.webp"
        )
        .as_deref(),
        Some("budget")
    );
    assert_eq!(
        source_kind(
            "missing_source_dir",
            &SuggestionFields::default(),
            "devimg.toml"
        )
        .as_deref(),
        Some("source")
    );
    assert_eq!(
        source_kind("unknown", &SuggestionFields::default(), "assets/card.png").as_deref(),
        Some("source")
    );
    assert_eq!(
        source_kind(
            "missing_report",
            &SuggestionFields::default(),
            "devimg-report.md"
        )
        .as_deref(),
        Some("report")
    );
    assert_eq!(
        source_kind("unknown", &SuggestionFields::default(), "devimg.toml").as_deref(),
        Some("project")
    );
}

#[test]
fn suggested_config_patches_cover_supported_codes() {
    let fields = SuggestionFields {
        source_path: Some("assets/card.png".to_string()),
        preset: Some("card".to_string()),
        ..SuggestionFields::default()
    };

    let low_lossy = suggested_config_patch(
        QUALITY_LOW_LOSSY,
        &fields,
        "assets/card.png preset card uses webp quality 70 below the suggested minimum 82",
    )
    .expect("low lossy patch");
    assert_eq!(low_lossy.target, "preset");
    assert_eq!(low_lossy.changes.get("quality"), Some(&json!(82)));
    assert_eq!(low_lossy.source_path.as_deref(), Some("assets/card.png"));
    assert_eq!(low_lossy.preset.as_deref(), Some("card"));

    let low_lossy_fallback = suggested_config_patch(QUALITY_LOW_LOSSY, &fields, "quality is low")
        .expect("fallback patch");
    assert_eq!(
        low_lossy_fallback.changes.get("quality"),
        Some(&json!("raise"))
    );

    for (code, expected_target, expected_keys) in [
        (QUALITY_COVER_CROP, "preset", vec!["fit"]),
        (QUALITY_SKIPPED_UPSCALE, "preset", vec!["widths"]),
        (QUALITY_ALLOWED_UPSCALE, "preset", vec!["widths"]),
        (QUALITY_GENERATED_UPSCALE, "preset", vec!["widths"]),
        (
            QUALITY_OUTPUT_LARGER_THAN_SOURCE,
            "preset",
            vec!["formats", "quality"],
        ),
        (PLAN_NO_VARIANTS, "preset", vec!["widths"]),
        ("missing_source_dir", "source", vec!["include", "input"]),
        ("output_inside_source", "source", vec!["input", "output"]),
        (
            "oversized_file",
            "budget",
            vec!["max_file_bytes", "max_total_bytes"],
        ),
        (
            BUDGET_FAILED,
            "budget",
            vec!["max_file_bytes", "max_total_bytes"],
        ),
        ("framework_nextjs", "project", vec!["manifest_export"]),
    ] {
        let patch = suggested_config_patch(code, &fields, "message").expect(code);
        assert_eq!(patch.target, expected_target);
        let keys = patch
            .changes
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(keys, expected_keys.into_iter().collect::<BTreeSet<_>>());
    }

    assert!(suggested_config_patch("unknown", &fields, "message").is_none());
}

#[test]
fn commands_for_diagnostics_are_specific_and_deduped() {
    let config = config("devimg.toml");
    let output_fields = SuggestionFields {
        output_path: Some("public/card image.webp".to_string()),
        ..SuggestionFields::default()
    };

    assert_eq!(
        commands_for_diagnostic(
            &config,
            &diagnostic(QUALITY_COVER_CROP, "public/card.webp", "quality warning"),
            &output_fields,
        ),
        vec![
            "devimg review --manifest public/images/devimg-manifest.json --output .devimg/review.html",
            "devimg optimize --allow-overwrite",
            "devimg doctor",
        ]
    );
    assert_eq!(
        commands_for_diagnostic(
            &config,
            &diagnostic(QUALITY_LOW_LOSSY, "assets/card.png", "quality warning"),
            &SuggestionFields::default(),
        ),
        vec!["devimg optimize --allow-overwrite", "devimg doctor"]
    );
    assert_eq!(
        commands_for_diagnostic(
            &config,
            &diagnostic("stale", "public/card.webp", "stale output"),
            &SuggestionFields::default(),
        ),
        vec!["devimg optimize --allow-overwrite", "devimg doctor"]
    );
    assert_eq!(
        commands_for_diagnostic(
            &config,
            &diagnostic("invalid_output", "public/card.webp", "invalid output"),
            &output_fields,
        ),
        vec![
            "devimg inspect 'public/card image.webp'",
            "devimg optimize --allow-overwrite",
            "devimg doctor",
        ]
    );
    assert_eq!(
        commands_for_diagnostic(
            &config,
            &diagnostic("missing_report", "devimg-report.md", "missing report"),
            &SuggestionFields::default(),
        ),
        vec!["devimg check", "devimg doctor"]
    );
    assert_eq!(
        commands_for_diagnostic(
            &config,
            &diagnostic(
                "manifest_export_missing",
                "lib/devimg.generated.ts",
                "missing export"
            ),
            &SuggestionFields::default(),
        ),
        vec![
            "devimg manifest export --manifest <manifest> --output <file>",
            "devimg doctor",
        ]
    );
    assert_eq!(
        commands_for_diagnostic(
            &config,
            &diagnostic("unknown", "devimg.toml", "unknown"),
            &SuggestionFields::default(),
        ),
        vec!["devimg doctor"]
    );

    assert_eq!(
        dedup_commands(vec![
            "devimg doctor".to_string(),
            "devimg doctor".to_string(),
            "devimg check".to_string(),
        ]),
        vec!["devimg doctor", "devimg check"]
    );
}

#[test]
fn command_helpers_quote_paths_and_handle_non_default_configs() {
    assert_eq!(
        optimize_command("devimg.toml"),
        "devimg optimize --allow-overwrite"
    );
    assert_eq!(doctor_command("devimg.toml"), "devimg doctor");
    assert_eq!(check_command("devimg.toml"), "devimg check");
    assert_eq!(
        optimize_command("configs/dev img.toml"),
        "devimg optimize --config 'configs/dev img.toml' --allow-overwrite"
    );
    assert_eq!(shell_arg("public/card.webp"), "public/card.webp");
    assert_eq!(shell_arg(""), "''");
    assert_eq!(shell_arg("assets/it's.png"), "'assets/it'\\''s.png'");

    let mut escaping_config = config("devimg.toml");
    escaping_config.project.manifest = PathBuf::from("../manifest.json");
    assert_eq!(
        review_command(&escaping_config),
        "devimg review --manifest ../manifest.json --output .devimg/review.html"
    );
}

#[test]
fn ordering_ids_and_message_helpers_are_stable() {
    let diagnostic = diagnostic("modified", "public/card.webp", "modified output");
    let fields = SuggestionFields {
        source_path: Some("assets/card.png".to_string()),
        output_path: Some("public/card.webp".to_string()),
        preset: Some("card".to_string()),
        width: Some(800),
        format: Some("webp".to_string()),
    };
    assert_eq!(
        suggestion_id(&diagnostic, &fields, "warning", true),
        "warning:modified:assets/card.png:public/card.webp:card:800:webp:true"
    );
    assert!(output_like_code("stale"));
    assert!(!output_like_code("missing_manifest"));
    assert_eq!(
        lossy_format_from_message("assets/card.png preset card uses jpeg quality 80"),
        Some("jpeg".to_string())
    );
    assert_eq!(lossy_format_from_message("no lossy quality here"), None);
    assert_eq!(suggested_min_quality_from_message("no minimum"), None);

    let mut items = [
        item("advisory", "z_code", None, None, None, None, None, "c"),
        item("warning", "a_code", None, None, None, None, None, "b"),
        item(
            "error",
            "a_code",
            Some("assets/card.png"),
            Some("public/card.webp"),
            Some("card"),
            Some(800),
            Some("webp"),
            "a",
        ),
    ];
    items.sort_by(compare_suggestions);
    assert_eq!(items[0].id, "a");
    assert_eq!(items[1].id, "b");
    assert_eq!(items[2].id, "c");

    let mut width_tied_items = [
        item(
            "warning",
            "same",
            Some("assets/card.png"),
            Some("public/card.webp"),
            Some("card"),
            Some(900),
            Some("webp"),
            "large",
        ),
        item(
            "warning",
            "same",
            Some("assets/card.png"),
            Some("public/card.webp"),
            Some("card"),
            Some(300),
            Some("webp"),
            "small",
        ),
    ];
    width_tied_items.sort_by(compare_suggestions);
    assert_eq!(width_tied_items[0].id, "small");

    let mut format_tied_items = [
        item(
            "warning",
            "same",
            Some("assets/card.png"),
            Some("public/card.webp"),
            Some("card"),
            Some(800),
            Some("webp"),
            "webp",
        ),
        item(
            "warning",
            "same",
            Some("assets/card.png"),
            Some("public/card.webp"),
            Some("card"),
            Some(800),
            Some("avif"),
            "avif",
        ),
    ];
    format_tied_items.sort_by(compare_suggestions);
    assert_eq!(format_tied_items[0].id, "avif");

    let mut id_tied_items = [
        item(
            "warning",
            "same",
            Some("assets/card.png"),
            Some("public/card.webp"),
            Some("card"),
            Some(800),
            Some("webp"),
            "z",
        ),
        item(
            "warning",
            "same",
            Some("assets/card.png"),
            Some("public/card.webp"),
            Some("card"),
            Some(800),
            Some("webp"),
            "a",
        ),
    ];
    id_tied_items.sort_by(compare_suggestions);
    assert_eq!(id_tied_items[0].id, "a");
}

#[test]
fn suggestion_from_diagnostic_builds_complete_item() {
    let config = config("configs/dev img.toml");
    let mut context = SuggestionContext::default();
    context.operations_by_source_preset_width.insert(
        ("assets/card.png".to_string(), "card".to_string(), 800),
        vec![metadata(
            "assets/card.png",
            "public/card.webp",
            "card",
            800,
            "webp",
        )],
    );

    let item = suggestion_from_diagnostic(
        &config,
        &context,
        &diagnostic(
            QUALITY_LOW_LOSSY,
            "assets/card.png",
            "quality:low-lossy-quality: assets/card.png preset card width 800 uses webp quality 70 below the suggested minimum 82",
        ),
        "advisory",
        true,
    );

    assert_eq!(item.source_path.as_deref(), Some("assets/card.png"));
    assert_eq!(item.output_path.as_deref(), Some("public/card.webp"));
    assert_eq!(item.affected_path, "public/card.webp");
    assert_eq!(item.format.as_deref(), Some("webp"));
    assert_eq!(item.severity, "advisory");
    assert!(item.acknowledged);
    assert_eq!(
        item.next_command,
        "devimg optimize --config 'configs/dev img.toml' --allow-overwrite"
    );
    assert_eq!(
        item.commands,
        vec![
            "devimg optimize --config 'configs/dev img.toml' --allow-overwrite",
            "devimg doctor --config 'configs/dev img.toml'",
        ]
    );
}

fn config(path: &str) -> Config {
    Config {
        path: PathBuf::from(path),
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

fn diagnostic(code: &str, path: &str, message: &str) -> DoctorDiagnostic {
    DoctorDiagnostic {
        code: code.to_string(),
        path: path.to_string(),
        message: message.to_string(),
        hint: "hint".to_string(),
    }
}

fn metadata(
    source_path: &str,
    output_path: &str,
    preset: &str,
    width: u32,
    format: &str,
) -> SuggestionMetadata {
    SuggestionMetadata {
        source_path: source_path.to_string(),
        output_path: output_path.to_string(),
        preset: preset.to_string(),
        width,
        format: format.to_string(),
    }
}

fn manifest_output(output_path: &str, format: &str, bytes: u64) -> ManifestOutput {
    ManifestOutput {
        source_path: "assets/card.png".to_string(),
        source_hash: "blake3:source".to_string(),
        source_width: 1000,
        source_height: 600,
        source_bytes: 123,
        output_path: output_path.to_string(),
        preset: "card".to_string(),
        fit: "cover".to_string(),
        width: 800,
        height: 480,
        format: format.to_string(),
        bytes,
        hash: "blake3:output".to_string(),
        operation_hash: "operation-hash".to_string(),
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

#[allow(clippy::too_many_arguments)]
fn item(
    severity: &str,
    code: &str,
    source_path: Option<&str>,
    output_path: Option<&str>,
    preset: Option<&str>,
    width: Option<u32>,
    format: Option<&str>,
    id: &str,
) -> SuggestionItem {
    SuggestionItem {
        id: id.to_string(),
        source_path: source_path.map(str::to_string),
        source_kind: None,
        output_path: output_path.map(str::to_string),
        affected_path: output_path
            .or(source_path)
            .unwrap_or("devimg.toml")
            .to_string(),
        preset: preset.map(str::to_string),
        width,
        format: format.map(str::to_string),
        warning_code: code.to_string(),
        severity: severity.to_string(),
        acknowledged: false,
        rationale: "rationale".to_string(),
        suggested_config: None,
        commands: Vec::new(),
        next_command: String::new(),
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
            "devimg-suggest-tests-{name}-{}-{unique}",
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

fn write_png(path: &Path, width: u32, height: u32) {
    let mut image = RgbaImage::new(width, height);
    for pixel in image.pixels_mut() {
        *pixel = Rgba([10, 20, 30, 255]);
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("png encoded");
    fs::write(path, bytes.into_inner()).expect("png bytes written");
}
