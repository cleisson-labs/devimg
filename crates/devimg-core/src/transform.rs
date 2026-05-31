use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use image::{
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::FilterType,
    DynamicImage, ExtendedColorType, ImageEncoder,
};
use ravif::{Encoder as AvifEncoder, Img as AvifImage, RGBA8};

use crate::config::{CropPosition, FitMode, FormatKind};
use crate::hash::{hash_bytes, hash_file};
use crate::manifest::ManifestOutput;
use crate::pipeline::Operation;
use crate::{DevimgError, Result};

pub(crate) fn execute_operation(
    operation: &Operation,
    allow_overwrite: bool,
) -> Result<ManifestOutput> {
    let source_image = image::open(&operation.source.path).map_err(|source| {
        DevimgError::image(
            &operation.source.path,
            format!("could not decode image: {source}"),
        )
    })?;
    let processed = transform_image(
        source_image,
        operation.fit,
        operation.crop,
        operation.width,
        operation.height,
    );
    let encoded = encode_image(&processed, operation.format, operation.quality)
        .map_err(|message| DevimgError::image(&operation.output_path, message))?;
    let encoded_hash = hash_bytes(&encoded);
    let output_path = final_output_path(&operation.output_path, &encoded_hash, operation)?;
    let output_project_path =
        final_output_project_path(&operation.output_project_path, &encoded_hash, operation)?;

    if output_path.exists() {
        let existing_hash = hash_file(&output_path)?;
        if existing_hash == encoded_hash {
            let metadata = fs::metadata(&output_path)
                .map_err(|source| DevimgError::io(&output_path, source))?;
            return Ok(manifest_output(
                operation,
                output_project_path,
                metadata.len(),
                encoded_hash,
            ));
        }
        if !allow_overwrite {
            return Err(DevimgError::UnsafeOverwrite { path: output_path });
        }
    }

    safe_write(&output_path, &encoded, allow_overwrite)?;
    Ok(manifest_output(
        operation,
        output_project_path,
        encoded.len() as u64,
        encoded_hash,
    ))
}

fn manifest_output(
    operation: &Operation,
    output_project_path: String,
    bytes: u64,
    hash: String,
) -> ManifestOutput {
    ManifestOutput {
        source_path: operation.source.project_path.clone(),
        source_hash: operation.source.hash.clone(),
        source_width: operation.source.width,
        source_height: operation.source.height,
        source_bytes: operation.source.bytes,
        output_path: output_project_path,
        preset: operation.preset.clone(),
        fit: operation.fit.label().to_string(),
        width: operation.width,
        height: operation.height,
        format: operation.format.label().to_string(),
        bytes,
        hash,
        operation_hash: operation.operation_hash.clone(),
    }
}

pub(crate) fn final_output_project_path(
    output_project_path: &str,
    hash: &str,
    operation: &Operation,
) -> Result<String> {
    if operation.content_hash_filenames {
        add_hash_to_output_project_path(output_project_path, hash)
    } else {
        Ok(output_project_path.to_string())
    }
}

fn final_output_path(path: &Path, hash: &str, operation: &Operation) -> Result<PathBuf> {
    if operation.content_hash_filenames {
        add_hash_to_output_path(path, hash)
    } else {
        Ok(path.to_path_buf())
    }
}

fn add_hash_to_output_project_path(output_project_path: &str, hash: &str) -> Result<String> {
    let separator = output_project_path
        .rfind('/')
        .map(|index| index + 1)
        .unwrap_or(0);
    let (parent, file_name) = output_project_path.split_at(separator);
    let (stem, extension) = file_name.rsplit_once('.').ok_or_else(|| {
        DevimgError::image(
            output_project_path,
            "could not derive extension for hashed output filename",
        )
    })?;
    Ok(format!(
        "{parent}{stem}.{}.{}",
        hash_fragment(hash),
        extension
    ))
}

fn add_hash_to_output_path(path: &Path, hash: &str) -> Result<PathBuf> {
    let stem = path
        .file_stem()
        .ok_or_else(|| DevimgError::image(path, "could not derive file stem"))?
        .to_string_lossy();
    let extension = path
        .extension()
        .ok_or_else(|| DevimgError::image(path, "could not derive file extension"))?
        .to_string_lossy();
    let file_name = format!("{stem}.{}.{}", hash_fragment(hash), extension);
    Ok(path.with_file_name(file_name))
}

