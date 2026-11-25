use std::path::PathBuf;

use crate::config::{AspectRatio, CropPosition, FitMode, FormatKind, PresetConfig};
use crate::manifest::{Manifest, ManifestOutput};
use crate::pipeline::SourceImage;

use super::{
    allowed_upscale_warning, append_unique, cover_crop_warning, lossy_quality_warnings,
    manifest_quality_warnings, skipped_upscale_warning,
};

#[test]
fn combining_warnings_preserves_order_and_removes_duplicates() {
    let mut warnings = vec!["existing warning".to_string()];

    append_unique(
        &mut warnings,
        ["existing warning", "new warning", "new warning"].map(str::to_string),
    );

    assert_eq!(warnings, vec!["existing warning", "new warning"]);
}

#[test]
fn warns_for_low_lossy_quality_on_detail_sensitive_assets() {
    let source = source("assets/images/hero-screenshot.png", 1600, 900, 120_000);
    let preset = preset(
        "hero",
        74,
        vec![FormatKind::Webp, FormatKind::Jpeg, FormatKind::Png],
    );

    let warnings = lossy_quality_warnings(&source, &preset);

    assert_eq!(warnings.len(), 2);
    assert!(warnings[0].contains("webp quality 74"));
    assert!(warnings[0].contains("suggested minimum 82"));
    assert!(warnings[1].contains("jpeg quality 74"));
}

#[test]
fn general_lossy_quality_threshold_is_less_noisy() {
    let source = source("assets/photos/trip.png", 1600, 900, 120_000);
    let acceptable_quality = preset("thumbnail", 74, vec![FormatKind::Webp, FormatKind::Avif]);

    assert!(lossy_quality_warnings(&source, &acceptable_quality).is_empty());

    let low_quality = preset("thumbnail", 60, vec![FormatKind::Webp]);
    let warnings = lossy_quality_warnings(&source, &low_quality);
    assert!(warnings[0].contains("general lossy output"));
}

#[test]
fn warns_for_allowed_and_skipped_upscaling() {
    let source = source("assets/images/small.png", 320, 180, 4_000);
    let mut preset = preset("project-card", 82, vec![FormatKind::Webp]);
    preset.allow_upscale = true;

    let allowed =
        allowed_upscale_warning(&source, &preset, 640, 640, 360).expect("allowed upscale warns");
    let skipped = skipped_upscale_warning(&source, &preset, 640, 640, 360);

    assert!(allowed.contains("allows upscaling"));
    assert!(allowed.contains("generated images may look soft"));
    assert!(skipped.contains("would require upscaling"));
}

#[test]
fn warns_for_material_cover_crop_loss() {
    let source = source("assets/images/banner.png", 1000, 1000, 40_000);
    let preset = preset("hero", 90, vec![FormatKind::Jpeg]);

    let warning = cover_crop_warning(&source, &preset, 1200, 1200, 675).expect("cover crop warns");

    assert!(warning.contains("uses cover"));
    assert!(warning.contains("crops about 44%"));
    assert!(warning.contains("fit=contain"));
}

#[test]
fn manifest_warnings_cover_upscale_and_larger_outputs() {
    let manifest = Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:config".to_string(),
        outputs: vec![ManifestOutput {
            source_path: "assets/small.png".to_string(),
            source_hash: "blake3:source".to_string(),
            source_width: 320,
            source_height: 180,
            source_bytes: 100,
            output_path: "public/images/small.project-card.640.webp".to_string(),
            preset: "project-card".to_string(),
            fit: "cover".to_string(),
            width: 640,
            height: 360,
            format: "webp".to_string(),
            bytes: 200,
            hash: "blake3:output".to_string(),
            operation_hash: "blake3:operation".to_string(),
        }],
    };

    let warnings = manifest_quality_warnings(&manifest);

    assert_eq!(warnings.len(), 2);
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("larger than its source")));
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("may look soft")));

    let mb_manifest = Manifest {
        outputs: vec![ManifestOutput {
            bytes: 2 * 1024 * 1024,
            source_bytes: 1024,
            output_path: "public/images/large.webp".to_string(),
            ..manifest.outputs[0].clone()
        }],
        ..manifest
    };
    let mb_warnings = manifest_quality_warnings(&mb_manifest);
    assert!(mb_warnings
        .iter()
        .any(|warning| warning.contains("2.00 MB")));
}

fn source(path: &str, width: u32, height: u32, bytes: u64) -> SourceImage {
    SourceImage {
        source_name: "portfolio".to_string(),
        path: PathBuf::from(path),
        project_path: path.to_string(),
        relative_path: PathBuf::from(path)
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_default(),
        output_root: PathBuf::from("public/images/generated"),
        output_root_project_path: "public/images/generated".to_string(),
        width,
        height,
        bytes,
        hash: "blake3:source".to_string(),
        format: FormatKind::Png,
    }
}

fn preset(name: &str, quality: u8, formats: Vec<FormatKind>) -> PresetConfig {
    PresetConfig {
        name: name.to_string(),
        widths: vec![1200],
        formats,
        quality,
        fit: FitMode::Cover,
        crop: CropPosition::CENTER,
        aspect_ratio: Some(AspectRatio {
            width: 16,
            height: 9,
        }),
        allow_upscale: false,
    }
}
