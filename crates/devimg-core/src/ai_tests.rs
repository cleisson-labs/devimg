use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use super::{
    ai_alt_report_to_json, ai_consent_preview_to_json, ai_draft_report_to_json, ai_image_mime_type,
    ai_review_report_to_json, build_ai_alt_placeholder_report, build_ai_alt_report,
    build_ai_alt_request, build_ai_consent_preview, build_ai_draft_placeholder_report,
    build_ai_draft_report, build_ai_draft_request, build_ai_review_dry_run_report,
    build_ai_review_report, build_ai_review_request, display_path, render_ai_alt_markdown,
    render_ai_draft_markdown, render_ai_review_markdown, sanitize_draft_text, AiAltDraft,
    AiAltOptions, AiAltProviderClient, AiAltReport, AiAltReportSummary, AiConsentOptions,
    AiConsentPreview, AiDraftArtifactSummary, AiDraftManifestSummary, AiDraftOptions,
    AiDraftProviderClient, AiDraftProviderPayload, AiDraftRequest, AiDraftSection, AiDraftType,
    AiGeneratedOutput, AiProvider, AiProviderClient, AiReviewObservation, AiReviewOptions,
    AiReviewProviderClient, AiReviewProviderPayload, AiSelectedFile, MockAiAltProviderClient,
    MockAiDraftProviderClient, MockAiProviderClient, MockAiReviewProviderClient,
};
use crate::config::parse_config;
use crate::manifest::{write_manifest, Manifest, ManifestOutput};

#[test]
fn provider_labels_and_env_vars_are_stable() {
    assert_eq!(AiProvider::Openai.label(), "openai");
    assert_eq!(AiProvider::Openai.credential_env_var(), "OPENAI_API_KEY");
    assert_eq!(AiProvider::Anthropic.label(), "anthropic");
    assert_eq!(
        AiProvider::Anthropic.credential_env_var(),
        "ANTHROPIC_API_KEY"
    );
}

#[test]
fn mock_provider_client_returns_stable_non_network_response() {
    let preview = AiConsentPreview {
            provider: AiProvider::Openai,
            model: "test-model".to_string(),
            command: "devimg ai consent".to_string(),
            dry_run_command: "devimg ai consent --config devimg.toml --ai-provider openai --model test-model --metadata-only --dry-run".to_string(),
            config_path: "devimg.toml".to_string(),
            project_root: ".".to_string(),
            mode: "metadata-only".to_string(),
            dry_run: true,
            paths_included: true,
            image_bytes_included: false,
            output_path: None,
            manifest_path: "public/images/devimg-manifest.json".to_string(),
            report_path: "devimg-report.md".to_string(),
            manifest_readable: false,
            source_files: vec![AiSelectedFile {
                source_name: "portfolio".to_string(),
                path: "assets/images/sample.png".to_string(),
                role: "source-image".to_string(),
                width: 100,
                height: 50,
                format: "png".to_string(),
                bytes: 12,
                hash: "blake3:source".to_string(),
                image_bytes_included: false,
            }],
            generated_outputs: vec![AiGeneratedOutput {
                source_path: "assets/images/sample.png".to_string(),
                output_path: "public/images/generated/sample.webp".to_string(),
                preset: "card".to_string(),
                fit: "cover".to_string(),
                width: 64,
                height: 36,
                format: "webp".to_string(),
                bytes: 10,
                hash: "blake3:output".to_string(),
            }],
        };

    let response = MockAiProviderClient::new(AiProvider::Anthropic).consent_response(&preview);

    assert_eq!(response.provider, AiProvider::Anthropic);
    assert_eq!(response.model, "test-model");
    assert_eq!(response.request_kind, "consent-preview");
    assert!(!response.network_called);
    assert_eq!(response.selected_file_count, 1);
    assert_eq!(response.generated_output_count, 1);
}

#[test]
fn consent_preview_json_is_timestamp_free() {
    let preview = AiConsentPreview {
            provider: AiProvider::Anthropic,
            model: "stable-model".to_string(),
            command: "devimg ai consent".to_string(),
            dry_run_command: "devimg ai consent --config devimg.toml --ai-provider anthropic --model stable-model --metadata-only --dry-run --output /tmp/consent.json".to_string(),
            config_path: "devimg.toml".to_string(),
            project_root: ".".to_string(),
            mode: "metadata-only".to_string(),
            dry_run: true,
            paths_included: true,
            image_bytes_included: false,
            output_path: Some("/tmp/consent.json".to_string()),
            manifest_path: "public/images/devimg-manifest.json".to_string(),
            report_path: "devimg-report.md".to_string(),
            manifest_readable: false,
            source_files: Vec::new(),
            generated_outputs: Vec::new(),
        };

    let json = ai_consent_preview_to_json(&preview);

    assert!(json.contains("\"provider\": \"anthropic\""));
    assert!(json.contains("\"model\": \"stable-model\""));
    assert!(!json.contains("generated_at"));
    assert!(!json.contains("timestamp"));
}

