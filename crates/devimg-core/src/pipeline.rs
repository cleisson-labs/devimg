use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::budget::evaluate_budgets;
pub use crate::check::{check, check_with_options, CheckOptions};
use crate::config::{resolve_project_path_checked, Config, CropPosition, FitMode, FormatKind};
use crate::incremental::{IncrementalCache, IncrementalLookup};
use crate::manifest::{write_manifest, Manifest};
pub use crate::plan::build_plan;
use crate::quality::{append_unique, manifest_quality_warnings};
use crate::report::render_run_report;
pub use crate::scan::{inspect_image, scan_sources};
use crate::transform::execute_operation;
use crate::warnings::{split_acknowledged_warnings, warning_message, BUDGET_FAILED};
use crate::{DevimgError, Result};

#[derive(Debug, Clone)]
pub struct SourceImage {
    pub source_name: String,
    pub path: PathBuf,
    pub project_path: String,
    pub relative_path: PathBuf,
    pub output_root: PathBuf,
    pub output_root_project_path: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub hash: String,
    pub format: FormatKind,
}

#[derive(Debug, Clone)]
pub struct Operation {
    pub source: SourceImage,
    pub preset: String,
    pub fit: FitMode,
    pub crop: CropPosition,
    pub quality: u8,
    pub format: FormatKind,
    pub width: u32,
    pub height: u32,
    pub strip_metadata: bool,
    pub content_hash_filenames: bool,
    pub output_path: PathBuf,
    pub output_project_path: String,
    pub operation_hash: String,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub operations: Vec<Operation>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OptimizeOptions {
    pub dry_run: bool,
    pub allow_overwrite: bool,
}

#[derive(Debug, Clone)]
pub struct OptimizeResult {
    pub mode: String,
    pub source_count: usize,
    pub planned_count: usize,
    pub generated_count: usize,
    pub skipped_count: usize,
    pub stale_count: usize,
    pub source_bytes: u64,
    pub output_bytes: u64,
    pub warnings: Vec<String>,
    pub acknowledged_warnings: Vec<String>,
    pub issues: Vec<CheckIssue>,
    pub budget_status: String,
    pub manifest: Manifest,
}

#[derive(Debug, Clone)]
pub struct CheckResult {
    pub passed: bool,
    pub result: OptimizeResult,
}

#[derive(Debug, Clone)]
pub struct CheckIssue {
    pub kind: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ImageInspection {
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub bytes: u64,
    pub hash: String,
}

pub fn optimize(config: &Config, options: OptimizeOptions) -> Result<OptimizeResult> {
    let sources = scan_sources(config)?;
    let plan = build_plan(config, &sources)?;
    let source_bytes = unique_source_bytes(&sources);
    let mode = if options.dry_run {
        "dry-run"
    } else {
        "optimize"
    }
    .to_string();

    if options.dry_run {
        let warning_groups = split_acknowledged_warnings(config, plan.warnings);
        let result = OptimizeResult {
            mode,
            source_count: sources.len(),
            planned_count: plan.operations.len(),
            generated_count: 0,
            skipped_count: 0,
            stale_count: 0,
            source_bytes,
            output_bytes: 0,
            warnings: warning_groups.active,
            acknowledged_warnings: warning_groups.acknowledged,
            issues: Vec::new(),
            budget_status: "not evaluated during dry-run".to_string(),
            manifest: Manifest::new(
                path_to_string(&config.path),
                config.config_hash.clone(),
                Vec::new(),
            ),
        };
        return Ok(result);
    }

    let manifest_path =
        resolve_project_path_checked(config, &config.project.manifest, "manifest path")?;
    let _report_path = resolve_project_path_checked(config, &config.project.report, "report path")?;

    let mut outputs = Vec::new();
    let mut warnings = plan.warnings;
    let incremental_cache = IncrementalCache::read(config, &manifest_path);
    let mut generated_count = 0usize;
    let mut skipped_count = 0usize;
    let mut stale_count = 0usize;
    for operation in &plan.operations {
        if let Some(cache) = &incremental_cache {
            match cache.lookup_current(config, operation)? {
                IncrementalLookup::Current(output) => {
                    skipped_count += 1;
                    outputs.push(*output);
                    continue;
                }
                IncrementalLookup::Stale => {
                    stale_count += 1;
                }
            }
        }
        outputs.push(execute_operation(
            operation,
            config.project.overwrite || options.allow_overwrite,
        )?);
        generated_count += 1;
    }

    let manifest = Manifest::new(
        path_to_string(&config.path),
        config.config_hash.clone(),
        outputs,
    );
    let (budget_status, issues) = evaluate_budgets(config, &manifest.outputs);
    let output_bytes = manifest.output_bytes_total();
    append_unique(&mut warnings, manifest_quality_warnings(&manifest));
    if !issues.is_empty() {
        warnings.push(warning_message(
            BUDGET_FAILED,
            format!(
                "budget status is fail with {} issue(s); `devimg check` will fail",
                issues.len()
            ),
        ));
    }
    let warning_groups = split_acknowledged_warnings(config, warnings);

    let result = OptimizeResult {
        mode,
        source_count: sources.len(),
        planned_count: plan.operations.len(),
        generated_count,
        skipped_count,
        stale_count,
        source_bytes,
        output_bytes,
        warnings: warning_groups.active,
        acknowledged_warnings: warning_groups.acknowledged,
        issues,
        budget_status,
        manifest,
    };

    write_manifest(&manifest_path, &result.manifest)?;
    write_report(config, &result)?;
    Ok(result)
}

pub(crate) fn write_report(config: &Config, result: &OptimizeResult) -> Result<()> {
    let report_path = resolve_project_path_checked(config, &config.project.report, "report path")?;
    report_path.parent().map_or(Ok(()), |parent| {
        fs::create_dir_all(parent).map_err(|source| DevimgError::io(parent, source))
    })?;
    write_report_markdown(&report_path, render_run_report(result))
}

fn write_report_markdown(path: &Path, markdown: String) -> Result<()> {
    match fs::write(path, markdown) {
        Ok(()) => Ok(()),
        Err(source) => Err(DevimgError::io(path, source)),
    }
}

pub(crate) fn unique_source_bytes(sources: &[SourceImage]) -> u64 {
    let mut by_path = BTreeMap::new();
    for source in sources {
        by_path.insert(source.project_path.as_str(), source.bytes);
    }
    by_path.values().sum()
}

pub(crate) fn path_to_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

#[cfg(test)]
#[path = "pipeline_tests.rs"]
mod tests;
