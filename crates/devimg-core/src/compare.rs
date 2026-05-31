use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::Serialize;

use crate::manifest::{Manifest, ManifestOutput};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestCompareOptions {
    pub top_limit: usize,
}

impl Default for ManifestCompareOptions {
    fn default() -> Self {
        Self { top_limit: 5 }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManifestCompare {
    pub base_generated_at: String,
    pub head_generated_at: String,
    pub base_config_hash: String,
    pub head_config_hash: String,
    pub summary: ManifestCompareSummary,
    pub added: Vec<ManifestCompareOutput>,
    pub removed: Vec<ManifestCompareOutput>,
    pub changed: Vec<ManifestCompareChange>,
    pub metadata_changed: Vec<ManifestCompareMetadataChange>,
    pub top_byte_contributors: Vec<ManifestCompareOutput>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManifestCompareSummary {
    pub base_variant_count: u64,
    pub head_variant_count: u64,
    pub variant_count_delta: i64,
    pub base_output_bytes: u64,
    pub head_output_bytes: u64,
    pub output_bytes_delta: i64,
    pub added_count: u64,
    pub removed_count: u64,
    pub changed_count: u64,
    pub metadata_changed_count: u64,
    pub unchanged_count: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManifestCompareOutput {
    pub source_path: String,
    pub output_path: String,
    pub preset: String,
    pub fit: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub bytes: u64,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManifestCompareChange {
    pub source_path: String,
    pub preset: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub base_fit: String,
    pub head_fit: String,
    pub base_output_path: String,
    pub head_output_path: String,
    pub base_bytes: u64,
    pub head_bytes: u64,
    pub byte_delta: i64,
    pub base_hash: String,
    pub head_hash: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManifestCompareMetadataChange {
    pub source_path: String,
    pub preset: String,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub output_path: String,
    pub bytes: u64,
    pub hash: String,
    pub base_operation_hash: String,
    pub head_operation_hash: String,
}

pub fn compare_manifests(
    base: &Manifest,
    head: &Manifest,
    options: ManifestCompareOptions,
) -> ManifestCompare {
    let base_outputs = outputs_by_identity(base);
    let mut head_outputs = outputs_by_identity(head);
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut changed = Vec::new();
    let mut metadata_changed = Vec::new();
    let mut unchanged_count = 0u64;

    for (identity, base_output) in base_outputs {
        match head_outputs.remove(&identity) {
            Some(head_output) => {
                if outputs_match(base_output, head_output) {
                    unchanged_count += 1;
                } else if outputs_match_except_operation_hash(base_output, head_output) {
                    metadata_changed.push(ManifestCompareMetadataChange::from_outputs(
                        base_output,
                        head_output,
                    ));
                } else {
                    changed.push(ManifestCompareChange::from_outputs(
                        base_output,
                        head_output,
                    ));
                }
            }
            None => removed.push(ManifestCompareOutput::from_output(base_output)),
        }
    }

    for head_output in head_outputs.into_values() {
        added.push(ManifestCompareOutput::from_output(head_output));
    }

    let base_variant_count = base.outputs.len() as u64;
    let head_variant_count = head.outputs.len() as u64;
    let base_output_bytes = base.output_bytes_total();
    let head_output_bytes = head.output_bytes_total();
    let top_byte_contributors = top_byte_contributors(head, options.top_limit);

    ManifestCompare {
        base_generated_at: base.generated_at.clone(),
        head_generated_at: head.generated_at.clone(),
        base_config_hash: base.config_hash.clone(),
        head_config_hash: head.config_hash.clone(),
        summary: ManifestCompareSummary {
            base_variant_count,
            head_variant_count,
            variant_count_delta: signed_delta(head_variant_count, base_variant_count),
            base_output_bytes,
            head_output_bytes,
            output_bytes_delta: signed_delta(head_output_bytes, base_output_bytes),
            added_count: added.len() as u64,
            removed_count: removed.len() as u64,
            changed_count: changed.len() as u64,
            metadata_changed_count: metadata_changed.len() as u64,
            unchanged_count,
        },
        added,
        removed,
        changed,
        metadata_changed,
        top_byte_contributors,
    }
}

pub fn manifest_compare_to_json(compare: &ManifestCompare) -> String {
    serde_json::to_string_pretty(compare).expect("manifest compare serialization cannot fail")
        + "\n"
}

impl ManifestCompareOutput {
    fn from_output(output: &ManifestOutput) -> Self {
        Self {
            source_path: output.source_path.clone(),
            output_path: output.output_path.clone(),
            preset: output.preset.clone(),
            fit: output.fit.clone(),
            width: output.width,
            height: output.height,
            format: output.format.clone(),
            bytes: output.bytes,
            hash: output.hash.clone(),
        }
    }
}

impl ManifestCompareChange {
    fn from_outputs(base: &ManifestOutput, head: &ManifestOutput) -> Self {
        Self {
            source_path: head.source_path.clone(),
            preset: head.preset.clone(),
            width: head.width,
            height: head.height,
            format: head.format.clone(),
            base_fit: base.fit.clone(),
            head_fit: head.fit.clone(),
            base_output_path: base.output_path.clone(),
            head_output_path: head.output_path.clone(),
            base_bytes: base.bytes,
            head_bytes: head.bytes,
            byte_delta: signed_delta(head.bytes, base.bytes),
            base_hash: base.hash.clone(),
            head_hash: head.hash.clone(),
        }
    }
}

impl ManifestCompareMetadataChange {
    fn from_outputs(base: &ManifestOutput, head: &ManifestOutput) -> Self {
        Self {
            source_path: head.source_path.clone(),
            preset: head.preset.clone(),
            width: head.width,
            height: head.height,
            format: head.format.clone(),
            output_path: head.output_path.clone(),
            bytes: head.bytes,
            hash: head.hash.clone(),
            base_operation_hash: base.operation_hash.clone(),
            head_operation_hash: head.operation_hash.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ManifestOutputIdentity {
    source_path: String,
    preset: String,
    width: u32,
    height: u32,
    format: String,
}

impl ManifestOutputIdentity {
    fn from_output(output: &ManifestOutput) -> Self {
        Self {
            source_path: output.source_path.clone(),
            preset: output.preset.clone(),
            width: output.width,
            height: output.height,
            format: output.format.clone(),
        }
    }
}

fn outputs_by_identity(manifest: &Manifest) -> BTreeMap<ManifestOutputIdentity, &ManifestOutput> {
    manifest
        .outputs
        .iter()
        .map(|output| (ManifestOutputIdentity::from_output(output), output))
        .collect()
}

fn outputs_match(base: &ManifestOutput, head: &ManifestOutput) -> bool {
    outputs_match_except_operation_hash(base, head) && base.operation_hash == head.operation_hash
}

fn outputs_match_except_operation_hash(base: &ManifestOutput, head: &ManifestOutput) -> bool {
    base.source_hash == head.source_hash
        && base.source_width == head.source_width
        && base.source_height == head.source_height
        && base.source_bytes == head.source_bytes
        && base.output_path == head.output_path
        && base.fit == head.fit
        && base.bytes == head.bytes
        && base.hash == head.hash
}

fn top_byte_contributors(manifest: &Manifest, limit: usize) -> Vec<ManifestCompareOutput> {
    let mut outputs = manifest.outputs.iter().collect::<Vec<_>>();
    outputs.sort_by(|a, b| match b.bytes.cmp(&a.bytes) {
        Ordering::Equal => a.output_path.cmp(&b.output_path),
        ordering => ordering,
    });
    outputs
        .into_iter()
        .take(limit)
        .map(ManifestCompareOutput::from_output)
        .collect()
}

fn signed_delta(head: u64, base: u64) -> i64 {
    if head >= base {
        head.saturating_sub(base).min(i64::MAX as u64) as i64
    } else {
        -(base.saturating_sub(head).min(i64::MAX as u64) as i64)
    }
}

#[cfg(test)]
#[path = "compare_tests.rs"]
mod tests;