#[test]
fn build_ai_consent_preview_scans_sources_and_deduplicates_manifest_outputs() {
    let project = TempProject::new("consent-preview");
    let input = project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    write_png(&input.join("sample.png"));
    fs::create_dir_all(project.root.join("public/images")).expect("manifest parent created");
    let mut manifest = sample_manifest();
    manifest.outputs.push(manifest.outputs[0].clone());
    write_manifest(
        &project.root.join("public/images/devimg-manifest.json"),
        &manifest,
    )
    .expect("manifest written");
    let config = parse_config(
        &project.root.join("devimg.toml"),
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
widths = [100]
formats = ["webp"]
"#,
    )
    .expect("config parses");

    let preview = build_ai_consent_preview(
        &config,
        &AiConsentOptions {
            provider: AiProvider::Openai,
            model: "vision model".to_string(),
            command: "devimg ai consent".to_string(),
            config_path: project.root.join("devimg.toml"),
            dry_run: true,
            include_images: true,
            output_path: Some(project.root.join("consent preview.json")),
        },
    )
    .expect("preview builds");

    assert_eq!(preview.mode, "include-images");
    assert!(preview.dry_run_command.contains("--include-images"));
    assert!(preview.dry_run_command.contains("'vision model'"));
    assert!(preview.dry_run_command.contains("'"));
    assert!(preview.manifest_readable);
    assert_eq!(preview.source_files.len(), 1);
    assert!(preview.source_files[0].image_bytes_included);
    assert_eq!(preview.generated_outputs.len(), 2);
    assert_eq!(preview.generated_outputs[0].format, "avif");
    assert_eq!(preview.manifest_path, "public/images/devimg-manifest.json");
    assert_eq!(preview.report_path, "devimg-report.md");

    let metadata_only = build_ai_consent_preview(
        &config,
        &AiConsentOptions {
            provider: AiProvider::Openai,
            model: "vision-model".to_string(),
            command: "devimg ai consent".to_string(),
            config_path: project.root.join("devimg.toml"),
            dry_run: true,
            include_images: false,
            output_path: None,
        },
    )
    .expect("metadata preview builds");
    assert_eq!(metadata_only.mode, "metadata-only");
    assert!(metadata_only.dry_run_command.contains("--metadata-only"));

    fs::remove_file(project.root.join("public/images/devimg-manifest.json"))
        .expect("manifest removed");
    let missing_manifest = build_ai_consent_preview(
        &config,
        &AiConsentOptions {
            provider: AiProvider::Openai,
            model: "vision-model".to_string(),
            command: "devimg ai consent".to_string(),
            config_path: project.root.join("devimg.toml"),
            dry_run: true,
            include_images: false,
            output_path: None,
        },
    )
    .expect("missing manifest preview builds");
    assert!(!missing_manifest.manifest_readable);
    assert!(missing_manifest.generated_outputs.is_empty());
}

#[test]
fn ai_review_request_selects_metadata_and_image_inputs() {
    let manifest = sample_manifest();
    let request = build_ai_review_request(
        &manifest,
        &AiReviewOptions {
            provider: AiProvider::Openai,
            model: "vision-model".to_string(),
            command: "devimg review --ai".to_string(),
            manifest_path: PathBuf::from("/repo/public/images/devimg-manifest.json"),
            project_root: PathBuf::from("/repo"),
            dry_run: true,
            include_images: true,
            image_detail: "low".to_string(),
            max_images: 1,
            ai_output_path: Some(PathBuf::from("/repo/devimg-ai-review.json")),
            markdown_path: Some(PathBuf::from("/tmp/devimg-ai-review.md")),
        },
    );

    assert_eq!(request.provider, AiProvider::Openai);
    assert_eq!(request.mode, "include-images");
    assert_eq!(request.manifest_path, "public/images/devimg-manifest.json");
    assert_eq!(
        request.ai_output_path.as_deref(),
        Some("devimg-ai-review.json")
    );
    assert_eq!(
        request.markdown_path.as_deref(),
        Some("/tmp/devimg-ai-review.md")
    );
    assert_eq!(request.manifest_summary.source_count, 1);
    assert_eq!(request.manifest_summary.output_count, 2);
    assert_eq!(request.outputs.len(), 2);
    assert_eq!(request.selected_images.len(), 1);
    assert_eq!(request.selected_images[0].mime_type, "image/webp");
    let webp_output = request
        .outputs
        .iter()
        .find(|output| output.format == "webp")
        .expect("webp output exists");
    let avif_output = request
        .outputs
        .iter()
        .find(|output| output.format == "avif")
        .expect("avif output exists");
    assert!(webp_output.image_bytes_included);
    assert!(!avif_output.image_bytes_included);
    assert_eq!(request.skipped_image_count, 1);
}

