use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use crate::config::{parse_config, FormatKind};

use super::{
    build_glob_set, collect_files, compile_globs, image_format_label, invalid_glob_set_error,
    path_is_under, relative_source_path, scan_sources, source_input_format,
};

#[test]
fn globstar_patterns_match_root_nested_and_case_insensitive_paths() {
    let globs = compile_globs(
        Path::new("devimg.toml"),
        "include",
        &[String::from("**/*.PNG")],
    )
    .expect("glob compiles");

    assert!(globs.is_match("card.png"));
    assert!(globs.is_match("nested/card.png"));
    assert!(globs.is_match("NESTED/CARD.PNG"));
    assert!(!globs.is_match("card.jpg"));
}

#[test]
fn invalid_glob_reports_label_and_pattern() {
    let error = compile_globs(Path::new("devimg.toml"), "exclude", &[String::from("[")])
        .expect_err("glob should fail");

    let rendered = error.to_string();
    assert!(rendered.contains("invalid exclude glob `[`"));
}

#[test]
fn invalid_glob_set_error_formats_label() {
    let source = globset::GlobBuilder::new("[")
        .build()
        .expect_err("glob parse error");
    let error = invalid_glob_set_error(Path::new("devimg.toml"), "include", source);

    assert!(error.to_string().contains("invalid include glob set"));
}

#[test]
fn build_glob_set_wraps_builder_errors() {
    let source = globset::GlobBuilder::new("[")
        .build()
        .expect_err("glob parse error");
    let error = build_glob_set(Path::new("devimg.toml"), "include", Err(source))
        .expect_err("builder error should be wrapped");

    assert!(error.to_string().contains("invalid include glob set"));
}

#[test]
fn supported_source_extensions_exclude_avif_and_unknown_extensions() {
    assert_eq!(
        source_input_format(Path::new("card.PNG")),
        Some(FormatKind::Png)
    );
    assert_eq!(
        source_input_format(Path::new("card.jpeg")),
        Some(FormatKind::Jpeg)
    );
    assert_eq!(
        source_input_format(Path::new("card.webp")),
        Some(FormatKind::Webp)
    );
    assert_eq!(source_input_format(Path::new("card.avif")), None);
    assert_eq!(source_input_format(Path::new("card.gif")), None);
    assert_eq!(source_input_format(Path::new("card")), None);
}

#[test]
fn image_format_labels_unsupported_formats_explicitly() {
    assert_eq!(image_format_label(FormatKind::Png.image_format()), "png");
    assert_eq!(image_format_label(FormatKind::Jpeg.image_format()), "jpeg");
    assert_eq!(image_format_label(FormatKind::Webp.image_format()), "webp");
    assert_eq!(image_format_label(FormatKind::Avif.image_format()), "avif");
    assert_eq!(image_format_label(ImageFormat::Gif), "unsupported");
}

#[test]
fn path_is_under_rejects_empty_parent_and_normalizes_paths() {
    assert!(!path_is_under(
        Path::new("assets/generated/card.png"),
        Path::new("")
    ));
    assert!(path_is_under(
        Path::new("assets/images/../generated/card.png"),
        Path::new("assets/generated")
    ));
}

#[test]
fn relative_source_path_strips_input_root_or_falls_back_to_path() {
    assert_eq!(
        relative_source_path(
            Path::new("/project/assets/images/card.png"),
            Path::new("/project/assets/images")
        ),
        PathBuf::from("card.png")
    );
    assert_eq!(
        relative_source_path(
            Path::new("/project/other/card.png"),
            Path::new("/project/assets/images")
        ),
        PathBuf::from("/project/other/card.png")
    );
}

