#!/usr/bin/env bash
# Run the same portable checks as the library and demo CI jobs.
set -euo pipefail

usage() {
  cat <<'HELP'
Usage: bin/check.sh [--library | --demo | --help]

Run formatting, strict Clippy checks, and the library tests with the committed
dependency lockfiles. The demo's formatting and Clippy checks are included.

  --library  Check only the library (used by its separate CI job).
  --demo     Check only the demo (used by its separate CI job).
  --help     Show this help.

Native integration checks are documented in demo/ios/README.md.
HELP
}

scope=all
if [ "$#" -gt 1 ]; then
  usage >&2
  exit 2
fi
case "${1:-}" in
  --help|-h) usage; exit 0 ;;
  --library) scope=library ;;
  --demo) scope=demo ;;
  '') ;;
  *) usage >&2; exit 2 ;;
esac

cd "$(dirname "$0")/.."
if [ "$scope" != demo ]; then
  cargo fmt --all -- --check
  cargo clippy --locked --workspace --all-features --all-targets -- -D warnings
  cargo test --locked --workspace --all-features
fi
if [ "$scope" != library ]; then
  cargo fmt --manifest-path demo/Cargo.toml -- --check
  cargo clippy --locked --manifest-path demo/Cargo.toml --all-targets -- -D warnings
fi