#[test]
fn ai_review_dry_run_json_and_markdown_are_timestamp_free() {
    let manifest = sample_manifest();
    let request = build_ai_review_request(
        &manifest,
        &AiReviewOptions {
            provider: AiProvider::Openai,
            model: "dry-run-model".to_string(),
            command: "devimg review --ai".to_string(),
            manifest_path: PathBuf::from("public/images/devimg-manifest.json"),
            project_root: PathBuf::from("."),
            dry_run: true,
            include_images: false,
            image_detail: "low".to_string(),
            max_images: 8,
            ai_output_path: None,
            markdown_path: None,
        },
    );
    let report = build_ai_review_dry_run_report(&request);
    let json = ai_review_report_to_json(&report);
    let markdown = render_ai_review_markdown(&report);

    assert!(json.contains("\"provider\": \"openai\""));
    assert!(json.contains("\"dry_run\": true"));
    assert!(json.contains("\"provider_called\": false"));
    assert!(!json.contains("generated_at"));
    assert!(!json.contains("timestamp"));
    assert!(markdown.contains("# DevImg AI Review"));
    assert!(markdown.contains("Dry run: `true`"));
}

#[test]
fn mock_ai_review_provider_returns_stable_payload_and_failure() {
    let manifest = sample_manifest();
    let request = build_ai_review_request(
        &manifest,
        &AiReviewOptions {
            provider: AiProvider::Anthropic,
            model: "mock-model".to_string(),
            command: "devimg review --ai".to_string(),
            manifest_path: PathBuf::from("public/images/devimg-manifest.json"),
            project_root: PathBuf::from("."),
            dry_run: false,
            include_images: false,
            image_detail: "low".to_string(),
            max_images: 8,
            ai_output_path: None,
            markdown_path: None,
        },
    );

    let payload = MockAiReviewProviderClient::new(AiProvider::Anthropic)
        .review(&request)
        .expect("mock provider succeeds");
    let report = build_ai_review_report(&request, payload, true);

    assert!(report.provider_called);
    assert_eq!(report.provider_summary, "mock anthropic AI review");
    assert_eq!(report.observations.len(), 1);
    assert_eq!(report.observations[0].severity, "advisory");

    let error = MockAiReviewProviderClient::failing(AiProvider::Openai)
        .review(&request)
        .expect_err("mock provider fails");
    assert!(error
        .to_string()
        .contains("mock AI review provider failure"));
}

#[test]
fn ai_review_markdown_renders_signals_and_normalized_observations() {
    let mut manifest = sample_manifest();
    manifest.outputs[0].bytes = 2_000;
    let request = build_ai_review_request(
        &manifest,
        &AiReviewOptions {
            provider: AiProvider::Openai,
            model: "review-model".to_string(),
            command: "devimg review --ai".to_string(),
            manifest_path: PathBuf::from("public/images/devimg-manifest.json"),
            project_root: PathBuf::from("."),
            dry_run: false,
            include_images: false,
            image_detail: "low".to_string(),
            max_images: 8,
            ai_output_path: None,
            markdown_path: None,
        },
    );
    let report = build_ai_review_report(
        &request,
        AiReviewProviderPayload {
            summary: "provider summary".to_string(),
            observations: vec![AiReviewObservation {
                category: "unknown category".to_string(),
                severity: "critical".to_string(),
                source_path: "assets/images/sample.png".to_string(),
                preset: "project-card".to_string(),
                output_path: "public/images/generated/sample.project-card.64.webp".to_string(),
                rationale: "review the crop".to_string(),
                suggested_next_command: "devimg review".to_string(),
            }],
        },
        true,
    );
    let markdown = render_ai_review_markdown(&report);

    assert_eq!(report.observations[0].severity, "advisory");
    assert_eq!(report.observations[0].category, "format-quality concern");
    assert!(markdown.contains("## Deterministic Signals"));
    assert!(markdown.contains("### format-quality concern"));
    assert!(markdown.contains("- Suggested next command: `devimg review`"));
}

#[test]
fn ai_alt_request_groups_sources_and_selects_representative_images() {
    let manifest = sample_manifest();
    let request = build_ai_alt_request(
        &manifest,
        &AiAltOptions {
            provider: Some(AiProvider::Openai),
            model: Some("alt-model".to_string()),
            command: "devimg alt".to_string(),
            config_path: PathBuf::from("/repo/devimg.toml"),
            manifest_path: PathBuf::from("/repo/public/images/devimg-manifest.json"),
            project_root: PathBuf::from("/repo"),
            dry_run: true,
            include_images: true,
            image_detail: "low".to_string(),
            max_images: 1,
            output_path: Some(PathBuf::from("/repo/devimg-alt.json")),
            markdown_path: Some(PathBuf::from("/tmp/devimg-alt.md")),
        },
    );

    assert_eq!(request.provider, Some(AiProvider::Openai));
    assert_eq!(request.mode, "include-images");
    assert_eq!(request.config_path, "devimg.toml");
    assert_eq!(request.manifest_path, "public/images/devimg-manifest.json");
    assert_eq!(request.output_path.as_deref(), Some("devimg-alt.json"));
    assert_eq!(request.markdown_path.as_deref(), Some("/tmp/devimg-alt.md"));
    assert_eq!(request.sources.len(), 1);
    assert_eq!(request.sources[0].variants.len(), 2);
    assert_eq!(
        request.sources[0].representative_image_path,
        "assets/images/sample.png"
    );
    assert!(request.sources[0].image_bytes_included);
    assert_eq!(request.selected_images.len(), 1);
    assert_eq!(request.selected_images[0].mime_type, "image/png");
    assert_eq!(request.skipped_image_count, 0);
}

