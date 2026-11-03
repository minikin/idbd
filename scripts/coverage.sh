#!/usr/bin/env bash
# Collects coverage from the test suites and from full runs of the experiment
# binaries, then scores every function with cargo-crap.
#
# Usage: scripts/coverage.sh [--baseline]
#   --baseline  rewrite quality/crap-baseline.json instead of checking against it
set -euo pipefail

cd "$(dirname "$0")/.."
out=target/coverage
mkdir -p "$out"

cargo llvm-cov clean --workspace
cargo llvm-cov --no-report -p idbd -p idbd-lab --all-features
for bin in sutton1992-exp1 evaluate sutton1992-exp2 autostep-eval house-eval; do
  cargo llvm-cov run --no-report -p idbd-lab --bin "$bin" -- --out "$out/$bin"
done
cargo llvm-cov report -p idbd -p idbd-lab --lcov --output-path lcov.info
cargo llvm-cov report -p idbd -p idbd-lab --summary-only

# bench.rs measures wall-clock time, so its output cannot be checked by a test. It is
# run by hand and its result is recorded in docs/benchmarks.md.
crap=(cargo crap -p idbd -p idbd-lab --lcov lcov.info --allow 'crates/idbd-lab/src/bin/bench.rs')

if [[ "${1:-}" == "--baseline" ]]; then
  "${crap[@]}" --format json --sort file --output quality/crap-baseline.json
else
  "${crap[@]}" --threshold 30 --fail-above
  "${crap[@]}" --baseline quality/crap-baseline.json --fail-regression
fi