fn hash_fragment(hash: &str) -> &str {
    let raw = hash.strip_prefix("blake3:").unwrap_or(hash);
    &raw[..raw.len().min(12)]
}

fn transform_image(
    image: DynamicImage,
    fit: FitMode,
    crop: CropPosition,
    width: u32,
    height: u32,
) -> DynamicImage {
    match fit {
        FitMode::Fill => image.resize_exact(width, height, FilterType::Lanczos3),
        FitMode::Contain => image.resize(width, height, FilterType::Lanczos3),
        FitMode::Cover => {
            let (resize_width, resize_height) =
                crate::plan::cover_dimensions(image.width(), image.height(), width, height);
            let resized = image.resize_exact(resize_width, resize_height, FilterType::Lanczos3);
            let x = crop_offset(resize_width.saturating_sub(width), crop.x);
            let y = crop_offset(resize_height.saturating_sub(height), crop.y);
            resized.crop_imm(x, y, width, height)
        }
    }
}

fn crop_offset(overflow: u32, position: f32) -> u32 {
    ((overflow as f32) * position).round() as u32
}

fn encode_image(
    image: &DynamicImage,
    format: FormatKind,
    quality: u8,
) -> std::result::Result<Vec<u8>, String> {
    let mut cursor = Cursor::new(Vec::new());
    match format {
        FormatKind::Jpeg => {
            let rgb = image.to_rgb8();
            JpegEncoder::new_with_quality(&mut cursor, quality)
                .write_image(
                    rgb.as_raw(),
                    rgb.width(),
                    rgb.height(),
                    ExtendedColorType::Rgb8,
                )
                .map_err(|source| source.to_string())?;
        }
        FormatKind::Png => {
            let rgba = image.to_rgba8();
            PngEncoder::new(&mut cursor)
                .write_image(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    ExtendedColorType::Rgba8,
                )
                .map_err(|source| source.to_string())?;
        }
        FormatKind::Webp => return encode_webp(image, quality),
        FormatKind::Avif => return encode_avif(image, quality),
    }
    Ok(cursor.into_inner())
}

fn encode_webp(image: &DynamicImage, quality: u8) -> std::result::Result<Vec<u8>, String> {
    let rgba = image.to_rgba8();
    let encoder = webp::Encoder::from_rgba(&rgba, rgba.width(), rgba.height());
    let encoded = encoder.encode(f32::from(quality));
    non_empty_encoded("WebP", encoded.to_vec())
}

fn encode_avif(image: &DynamicImage, quality: u8) -> std::result::Result<Vec<u8>, String> {
    let rgba = image.to_rgba8();
    let pixels = rgba
        .pixels()
        .map(|pixel| {
            let [r, g, b, a] = pixel.0;
            RGBA8 { r, g, b, a }
        })
        .collect::<Vec<_>>();
    let encoded = AvifEncoder::new()
        .with_quality(f32::from(quality.max(1)))
        .with_speed(8)
        .encode_rgba(AvifImage::new(
            &pixels,
            rgba.width() as usize,
            rgba.height() as usize,
        ))
        .map_err(avif_encode_error)?;
    non_empty_encoded("AVIF", encoded.avif_file)
}

fn avif_encode_error(source: ravif::Error) -> String {
    source.to_string()
}

fn non_empty_encoded(format: &str, bytes: Vec<u8>) -> std::result::Result<Vec<u8>, String> {
    if bytes.is_empty() {
        return Err(format!("{format} encoder returned empty output"));
    }
    Ok(bytes)
}

fn safe_write(path: &Path, bytes: &[u8], allow_replace: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| DevimgError::io(parent, source))?;
    }
    let file_name = path.file_name().ok_or_else(|| {
        DevimgError::io(
            path,
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "missing filename"),
        )
    })?;
    let tmp_name = format!("{}.tmp-{}", file_name.to_string_lossy(), std::process::id());
    let tmp_path = path.with_file_name(tmp_name);
    fs::write(&tmp_path, bytes).map_err(|source| DevimgError::io(&tmp_path, source))?;
    if path.exists() && allow_replace {
        fs::remove_file(path).map_err(|source| DevimgError::io(path, source))?;
    }
    fs::rename(&tmp_path, path).map_err(|source| DevimgError::io(path, source))
}

#[cfg(test)]
#[path = "transform_tests.rs"]
mod tests;