#[test]
fn scan_sources_rejects_image_bytes_that_do_not_match_extension() {
    let project = TempProject::new("mismatched-extension");
    let input = project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input directory created");
    write_png_bytes(&input.join("not-jpeg.jpg"));
    let config = parse_config(
        &project.root.join("devimg.toml"),
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

    let error = scan_sources(&config).expect_err("scan should reject mismatched bytes");
    let rendered = error.to_string();
    assert!(rendered.contains("file extension suggests jpeg, but image bytes are png"));
}

#[test]
fn scan_sources_reports_missing_and_non_directory_inputs() {
    let missing_project = TempProject::new("missing-input");
    let missing_config = parse_config(
        &missing_project.root.join("devimg.toml"),
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
    let missing = scan_sources(&missing_config).expect_err("missing input should fail");
    assert!(missing.to_string().contains("input does not exist"));

    let file_project = TempProject::new("file-input");
    let input_file = file_project.root.join("assets/images");
    fs::create_dir_all(input_file.parent().expect("parent")).expect("parent created");
    fs::write(&input_file, b"not a directory").expect("input file written");
    let file_config = parse_config(
        &file_project.root.join("devimg.toml"),
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
    let file_error = scan_sources(&file_config).expect_err("file input should fail");
    assert!(file_error.to_string().contains("input is not a directory"));
}

#[test]
fn scan_sources_filters_supported_images_and_inspects_image_metadata() {
    let project = TempProject::new("filters");
    let input = project.root.join("assets/images");
    let nested = input.join("nested");
    let generated = input.join("generated");
    fs::create_dir_all(&nested).expect("nested created");
    fs::create_dir_all(&generated).expect("generated created");
    let card = input.join("card.png");
    write_png_bytes(&card);
    write_png_bytes(&nested.join("ignored.png"));
    write_png_bytes(&generated.join("generated.png"));
    fs::write(input.join("notes.txt"), b"not an image").expect("text written");
    fs::write(input.join("other.webp"), b"skipped before read").expect("webp placeholder written");

    let config = parse_config(
        &project.root.join("devimg.toml"),
        r#"
[project]
manifest = "public/images/devimg-manifest.json"

[[sources]]
name = "portfolio"
input = "assets/images"
output = "assets/images/generated"
include = ["**/*.png"]
exclude = ["nested/*"]

[[preset]]
name = "card"
widths = [100]
formats = ["webp"]
"#,
    )
    .expect("config parses");

    let sources = scan_sources(&config).expect("scan succeeds");
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].source_name, "portfolio");
    assert_eq!(sources[0].project_path, "assets/images/card.png");
    assert_eq!(sources[0].relative_path, PathBuf::from("card.png"));
    assert_eq!(sources[0].format, FormatKind::Png);

    let inspected = super::inspect_image(&card).expect("image inspected");
    assert_eq!((inspected.width, inspected.height), (2, 2));
    assert_eq!(inspected.format, "png");
    assert!(inspected.path.ends_with("assets/images/card.png"));

    let text_error = super::inspect_image(&input.join("notes.txt")).expect_err("text is invalid");
    assert!(text_error
        .to_string()
        .contains("could not determine format"));
}

#[test]
fn inspect_image_reports_dimension_errors_after_format_detection() {
    let project = TempProject::new("inspect-bad-dimensions");
    let broken_png = project.root.join("broken.png");
    fs::write(&broken_png, b"\x89PNG\r\n\x1a\n\0\0").expect("truncated png written");

    let error = super::inspect_image(&broken_png).expect_err("dimensions should fail");
    assert!(error.to_string().contains("could not read dimensions"));
}

#[test]
fn inspect_image_reports_file_read_errors() {
    let project = TempProject::new("inspect-missing-file");
    let missing = project.root.join("missing.png");

    let error = super::inspect_image(&missing).expect_err("missing image should fail");
    assert!(error.to_string().contains("I/O error"));
}

#[cfg(unix)]
#[test]
fn collect_files_reports_walkdir_io_errors() {
    use std::os::unix::fs::PermissionsExt;

    let project = TempProject::new("walkdir-error");
    let blocked = project.root.join("blocked");
    fs::create_dir_all(&blocked).expect("blocked directory created");
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o000))
        .expect("blocked permissions set");

    let result = collect_files(&project.root);
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700))
        .expect("blocked permissions restored");
    let error = result.expect_err("unreadable directory should fail");

    assert!(error.to_string().contains("I/O error"));
}

#[test]
fn scan_sources_reports_dimension_decode_errors_after_format_guess() {
    let project = TempProject::new("bad-dimensions");
    let input = project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    fs::write(input.join("truncated.png"), b"\x89PNG\r\n\x1a\n\0\0")
        .expect("truncated png written");
    let config = parse_config(
        &project.root.join("devimg.toml"),
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

    let error = scan_sources(&config).expect_err("dimension decode should fail");
    assert!(error.to_string().contains("could not read dimensions"));
}

#[test]
fn scan_sources_reports_format_guess_errors_for_unrecognized_image_bytes() {
    let project = TempProject::new("bad-format");
    let input = project.root.join("assets/images");
    fs::create_dir_all(&input).expect("input created");
    fs::write(input.join("not-really.png"), b"plain text").expect("invalid png written");
    let config = parse_config(
        &project.root.join("devimg.toml"),
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

    let error = scan_sources(&config).expect_err("format guess should fail");
    assert!(error
        .to_string()
        .contains("could not determine image format"));
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
            "devimg-scan-tests-{name}-{}-{unique}",
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

fn write_png_bytes(path: &Path) {
    let mut image = RgbaImage::new(2, 2);
    for pixel in image.pixels_mut() {
        *pixel = Rgba([10, 20, 30, 255]);
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("png encoded");
    fs::write(path, bytes.into_inner()).expect("png bytes written");
}
