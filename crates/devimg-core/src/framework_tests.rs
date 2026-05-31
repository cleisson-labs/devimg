use std::fs;
use std::path::{Path, PathBuf};

use crate::config::parse_config;

use super::{first_component, inspect_frameworks};

#[test]
fn detects_frameworks_from_package_json_and_config_files() {
    let root = temp_project("framework_detect");
    fs::write(
        root.join("package.json"),
        r#"{"dependencies":{"next":"15.0.0","react":"19.0.0"},"devDependencies":{"vite":"7.0.0"}}"#,
    )
    .expect("package writes");
    fs::write(root.join("astro.config.mjs"), "export default {}\n").expect("astro config writes");
    let config = config(&root, "content_hash_filenames = false");

    let report = inspect_frameworks(&config, None);

    assert_eq!(report.frameworks, vec!["astro", "next", "vite"]);
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.code == "framework_multiple_detected"));
    cleanup(&root);
}

#[test]
fn invalid_package_json_is_ignored_for_framework_detection() {
    let root = temp_project("framework_invalid_package");
    fs::write(root.join("package.json"), "{not json").expect("package writes");
    let config = config(&root, "content_hash_filenames = false");

    let report = inspect_frameworks(&config, None);

    assert!(report.frameworks.is_empty());
    cleanup(&root);
}

#[test]
fn first_component_ignores_non_normal_path_components() {
    assert_eq!(first_component(Path::new("/")), None);
    assert_eq!(
        first_component(Path::new("/public/images")).as_deref(),
        Some("public")
    );
}

#[test]
fn warns_for_hashed_framework_outputs_without_export_check() {
    let root = temp_project("framework_export");
    fs::write(root.join("vite.config.ts"), "export default {}\n").expect("vite config writes");
    let config = config(&root, "content_hash_filenames = true");

    let without_export = inspect_frameworks(&config, None);
    let with_export = inspect_frameworks(&config, Some(Path::new("lib/other.ts")));

    assert!(without_export
        .warnings
        .iter()
        .any(|warning| warning.code == "framework_manifest_export_missing"));
    assert!(!with_export
        .warnings
        .iter()
        .any(|warning| warning.code == "framework_manifest_export_missing"));
    cleanup(&root);
}

#[test]
fn discovers_common_manifest_helpers() {
    let root = temp_project("framework_helpers");
    fs::create_dir_all(root.join("lib")).expect("lib creates");
    fs::create_dir_all(root.join("src/lib")).expect("src lib creates");
    fs::write(root.join("vite.config.ts"), "export default {}\n").expect("vite config writes");
    fs::write(root.join("lib/devimg.generated.ts"), "generated\n")
        .expect("generated helper writes");
    fs::write(root.join("lib/devimg-seo.generated.ts"), "seo generated\n")
        .expect("seo generated helper writes");
    fs::write(root.join("lib/devimg.ts"), "runtime\n").expect("runtime helper writes");
    fs::write(
        root.join("src/lib/devimg-gallery.generated.ts"),
        "gallery generated\n",
    )
    .expect("gallery generated helper writes");
    fs::write(root.join("src/lib/devimg.ts"), "src runtime\n").expect("src helper writes");
    let config = config(&root, "content_hash_filenames = true");

    let report = inspect_frameworks(&config, None);

    assert_eq!(
        report.manifest_helpers,
        vec![
            "lib/devimg-seo.generated.ts",
            "lib/devimg.generated.ts",
            "lib/devimg.ts",
            "src/lib/devimg-gallery.generated.ts",
            "src/lib/devimg.ts"
        ]
    );
    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.code == "framework_manifest_helper_unchecked"));
    assert!(!report
        .warnings
        .iter()
        .any(|warning| warning.code == "framework_manifest_export_missing"));
    cleanup(&root);
}

#[test]
fn absolute_export_output_matches_discovered_helper() {
    let root = temp_project("framework_helper_match");
    fs::create_dir_all(root.join("lib")).expect("lib creates");
    fs::write(root.join("vite.config.ts"), "export default {}\n").expect("vite config writes");
    fs::write(root.join("lib/devimg.generated.ts"), "generated\n")
        .expect("generated helper writes");
    let config = config(&root, "content_hash_filenames = true");
    let export_output = root.join("lib/devimg.generated.ts");

    let report = inspect_frameworks(&config, Some(&export_output));

    assert_eq!(report.manifest_helpers, vec!["lib/devimg.generated.ts"]);
    assert!(!report.warnings.iter().any(|warning| {
        warning.code == "framework_manifest_helper_unchecked"
            || warning.code == "framework_manifest_export_missing"
    }));
    cleanup(&root);
}