#[test]
fn ai_alt_request_uses_supported_variant_when_source_format_is_not_supported() {
    let mut manifest = sample_manifest();
    for output in &mut manifest.outputs {
        output.source_path = "assets/vector.svg".to_string();
        output.source_hash = "blake3:vector".to_string();
    }
    let request = build_ai_alt_request(
        &manifest,
        &AiAltOptions {
            provider: None,
            model: None,
            command: "devimg alt".to_string(),
            config_path: PathBuf::from("devimg.toml"),
            manifest_path: PathBuf::from("public/images/devimg-manifest.json"),
            project_root: PathBuf::from("."),
            dry_run: true,
            include_images: true,
            image_detail: "high".to_string(),
            max_images: 2,
            output_path: None,
            markdown_path: None,
        },
    );

    assert_eq!(
        request.sources[0].representative_image_path,
        "public/images/generated/sample.project-card.64.webp"
    );
    assert_eq!(request.selected_images.len(), 1);
    assert_eq!(request.selected_images[0].mime_type, "image/webp");
}

#[test]
fn ai_alt_representative_image_falls_back_to_first_variant_or_source() {
    let builder = super::AiAltSourceBuilder {
        source_path: "assets/vector.svg".to_string(),
        source_hash: "blake3:source".to_string(),
        source_width: 100,
        source_height: 50,
        source_bytes: 10,
        variants: vec![super::AiAltVariant {
            output_path: "public/vector.avif".to_string(),
            preset: "card".to_string(),
            fit: "cover".to_string(),
            width: 80,
            height: 40,
            format: "avif".to_string(),
            bytes: 8,
            hash: "blake3:avif".to_string(),
        }],
    };
    let representative = builder.representative_image();
    assert_eq!(representative.path, "public/vector.avif");
    assert!(representative
        .as_image_input("assets/vector.svg", "low")
        .is_none());

    let empty_builder = super::AiAltSourceBuilder {
        variants: Vec::new(),
        ..builder
    };
    let source = empty_builder.representative_image();
    assert_eq!(source.path, "assets/vector.svg");
    assert_eq!(source.format, "svg");
}

#[test]
fn ai_alt_placeholder_json_and_markdown_are_timestamp_free() {
    let manifest = sample_manifest();
    let request = build_ai_alt_request(
        &manifest,
        &AiAltOptions {
            provider: Some(AiProvider::Openai),
            model: Some("dry-run-model".to_string()),
            command: "devimg alt".to_string(),
            config_path: PathBuf::from("devimg.toml"),
            manifest_path: PathBuf::from("public/images/devimg-manifest.json"),
            project_root: PathBuf::from("."),
            dry_run: true,
            include_images: false,
            image_detail: "low".to_string(),
            max_images: 8,
            output_path: None,
            markdown_path: None,
        },
    );
    let report = build_ai_alt_placeholder_report(&request);
    let json = ai_alt_report_to_json(&report);
    let markdown = render_ai_alt_markdown(&report);

    assert!(json.contains("\"provider\": \"openai\""));
    assert!(json.contains("\"provider_called\": false"));
    assert!(json.contains("\"candidate_alt_text\": \"\""));
    assert!(!json.contains("generated_at"));
    assert!(!json.contains("timestamp"));
    assert!(markdown.contains("# DevImg Alt-Text Drafts"));
    assert!(markdown.contains("Provider called: `false`"));
    assert!(markdown.contains("_No draft generated in metadata-only mode._"));
}

#[test]
fn mock_ai_alt_provider_returns_stable_payload_and_failure() {
    let manifest = sample_manifest();
    let request = build_ai_alt_request(
        &manifest,
        &AiAltOptions {
            provider: Some(AiProvider::Anthropic),
            model: Some("mock-model".to_string()),
            command: "devimg alt".to_string(),
            config_path: PathBuf::from("devimg.toml"),
            manifest_path: PathBuf::from("public/images/devimg-manifest.json"),
            project_root: PathBuf::from("."),
            dry_run: false,
            include_images: false,
            image_detail: "low".to_string(),
            max_images: 8,
            output_path: None,
            markdown_path: None,
        },
    );

    let payload = MockAiAltProviderClient::new(AiProvider::Anthropic)
        .alt(&request)
        .expect("mock provider succeeds");
    let report = build_ai_alt_report(&request, payload, true);

    assert!(report.provider_called);
    assert_eq!(report.provider_summary, "mock anthropic alt text");
    assert_eq!(report.drafts.len(), 1);
    assert_eq!(report.drafts[0].confidence, "medium");
    assert_eq!(report.drafts[0].warnings, vec!["needs-human-review"]);

    let error = MockAiAltProviderClient::failing(AiProvider::Openai)
        .alt(&request)
        .expect_err("mock provider fails");
    assert!(error.to_string().contains("mock AI alt provider failure"));
}

