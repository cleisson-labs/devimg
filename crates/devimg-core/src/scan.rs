use std::fs;
use std::path::{Path, PathBuf};

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use image::ImageFormat;
use walkdir::WalkDir;

use crate::config::{project_relative, resolve_project_path_checked, Config, FormatKind};
use crate::hash::hash_bytes;
use crate::pipeline::{path_to_string, ImageInspection, SourceImage};
use crate::{DevimgError, Result};

pub fn scan_sources(config: &Config) -> Result<Vec<SourceImage>> {
    let mut images = Vec::new();
    for source in &config.sources {
        let input_root = resolve_project_path_checked(config, &source.input, "source input")?;
        let output_root = resolve_project_path_checked(config, &source.output, "source output")?;
        if !input_root.exists() {
            return Err(DevimgError::config(
                &config.path,
                format!(
                    "source `{}` input does not exist: {}",
                    source.name,
                    input_root.display()
                ),
            ));
        }
        if !input_root.is_dir() {
            return Err(DevimgError::config(
                &config.path,
                format!(
                    "source `{}` input is not a directory: {}",
                    source.name,
                    input_root.display()
                ),
            ));
        }

        let includes = compile_globs(&config.path, "include", &source.include)?;
        let excludes = compile_globs(&config.path, "exclude", &source.exclude)?;
        let mut files = collect_files(&input_root)?;
        files.sort();
        for path in files {
            let normalized = crate::config::normalize_path(&path);
            if path_is_under(&normalized, &output_root) {
                continue;
            }
            let Some(expected) = source_input_format(&normalized) else {
                continue;
            };
            let relative_path = relative_source_path(&normalized, &input_root);
            let relative_string = path_to_string(&relative_path);
            if !source.include.is_empty() && !includes.is_match(&relative_string) {
                continue;
            }
            if excludes.is_match(&relative_string) {
                continue;
            }

            let bytes = read_file(&normalized)?;
            let guessed = image::guess_format(&bytes).map_err(|source| {
                DevimgError::image(
                    &normalized,
                    format!("could not determine image format: {source}"),
                )
            })?;
            if guessed != expected.image_format() {
                return Err(DevimgError::image(
                    &normalized,
                    format!(
                        "file extension suggests {}, but image bytes are {}",
                        expected.label(),
                        image_format_label(guessed)
                    ),
                ));
            }
            let (width, height) = image::image_dimensions(&normalized).map_err(|source| {
                DevimgError::image(&normalized, format!("could not read dimensions: {source}"))
            })?;

            images.push(SourceImage {
                source_name: source.name.clone(),
                project_path: path_to_string(&project_relative(config, &normalized)),
                relative_path,
                output_root_project_path: path_to_string(&project_relative(config, &output_root)),
                output_root: output_root.clone(),
                path: normalized,
                width,
                height,
                bytes: bytes.len() as u64,
                hash: hash_bytes(&bytes),
                format: expected,
            });
        }
    }
    images.sort_by(|left, right| left.project_path.cmp(&right.project_path));
    Ok(images)
}

pub fn inspect_image(path: &Path) -> Result<ImageInspection> {
    let bytes = read_file(path)?;
    let format = image::guess_format(&bytes)
        .map(image_format_label)
        .map_err(|source| {
            DevimgError::image(path, format!("could not determine format: {source}"))
        })?;
    let (width, height) = image::image_dimensions(path).map_err(|source| {
        DevimgError::image(path, format!("could not read dimensions: {source}"))
    })?;
    Ok(ImageInspection {
        path: path_to_string(path),
        width,
        height,
        format: format.to_string(),
        bytes: bytes.len() as u64,
        hash: hash_bytes(&bytes),
    })
}

fn collect_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|source| {
            let path = source.path().unwrap_or(dir).to_path_buf();
            let message = source.to_string();
            let io_error = source
                .into_io_error()
                .unwrap_or(std::io::Error::other(message));
            DevimgError::io(path, io_error)
        })?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    Ok(files)
}

fn source_input_format(path: &Path) -> Option<FormatKind> {
    let format = format_from_extension(path)?;
    if format.supports_source_input() {
        Some(format)
    } else {
        None
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(source) => Err(DevimgError::io(path, source)),
    }
}

fn relative_source_path(path: &Path, input_root: &Path) -> PathBuf {
    match path.strip_prefix(input_root) {
        Ok(relative_path) => relative_path.to_path_buf(),
        Err(_) => path.to_path_buf(),
    }
}

fn format_from_extension(path: &Path) -> Option<FormatKind> {
    FormatKind::parse(&path.extension()?.to_string_lossy())
}

fn image_format_label(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::WebP => "webp",
        ImageFormat::Avif => "avif",
        _ => "unsupported",
    }
}

fn path_is_under(path: &Path, parent: &Path) -> bool {
    let path = crate::config::normalize_path(path);
    let parent = crate::config::normalize_path(parent);
    !parent.as_os_str().is_empty() && path.starts_with(parent)
}

pub(crate) fn compile_globs(
    config_path: &Path,
    label: &str,
    patterns: &[String],
) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        add_glob(config_path, label, &mut builder, pattern)?;
        if let Some(root_pattern) = pattern.replace('\\', "/").strip_prefix("**/") {
            add_glob(config_path, label, &mut builder, root_pattern)?;
        }
    }
    build_glob_set(config_path, label, builder.build())
}

fn build_glob_set(
    config_path: &Path,
    label: &str,
    result: std::result::Result<GlobSet, globset::Error>,
) -> Result<GlobSet> {
    match result {
        Ok(globs) => Ok(globs),
        Err(source) => Err(invalid_glob_set_error(config_path, label, source)),
    }
}

fn invalid_glob_set_error(config_path: &Path, label: &str, source: globset::Error) -> DevimgError {
    DevimgError::config(config_path, format!("invalid {label} glob set: {source}"))
}

fn add_glob(
    config_path: &Path,
    label: &str,
    builder: &mut GlobSetBuilder,
    pattern: &str,
) -> Result<()> {
    let normalized = pattern.replace('\\', "/");
    let glob = GlobBuilder::new(&normalized)
        .case_insensitive(true)
        .build()
        .map_err(|source| {
            DevimgError::config(
                config_path,
                format!("invalid {label} glob `{pattern}`: {source}"),
            )
        })?;
    builder.add(glob);
    Ok(())
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
