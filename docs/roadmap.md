# DevImg Roadmap

This roadmap starts after `0.2.7`. The completed v0.2 AI plan has shipped; the rest of `0.2.x` should harden DevImg's current local-first, CI-first, provider-explicit contracts. Remote/dynamic image work is a new capability line and should start in `0.3.x` unless a release is explicitly marked as design/test prep only.

DevImg's durable product constraints remain:

- deterministic image commands stay offline and provider-free;
- provider-backed AI commands stay explicit, opt-in, and review-only;
- image bytes are sent to providers only when a user passes `--include-images`;
- DevImg does not store API keys in config, reports, generated artifacts, logs, or package contents;
- DevImg does not publish, commit, post, or rewrite application prose automatically;
- networked remote-image checks must be explicit, read-only, and safe by default;
- remote source snapshotting belongs in DevImg only if it converts allowlisted remote content into explicit local cached inputs plus a reviewed lockfile;
- publish, sync, upload, delete, purge, cache invalidation, rollback, and social-platform posting belong outside DevImg core unless a future roadmap explicitly documents why that boundary changed;
- generated variants, manifests, reports, review artifacts, and helper exports are regenerated through DevImg commands rather than hand-edited.

## `0.2.8`: Dogfood Diagnostic Precision

Goal: reduce noisy advisory output found during `cleisson.com` dogfooding without weakening safety checks.

Scope:

- Make framework diagnostics more precise for Next.js projects that already render DevImg-owned images with `unoptimized` or use generated assets only in metadata.
- Align `devimg suggest --check` with acknowledged warning behavior so reviewed, scoped warnings do not keep producing blocking warning-level suggestions.
- Allow `suggest` to receive the same manifest export context used by `doctor --export-output`, so helper-drift suggestions can distinguish checked helpers from unknown helpers.
- Add focused tests for acknowledged crop warnings, checked helper exports, and Next.js `unoptimized` consumption patterns.
- Update `cleisson.com` dogfood docs and CI guidance only when behavior changes are proven locally.

Non-goals:

- No new AI provider surface.
- No changes to generated image transform semantics.
- No automatic source/config edits from suggestions.

Done criteria:

- `cleisson.com` can run the intended DevImg doctor/suggest checks without false-positive blocking output.
- Existing warning visibility remains intact in reports and `doctor --json`.
- Current DevImg verification passes: fmt, check, clippy, tests, diff check, security checks, and package `.env` exclusion.

## `0.2.9`: Provider Hardening

Goal: make OpenAI-backed commands easier to test and support without making CI depend on real provider calls.

Scope:

- Add CLI-level provider failure tests through an injectable test transport or local fake server.
- Cover OpenAI transport errors, malformed structured output, schema mismatches, refusal/error payloads, and timeout handling.
- Keep provider error sanitization tests explicit: no API keys, data URLs, image bytes, or raw provider responses should persist in artifacts.
- Document an optional manual `workflow_dispatch` pattern for teams that want CI-assisted drafting with `OPENAI_API_KEY`, while keeping default CI examples dry-run only.
- Keep fake transport/test seams reusable for future explicit networked commands, but do not add remote image commands in `0.2.9`.
- Keep local `.env` loading limited to explicit AI commands.

Non-goals:

- No default GitHub Action secret requirement.
- No real provider calls in normal CI.
- No remote image command surface.
- No Anthropic HTTP implementation unless its contract, fixtures, and privacy behavior are ready.

Done criteria:

- Provider-backed command failures are covered by deterministic tests.
- Real OpenAI smoke remains optional and local/manual.
- Docs clearly state that production CI can remain keyless.

## `0.2.10`: Artifact Contract Stabilization

Goal: formalize the contracts that external projects and coding agents consume before expanding the feature surface again.

Scope:

- Document stable fields for manifest exports, doctor JSON, compare JSON, suggestion JSON, AI review JSON, alt-text JSON, draft Markdown, and agent-task Markdown.
- Decide which artifacts need explicit schema/version fields and migration notes.
- Review whether `doctor`, `suggest`, `review`, `alt`, and `draft` should share more context plumbing for manifest helpers, framework diagnostics, changelog excerpts, and optional artifacts.
- Decide Anthropic's status for the next cycle: implement real calls with matching tests and privacy behavior, or keep it explicitly deferred.
- Document the remote-image boundary for the next minor line: read-only audit belongs in DevImg; remote snapshots require lockfiles and allowlists; publish/sync/upload belongs outside DevImg core unless a future design explicitly reverses that decision.
- Document remote audit artifact principles: stable field names and deterministic ordering where possible, but no promise that live HTTP status, headers, redirects, ETags, content length, or cache behavior are deterministic.
- Add migration guidance for teams using the GitHub Action and checked-in helper exports.