#[test]
fn ai_alt_provider_payload_is_normalized_and_missing_drafts_get_placeholders() {
    let mut manifest = sample_manifest();
    manifest.outputs.push(ManifestOutput {
        source_path: "assets/images/other.png".to_string(),
        source_hash: "blake3:other-source".to_string(),
        source_width: 50,
        source_height: 50,
        source_bytes: 500,
        output_path: "public/images/generated/other.webp".to_string(),
        preset: "project-card".to_string(),
        fit: "cover".to_string(),
        width: 40,
        height: 40,
        format: "webp".to_string(),
        bytes: 300,
        hash: "blake3:other-webp".to_string(),
        operation_hash: "blake3:other-operation".to_string(),
    });
    let request = build_ai_alt_request(
        &manifest,
        &AiAltOptions {
            provider: Some(AiProvider::Openai),
            model: Some("alt-model".to_string()),
            command: "devimg alt".to_string(),
            config_path: PathBuf::from("devimg.toml"),
            manifest_path: PathBuf::from("public/images/devimg-manifest.json"),
            project_root: PathBuf::from("."),
            dry_run: false,
            include_images: false,
            image_detail: "low".to_string(),
            max_images: 8,
            output_path: None,
            markdown_path: None,
        },
    );
    let report = build_ai_alt_report(
        &request,
        super::AiAltProviderPayload {
            summary: "provider alt".to_string(),
            drafts: vec![AiAltDraft {
                source_path: "assets/images/sample.png".to_string(),
                representative_image_path: "wrong.png".to_string(),
                candidate_alt_text: "Sample".to_string(),
                review_note: "Review.".to_string(),
                confidence: "high".to_string(),
                image_category: "not-a-category".to_string(),
                warnings: vec![
                    "unknown-warning".to_string(),
                    "screenshot".to_string(),
                    "screenshot".to_string(),
                ],
            }],
        },
        true,
    );

    assert_eq!(report.drafts.len(), 2);
    let sample = report
        .drafts
        .iter()
        .find(|draft| draft.source_path == "assets/images/sample.png")
        .expect("sample draft");
    assert_eq!(sample.confidence, "high");
    assert_eq!(sample.image_category, "unknown");
    assert_eq!(sample.warnings, vec!["needs-human-review", "screenshot"]);
    let other = report
        .drafts
        .iter()
        .find(|draft| draft.source_path == "assets/images/other.png")
        .expect("other placeholder");
    assert_eq!(
        other.review_note,
        "Provider did not return a draft for this source; write human-reviewed alt text manually."
    );
}

#[test]
fn ai_alt_normalizers_default_unknown_values_for_review() {
    assert_eq!(super::normalize_alt_confidence("certain"), "low");
    assert_eq!(super::normalize_alt_category("photogram"), "unknown");
    assert_eq!(
        super::normalize_alt_warnings(Vec::new()),
        vec!["needs-human-review"]
    );
}

#[test]
fn ai_alt_markdown_renders_empty_and_providerless_draft_states() {
    let empty_report = AiAltReport {
        schema_version: 1,
        provider: None,
        model: None,
        command: "devimg alt".to_string(),
        config_path: "devimg.toml".to_string(),
        manifest_path: "public/images/devimg-manifest.json".to_string(),
        project_root: ".".to_string(),
        mode: "metadata-only".to_string(),
        dry_run: true,
        paths_included: true,
        image_bytes_included: false,
        image_detail: "low".to_string(),
        max_images: 8,
        output_path: None,
        markdown_path: None,
        provider_called: false,
        summary: AiAltReportSummary {
            source_count: 0,
            output_count: 0,
            selected_image_count: 0,
            skipped_image_count: 0,
            draft_count: 0,
        },
        sources: Vec::new(),
        selected_images: Vec::new(),
        provider_summary: String::new(),
        drafts: Vec::new(),
    };
    let empty = render_ai_alt_markdown(&empty_report);
    assert!(empty.contains("- Provider: `none`"));
    assert!(empty.contains("- Model: `none`"));
    assert!(empty.contains("No alt-text drafts."));

    let filled = AiAltReport {
        summary: AiAltReportSummary {
            source_count: 1,
            output_count: 1,
            selected_image_count: 1,
            skipped_image_count: 0,
            draft_count: 1,
        },
        drafts: vec![AiAltDraft {
            source_path: "assets/card.png".to_string(),
            representative_image_path: "assets/card.png".to_string(),
            candidate_alt_text: "Dashboard screenshot".to_string(),
            review_note: "Review before use.".to_string(),
            confidence: "high".to_string(),
            image_category: "screenshot".to_string(),
            warnings: vec!["text-heavy".to_string()],
        }],
        ..empty_report
    };
    let markdown = render_ai_alt_markdown(&filled);
    assert!(markdown.contains("- Candidate alt text: Dashboard screenshot"));
    assert!(markdown.contains("- Warnings: `text-heavy`"));
}

