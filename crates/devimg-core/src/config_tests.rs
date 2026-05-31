use std::path::{Path, PathBuf};

use super::{
    load_config, normalize_path, parse_byte_size, parse_config, project_relative,
    reject_unknown_top_level, resolve_project_path, resolve_project_path_checked, CropPosition,
    FitMode, FormatKind,
};

#[test]
fn parses_minimal_config() {
    let raw = r#"
[project]
root = "."
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
include = ["**/*.png", "**/*.jpg"]

[[preset]]
name = "project-card"
widths = [640, 960]
formats = ["webp", "jpeg"]
quality = 82
fit = "cover"
aspect_ratio = "16:9"

[budgets]
max_total_bytes = "3mb"
max_file_bytes = "350kb"
fail_on_regression = true
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");
    assert_eq!(config.sources[0].name, "portfolio");
    assert_eq!(
        config.presets[0].formats,
        vec![FormatKind::Webp, FormatKind::Jpeg]
    );
    assert_eq!(config.presets[0].fit, FitMode::Cover);
    assert_eq!(config.presets[0].crop, CropPosition::CENTER);
    assert_eq!(config.budgets.max_file_bytes, Some(350 * 1024));
}

#[test]
fn parses_numeric_budget_byte_sizes() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]

[budgets]
max_total_bytes = 2048
max_file_bytes = 1024
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");

    assert_eq!(config.budgets.max_total_bytes, Some(2048));
    assert_eq!(config.budgets.max_file_bytes, Some(1024));
}

#[test]
fn supports_aliases_and_defaults() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"
allow_overwrite = true

[[source]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[presets]]
name = "project-card"
widths = [640]
formats = ["jpg"]
upscale = true
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");
    assert!(config.project.overwrite);
    assert!(!config.project.content_hash_filenames);
    assert!(config.presets[0].allow_upscale);
    assert_eq!(config.presets[0].quality, 82);
    assert_eq!(config.presets[0].formats, vec![FormatKind::Jpeg]);
}

#[test]
fn parses_avif_as_output_only_format() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp", "avif"]
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");

    assert_eq!(
        config.presets[0].formats,
        vec![FormatKind::Webp, FormatKind::Avif]
    );
    assert!(FormatKind::Webp.supports_source_input());
    assert!(!FormatKind::Avif.supports_source_input());
}

#[test]
fn format_fit_crop_and_aspect_helpers_cover_labels_aliases_and_errors() {
    assert_eq!(FormatKind::Png.extension(), "png");
    assert_eq!(FormatKind::Jpeg.extension(), "jpeg");
    assert_eq!(FormatKind::Webp.extension(), "webp");
    assert_eq!(FormatKind::Avif.extension(), "avif");
    assert_eq!(FormatKind::Png.label(), "png");
    assert_eq!(FormatKind::Jpeg.label(), "jpeg");
    assert_eq!(FormatKind::Webp.label(), "webp");
    assert_eq!(FormatKind::Avif.label(), "avif");
    assert_eq!(FormatKind::Jpeg.image_format(), image::ImageFormat::Jpeg);
    assert_eq!(FormatKind::Webp.image_format(), image::ImageFormat::WebP);
    assert_eq!(FormatKind::Avif.image_format(), image::ImageFormat::Avif);
    assert_eq!(FitMode::Cover.label(), "cover");
    assert_eq!(FitMode::Contain.label(), "contain");
    assert_eq!(FitMode::Fill.label(), "fill");
    assert_eq!(
        (CropPosition { x: 0.25, y: 0.75 }).label(),
        "crop:0.2500:0.7500"
    );

    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]
fit = "stretch"
crop = "bottom left"
aspect_ratio = "4:3"
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");
    assert_eq!(config.presets[0].fit, FitMode::Fill);
    assert_eq!(config.presets[0].crop, CropPosition { x: 0.0, y: 1.0 });
    assert_eq!(config.presets[0].aspect_ratio.expect("aspect").width, 4);

    for (raw, expected) in [
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
[[preset]]
name = "project-card"
widths = [640]
formats = ["gif"]
"#,
            "unsupported image format `gif`",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]
