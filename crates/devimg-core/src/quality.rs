use crate::config::{CropPosition, FitMode, FormatKind, PresetConfig};
use crate::manifest::Manifest;
use crate::pipeline::SourceImage;
use crate::warnings::{
    warning_message, QUALITY_ALLOWED_UPSCALE, QUALITY_COVER_CROP, QUALITY_GENERATED_UPSCALE,
    QUALITY_LOW_LOSSY, QUALITY_OUTPUT_LARGER_THAN_SOURCE, QUALITY_SKIPPED_UPSCALE,
};

const DETAIL_SENSITIVE_KEYWORDS: &[&str] = &[
    "banner",
    "card",
    "chart",
    "diagram",
    "hero",
    "logo",
    "og",
    "open-graph",
    "opengraph",
    "screen",
    "screenshot",
    "text",
    "ui",
];

pub(crate) fn append_unique(
    warnings: &mut Vec<String>,
    new_warnings: impl IntoIterator<Item = String>,
) {
    for warning in new_warnings {
        if !warnings.contains(&warning) {
            warnings.push(warning);
        }
    }
}

pub(crate) fn lossy_quality_warnings(source: &SourceImage, preset: &PresetConfig) -> Vec<String> {
    let detail_sensitive = is_detail_sensitive_asset(&source.project_path, &preset.name);
    let mut warnings = Vec::new();
    for format in &preset.formats {
        let Some(minimum) = suggested_min_quality(*format, detail_sensitive) else {
            continue;
        };
        if preset.quality >= minimum {
            continue;
        }

        let reason = if detail_sensitive {
            "screenshot/banner/text-heavy assets"
        } else {
            "general lossy output"
        };
        warnings.push(warning_message(
            QUALITY_LOW_LOSSY,
            format!(
                "{} preset {} uses {} quality {} below the suggested minimum {} for {}; raise quality or use png when crisp text/graphics matter",
            source.project_path,
            preset.name,
            format.label(),
            preset.quality,
            minimum,
            reason
            ),
        ));
    }
    warnings
}

pub(crate) fn skipped_upscale_warning(
    source: &SourceImage,
    preset: &PresetConfig,
    requested_width: u32,
    target_width: u32,
    target_height: u32,
) -> String {
    warning_message(
        QUALITY_SKIPPED_UPSCALE,
        format!(
            "skipped {} preset {} width {} because it would require upscaling from {}x{} to {}x{}; provide a larger source, reduce widths, or set allow_upscale=true if softness is acceptable",
        source.project_path,
        preset.name,
        requested_width,
        source.width,
        source.height,
        target_width,
        target_height
        ),
    )
}

pub(crate) fn allowed_upscale_warning(
    source: &SourceImage,
    preset: &PresetConfig,
    requested_width: u32,
    target_width: u32,
    target_height: u32,
) -> Option<String> {
    if target_width <= source.width && target_height <= source.height {
        return None;
    }

    Some(warning_message(
        QUALITY_ALLOWED_UPSCALE,
        format!(
            "{} preset {} width {} allows upscaling from {}x{} to {}x{}; generated images may look soft, so prefer a larger source or reduce widths",
        source.project_path,
        preset.name,
        requested_width,
        source.width,
        source.height,
        target_width,
        target_height
        ),
    ))
}

pub(crate) fn cover_crop_warning(
    source: &SourceImage,
    preset: &PresetConfig,
    requested_width: u32,
    target_width: u32,
    target_height: u32,
) -> Option<String> {
    if preset.fit != FitMode::Cover || preset.aspect_ratio.is_none() {
        return None;
    }

    let loss = cover_crop_loss(source.width, source.height, target_width, target_height);
    if loss < 0.15 {
        return None;
    }

    let crop_hint = if preset.crop == CropPosition::CENTER {
        "set crop to an anchor/focal point"
    } else {
        "confirm the crop anchor/focal point"
    };
    Some(warning_message(
        QUALITY_COVER_CROP,
        format!(
            "{} preset {} width {} uses cover and crops about {}% of the resized image; {}, or use fit=contain if the full composition must remain visible",
        source.project_path,
        preset.name,
        requested_width,
        (loss * 100.0).round() as u32,
        crop_hint
        ),
    ))
}

pub(crate) fn manifest_quality_warnings(manifest: &Manifest) -> Vec<String> {
    let mut warnings = Vec::new();
    for output in &manifest.outputs {
        if output.width > output.source_width || output.height > output.source_height {
            warnings.push(warning_message(
                QUALITY_GENERATED_UPSCALE,
                format!(
                    "{} is {}x{} from a {}x{} source; generated images may look soft, so use a larger source or reduce configured widths",
                output.output_path,
                output.width,
                output.height,
                output.source_width,
                output.source_height
                ),
            ));
        }
        if output.bytes > output.source_bytes {
            warnings.push(warning_message(
                QUALITY_OUTPUT_LARGER_THAN_SOURCE,
                format!(
                    "{} is larger than its source file ({} vs {}); this can happen with small optimized sources, high quality settings, format changes, or graphic/transparent assets",
                output.output_path,
                format_bytes(output.bytes),
                format_bytes(output.source_bytes)
                ),
            ));
        }
    }
    warnings.sort();
    warnings.dedup();
    warnings
}

fn suggested_min_quality(format: FormatKind, detail_sensitive: bool) -> Option<u8> {
    match format {
        FormatKind::Jpeg | FormatKind::Webp => Some(if detail_sensitive { 82 } else { 70 }),
        FormatKind::Avif => Some(if detail_sensitive { 60 } else { 45 }),
        FormatKind::Png => None,
    }
}

fn is_detail_sensitive_asset(source_path: &str, preset_name: &str) -> bool {
    let haystack = format!(
        "{} {}",
        source_path.to_ascii_lowercase().replace('_', "-"),
        preset_name.to_ascii_lowercase().replace('_', "-")
    );
    DETAIL_SENSITIVE_KEYWORDS
        .iter()
        .any(|keyword| haystack.contains(keyword))
}

fn cover_crop_loss(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> f64 {
    let (resize_width, resize_height) =
        crate::plan::cover_dimensions(source_width, source_height, target_width, target_height);
    let width_loss =
        resize_width.saturating_sub(target_width) as f64 / f64::from(resize_width.max(1));
    let height_loss =
        resize_height.saturating_sub(target_height) as f64 / f64::from(resize_height.max(1));
    width_loss.max(height_loss)
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= MB {
        format!("{:.2} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes / KB)
    } else {
        format!("{} B", bytes as u64)
    }
}

#[cfg(test)]
#[path = "quality_tests.rs"]
mod tests;
