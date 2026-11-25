# DevImg Agent Guide

This file is the durable operating guide for future Codex or AI-agent sessions in this repository. Use it with the canonical docs in `docs/`; when this file conflicts with a more specific user request, follow the user request while preserving the project safety rules below.

## Project Identity

DevImg is a Rust CLI and GitHub Action for deterministic web image pipelines. It scans configured local image sources, generates responsive PNG/JPEG/WebP/AVIF variants, writes a JSON manifest and Markdown report, and lets CI fail when generated outputs are missing, stale, unsafe to overwrite, or over budget.

The product center is local-first and CI-first:

- no web UI;
- no hosted storage;
- no default network calls for deterministic image commands;
- no automatic PR comments, commits, publishing, uploads, social posts, or prose edits;
- generated variants, manifests, reports, helper exports, review HTML, AI artifacts, and draft prose are regenerated through DevImg commands rather than hand-edited.

Canonical references:

- `README.md`: user-facing overview and command examples.
- `docs/architecture.md`: pipeline and module boundaries.
- `docs/config.md`: TOML schema, diagnostics, warnings, and framework guidance.
- `docs/agents.md`: DevImg commands intended for coding agents.
- `docs/agent-contract.md`: file ownership contract for projects using DevImg.
- `docs/action.md`: composite GitHub Action behavior.
- `docs/release.md`: crates.io and GitHub Release process.
- `docs/public-distribution.md`: distribution channels and release boundaries.
- `docs/roadmap.md`: active roadmap and future boundaries.
- `examples/dogfood/README.md`: best local fixture for feature smoke tests.

## Maintaining This Guide

`AGENTS.md` is a living operating contract. Update it in the same PR as the change that makes the current guidance stale.

Update this file when:

- architecture boundaries move between `devimg-core`, `devimg-cli`, `action/`, docs, or examples;
- a command changes ownership, exit codes, overwrite behavior, generated artifacts, provider behavior, or network behavior;
- a new project-level rule is decided during implementation, review, release, or dogfood;
- release versions, Action pins, MSRV, security tooling, required checks, or packaging rules change;
- `cleisson.com` dogfood paths, configs, generated helpers, CI steps, or verification commands change;
- new env vars, provider keys, secret-handling rules, or package-exclusion rules are introduced;
- roadmap boundaries change, especially around AI providers, remote/dynamic images, publishing, storage, or social integrations;
- repeated mistakes show that future agents need a sharper rule or checklist item.

Do not update this file for one-off task details, speculative ideas, personal notes, or information already captured clearly in a more specific doc. Prefer linking to canonical docs over copying long reference material.

When updating this file:

- keep guidance objective, current, and command-oriented;
- remove stale release numbers, branch names, paths, or commands instead of appending contradictory notes;
- keep security and secret guidance conservative;
- verify every referenced path still exists;
- run `git diff --check`;
- run a redacted secret scan when adding env-var, provider, workflow, release, or packaging guidance;
- update `CHANGELOG.md` when the change affects future contributors or release behavior.

## Tech Stack

- Workspace: Rust 2021, `resolver = "2"`.
- MSRV: Rust `1.88`.
- Crates:
  - `crates/devimg-core`: config, scanning, planning, transforms, manifests, reports, diagnostics, compare/review/suggest/AI models.
  - `crates/devimg-cli`: `devimg` binary, `clap` command surface, user output, exit codes, OpenAI HTTP adapter, `.env` loading for explicit AI commands.
- GitHub Action: composite action under `action/`, wrapping the release or local `devimg` binary.
- Important libraries: `image`, `webp`, `ravif`, `globset`, `blake3`, `serde`, `toml`, `reqwest` with rustls.
- Security tooling: `gitleaks`, `cargo-audit`, `cargo-deny`, `zizmor`, `actionlint`, GitHub CodeQL, GitGuardian.

## Architecture Vision

Keep DevImg deterministic by default. The core pipeline is:

```text
config -> scan -> plan -> execute -> manifest/report
config -> scan -> plan -> read-only check -> doctor diagnostics
```

High-level ownership:

- `devimg-core` owns config parsing, source walking, planning, transforms, quality diagnostics, budget checks, manifest/report rendering, compare/review data, doctor diagnostics, suggestion models, and provider-agnostic AI request/report models.
- `devimg-cli` owns command parsing, file IO at the command edge, terminal output, exit codes, starter config generation, agent instructions, `.env` lookup for explicit AI commands, and real provider HTTP clients.
- `action/` owns GitHub-specific invocation, binary resolution, checksum verification, summary output, and Action outputs.

Design rules:

- Put deterministic business logic in `devimg-core`; keep CLI adapters thin.
- Add real network calls only to explicit commands and only at the CLI edge unless a design document says otherwise.
- Use injectable clients/transports for provider-backed and future networked features.
- Keep `doctor`, `check`, `optimize`, deterministic `suggest`, `agent task`, and manifest/report/review generation offline.
- Treat quality diagnostics as review signals, not automatic tuning.
- Preserve safe-overwrite behavior. Existing changed outputs are refused unless the user opted into overwrite or `--force` for that output artifact.
- Do not introduce a hosted service, daemon, web app, account model, remote storage layer, or automatic publishing path.

## Current Roadmap Direction

Read `docs/roadmap.md` before starting feature work.

Current intended sequence:

- Next diagnostic milestone, deferred from `0.2.8`: dogfood diagnostic precision, especially noisy Next.js/framework and suggestion behavior found in `cleisson.com`.
- `0.2.9`: provider hardening through fake transports and deterministic failure tests. No remote image commands.
- `0.2.10`: artifact contract stabilization for manifests, doctor JSON, compare JSON, suggestions, AI review JSON, alt-text JSON, draft Markdown, and agent-task Markdown. No `devimg remote` command.
- `0.2.11` optional: remote safety design gate only. No public remote behavior unless explicitly re-scoped.
- `0.3.0`: first remote/dynamic feature line, starting with explicit read-only `devimg remote audit` for deployed social/Open Graph/X preview images.

Remote image boundary:

- Read-only remote audit belongs in DevImg if it is explicit, safe, and does not merge live state into local manifests.
- Remote source snapshotting only belongs in DevImg if it turns allowlisted remote content into explicit local cached inputs plus a reviewed lockfile.
- Publish, sync, upload, delete, purge, cache invalidation, rollback, object-store credentials, and social-platform posting stay outside DevImg core by default.
- Do not add `[remote]` config in early remote work unless the artifact/schema contract is stable enough to justify it.

## Repository Workflow

For non-trivial tasks, write the task plan to `ai_tasks/todo.md` before editing. `ai_tasks/` is ignored and is for local execution notes only.

Agent operating rules:

- Use installed Codex skills automatically when they match the task. In this repo, `swe-prompt-builder` is useful for prompts/specs/AGENTS updates, `karpathy-guidelines` for implementation/debugging, `pragmatic-refactor` for explicit cleanup, `security-audit` for secret/security review, `github:*` skills for PR/CI work, and `playwright` for browser verification.
- For complex exploration, use subagents when the active runtime provides them, but keep one focused task per subagent and verify conclusions locally before editing.
- Use TDD for every feature and behavior change: write or update the failing tests first, run the narrow test to prove the gap when practical, implement only enough code to pass, then rerun the relevant broader checks.
- Stop and re-plan if an assumption proves wrong, a check fails for a non-obvious reason, or a change starts crossing module/product boundaries.
- Prefer simple, surgical changes. Add abstractions only when they remove real duplication or match an existing pattern.

Default loop:

1. Inspect current status with `git status --short --branch`.
2. Read the relevant docs and code before deciding.
3. For feature or behavior changes, add or update tests before implementation code.
4. Keep changes narrowly scoped.
5. Use `rg` and `rg --files` for search.
6. Use `apply_patch` for manual edits.
7. Do not hand-edit generated artifacts unless the task is specifically about a generated fixture and the command cannot produce it.
8. Verify with the smallest meaningful checks, then broaden checks when behavior, release, or shared contracts changed.
9. Update `ai_tasks/todo.md` review notes with what changed and what passed.

Generated files and ownership:

- Agents may edit source images, `devimg.toml`, docs, workflows, tests, source code, and app code that consumes generated exports.
- Agents must not hand-edit generated image variants, `devimg-manifest.json`, `devimg-report.md`, `lib/devimg.generated.ts`-style helper exports, AI review JSON, alt-text draft JSON, draft Markdown, or `.devimg/review.html`.
- Regenerate those through `devimg optimize`, `devimg manifest export`, `devimg review`, `devimg suggest`, `devimg alt`, or `devimg draft`.