fit = "squish"
"#,
            "unsupported fit `squish`",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]
crop = "middle"
"#,
            "unsupported crop `middle`",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]
aspect_ratio = "0:1"
"#,
            "invalid aspect_ratio `0:1`",
        ),
    ] {
        let error = parse_config(Path::new("devimg.toml"), raw).expect_err(expected);
        assert!(error.to_string().contains(expected));
    }
}

#[test]
fn parses_crop_anchor() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]
crop = "top-right"
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");

    assert_eq!(config.presets[0].crop, CropPosition { x: 1.0, y: 0.0 });
}

#[test]
fn parses_crop_focal_point() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]
crop = { x = 0.5, y = 0.0 }
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");

    assert_eq!(config.presets[0].crop, CropPosition { x: 0.5, y: 0.0 });
}

#[test]
fn rejects_invalid_crop_focal_point() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]
crop = { x = 0.5, y = 1.5 }
"#;
    let error = parse_config(Path::new("devimg.toml"), raw).expect_err("config fails");

    assert!(error.to_string().contains("crop x and y"));
}

#[test]
fn crop_position_rejects_non_finite_coordinates() {
    let error = CropPosition::new(f32::NAN, 0.5).expect_err("NaN crop is invalid");

    assert_eq!(error, "crop x and y must be finite numbers");
}

#[test]
fn parses_content_hash_filename_alias() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"
hash_filenames = true

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");

    assert!(config.project.content_hash_filenames);
}

#[test]
fn parses_preset_overrides() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]

[[overrides]]
include = ["cli_tools.png"]
presets = ["project-card"]
quality = 74
fit = "contain"
crop = "top"
upscale = true
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");

    assert_eq!(config.overrides.len(), 1);
    assert_eq!(config.overrides[0].include, vec!["cli_tools.png"]);
    assert_eq!(config.overrides[0].presets, vec!["project-card"]);
    assert_eq!(config.overrides[0].quality, Some(74));
    assert_eq!(config.overrides[0].fit, Some(FitMode::Contain));
    assert_eq!(
        config.overrides[0].crop,
        Some(CropPosition { x: 0.5, y: 0.0 })
    );
    assert_eq!(config.overrides[0].allow_upscale, Some(true));
}

#[test]
fn parses_scoped_warning_acknowledgements() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]

[[warnings.acknowledge]]
code = "quality:cover-crop"
source = "./assets/images/accesstrace.png"
preset = "project-card"
reason = "Intentional social-card framing."
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");

    assert_eq!(config.warnings.acknowledgements.len(), 1);
    let acknowledgement = &config.warnings.acknowledgements[0];
    assert_eq!(acknowledgement.code, "quality:cover-crop");
    assert_eq!(
        acknowledgement.source.as_deref(),
        Some("assets/images/accesstrace.png")
    );
    assert_eq!(acknowledgement.preset.as_deref(), Some("project-card"));
    assert_eq!(
        acknowledgement.reason.as_deref(),
        Some("Intentional social-card framing.")
    );
}

#[test]
fn rejects_broad_cover_crop_warning_acknowledgements() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]

[[warnings.acknowledge]]
code = "quality:cover-crop"
preset = "project-card"
"#;
    let error = parse_config(Path::new("devimg.toml"), raw).expect_err("config fails");

    assert!(error.to_string().contains("source and preset"));
}

#[test]
fn rejects_invalid_override_quality() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]

[[overrides]]
include = ["card.png"]
quality = 101
"#;
    let error = parse_config(Path::new("devimg.toml"), raw).expect_err("config fails");

    assert!(error.to_string().contains("override quality"));
}

