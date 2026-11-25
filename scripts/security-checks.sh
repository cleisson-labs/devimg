#!/usr/bin/env bash
set -euo pipefail

missing=()
for tool in gitleaks cargo-audit cargo-deny zizmor; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    missing+=("$tool")
  fi
done

if ((${#missing[@]} > 0)); then
  echo "Missing security tools: ${missing[*]}" >&2
  echo "Install the release binaries pinned in CI and verify their published checksums." >&2
  echo "See CONTRIBUTING.md for tool releases." >&2
  exit 127
fi

gitleaks detect --redact --config .gitleaks.toml --source .
cargo audit
cargo_for_deny="$(rustup which --toolchain 1.88.0 cargo 2>/dev/null || command -v cargo)"
CARGO="$cargo_for_deny" cargo-deny --locked check
zizmor --offline .
