use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::{project_relative, resolve_project_path, Config};
use crate::pipeline::path_to_string;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameworkInspection {
    pub frameworks: Vec<String>,
    pub manifest_helpers: Vec<String>,
    pub warnings: Vec<FrameworkWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameworkWarning {
    pub code: String,
    pub path: String,
    pub message: String,
    pub hint: String,
}

pub(crate) fn inspect_frameworks(
    config: &Config,
    manifest_export_output: Option<&Path>,
) -> FrameworkInspection {
    let detected = detect_frameworks(&config.project.root);
    let frameworks = detected.keys().cloned().collect::<Vec<_>>();
    let manifest_helpers = detect_manifest_helpers(config);
    let mut warnings = Vec::new();
    let public_output = has_public_output(config);
    let checked_helper = manifest_export_output.is_some();
    let helper_checked = manifest_export_output
        .and_then(|output| matching_helper(config, &manifest_helpers, output))
        .is_some();

    if frameworks.len() > 1 {
        warnings.push(warning(
            "framework_multiple_detected",
            &config.path,
            format!(
                "multiple frontend frameworks were detected: {}",
                frameworks.join(", ")
            ),
            "Review generated image paths and manifest export usage for the actual app entrypoint.",
        ));
    }

    if frameworks.iter().any(|framework| framework == "next") && public_output {
        warnings.push(warning(
            "framework_next_image_double_optimization",
            &config.path,
            "Next.js was detected with generated assets under public/. These files are static assets that Vercel/CDNs can cache directly, but next/image may optimize them again if they are rendered through Image.",
            "Use generated variants through img/picture or set unoptimized on Next Image when DevImg owns resizing and quality. If Next Image optimization is intentional, keep DevImg focused on deterministic source variants, content hashes, budgets, and manifest checks.",
        ));
    }

    if !frameworks.is_empty() && public_output && !config.project.content_hash_filenames {
        warnings.push(warning(
            "framework_cache_without_hash",
            &config.path,
            "frontend framework project outputs generated assets under public/ without content-hash filenames",
            "Enable [project].content_hash_filenames before using long-lived immutable caching for generated assets.",
        ));
    }

    if !frameworks.is_empty() && config.project.content_hash_filenames && !checked_helper {
        if manifest_helpers.is_empty() {
            warnings.push(warning(
                "framework_manifest_export_missing",
                &config.path,
                "content-hash filenames are enabled in a framework project, but no checked manifest helper was configured or discovered",
                "If the app consumes generated paths from a helper, generate one with devimg manifest export and pass --export-output to doctor in CI.",
            ));
        } else {
            warnings.push(warning(
                "framework_manifest_helper_unchecked",
                &config.path,
                format!(
                    "content-hash filenames are enabled and helper file(s) were discovered, but doctor was not given --export-output to verify them: {}",
                    manifest_helpers.join(", ")
                ),
                "Pass --export-output with the generated helper path and the same options used by devimg manifest export.",
            ));
        }
    } else if !frameworks.is_empty()
        && config.project.content_hash_filenames
        && checked_helper
        && !manifest_helpers.is_empty()
        && !helper_checked
    {
        warnings.push(warning(
            "framework_manifest_helper_unchecked",
            &config.path,
            format!(
                "helper file(s) were discovered but the configured --export-output does not match them: {}",
                manifest_helpers.join(", ")
            ),
            "Verify every checked-in helper that contains generated DevImg URLs, or remove stale helper files.",
        ));
    }

    FrameworkInspection {
        frameworks,
        manifest_helpers,
        warnings,
    }
}

fn detect_frameworks(root: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let mut detected = BTreeMap::<String, BTreeSet<String>>::new();
    detect_config_files(root, &mut detected);
    detect_package_json(root, &mut detected);
    detected
}

fn detect_config_files(root: &Path, detected: &mut BTreeMap<String, BTreeSet<String>>) {
    for file in [
        "next.config.js",
        "next.config.mjs",
        "next.config.cjs",
        "next.config.ts",
    ] {
        detect_file(root, file, "next", detected);
    }
    for file in ["astro.config.js", "astro.config.mjs", "astro.config.ts"] {
        detect_file(root, file, "astro", detected);
    }
    for file in [
        "vite.config.js",
        "vite.config.mjs",
        "vite.config.cjs",
        "vite.config.ts",
    ] {
        detect_file(root, file, "vite", detected);
    }
}

fn detect_file(
    root: &Path,
    file: &str,
    framework: &str,
    detected: &mut BTreeMap<String, BTreeSet<String>>,
) {
    if root.join(file).exists() {
        detected
            .entry(framework.to_string())
            .or_default()
            .insert(file.to_string());
    }
}

fn detect_package_json(root: &Path, detected: &mut BTreeMap<String, BTreeSet<String>>) {
    let path = root.join("package.json");
    let Ok(raw) = fs::read_to_string(&path) else {
        return;
    };
    let Ok(document) = serde_json::from_str::<Value>(&raw) else {
        return;
    };
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        let Some(deps) = document.get(section).and_then(Value::as_object) else {
            continue;
        };
        for (package, _value) in deps {
            match package.as_str() {
                "next" => add_dependency(detected, "next", package),
                "astro" => add_dependency(detected, "astro", package),
                "vite" => add_dependency(detected, "vite", package),
                _ => {}
            }
        }
    }
}