Non-goals:

- No breaking artifact changes without migration notes.
- No hosted service, remote storage, or automatic publishing behavior.
- No `devimg remote` command in `0.2.10`.
- No broad UI layer.

Done criteria:

- Artifact contracts are documented enough for external projects and agents to rely on them.
- Any schema/version additions are backward compatible or explicitly migrated.
- Release docs and Action docs reflect the stabilized contract.

## `0.2.11` Optional: Remote Safety Design Gate

Goal: prepare the `0.3.x` remote audit line without shipping user-facing remote behavior.

Scope:

- Define the `devimg remote audit` command shape, output contract, exit-code semantics, overwrite behavior, and manual workflow guidance.
- Define the remote audit JSON schema and Markdown summary shape, including `schema_version`, artifact `kind`, normalized inputs, summary counts, page records, image records, findings, and redacted URL fields.
- Define the SSRF policy: HTTPS by default, no cookies/auth, no local `.env` or provider keys, block private/local/link-local/metadata IPs, validate all redirect targets, cap redirects/body bytes/timeouts/concurrency, and redact query strings in artifacts.
- Define fake transport and fake resolver strategy for deterministic tests, including redirects, DNS answers, blocked addresses, HEAD fallback, timeout behavior, malformed HTML, and byte caps.
- Decide whether remote audit artifacts get a standalone schema version or share a broader DevImg artifact schema.
- Decide how `doctor` and `suggest` may reference remote audit in the future without fetching network resources themselves.

Non-goals:

- No public `devimg remote` command.
- No remote source snapshots.
- No bucket listing.
- No upload, delete, purge, publish, sync, cache invalidation, rollback, or social posting.
- No AI integration.

Done criteria:

- The `0.3.0` implementation contract is specific enough to build without reopening product-boundary, security, artifact, or testing questions.
- The design keeps normal DevImg CI offline and does not require public-network fixtures.

## `0.3.0`: Remote Social Image Audit

Goal: make deployed social-preview images observable without changing local transforms, downloading image bytes into the repo, or publishing anything.

Product boundary:

- Remote audit belongs in DevImg only as explicit, read-only observability for deployed image behavior.
- The strongest fit is validating public pages that expose DevImg-managed images through Open Graph, X/Twitter card metadata, SEO images, or CDN-hosted generated variants.
- Remote audit observes live systems; it must report status and evidence without treating remote headers, redirects, ETags, content length, or cache behavior as deterministic build outputs.
- `doctor`, `check`, `optimize`, and the GitHub Action's default `check` mode remain offline.
- Avoid adding a `[remote]` section to `devimg.toml` in `0.3.0`; use explicit CLI and file inputs until the schema proves stable.

Minimal command shape:

```bash
devimg remote audit \
  --url https://www.cleisson.com/en-US \
  --url https://www.cleisson.com/en-US/projects/devimg \
  --output /tmp/devimg-remote-audit.json \
  --markdown /tmp/devimg-remote-audit.md
```

Scope:

- Add explicit `devimg remote audit`.
- Accept repeatable `--url <https-url>`, `--urls <path>`, and optional `--sitemap <https-url-or-file>` inputs.
- Add output flags: `--output <path>` for JSON, `--markdown <path>` for a human summary, `--stdout` for JSON to stdout, and `--force` for replacing existing outputs.
- Add policy flags: `--fail-on <error|warning|off>`, `--max-concurrency <n>`, `--timeout <duration>`, `--max-redirects <n>`, `--max-page-bytes <bytes>`, and `--max-image-probe-bytes <bytes>`.
- Parse Open Graph and X/Twitter image metadata: image URL, secure URL, declared width/height, card type, MIME type when declared, and alt text.
- Probe discovered image URLs with bounded `HEAD` or small `GET` requests.
- Report status, redirect chain summary, content type, content length, cache-control, ETag, Last-Modified, dimensions when safely available, missing alt text, missing dimensions, non-HTTPS URLs, likely aspect-ratio mismatches, oversized images, weak cache headers, and inaccessible images.
- Emit stable JSON by default and optional Markdown for humans and coding agents.
- Use deterministic ordering and stable enums/field names, while documenting remote observations as live evidence rather than reproducible build facts.
- Dogfood against owned `cleisson.com` public URLs after deploy, starting locally.

