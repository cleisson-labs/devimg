use super::{compare_manifests, manifest_compare_to_json, ManifestCompareOptions};
use crate::manifest::{Manifest, ManifestOutput};

#[test]
fn compare_manifest_records_added_removed_changed_and_unchanged_outputs() {
    let base = Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:base".to_string(),
        outputs: vec![
            output(
                "assets/card.png",
                "project-card",
                640,
                360,
                "webp",
                100,
                "same",
            ),
            output(
                "assets/card.png",
                "project-card",
                960,
                540,
                "webp",
                200,
                "old",
            ),
            output(
                "assets/card.png",
                "project-card",
                1280,
                720,
                "webp",
                300,
                "removed",
            ),
        ],
    };
    let mut changed = output(
        "assets/card.png",
        "project-card",
        960,
        540,
        "webp",
        260,
        "new",
    );
    changed.output_path = "public/images/generated/card.project-card.960.newhash.webp".to_string();
    changed.operation_hash = "blake3:new-operation".to_string();
    let head = Manifest {
        version: 1,
        generated_at: "unix:2".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:head".to_string(),
        outputs: vec![
            output(
                "assets/card.png",
                "project-card",
                640,
                360,
                "webp",
                100,
                "same",
            ),
            changed,
            output("assets/avatar.png", "avatar", 256, 256, "jpeg", 50, "added"),
        ],
    };

    let compare = compare_manifests(&base, &head, ManifestCompareOptions { top_limit: 2 });

    assert_eq!(compare.summary.base_variant_count, 3);
    assert_eq!(compare.summary.head_variant_count, 3);
    assert_eq!(compare.summary.variant_count_delta, 0);
    assert_eq!(compare.summary.base_output_bytes, 600);
    assert_eq!(compare.summary.head_output_bytes, 410);
    assert_eq!(compare.summary.output_bytes_delta, -190);
    assert_eq!(compare.summary.added_count, 1);
    assert_eq!(compare.summary.removed_count, 1);
    assert_eq!(compare.summary.changed_count, 1);
    assert_eq!(compare.summary.metadata_changed_count, 0);
    assert_eq!(compare.summary.unchanged_count, 1);
    assert_eq!(
        compare.added[0].output_path,
        "public/images/generated/avatar.avatar.256.jpeg"
    );
    assert_eq!(compare.removed[0].hash, "blake3:removed");
    assert_eq!(compare.changed[0].base_bytes, 200);
    assert_eq!(compare.changed[0].head_bytes, 260);
    assert_eq!(compare.changed[0].byte_delta, 60);
    assert_eq!(
        compare.changed[0].base_output_path,
        "public/images/generated/card.project-card.960.webp"
    );
    assert_eq!(
        compare.changed[0].head_output_path,
        "public/images/generated/card.project-card.960.newhash.webp"
    );
    assert_eq!(compare.top_byte_contributors.len(), 2);
    assert_eq!(compare.top_byte_contributors[0].bytes, 260);
    assert_eq!(compare.top_byte_contributors[1].bytes, 100);
}

#[test]
fn compare_manifest_separates_metadata_only_operation_changes() {
    let base = Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:base".to_string(),
        outputs: vec![output(
            "assets/card.png",
            "project-card",
            640,
            360,
            "webp",
            100,
            "same",
        )],
    };
    let mut head = base.clone();
    head.generated_at = "unix:2".to_string();
    head.config_hash = "blake3:head".to_string();
    head.outputs[0].operation_hash = "blake3:operation-new-metadata".to_string();

    let compare = compare_manifests(&base, &head, ManifestCompareOptions::default());

    assert_eq!(compare.summary.changed_count, 0);
    assert_eq!(compare.summary.metadata_changed_count, 1);
    assert_eq!(compare.summary.unchanged_count, 0);
    assert_eq!(
        compare.metadata_changed[0].output_path,
        base.outputs[0].output_path
    );
    assert_eq!(compare.metadata_changed[0].bytes, 100);
    assert_eq!(compare.metadata_changed[0].hash, "blake3:same");
}

#[test]
fn compare_manifest_json_is_machine_readable() {
    let manifest = Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:config".to_string(),
        outputs: vec![output(
            "assets/card.png",
            "project-card",
            640,
            360,
            "webp",
            100,
            "same",
        )],
    };

    let compare = compare_manifests(&manifest, &manifest, ManifestCompareOptions::default());
    let json = manifest_compare_to_json(&compare);

    assert!(json.contains("\"unchanged_count\": 1"));
    assert!(json.contains("\"metadata_changed_count\": 0"));
    assert!(json.contains("\"top_byte_contributors\""));
}

#[test]
fn top_byte_contributors_sort_equal_sizes_by_output_path() {
    let manifest = Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:config".to_string(),
        outputs: vec![
            {
                let mut output = output("assets/b.png", "card", 100, 100, "webp", 100, "z");
                output.output_path = "public/z.webp".to_string();
                output
            },
            {
                let mut output = output("assets/a.png", "card", 100, 100, "webp", 100, "a");
                output.output_path = "public/a.webp".to_string();
                output
            },
        ],
    };

    let contributors = super::top_byte_contributors(&manifest, 2);

    assert_eq!(contributors[0].output_path, "public/a.webp");
    assert_eq!(contributors[1].output_path, "public/z.webp");
}

fn output(
    source_path: &str,
    preset: &str,
    width: u32,
    height: u32,
    format: &str,
    bytes: u64,
    hash_suffix: &str,
) -> ManifestOutput {
    let stem = source_path
        .rsplit('/')
        .next()
        .expect("source filename")
        .split('.')
        .next()
        .expect("source stem");
    ManifestOutput {
        source_path: source_path.to_string(),
        source_hash: format!("blake3:source-{stem}"),
        source_width: 1600,
        source_height: 900,
        source_bytes: 1000,
        output_path: format!("public/images/generated/{stem}.{preset}.{width}.{format}"),
        preset: preset.to_string(),
        fit: "cover".to_string(),
        width,
        height,
        format: format.to_string(),
        bytes,
        hash: format!("blake3:{hash_suffix}"),
        operation_hash: format!("blake3:operation-{hash_suffix}"),
    }
}
