#!/usr/bin/env bash
set -euo pipefail

# optional local-only prereq check for dfinity/sns-testing rehearsal

missing=0
for tool in cargo icp; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf 'missing optional local tool: %s\n' "$tool" >&2
    missing=1
  fi
done

if ! command -v dfx >/dev/null 2>&1; then
  printf 'missing optional local tool: dfx\n' >&2
  missing=1
elif ! dfx sns --help >/dev/null 2>&1; then
  printf 'missing optional dfx sns extension; install it only for manual official SNS rehearsal\n' >&2
  missing=1
fi

if [ ! -f tools/sns/sns_init.io.local.yaml ]; then
  printf 'missing tools/sns/sns_init.io.local.yaml\n' >&2
  missing=1
fi

if [ "$missing" -ne 0 ]; then
  exit 1
fi

printf 'optional local SNS testing prerequisites are present\n'