## Development Commands

Use source checkout commands while developing DevImg:

```bash
cargo run -p devimg -- --help
cargo run -p devimg -- doctor --config examples/portfolio/devimg.toml
cargo run -p devimg -- optimize --config examples/portfolio/devimg.toml --dry-run
cargo run -p devimg -- optimize --config examples/dogfood/devimg.toml
cargo run -p devimg -- check --config examples/dogfood/devimg.toml
cargo run -p devimg -- manifest export --manifest examples/dogfood/public/images/devimg-manifest.json --strip-prefix public --url-prefix / --format typescript --typescript-helpers --output /tmp/devimg-dogfood.generated.ts
cargo run -p devimg -- review --manifest examples/dogfood/public/images/devimg-manifest.json --output examples/dogfood/.devimg/review.html --force
```

Use installed-binary commands when testing consumer behavior or release assets:

```bash
devimg doctor
devimg optimize
devimg check
devimg manifest export --manifest public/images/devimg-manifest.json --strip-prefix public --url-prefix / --format typescript --output lib/devimg.generated.ts
```

Exit codes:

- `0`: success or help output.
- `1`: runtime error outside config validation.
- `2`: usage or config error.
- `3`: check/doctor/export-check/suggestion gate found required work.
- `4`: unsafe overwrite refused.

## Testing And Verification

Use risk-based verification. For docs-only changes, run reference searches and `git diff --check`. For code, run the Rust checks below. For release work, run all release checks.

Core verification:

```bash
scripts/coverage.sh
cargo +1.88.0 fmt --all -- --check
cargo +1.88.0 check --workspace
cargo +1.88.0 clippy --all-targets --all-features -- -D warnings
cargo +1.88.0 test --all
git diff --check
scripts/security-checks.sh
```

The shared Rust workflow runs stable fmt, Clippy, tests, a locked build and the MSRV check. Its debug binary feeds the local Action and dogfood smokes. The Coverage job runs `scripts/coverage.sh --reports` with a checksum-verified `cargo-llvm-cov 0.9.2` binary. Keep local checks aligned with CI when touching code, workflows, or the Action.

Keep the Rust CodeQL report under both `/language:rust` and the historical `.github/workflows/codeql.yml:analyze` category. GitHub uses these categories to compare pull request scans against main.

Rust coverage policy:

- `scripts/coverage.sh` is the required exact line/function coverage gate for `devimg-core` and `devimg-cli`; it fails below 100% lines/functions and on any uncovered line/function count.
- The script runs split package reports (`devimg-core --lib`, then `devimg --bin devimg --exclude-from-report devimg-core`) to avoid duplicate target/source artifacts from merged workspace coverage.
- Test files such as `*_tests.rs` and integration-test files are ignored in coverage reports; integration behavior remains covered by `cargo +1.88.0 test --all`.
- The only production coverage file exclusion is `crates/devimg-cli/src/openai_transport.rs`, the real OpenAI HTTP adapter. OpenAI request building, response parsing, schema errors, transport errors, provider command behavior, and error sanitization must stay covered with fake transports.
- CLI `main`/`run` process-exit glue is excluded from unit coverage with `#[cfg(not(test))]` and covered through integration tests.
- Coverage reports and local `cargo-llvm-cov` artifacts must stay under `target/llvm-cov/` or ignored target directories.
- The CI Coverage job writes a GitHub Actions summary table and uploads split HTML/JSON coverage artifacts. Keep coverage visibility GitHub-native; do not add PR comment bots unless explicitly requested.

Package safety checks before release or secret-sensitive changes:

```bash
cargo +1.88.0 publish --dry-run -p devimg-core
cargo +1.88.0 package -p devimg --no-verify --list
cargo package --list --allow-dirty -p devimg-core | rg '\\.env$|/\\.env$'
cargo package --list --allow-dirty -p devimg | rg '\\.env$|/\\.env$'
```

The `.env` package-list commands should produce no matches. If they print a path, stop and fix packaging before proceeding.