Architecture:

- Keep remote audit logic separate from the local transform path.
- Put pure audit state, models, metadata parsing, policy decisions, and transport traits in `devimg-core`.
- Put real HTTP adapters, CLI argument parsing, output file handling, and process exit codes in `devimg-cli`.
- Test core behavior through injected fake transports and fake resolvers.
- Do not merge remote observations into local manifests; optionally compare saved remote audit artifacts to manifest/export URLs in later releases.

JSON artifact principles:

- Include `schema_version`, `kind: "devimg.remote.audit"`, normalized inputs, summary counts, page records, image records, and findings.
- Redact query strings by default and include a stable URL hash when correlation is needed.
- Do not persist raw response bodies, raw HTML bodies, downloaded image bytes, cookies, auth headers, signed URL tokens, provider keys, or local environment values.
- Treat signed CDN/S3/R2 query parameters as sensitive.

Safety defaults:

- Require `https://` by default.
- Reject `file:`, `data:`, `ftp:`, `gopher:`, `blob:`, local paths, and non-HTTP schemes.
- Reject localhost, loopback, private IPv4 ranges, link-local, multicast, IPv6 unique-local, IPv6 link-local, and cloud metadata addresses such as `169.254.169.254`.
- Resolve DNS before connecting and validate all A/AAAA answers as public.
- Re-validate every redirect target before following it.
- Keep redirects low by default.
- Use no cookies, auth headers, provider keys, or local `.env` values.
- Send a minimal, identifiable User-Agent.
- Use `HEAD` first, but fall back to bounded `GET` because some origins reject `HEAD`.
- Cap page bytes, image probe bytes, decompressed bytes, timeout, and concurrency.
- Make CI failure opt-in with `--fail-on`; default behavior should report findings without breaking builds.

Test strategy:

- Required DevImg CI uses fake transports, fake resolvers, and local fake servers only.
- No required public internet, `cleisson.com`, social CDN, or random third-party image dependency.
- Cover pages with no metadata, multiple `og:image` tags, structured OG fields attached to the wrong root image, relative image URLs, HTTP image URLs, non-image content types, missing content length, oversized content length, weak cache headers, wrong dimensions, missing alt text, malformed HTML, HEAD `405` fallback, redirect limits, blocked private URLs, timeouts, and byte caps.
- If production policy blocks localhost, tests should use injected transports, fake resolvers, or test-only fixture plumbing instead of exposing a user-facing `allow-localhost` escape hatch.
- Use `cleisson.com` as the first real dogfood target because its generated SEO images are owned, public, stable, and already managed by DevImg.
- Run real public URL audits locally first, then consider an optional manual `workflow_dispatch` dogfood workflow after stability is proven.

AI stance:

- Do not embed AI in `0.3.0`.
- Existing text-only draft flows may summarize a saved remote audit JSON manually, for example `devimg draft --draft-type social-post-outline --compare-json /tmp/devimg-remote-audit.json`.
- Defer any future AI explanation or review command until deterministic remote auditing is stable, secret-free, and byte-free by default.
- Never use AI to decide pass/fail, fetch remote images, upload or rewrite assets, or process remote image bytes without explicit human consent.

Reference constraints:

- Open Graph image metadata includes image URL, secure URL, width, height, MIME type, and alt fields: <https://ogp.me/>.
- X summary-large cards define image URL, dimensions, byte-size, format, and alt-text expectations: <https://developer.x.com/cards/types/summary-large-image>.
- Next.js remote image usage requires explicit dimensions and strict remote host/path matching: <https://nextjs.org/docs/pages/api-reference/components/image>.
- SSRF prevention requires strict destination validation, redirect validation, input validation, and network controls: <https://owasp.org/Top10/2021/A10_2021-Server-Side_Request_Forgery_%28SSRF%29/>.

Non-goals:

- No local mirroring.
- No transforms from remote URLs.
- No remote image byte persistence.
- No AI.
- No bucket provider APIs or credentials.
- No upload, publish, sync, delete, purge, cache invalidation, rollback, or social posting.
- No required public-network CI.

Done criteria:

