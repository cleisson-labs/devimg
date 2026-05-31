#!/usr/bin/env bash
set -euo pipefail

cargo llvm-cov clean --workspace

test_ignore_regex='(.*/[^/]+_tests\.rs$|.*/tests/.*\.rs$)'

cargo llvm-cov \
  -p devimg-core \
  --all-features \
  --lib \
  --ignore-filename-regex "$test_ignore_regex" \
  --show-missing-lines \
  --fail-under-lines 100 \
  --fail-under-functions 100 \
  --fail-uncovered-lines 0 \
  --fail-uncovered-functions 0 \
  "$@"

cargo llvm-cov \
  -p devimg \
  --all-features \
  --bin devimg \
  --exclude-from-report devimg-core \
  --ignore-filename-regex "(.*/crates/devimg-cli/src/openai_transport\\.rs$|$test_ignore_regex)" \
  --show-missing-lines \
  --fail-under-lines 100 \
  --fail-under-functions 100 \
  --fail-uncovered-lines 0 \
  --fail-uncovered-functions 0 \
  "$@"
