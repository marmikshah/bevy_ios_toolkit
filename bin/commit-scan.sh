#!/usr/bin/env bash
#
# Scan commit messages for AI attribution. The single definition of what that
# means: the audit, the pre-commit hook, the pre-push hook, and the hosted gate
# all call this, so the rule cannot mean four slightly different things.
#
# Usage:
#   commit-scan.sh --history N        the last N commits reachable from HEAD
#   commit-scan.sh --range A..B       every commit in a range (a pull request)
#   commit-scan.sh --message FILE     one commit message, before it is written
#
# Prints offending lines and exits 1 when it finds any; exits 0 when clean.

set -euo pipefail

# Every spelling that has turned up in a trailer. Commits carry the configured
# human identity only: an agent may write the change, but the history records
# who is answerable for it.
readonly PATTERN='co-authored-by:.*(claude|anthropic|openai|chatgpt|gpt-|copilot|gemini|bard|llama|cursor|codeium|codex)|generated (with|by).*(claude|chatgpt|gpt|copilot|cursor|codex|gemini)|noreply@anthropic\.com'

mode=""
value=""

case "${1:-}" in
  --history|--range|--message) mode="$1"; value="${2:-}" ;;
  -h|--help|"") sed -n '2,14p' "${BASH_SOURCE[0]}" | sed 's/^# \?//'; exit 0 ;;
  *) echo "commit-scan: unknown option ${1:-}" >&2; exit 2 ;;
esac
[ -n "$value" ] || { echo "commit-scan: $mode needs a value" >&2; exit 2; }

case "$mode" in
  --history)
    text="$(git log --no-merges --format='%h %an <%ae>%n%s%n%b' -"$value")" \
      || { echo "commit-scan: could not read history" >&2; exit 2; }
    ;;
  --range)
    text="$(git log --no-merges --format='%h %an <%ae>%n%s%n%b' "$value")" \
      || { echo "commit-scan: could not read $value" >&2; exit 2; }
    ;;
  --message)
    [ -f "$value" ] || { echo "commit-scan: no such file: $value" >&2; exit 2; }
    text="$(cat "$value")"
    ;;
esac

# grep exits 1 for "found nothing", which is the good case here, and 2 for a
# real error — which must not be mistaken for a clean scan.
#
# The status is captured straight from the assignment, not after an `if`: the
# exit status of an `if` statement is the status of its *body*, so reading $?
# there reports 0 on a clean scan and the error branch can never be reached
# correctly. It fired on every clean message instead.
set +e
hits="$(printf '%s\n' "$text" | grep -iEn "$PATTERN")"
status=$?
set -e

case "$status" in
  0) echo "$hits"; exit 1 ;;
  1) exit 0 ;;
  *) echo "commit-scan: scan failed" >&2; exit "$status" ;;
esac