- `devimg remote audit` can validate public social-preview images for owned public URLs without writing repo-tracked files.
- JSON and Markdown reports are stable, secret-free, byte-free, and clear about live-network uncertainty.
- Local deterministic commands remain offline unless `remote audit` is explicitly invoked.

## `0.3.1`: Remote Audit Hardening

Goal: make saved remote audit evidence useful for humans, agents, and optional manual operations without making network checks a default CI dependency.

Scope:

- Add or harden `--fail-on <error|warning|off>` semantics after real dogfood.
- Add comparison of saved remote audit artifacts to show changed page metadata, image URLs, status, headers, dimensions, byte budgets, and findings.
- Improve Markdown summaries for release review, SEO/social review, and coding-agent handoff.
- Add optional manual `workflow_dispatch` dogfood guidance for owned public URLs.
- Dogfood against `cleisson.com` only in local/manual workflows.
- Improve diagnostics for environment-dependent outcomes such as timeout, DNS failure, bot protection, HEAD not allowed, content length unknown, and dimension probe unavailable.

Non-goals:

- No default Action network dependency.
- No remote source snapshot.
- No provider-backed AI.
- No publish/sync/upload behavior.

## `0.3.2`: Public Bucket/CDN Audit

Goal: help teams validate public bucket/CDN-hosted image URLs without adding storage credentials or mutation behavior.

Scope:

- Audit explicit public URL lists or manifest-derived public URLs.
- Check status, content type, cache-control, ETag, Last-Modified, content length, byte budgets, immutable-cache expectations for hashed filenames, and mutable-cache expectations for non-hashed filenames.
- Compare public URL behavior against DevImg manifest/export paths when provided.
- Report stale URLs, weak cache headers for hashed filenames, mutable headers for non-hashed filenames, missing objects, content-type mismatches, and oversized assets.

Non-goals:

- No bucket provider APIs.
- No credentials.
- No signed URL generation.
- No upload, delete, purge, cache invalidation, rollback, or publishing.
- No required third-party CDN fixtures in CI.

## `0.3.3+`: Remote Source Snapshot Decision

Goal: decide whether DevImg should snapshot allowlisted remote images into a local cache and lockfile.

Scope:

- Decide whether remote source snapshotting belongs in DevImg after remote audit is stable.
- Require host/path allowlists, SHA-256, dimensions, content type, byte size, ETag, Last-Modified, resolved URL, and explicit check mode.
- Convert remote content into reviewed local cached inputs before using existing local transform planning.
- Distinguish URL metadata changes from image-byte changes in lockfile diffs.
- Decide whether locked remote cache files are intended to be committed, regenerated, or ignored by default.

Non-goals:

- No direct transforms from arbitrary remote URLs.
- No background refresh.
- No credentials in config or artifacts.
- No upload/publish/sync behavior.

## Later Candidates

Consider these only after the `0.2.8` through `0.3.3+` work is complete or deliberately reprioritized:

- Real Anthropic provider calls for AI review, alt-text drafts, and prose drafts.
- Broader dogfooding for blog images, favicon/app icons, and experience logos in `cleisson.com` when there is measurable churn, size risk, or review value.
- Richer review artifacts for visual comparison, crop focus, and budget trend inspection.
- Optional machine-readable schemas for generated artifacts.
- Remote AI explanation of saved audit JSON, only after deterministic remote audit is stable and byte-free by default.
- Social preview cache-refresh integrations, only if they can be explicit, provider-safe, non-spammy, and non-destructive.
- Publish/sync/upload design, defaulting to external deployment tools that consume DevImg manifests. DevImg core should not own object-store credentials, destructive deletes, cache invalidation, rollback, or social-platform posting unless a future roadmap explicitly documents why that boundary changed.

## Release Discipline

Each release should include:

- a narrow changelog entry;
- focused tests scaled to behavioral risk;
- `cargo +1.88.0 fmt --all -- --check`;
- `cargo +1.88.0 check --workspace`;
- `cargo +1.88.0 clippy --all-targets --all-features -- -D warnings`;
- `cargo +1.88.0 test --all`;
- `git diff --check`;
- `scripts/security-checks.sh`;
- package-list checks confirming `.env` files are excluded;
- dogfood verification in `cleisson.com` when public Action behavior, framework diagnostics, or image-management guidance changes.
- explicit networked-command verification for any release that changes remote-image behavior.