#[test]
fn ai_draft_placeholder_markdown_is_timestamp_free_and_review_marked() {
    let request = sample_draft_request(AiDraftType::ProjectPageCopy, None, None, true);
    let report = build_ai_draft_placeholder_report(&request);
    let markdown = render_ai_draft_markdown(&report);
    let json = serde_json::to_string_pretty(&report).expect("draft report serializes");

    assert_eq!(report.draft_type, AiDraftType::ProjectPageCopy);
    assert_eq!(report.mode, "metadata-only");
    assert!(!report.provider_called);
    assert_eq!(report.summary.source_count, 1);
    assert_eq!(report.summary.output_count, 2);
    assert!(markdown.starts_with("# Draft; review before publishing"));
    assert!(markdown.contains("- Draft type: `project-page-copy`"));
    assert!(markdown.contains("- Provider called: `false`"));
    assert!(markdown.contains("## Draft"));
    assert!(!markdown.contains("generated_at"));
    assert!(!json.contains("timestamp"));
}

#[test]
fn draft_type_labels_and_titles_cover_all_variants() {
    assert_eq!(AiDraftType::ReleaseNotes.label(), "release-notes");
    assert_eq!(AiDraftType::ReadmeSnippet.label(), "readme-snippet");
    assert_eq!(AiDraftType::ProjectPageCopy.label(), "project-page-copy");
    assert_eq!(AiDraftType::BlogOutline.label(), "blog-outline");
    assert_eq!(
        AiDraftType::SocialPostOutline.label(),
        "social-post-outline"
    );
    assert_eq!(AiDraftType::ReadmeSnippet.title(), "README Snippet");
    assert_eq!(AiDraftType::ProjectPageCopy.title(), "Project Page Copy");
    assert_eq!(
        AiDraftType::SocialPostOutline.title(),
        "Social Post Outline"
    );
    assert_eq!(display_path(Path::new("")), ".");
}

#[test]
fn draft_request_summarizes_local_artifacts_and_markdown_context() {
    let project = TempProject::new("draft-request");
    fs::create_dir_all(project.root.join("public/images")).expect("manifest parent created");
    let mut manifest = sample_manifest();
    for index in 0..11 {
        let mut output = manifest.outputs[0].clone();
        output.output_path = format!("public/images/generated/extra-{index}.webp");
        output.hash = format!("blake3:extra-{index}");
        output.operation_hash = format!("blake3:operation-extra-{index}");
        manifest.outputs.push(output);
    }
    write_manifest(
        &project.root.join("public/images/devimg-manifest.json"),
        &manifest,
    )
    .expect("manifest written");
    fs::write(
        project.root.join("devimg-report.md"),
        "line one\nline two\n",
    )
    .expect("report written");
    fs::write(
        project.root.join("CHANGELOG.md"),
        "## Unreleased\n- Changed\n",
    )
    .expect("changelog written");
    fs::write(project.root.join("compare.json"), "{\"summary\":true}\n").expect("compare written");
    fs::create_dir_all(project.root.join(".devimg")).expect("review parent created");
    fs::write(
        project.root.join(".devimg/review.html"),
        "<html>review</html>\n",
    )
    .expect("review written");

    let config = parse_config(
        &project.root.join("devimg.toml"),
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
widths = [100]
formats = ["webp"]
"#,
    )
    .expect("config parses");

    let request = build_ai_draft_request(
        &config,
        &AiDraftOptions {
            draft_type: AiDraftType::SocialPostOutline,
            provider: Some(AiProvider::Openai),
            model: Some("draft-model".to_string()),
            command: "devimg draft --dry-run".to_string(),
            config_path: project.root.join("devimg.toml"),
            project_root: project.root.clone(),
            output_path: project.root.join("draft.md"),
            dry_run: true,
            compare_json_path: Some(project.root.join("compare.json")),
            ai_review_json_path: Some(project.root.join("missing-ai-review.json")),
            review_html_path: Some(project.root.join(".devimg/review.html")),
            changelog_path: Some(project.root.join("CHANGELOG.md")),
        },
    );
    let report = build_ai_draft_placeholder_report(&request);
    let markdown = render_ai_draft_markdown(&report);

    assert_eq!(request.manifest_summary.output_count, 13);
    assert_eq!(request.report_summary.line_count, 2);
    assert!(
        request
            .changelog_summary
            .as_ref()
            .expect("changelog")
            .readable
    );
    assert_eq!(request.optional_artifacts.len(), 3);
    assert!(!request.optional_artifacts[1].readable);
    assert_eq!(
        request.optional_artifacts[1].read_error.as_deref(),
        Some("file not found")
    );
    assert_eq!(
        report.provider_summary,
        "Dry run only; no provider call was made."
    );
    assert!(markdown.contains("### Social Post Outline Draft"));
    assert!(markdown.contains("Changelog"));
    assert!(markdown.contains("ai-review-json"));
    assert!(markdown.contains("Dry-run provider mode did not call any external AI service."));
    assert!(markdown.contains("additional outputs omitted from Markdown context"));

    let metadata_only = build_ai_draft_request(
        &config,
        &AiDraftOptions {
            draft_type: AiDraftType::ReleaseNotes,
            provider: None,
            model: None,
            command: "devimg draft".to_string(),
            config_path: project.root.join("devimg.toml"),
            project_root: project.root.clone(),
            output_path: project.root.join("draft.md"),
            dry_run: false,
            compare_json_path: None,
            ai_review_json_path: None,
            review_html_path: None,
            changelog_path: None,
        },
    );
    assert_eq!(metadata_only.mode, "metadata-only");
}