When provider behavior changes, add deterministic tests with mocks/fake transports. Real provider smokes are optional and local/manual only. Required CI must not depend on OpenAI, Anthropic, `cleisson.com`, or public internet availability unless a future roadmap explicitly changes that policy.

## Environment Variables And Secrets

Important environment variables:

- `OPENAI_API_KEY`: used only by explicit OpenAI non-dry-run AI commands.
- `ANTHROPIC_API_KEY`: validated by consent previews and reserved/deferred real Anthropic flows.
- `DEVIMG_OPENAI_TEST_MODEL`: optional local/manual model name for real OpenAI smoke tests.
- `GH_TOKEN`/`GITHUB_TOKEN`: used by GitHub CLI or workflows; do not print token values.
- `CARGO_TERM_COLOR`: used in release workflow.
- `PROFILE`: optional override for `scripts/package-release.sh`.

Secret rules:

- Never print, paste, commit, log, summarize, or include actual key values.
- Do not read ignored `.env` files unless an explicit AI command or secret-safety check requires it.
- AI commands may look for `OPENAI_API_KEY` in the process environment, then ignored `.env` files under the project root/current directory.
- Deterministic commands must stay isolated from provider env vars.
- `--dry-run` commands must not require keys or call providers.
- Metadata-only AI modes must not send image bytes.
- Only `review --ai --include-images` and `alt --include-images` may send image bytes, and only after explicit human approval.
- `draft --ai-provider openai` is text-only; it must not send image bytes.
- Anthropic real calls remain deferred unless a future task implements matching tests, privacy behavior, and docs.

Before committing or releasing secret-sensitive changes:

```bash
gitleaks detect --redact --config .gitleaks.toml --source .
scripts/security-checks.sh
git diff --check
```

If local `.env` files exist, verify exact ignored secret values are absent from tracked files without printing those values. Do not write helper scripts that echo secrets.

## AI Command Policy

Provider-backed AI is review-only. It must never automatically apply prose, alt text, review observations, config changes, or image edits.

Current AI surfaces:

- `devimg ai consent`: deterministic consent preview, no provider call.
- `devimg review --ai`: OpenAI-only real review, metadata-only by default, image bytes only with `--include-images`.
- `devimg alt`: metadata-only placeholder by default; OpenAI real alt-text drafts require `--include-images`.
- `devimg draft`: metadata-only Markdown by default; OpenAI real drafting is text-only and explicit.

Rules:

- Always require explicit provider and model for provider-backed modes.
- Do not add default model names.
- Dry-run provider flows must work without keys.
- Provider errors must be sanitized and must not include API keys, data URLs, image bytes, raw provider bodies, or raw provider responses in persisted artifacts.
- Generated AI prose is advisory. Do not paste it into README, changelog, project pages, blog posts, social posts, app code, configs, source images, generated images, manifests, or reports without explicit human review.

## GitHub Workflow

`main` is protected. Do not expect direct pushes to `main` to work.

Preferred flow:

1. Commit locally with a focused message.
2. Push a branch, typically `codex/<short-description>`.
3. Open a PR against `main`.
4. Wait for `ci-ok`, which requires Rust, MSRV, smoke tests, exact coverage, dependency checks, workflow security, documentation validation, and Rust CodeQL. Check any additional status required by GitHub.
5. Merge only after checks pass.
6. Delete the `codex/*` branch after merge.
7. Sync local `main` with `origin/main`.

Use the GitHub app tools for PR metadata when available. Use `gh` for check watching, merge operations, and branch cleanup when the GitHub app lacks permission. If GitHub creates a squash merge and local `main` has the original commit, use `git pull --rebase origin main`; Git should skip the already-applied local commit.

Never use destructive git commands such as `git reset --hard` or `git checkout -- <path>` unless the user explicitly asks and the impact is understood.

## Release Workflow

Follow `docs/release.md` exactly for public releases.

Release invariants:

- Workspace version, crate versions, Action defaults, README/docs examples, release docs, public distribution docs, and changelog must agree.
- Publish `devimg-core` to crates.io before publishing `devimg`.
- `devimg` uses a workspace path dependency with a registry version, so `cargo publish --dry-run -p devimg` can only pass after the matching `devimg-core` version is visible in the crates.io index.
- Tag the same commit after crates.io publish succeeds.
- GitHub Release assets are built by tag and must match the crate version.
- Verify at least one downloaded archive checksum and `devimg --version`.
- Crates are effectively permanent. If a bad version is published, yanking is not secret removal.

