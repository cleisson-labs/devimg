use super::{render_manifest_review, ManifestReviewOptions};
use crate::manifest::{Manifest, ManifestOutput};

#[test]
fn review_html_escapes_manifest_text() {
    let manifest = Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "dev\"img.toml".to_string(),
        config_hash: "blake3:<config>&\"".to_string(),
        outputs: vec![ManifestOutput {
            source_path: "assets/<card>&\".png".to_string(),
            source_hash: "blake3:source".to_string(),
            source_width: 800,
            source_height: 450,
            source_bytes: 123,
            output_path: "public/images/<card>.webp".to_string(),
            preset: "project-card<script>".to_string(),
            fit: "cover".to_string(),
            width: 640,
            height: 360,
            format: "webp".to_string(),
            bytes: 45,
            hash: "blake3:output".to_string(),
            operation_hash: "blake3:operation".to_string(),
        }],
    };

    let html = render_manifest_review(&manifest, &ManifestReviewOptions::default());

    assert!(html.contains("assets/&lt;card&gt;&amp;&quot;.png"));
    assert!(html.contains("project-card&lt;script&gt;"));
    assert!(html.contains("public/images/&lt;card&gt;.webp"));
    assert!(!html.contains("project-card<script>"));
}

#[test]
fn review_html_groups_sources_and_links_assets() {
    let manifest = Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "devimg.toml".to_string(),
        config_hash: "blake3:config".to_string(),
        outputs: vec![
            output(
                "assets/card.png",
                "public/images/card.960.webp",
                960,
                540,
                200,
            ),
            output(
                "assets/card.png",
                "public/images/card.640.webp",
                640,
                360,
                100,
            ),
            output(
                "assets/logo.png",
                "public/images/logo.256.webp",
                256,
                256,
                50,
            ),
        ],
    };
    let options = ManifestReviewOptions {
        asset_path_prefix: "..".to_string(),
        top_limit: 2,
    };

    let html = render_manifest_review(&manifest, &options);

    assert_eq!(html.matches("class=\"source-group\"").count(), 2);
    assert!(html.contains("Sources</dt>\n          <dd>2</dd>"));
    assert!(html.contains("Variants</dt>\n          <dd>3</dd>"));
    assert!(html.contains("src=\"../public/images/card.640.webp\""));
    assert!(html.contains("href=\"../assets/card.png\""));
    assert!(html.contains("Largest outputs"));
}

#[test]
fn review_html_renders_empty_manifest_and_format_helpers() {
    let manifest = Manifest {
        version: 1,
        generated_at: "unix:1".to_string(),
        config_path: "dev'img.toml".to_string(),
        config_hash: "blake3:config".to_string(),
        outputs: Vec::new(),
    };

    let html = render_manifest_review(&manifest, &ManifestReviewOptions::default());

    assert!(html.contains("Manifest has no generated variants."));
    assert!(html.contains("No entries."));
    assert!(html.contains("dev&#39;img.toml"));
    assert_eq!(super::asset_href("", "/assets"), "/assets");
    assert_eq!(super::format_bytes(1536), "1.5 KB");
    assert_eq!(super::format_bytes(2 * 1024 * 1024), "2.00 MB");
}

fn output(source: &str, path: &str, width: u32, height: u32, bytes: u64) -> ManifestOutput {
    ManifestOutput {
        source_path: source.to_string(),
        source_hash: format!("blake3:{source}"),
        source_width: 800,
        source_height: 450,
        source_bytes: 500,
        output_path: path.to_string(),
        preset: "project-card".to_string(),
        fit: "cover".to_string(),
        width,
        height,
        format: "webp".to_string(),
        bytes,
        hash: format!("blake3:{path}"),
        operation_hash: format!("blake3:{path}:operation"),
    }
}