#[test]
fn draft_request_records_missing_manifest_report_and_unreadable_artifacts() {
    let project = TempProject::new("draft-missing-artifacts");
    fs::create_dir_all(project.root.join("artifact-dir")).expect("artifact dir created");
    let config = parse_config(
        &project.root.join("devimg.toml"),
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
widths = [100]
formats = ["webp"]
"#,
    )
    .expect("config parses");
    let request = build_ai_draft_request(
        &config,
        &AiDraftOptions {
            draft_type: AiDraftType::BlogOutline,
            provider: None,
            model: None,
            command: "devimg draft".to_string(),
            config_path: project.root.join("devimg.toml"),
            project_root: project.root.clone(),
            output_path: project.root.join("draft.md"),
            dry_run: false,
            compare_json_path: Some(project.root.join("artifact-dir")),
            ai_review_json_path: None,
            review_html_path: None,
            changelog_path: Some(project.root.join("CHANGELOG.md")),
        },
    );
    let report = build_ai_draft_placeholder_report(&request);
    let markdown = render_ai_draft_markdown(&report);
    let json = ai_draft_report_to_json(&report);

    assert!(!request.manifest_summary.readable);
    assert!(!request.report_summary.readable);
    assert!(
        !request
            .changelog_summary
            .as_ref()
            .expect("changelog")
            .readable
    );
    assert!(!request.optional_artifacts[0].readable);
    assert!(request.optional_artifacts[0].read_error.is_some());
    assert!(markdown.contains("- Manifest read note:"));
    assert!(markdown.contains("DevImg report"));
    assert!(markdown.contains("Changelog"));
    assert!(markdown.contains("not readable"));
    assert!(json.ends_with('\n'));
    assert!(json.contains("\"draft_type\": \"blog-outline\""));
}

