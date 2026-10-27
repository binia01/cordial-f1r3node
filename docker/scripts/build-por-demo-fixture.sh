#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
output="${1:-$repo_root/docs/demos/por-weighted-slashing.json}"
if [[ "$output" != /* ]]; then output="$PWD/$output"; fi
mkdir -p "$(dirname "$output")"
temporary="$(mktemp "${output}.tmp.XXXXXX")"
trap 'rm -f "$temporary"' EXIT
cd "$repo_root"
POR_DEMO_OUTPUT="$temporary" cargo test -j 1 -p cordial-f1r3node-adapter \
  --test test_por_checkpoint_penalties four_observer_checkpoint_penalty_demo \
  -- --exact
chmod 644 "$temporary"
mv "$temporary" "$output"
echo "Wrote $output"