Do not publish a release while local checks are failing, while package lists include `.env`, or while secret scans are unclear.

## GitHub Action Boundaries

The Action is a thin wrapper around the CLI.

It may:

- run `check` or `optimize`;
- verify checked-in manifest exports in check mode;
- generate a review artifact when `review-output` is set;
- expose the resolved `binary-path` for downstream fully flagged commands.

It must not:

- post PR comments;
- commit files;
- upload artifacts by itself;
- rewrite reports in check mode;
- rewrite manifest helper files;
- require OpenAI or Anthropic keys by default.

Pin `cleisson-labs/devimg/action` to the published release commit and add the tag in a version comment. Use `uses: $/action` with `binary-path: target/debug/devimg` only when testing this repository's local Action.

## `cleisson.com` Dogfood

The primary real-world dogfood checkout is:

```text
/Users/cleissonom/Projects/personal/cleisson.com
```

Use it when DevImg changes affect framework diagnostics, Next.js consumption, content-hash filenames, manifest helper exports, SEO/Open Graph images, Action behavior, or public release dogfood.

Tracked DevImg setup in `cleisson.com`:

- `devimg.toml`: project cover images from `public/projects` to `public/images/generated/projects`.
- `devimg.seo.toml`: SEO/Open Graph images from `public/seo` to `public/images/generated/seo`.
- `content_hash_filenames = true` in both configs.
- project helper export: `lib/devimg.generated.ts`.
- SEO helper export: `lib/devimg-seo.generated.ts`.
- app helper: `lib/devimg.ts`.
- docs: `docs/devimg.md`.
- CI: `.github/workflows/ci.yml` downloads its pinned DevImg release from `cleisson-labs/devimg` and verifies the archive checksum.

Important implementation details:

- Project covers use generated JPEG variants and Next.js `Image` with `unoptimized` where DevImg owns sizing and quality.
- SEO source PNGs are converted into smaller content-hashed JPEGs for metadata.
- `next.config.mjs` applies headers for `/seo/:path*` and `/images/generated/:path*`.
- `public/images/devimg-manifest.json`, `public/images/seo-manifest.json`, `lib/devimg.generated.ts`, and `lib/devimg-seo.generated.ts` are committed when they change.
- `.devimg/` reports and review artifacts are ignored and regenerated locally/CI.
- Dry-run AI/draft artifacts go under `/tmp` or `$RUNNER_TEMP` and are not committed.

Useful `cleisson.com` loop:

```bash
cd /Users/cleissonom/Projects/personal/cleisson.com
devimg optimize --dry-run
devimg optimize
devimg manifest export --manifest public/images/devimg-manifest.json --strip-prefix public --url-prefix / --format typescript --output lib/devimg.generated.ts
devimg manifest export --manifest public/images/devimg-manifest.json --strip-prefix public --url-prefix / --format typescript --output lib/devimg.generated.ts --check
devimg check --fail-on-warning
devimg optimize --config devimg.seo.toml
devimg manifest export --manifest public/images/seo-manifest.json --strip-prefix public --url-prefix / --format typescript --output lib/devimg-seo.generated.ts
devimg manifest export --manifest public/images/seo-manifest.json --strip-prefix public --url-prefix / --format typescript --output lib/devimg-seo.generated.ts --check
devimg check --config devimg.seo.toml --fail-on-warning
```

Optional dry-run dogfood commands:

```bash
devimg ai consent --ai-provider openai --model openai-dry-run-model --dry-run --output /tmp/cleisson-devimg-openai-consent.json --force
devimg review --manifest public/images/devimg-manifest.json --ai --ai-provider openai --model openai-dry-run-model --dry-run --ai-output /tmp/cleisson-devimg-openai-ai-review.json --markdown /tmp/cleisson-devimg-openai-ai-review.md --force
devimg alt --output /tmp/cleisson-devimg-alt.json --markdown /tmp/cleisson-devimg-alt.md --force
devimg draft --draft-type project-page-copy --ai-provider openai --model openai-dry-run-model --dry-run --ai-review-json /tmp/cleisson-devimg-openai-ai-review.json --review-html .devimg/review.html --output /tmp/cleisson-devimg-project-page-copy.md --force
```