#[test]
fn draft_request_falls_back_for_manifest_and_report_paths_outside_root() {
    let project = TempProject::new("draft-escaping-artifacts");
    let config = parse_config(
        &project.root.join("devimg.toml"),
        r#"
[project]
manifest = "../outside-manifest.json"
report = "../outside-report.md"

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

    let request = build_ai_draft_request(
        &config,
        &AiDraftOptions {
            draft_type: AiDraftType::ProjectPageCopy,
            provider: None,
            model: None,
            command: "devimg draft".to_string(),
            config_path: project.root.join("devimg.toml"),
            project_root: project.root.clone(),
            output_path: project.root.join("draft.md"),
            dry_run: false,
            compare_json_path: None,
            ai_review_json_path: None,
            review_html_path: None,
            changelog_path: None,
        },
    );

    assert!(request
        .manifest_summary
        .path
        .contains("outside-manifest.json"));
    assert!(request.report_summary.path.contains("outside-report.md"));
    assert!(!request.manifest_summary.readable);
    assert!(!request.report_summary.readable);
}

#[test]
fn local_ai_draft_payload_covers_release_notes_and_blog_outline() {
    let release = build_ai_draft_placeholder_report(&sample_draft_request(
        AiDraftType::ReleaseNotes,
        None,
        None,
        false,
    ));
    assert_eq!(release.sections[0].heading, "Release Notes Draft");

    let blog = build_ai_draft_placeholder_report(&sample_draft_request(
        AiDraftType::BlogOutline,
        None,
        None,
        false,
    ));
    assert_eq!(blog.sections[0].heading, "Blog Outline Draft");
}

#[test]
fn mock_ai_draft_provider_returns_stable_payload_and_failure() {
    let request = sample_draft_request(
        AiDraftType::ReleaseNotes,
        Some(AiProvider::Anthropic),
        Some("mock-model".to_string()),
        false,
    );

    let payload = MockAiDraftProviderClient::new(AiProvider::Anthropic)
        .draft(&request)
        .expect("mock provider succeeds");
    let report = build_ai_draft_report(&request, payload, true);

    assert!(report.provider_called);
    assert_eq!(report.provider_summary, "mock anthropic draft");
    assert_eq!(report.sections.len(), 2);
    assert_eq!(report.sections[0].heading, "Release Notes");

    let error = MockAiDraftProviderClient::failing(AiProvider::Openai)
        .draft(&request)
        .expect_err("mock provider fails");
    assert!(error.to_string().contains("mock AI draft provider failure"));
}

#[test]
fn ai_draft_provider_payload_is_normalized_before_markdown() {
    let request = sample_draft_request(
        AiDraftType::BlogOutline,
        Some(AiProvider::Openai),
        Some("draft-model".to_string()),
        false,
    );
    let report = build_ai_draft_report(
        &request,
        super::AiDraftProviderPayload {
            summary: "summary with data:image/png;base64,abc".to_string(),
            sections: vec![AiDraftSection {
                heading: String::new(),
                body: "body\nOPENAI_API_KEY=test-secret\nok".to_string(),
                bullets: vec!["ANTHROPIC_API_KEY=test-secret".to_string()],
            }],
        },
        true,
    );
    let markdown = render_ai_draft_markdown(&report);

    assert_eq!(
        report.provider_summary,
        "[redacted sensitive provider or image data]"
    );
    assert_eq!(report.sections[0].heading, "Blog Outline");
    assert!(!markdown.contains("test-secret"));
    assert!(!markdown.contains("data:image"));
    assert!(markdown.contains("[redacted sensitive provider or image data]"));
}

#[test]
fn draft_sanitization_truncates_without_splitting_utf8_and_redacts_sensitive_lines() {
    let sanitized = sanitize_draft_text(
        "ok\r\nOPENAI_API_KEY=test-secret\rdata:image/png;base64,abc\nkeep  ",
        400,
    );
    assert!(!sanitized.contains("test-secret"));
    assert!(!sanitized.contains("data:image"));
    assert!(sanitized.contains("ok\n[redacted sensitive provider or image data]"));
    assert!(sanitized.ends_with("keep"));

    let truncated = sanitize_draft_text("abcdéfg", 5);
    assert_eq!(truncated, "abcd\n[excerpt truncated]");

    let request = sample_draft_request(AiDraftType::ReadmeSnippet, None, None, false);
    let fallback = build_ai_draft_report(
        &request,
        AiDraftProviderPayload {
            summary: "summary".to_string(),
            sections: vec![AiDraftSection {
                heading: " ".to_string(),
                body: " ".to_string(),
                bullets: vec![" ".to_string()],
            }],
        },
        true,
    );
    assert_eq!(fallback.sections[0].heading, "README Snippet Draft");
}

#[test]
fn ai_image_mime_type_matches_openai_supported_inputs() {
    assert_eq!(ai_image_mime_type("png"), Some("image/png"));
    assert_eq!(ai_image_mime_type("jpeg"), Some("image/jpeg"));
    assert_eq!(ai_image_mime_type("jpg"), Some("image/jpeg"));
    assert_eq!(ai_image_mime_type("webp"), Some("image/webp"));
    assert_eq!(ai_image_mime_type("gif"), Some("image/gif"));
    assert_eq!(ai_image_mime_type("avif"), None);
}

fn sample_manifest() -> Manifest {
    Manifest {
        version: 1,
        generated_at: "unix:123".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:config".to_string(),
        outputs: vec![
            ManifestOutput {
                source_path: "assets/images/sample.png".to_string(),
                source_hash: "blake3:source".to_string(),
                source_width: 100,
                source_height: 80,
                source_bytes: 1000,
                output_path: "public/images/generated/sample.project-card.64.webp".to_string(),
                preset: "project-card".to_string(),
                fit: "cover".to_string(),
                width: 64,
                height: 64,
                format: "webp".to_string(),
                bytes: 700,
                hash: "blake3:webp".to_string(),
                operation_hash: "blake3:operation-webp".to_string(),
            },
            ManifestOutput {
                source_path: "assets/images/sample.png".to_string(),
                source_hash: "blake3:source".to_string(),
                source_width: 100,
                source_height: 80,
                source_bytes: 1000,
                output_path: "public/images/generated/sample.project-card.64.avif".to_string(),
                preset: "project-card".to_string(),
                fit: "cover".to_string(),
                width: 64,
                height: 64,
                format: "avif".to_string(),
                bytes: 600,
                hash: "blake3:avif".to_string(),
                operation_hash: "blake3:operation-avif".to_string(),
            },
        ],
    }
}

fn sample_draft_request(
    draft_type: AiDraftType,
    provider: Option<AiProvider>,
    model: Option<String>,
    dry_run: bool,
) -> AiDraftRequest {
    AiDraftRequest {
        schema_version: 1,
        draft_type,
        provider,
        model,
        command: "devimg draft".to_string(),
        config_path: "devimg.toml".to_string(),
        project_root: ".".to_string(),
        mode: if provider.is_some() {
            "provider-draft".to_string()
        } else {
            "metadata-only".to_string()
        },
        dry_run,
        output_path: "devimg-draft.md".to_string(),
        manifest_summary: AiDraftManifestSummary {
            path: "public/images/devimg-manifest.json".to_string(),
            readable: true,
            read_error: None,
            config_hash: Some("blake3:config".to_string()),
            source_count: 1,
            output_count: 2,
            source_bytes: 1000,
            output_bytes: 1300,
            outputs: Vec::new(),
        },
        report_summary: AiDraftArtifactSummary {
            label: "devimg-report".to_string(),
            path: "devimg-report.md".to_string(),
            readable: true,
            read_error: None,
            bytes: 120,
            line_count: 4,
            excerpt: "Dev Image Pipeline Report".to_string(),
        },
        changelog_summary: None,
        optional_artifacts: Vec::new(),
    }
}

fn write_png(path: &Path) {
    let mut image = RgbaImage::new(2, 2);
    for pixel in image.pixels_mut() {
        *pixel = Rgba([10, 20, 30, 255]);
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("png encoded");
    fs::write(path, bytes.into_inner()).expect("png written");
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
            "devimg-ai-tests-{name}-{}-{unique}",
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