#[test]
fn named_export_output_matches_discovered_generated_helper() {
    let root = temp_project("framework_named_helper_match");
    fs::create_dir_all(root.join("lib")).expect("lib creates");
    fs::write(root.join("vite.config.ts"), "export default {}\n").expect("vite config writes");
    fs::write(root.join("lib/devimg.generated.ts"), "generated\n")
        .expect("generated helper writes");
    fs::write(root.join("lib/devimg-seo.generated.ts"), "seo generated\n")
        .expect("seo generated helper writes");
    fs::write(root.join("lib/devimg.ts"), "runtime\n").expect("runtime helper writes");
    let config = config(&root, "content_hash_filenames = true");
    let export_output = PathBuf::from("lib/devimg-seo.generated.ts");

    let report = inspect_frameworks(&config, Some(&export_output));

    assert_eq!(
        report.manifest_helpers,
        vec![
            "lib/devimg-seo.generated.ts",
            "lib/devimg.generated.ts",
            "lib/devimg.ts"
        ]
    );
    assert!(!report.warnings.iter().any(|warning| {
        warning.code == "framework_manifest_helper_unchecked"
            || warning.code == "framework_manifest_export_missing"
    }));
    cleanup(&root);
}

#[test]
fn warns_when_configured_export_does_not_match_discovered_helper() {
    let root = temp_project("framework_helper_mismatch");
    fs::create_dir_all(root.join("lib")).expect("lib creates");
    fs::write(root.join("vite.config.ts"), "export default {}\n").expect("vite config writes");
    fs::write(root.join("lib/devimg.generated.ts"), "generated\n")
        .expect("generated helper writes");
    let config = config(&root, "content_hash_filenames = true");
    let other_output = PathBuf::from("lib/other.ts");

    let report = inspect_frameworks(&config, Some(&other_output));

    assert!(report
        .warnings
        .iter()
        .any(|warning| warning.code == "framework_manifest_helper_unchecked"));
    cleanup(&root);
}

#[test]
fn next_public_output_warning_explains_static_assets_and_cdn_behavior() {
    let root = temp_project("framework_next_copy");
    fs::write(root.join("next.config.mjs"), "export default {}\n").expect("next config writes");
    let config = config(&root, "content_hash_filenames = true");

    let report = inspect_frameworks(&config, Some(Path::new("lib/devimg.generated.ts")));
    let warning = report
        .warnings
        .iter()
        .find(|warning| warning.code == "framework_next_image_double_optimization")
        .expect("next warning exists");

    assert!(warning.message.contains("static assets"));
    assert!(warning.message.contains("Vercel/CDNs"));
    assert!(warning.hint.contains("unoptimized"));
    cleanup(&root);
}

#[test]
fn no_framework_project_has_no_framework_warnings() {
    let root = temp_project("framework_none");
    let config = config(&root, "content_hash_filenames = false");

    let report = inspect_frameworks(&config, None);

    assert!(report.frameworks.is_empty());
    assert!(report.manifest_helpers.is_empty());
    assert!(report.warnings.is_empty());
    cleanup(&root);
}

fn config(root: &Path, project_setting: &str) -> crate::Config {
    let raw = format!(
        r#"[project]
root = "."
manifest = "public/images/devimg-manifest.json"
report = "devimg-report.md"
{project_setting}

[[sources]]
name = "app"
input = "assets/images"
output = "public/images/generated"
include = ["**/*.png"]

[[preset]]
name = "project-card"
widths = [64]
formats = ["webp"]
quality = 82
fit = "cover"
aspect_ratio = "16:9"
"#
    );
    parse_config(&root.join("devimg.toml"), &raw).expect("config parses")
}

fn temp_project(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "devimg_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    fs::create_dir_all(&path).expect("project creates");
    path
}

fn cleanup(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).expect("cleanup temp project");
    }
}