#[test]
fn rejects_unknown_top_level_sections() {
    let raw = r#"
[project]
manifest = "public/images/devimg-manifest.json"

[unknown]
value = true

[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"

[[preset]]
name = "project-card"
widths = [640]
formats = ["webp"]
"#;
    let error = parse_config(Path::new("devimg.toml"), raw).expect_err("config fails");
    assert!(error.to_string().contains("unsupported top-level"));
}

#[test]
fn load_config_reports_missing_file_as_config_error() {
    let path = std::env::temp_dir().join(format!(
        "devimg-missing-config-{}-{}.toml",
        std::process::id(),
        "coverage"
    ));

    let error = load_config(&path).expect_err("missing borrowed config should fail");
    assert!(error.to_string().contains("config file not found"));

    let error = load_config(path).expect_err("missing owned config should fail");

    assert!(error.to_string().contains("config file not found"));
}

#[test]
fn load_config_accepts_owned_paths_and_parse_config_reports_invalid_toml() {
    let path = std::env::temp_dir().join(format!(
        "devimg-owned-config-{}-coverage.toml",
        std::process::id()
    ));
    std::fs::write(
        &path,
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
    .expect("config written");

    let config = load_config(&path).expect("borrowed path config loads");
    assert_eq!(
        config.project.manifest,
        PathBuf::from("public/images/devimg-manifest.json")
    );

    let config = load_config(path.clone()).expect("owned path config loads");
    assert_eq!(
        config.project.manifest,
        PathBuf::from("public/images/devimg-manifest.json")
    );
    let _ = std::fs::remove_file(path);

    let error = parse_config(Path::new("devimg.toml"), "[project").expect_err("TOML fails");
    assert!(error.to_string().contains("invalid TOML"));
}

#[test]
fn config_without_parent_path_uses_current_directory_root() {
    let config = parse_config(
        Path::new(""),
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

    assert_eq!(config.project.root, PathBuf::new());
}

#[test]
fn load_config_wraps_non_not_found_io_errors() {
    let dir =
        std::env::temp_dir().join(format!("devimg-config-dir-{}-coverage", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir created");

    let error = load_config(&dir).expect_err("borrowed directory read should fail");
    assert!(error.to_string().contains("I/O error"));

    let error = load_config(dir.clone()).expect_err("owned directory read should fail");

    assert!(error.to_string().contains("I/O error"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rejects_missing_required_sections_and_empty_preset_values() {
    for (raw, expected) in [
        (
            r#"
[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
[[preset]]
name = "card"
widths = [100]
formats = ["webp"]
"#,
            "missing [project].manifest",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[preset]]
name = "card"
widths = [100]
formats = ["webp"]
"#,
            "at least one [[sources]] entry is required",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
"#,
            "at least one [[preset]] entry is required",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
input = "assets/images"
output = "public/images/generated"
[[preset]]
name = "card"
widths = [100]
formats = ["webp"]
"#,
            "missing source.name",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
output = "public/images/generated"
[[preset]]
name = "card"
widths = [100]
formats = ["webp"]
"#,
            "missing source.input",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
input = "assets/images"
[[preset]]
name = "card"
widths = [100]
formats = ["webp"]
"#,
            "missing source.output",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
[[preset]]
widths = [100]
formats = ["webp"]
"#,
            "missing preset.name",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
[[preset]]
name = "card"
formats = ["webp"]
"#,
            "missing preset.widths",
        ),
        (
            r#"
[project]
manifest = "public/images/devimg-manifest.json"
[[sources]]
name = "portfolio"
input = "assets/images"
output = "public/images/generated"
[[preset]]
name = "card"
widths = []
formats = ["webp"]
"#,
            "preset.widths cannot be empty",
        ),
        (
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
"#,
            "missing preset.formats",
        ),
        (
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
formats = []
"#,
            "preset.formats cannot be empty",
        ),
        (
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
quality = 101
"#,
            "quality must be 0-100",
        ),
    ] {
        let error = parse_config(Path::new("devimg.toml"), raw).expect_err(expected);
        assert!(
            error.to_string().contains(expected),
            "expected `{expected}` in `{error}`"
        );
    }
}

#[test]
fn parses_byte_size_suffixes_and_rejects_unknown_units() {
    assert_eq!(parse_byte_size("5b").expect("bytes parse"), 5);
    assert_eq!(parse_byte_size("2 kib").expect("kib parses"), 2 * 1024);
    assert_eq!(parse_byte_size("3mb").expect("mb parses"), 3 * 1024 * 1024);
    assert_eq!(
        parse_byte_size("1gb").expect("gb parses"),
        1024 * 1024 * 1024
    );

    let error = parse_byte_size("10xb").expect_err("unknown unit should fail");
    assert!(error.contains("invalid byte size unit `xb`"));
    let empty = parse_byte_size("").expect_err("empty value should fail");
    assert!(empty.contains("invalid byte size"));
    let huge = parse_byte_size("999999999999999999999999999999999999999999gb")
        .expect_err("oversized number should fail");
    assert!(huge.contains("invalid byte size"));
}

#[test]
fn normalize_path_removes_current_and_parent_components() {
    assert_eq!(
        normalize_path(Path::new("assets/./images/../card.png")),
        PathBuf::from("assets/card.png")
    );
    assert!(normalize_path(Path::new("/tmp/../card.png")).is_absolute());
}

#[test]
fn resolve_project_path_checked_rejects_paths_that_escape_root() {
    let config = parse_config(
        Path::new("devimg.toml"),
        r#"
[project]
root = "project"
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

    let error = resolve_project_path_checked(&config, Path::new("../outside"), "source input")
        .expect_err("escaping path should fail");

    assert!(error
        .to_string()
        .contains("source input escapes project root"));
}

#[test]
fn project_path_helpers_handle_absolute_and_relative_paths() {
    let config = parse_config(
        Path::new("/tmp/devimg-config/devimg.toml"),
        r#"
[project]
root = "/tmp/devimg-config/project"
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

    assert_eq!(
        resolve_project_path(&config, Path::new("assets/card.png")),
        PathBuf::from("/tmp/devimg-config/project/assets/card.png")
    );
    assert_eq!(
        resolve_project_path(&config, Path::new("/tmp/outside/card.png")),
        PathBuf::from("/tmp/outside/card.png")
    );
    assert_eq!(
        project_relative(
            &config,
            Path::new("/tmp/devimg-config/project/assets/card.png")
        ),
        PathBuf::from("assets/card.png")
    );
    assert_eq!(
        project_relative(&config, Path::new("/tmp/outside/card.png")),
        PathBuf::from("/tmp/outside/card.png")
    );
}

#[test]
fn rejects_warning_acknowledgement_without_code() {
    let raw = r#"
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

[[warnings.acknowledge]]
source = "assets/card.png"
preset = "card"
"#;

    let error = parse_config(Path::new("devimg.toml"), raw).expect_err("config fails");

    assert!(error
        .to_string()
        .contains("missing warnings.acknowledge.code"));
}

#[test]
fn warning_acknowledgement_scope_requirements_are_enforced_and_trimmed() {
    let raw = r#"
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

[[warnings.acknowledge]]
code = "quality:generated-upscale"
output = " ./public/card.webp "
reason = "   "
"#;
    let config = parse_config(Path::new("devimg.toml"), raw).expect("config parses");
    let acknowledgement = &config.warnings.acknowledgements[0];
    assert_eq!(acknowledgement.output.as_deref(), Some("public/card.webp"));
    assert_eq!(acknowledgement.reason, None);

    for (raw, expected) in [
        (
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
[[warnings.acknowledge]]
code = "missing_report"
"#,
            "requires at least one of source, output, preset, or width",
        ),
        (
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
[[warnings.acknowledge]]
code = "quality:generated-upscale"
source = "assets/card.png"
"#,
            "quality:generated-upscale acknowledgements require output",
        ),
        (
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
[[warnings.acknowledge]]
code = "plan:no-variants"
preset = "card"
"#,
            "plan:no-variants acknowledgements require source",
        ),
    ] {
        let error = parse_config(Path::new("devimg.toml"), raw).expect_err(expected);
        assert!(error.to_string().contains(expected));
    }
}

#[test]
fn reject_unknown_top_level_requires_toml_table() {
    let error = reject_unknown_top_level(
        Path::new("devimg.toml"),
        &toml::Value::String("not a table".to_string()),
    )
    .expect_err("root string should fail");

    assert!(error
        .to_string()
        .contains("config root must be a TOML table"));
}