fn add_dependency(
    detected: &mut BTreeMap<String, BTreeSet<String>>,
    framework: &str,
    package: &str,
) {
    detected
        .entry(framework.to_string())
        .or_default()
        .insert(format!("package:{package}"));
}

fn detect_manifest_helpers(config: &Config) -> Vec<String> {
    let mut helpers = BTreeSet::new();
    for candidate in [
        "lib/devimg.generated.ts",
        "lib/devimg.ts",
        "src/lib/devimg.generated.ts",
        "src/lib/devimg.ts",
    ] {
        add_manifest_helper_if_file(config, &mut helpers, config.project.root.join(candidate));
    }
    for directory in ["lib", "src/lib"] {
        let path = config.project.root.join(directory);
        let Ok(entries) = fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let helper_path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            if helper_path.is_file()
                && file_name.starts_with("devimg-")
                && file_name.ends_with(".generated.ts")
            {
                add_manifest_helper_if_file(config, &mut helpers, helper_path);
            }
        }
    }
    helpers.into_iter().collect()
}

fn add_manifest_helper_if_file(config: &Config, helpers: &mut BTreeSet<String>, path: PathBuf) {
    if path.is_file() {
        helpers.insert(path_to_string(&project_relative(config, &path)));
    }
}

fn matching_helper<'a>(
    config: &Config,
    helpers: &'a [String],
    export_output: &Path,
) -> Option<&'a String> {
    let resolved = resolve_project_path(config, export_output);
    let relative = path_to_string(&project_relative(config, &resolved));
    helpers.iter().find(|helper| **helper == relative)
}

fn has_public_output(config: &Config) -> bool {
    config.sources.iter().any(|source| {
        first_component(&source.output).is_some_and(|component| component == "public")
    })
}

fn first_component(path: &Path) -> Option<String> {
    path.components().find_map(|component| match component {
        std::path::Component::Normal(value) => Some(value.to_string_lossy().to_string()),
        _ => None,
    })
}

fn warning(
    code: &str,
    path: impl Into<PathBuf>,
    message: impl Into<String>,
    hint: impl Into<String>,
) -> FrameworkWarning {
    FrameworkWarning {
        code: code.to_string(),
        path: path_to_string(&path.into()),
        message: message.into(),
        hint: hint.into(),
    }
}

#[cfg(test)]
#[path = "framework_tests.rs"]
mod tests;
