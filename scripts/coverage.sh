#!/usr/bin/env bash
set -euo pipefail

reports=false
if [[ ${1:-} == --reports ]]; then
  reports=true
  shift
fi

cargo llvm-cov clean --workspace
if "$reports"; then
  mkdir -p target/llvm-cov/json
fi

test_files='(.*/[^/]+_tests\.rs$|.*/tests/.*\.rs$)'
cli_files="(.*/crates/devimg-cli/src/openai_transport\\.rs$|$test_files)"

coverage() {
  local report=$1 package=$2 ignored=$3 html_ignored=$4
  shift 4
  local output=(--show-missing-lines)
  if "$reports"; then
    output=(--json --summary-only --output-path "target/llvm-cov/json/$report.json")
  fi

  cargo llvm-cov --locked -p "$package" --all-features \
    --ignore-filename-regex "$ignored" \
    --fail-under-lines 100 --fail-under-functions 100 \
    --fail-uncovered-lines 0 --fail-uncovered-functions 0 \
    "${output[@]}" "$@"

  if "$reports"; then
    cargo llvm-cov report -p "$package" --ignore-filename-regex "$html_ignored" \
      --html --output-dir "target/llvm-cov/html/$report"
  fi
}

coverage devimg-core devimg-core "$test_files" "$test_files" --lib "$@"
coverage devimg-cli devimg "$cli_files" "(.*/crates/devimg-core/.*|$cli_files)" \
  --no-clean --bin devimg --exclude-from-report devimg-core "$@"
