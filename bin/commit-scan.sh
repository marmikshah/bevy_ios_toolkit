#!/usr/bin/env bash
# Fail when a commit message in a range carries AI attribution. CI runs this on
# every pull request: commits carry the human author only.
#
# Usage: bin/commit-scan.sh A..B
set -euo pipefail

readonly PATTERN='co-authored-by:.*(claude|anthropic|openai|chatgpt|gpt-|copilot|gemini|bard|llama|cursor|codeium|codex)|generated (with|by).*(claude|chatgpt|gpt|copilot|cursor|codex|gemini)|noreply@anthropic\.com'

case "${1:-}" in
  -h|--help) sed -n '2,5s/^# \{0,1\}//p' "${BASH_SOURCE[0]}"; exit 0 ;;
  *..*) ;;
  *) echo "Usage: bin/commit-scan.sh A..B" >&2; exit 2 ;;
esac

text="$(git log --no-merges --format='%h %an <%ae>%n%s%n%b' "$1")" \
  || { echo "commit-scan: could not read $1" >&2; exit 2; }

# grep exits 1 when it finds nothing (clean) and 2 on error; keep them apart.
set +e
hits="$(printf '%s\n' "$text" | grep -iEn "$PATTERN")"
status=$?
set -e

case "$status" in
  0) echo "$hits"; exit 1 ;;
  1) exit 0 ;;
  *) echo "commit-scan: scan failed" >&2; exit "$status" ;;
esac
