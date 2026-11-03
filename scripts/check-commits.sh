#!/usr/bin/env bash
# Checks that every non-merge commit in RANGE follows Conventional Commits 1.0.0.
# Usage: scripts/check-commits.sh <base>..<head>
set -euo pipefail

range="${1:?usage: scripts/check-commits.sh <base>..<head>}"
types='feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert'
pattern="^(${types})(\([a-z0-9-]+\))?!?: [^ ].*"

status=0
while IFS=$'\t' read -r sha subject; do
  if [[ ! "$subject" =~ $pattern ]]; then
    echo "::error::${sha} does not follow Conventional Commits: ${subject}"
    status=1
  fi
done < <(git log --no-merges --format='%h%x09%s' "$range")

if [[ $status -eq 0 ]]; then
  echo "All commits in ${range} follow Conventional Commits."
fi
exit "$status"
