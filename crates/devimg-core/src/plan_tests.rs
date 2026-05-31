use std::path::{Path, PathBuf};

use crate::config::{
    AspectRatio, BudgetConfig, Config, CropPosition, FitMode, FormatKind, PresetConfig,
    PresetOverrideConfig, ProjectConfig, SourceConfig, WarningConfig,
};
use crate::pipeline::{Operation, SourceImage};

use super::{
    build_plan, cover_dimensions, encoder_hash_component, output_path_for, target_dimensions,
};

#[test]
fn target_dimensions_cover_contain_rounding_and_zero_width_clamp() {
    let mut preset = preset("card", FitMode::Contain);
    preset.aspect_ratio = Some(AspectRatio {
        width: 1,
        height: 1,
    });

    assert_eq!(target_dimensions(100, 400, 200, &preset), (50, 200));
    assert_eq!(target_dimensions(400, 100, 200, &preset), (200, 50));

    preset.aspect_ratio = None;
    assert_eq!(target_dimensions(800, 400, 0, &preset), (1, 1));
}

#[test]
fn cover_dimensions_covers_wide_and_tall_sources() {
    assert_eq!(cover_dimensions(400, 100, 100, 100), (400, 100));
    assert_eq!(cover_dimensions(100, 400, 100, 100), (100, 400));
}

#[test]
fn output_path_for_reports_missing_file_stem() {
    let mut source = source_image("card.png");
    source.relative_path = PathBuf::new();

    let error = output_path_for(
        &source,
        &preset("card", FitMode::Cover),
        100,
        FormatKind::Webp,
    )
    .expect_err("empty relative path should not have a file stem");

    assert!(error.to_string().contains("could not derive file stem"));
}

#[test]
fn preset_overrides_respect_preset_include_and_exclude_filters() {
    let config = config(
        vec![
            preset("card", FitMode::Cover),
            preset("thumb", FitMode::Cover),
        ],
        vec![PresetOverrideConfig {
            include: vec![String::from("**/*.png")],
            exclude: vec![String::from("nested/skip.png")],
            presets: vec![String::from("card")],
            quality: Some(40),
            fit: Some(FitMode::Contain),
            crop: Some(CropPosition { x: 0.5, y: 0.0 }),
            allow_upscale: Some(false),
        }],
    );
    let sources = vec![
        source_image("nested/card.png"),
        source_image("nested/skip.png"),
    ];

    let plan = build_plan(&config, &sources).expect("plan builds");

    let matched = find_operation(&plan.operations, "nested/card.png", "card");
    assert_eq!(matched.quality, 40);
    assert_eq!(matched.fit, FitMode::Contain);
    assert_eq!(matched.crop, CropPosition { x: 0.5, y: 0.0 });

    let preset_filtered = find_operation(&plan.operations, "nested/card.png", "thumb");
    assert_eq!(preset_filtered.quality, 80);
    assert_eq!(preset_filtered.fit, FitMode::Cover);

    let excluded = find_operation(&plan.operations, "nested/skip.png", "card");
    assert_eq!(excluded.quality, 80);
    assert_eq!(excluded.fit, FitMode::Cover);
}

#[test]
fn encoder_hash_component_is_only_set_for_external_lossy_encoders() {
    assert_eq!(encoder_hash_component(FormatKind::Png), None);
    assert_eq!(encoder_hash_component(FormatKind::Jpeg), None);
    assert_eq!(
        encoder_hash_component(FormatKind::Webp),
        Some("encoder:webp-libwebp-lossy-v1")
    );
    assert_eq!(
        encoder_hash_component(FormatKind::Avif),
        Some("encoder:ravif-0.13-speed8-v1")
    );
}

#[test]
fn build_plan_warns_when_metadata_cannot_be_preserved() {
    let mut config = config(vec![preset("card", FitMode::Cover)], Vec::new());
    config.project.strip_metadata = false;

    let plan = build_plan(&config, &[source_image("card.png")]).expect("plan builds");

    assert!(plan
        .warnings
        .iter()
        .any(|warning| warning.contains("plan:metadata-not-preserved")));
}

#[test]
fn build_plan_refuses_operation_that_would_overwrite_source() {
    let config = config(vec![preset("card", FitMode::Cover)], Vec::new());
    let mut source = source_image("card.png");
    source.path = PathBuf::from("public/images/generated/card.card.100.webp");

    let error = build_plan(&config, &[source]).expect_err("source overwrite should fail");

    assert!(error
        .to_string()
        .contains("refusing to overwrite unmanaged output"));
}

#[test]
fn build_plan_reports_invalid_override_globs() {
    let bad_include = config(
        vec![preset("card", FitMode::Cover)],
        vec![PresetOverrideConfig {
            include: vec!["[".to_string()],
            exclude: Vec::new(),
            presets: Vec::new(),
            quality: None,
            fit: None,
            crop: None,
            allow_upscale: None,
        }],
    );
    let include_error =
        build_plan(&bad_include, &[source_image("card.png")]).expect_err("include glob fails");
    assert!(include_error
        .to_string()
        .contains("invalid override include glob"));

    let bad_exclude = config(
        vec![preset("card", FitMode::Cover)],
        vec![PresetOverrideConfig {
            include: Vec::new(),
            exclude: vec!["[".to_string()],
            presets: Vec::new(),
            quality: None,
            fit: None,
            crop: None,
            allow_upscale: None,
        }],
    );
    let exclude_error =
        build_plan(&bad_exclude, &[source_image("card.png")]).expect_err("exclude glob fails");
    assert!(exclude_error
        .to_string()
        .contains("invalid override exclude glob"));
}

fn find_operation<'a>(
    operations: &'a [Operation],
    relative_path: &str,
    preset: &str,
) -> &'a Operation {
    operations
        .iter()
        .find(|operation| {
            operation.source.relative_path == Path::new(relative_path) && operation.preset == preset
        })
        .expect("operation exists")
}

fn source_image(relative_path: &str) -> SourceImage {
    let relative_path = PathBuf::from(relative_path);
    SourceImage {
        source_name: String::from("portfolio"),
        path: PathBuf::from("assets/images").join(&relative_path),
        project_path: format!("assets/images/{}", relative_path.display()),
        relative_path,
        output_root: PathBuf::from("public/images/generated"),
        output_root_project_path: String::from("public/images/generated"),
        width: 200,
        height: 100,
        bytes: 1024,
        hash: String::from("blake3:source"),
        format: FormatKind::Png,
    }
}

fn preset(name: &str, fit: FitMode) -> PresetConfig {
    PresetConfig {
        name: name.to_string(),
        widths: vec![100],
        formats: vec![FormatKind::Webp],
        quality: 80,
        fit,
        crop: CropPosition::CENTER,
        aspect_ratio: None,
        allow_upscale: true,
    }
}

fn config(presets: Vec<PresetConfig>, overrides: Vec<PresetOverrideConfig>) -> Config {
    Config {
        path: PathBuf::from("devimg.toml"),
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
        presets,
        overrides,
        warnings: WarningConfig::default(),
        budgets: BudgetConfig::default(),
    }
}