When dogfooding `cleisson.com`, also run the site's own checks when app code changes:

```bash
npm run format:check
npm run lint
npm run typecheck
npm run build
git diff --check
```

Do not read or print `cleisson.com/.env`. It may contain local provider credentials. Keep any real OpenAI smoke local/manual and place artifacts under `/tmp`.

## Documented Solutions And Tricky Areas

Content-hash filenames:

- Use them before immutable CDN cache headers.
- Do not hard-code generated hashed URLs in app code; consume a manifest export instead.
- When hashed outputs change, update manifest and helper export together.

Manifest helper exports:

- Use `devimg manifest export`.
- If a helper was generated with `--typescript-helpers`, pass `--typescript-helpers` to `manifest export --check`, `doctor --export-output`, and the Action export check.
- `doctor` warns when it discovers likely helper files that were not verified.

Warnings:

- `quality:` warnings are advisory by default.
- Use `check --fail-on-warning` only when the project wants warnings to block CI.
- Add `[[warnings.acknowledge]]` only after visual review, scoped to a specific source/preset or output, with a useful reason.
- After changing acknowledgements, run `optimize` so manifest/report metadata reflects the current config hash.

Incremental behavior:

- `optimize` may skip unchanged variants. Skips are successful reuse, not missing work.
- Operation hashes track transform inputs, not every config byte.
- Config-only metadata changes can refresh manifest/report without re-encoding unchanged images.

Framework diagnostics:

- `doctor` detects Next.js, Astro, and Vite and emits advisory framework warnings.
- Framework diagnostics must not change transforms or output paths.
- For Next.js, acceptable consumption modes include plain `img`/`picture` URLs from exports, `next/image` with `unoptimized` when DevImg owns sizing/quality, or a consciously layered framework optimization setup.

Review artifacts:

- `devimg review` renders local static HTML from one manifest.
- It does not evaluate budgets; budgets are owned by `check`.
- Use review artifacts for visual inspection or CI upload, not as source-of-truth data.

OpenAI Responses API:

- Keep request/response logic text-only unless the command explicitly supports image bytes and the user approves `--include-images`.
- Use structured outputs and sanitize provider errors.
- Do not persist raw provider responses.

Remote/dynamic images:

- Do not implement remote fetching inside existing deterministic local commands.
- Early remote tests must use fake transports/resolvers or controlled local fixtures. Required CI should not depend on public URLs.
- SSRF rules for future remote audit are strict: HTTPS by default, block private/local/link-local/metadata destinations, validate redirects, cap bytes/timeouts/concurrency, no cookies/auth/provider keys, and redact query strings.

## Implementation Checklist

Before editing:

- [ ] Read this file plus the canonical docs relevant to the task.
- [ ] Run `git status --short --branch`.
- [ ] Record a task plan in `ai_tasks/todo.md` for non-trivial work.
- [ ] Identify generated files and decide which commands regenerate them.
- [ ] Decide whether `cleisson.com` dogfood is relevant.

During implementation:

- [ ] Keep edits scoped to the requested behavior.
- [ ] Update tests when behavior changes.
- [ ] Update docs/help/changelog when user-facing behavior changes.
- [ ] Add fake-provider or fake-transport seams for network/provider behavior.
- [ ] Avoid new default network or provider requirements.
- [ ] Keep secrets out of output, logs, artifacts, fixtures, and final responses.

Before final response:

- [ ] Run relevant verification commands and record pass/fail.
- [ ] Run `git diff --check`.
- [ ] Run `scripts/security-checks.sh` for release, workflow, provider, secret, or packaging changes.
- [ ] Confirm package lists exclude `.env` for release or secret-sensitive changes.
- [ ] Confirm deterministic commands still do not require provider env vars.
- [ ] If touching Action/release behavior, verify docs/action defaults and release docs match.
- [ ] If touching `cleisson.com`, verify both DevImg configs and the site checks when applicable.
- [ ] Summarize changed files, generated files, checks run, and remaining risks.

## Final Response Shape

Keep the final response short and concrete:

- what changed;
- important files touched;
- verification run and result;
- any remaining risk or follow-up;
- whether changes were committed, pushed, or left unstaged.
