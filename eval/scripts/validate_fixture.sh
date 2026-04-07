#!/usr/bin/env bash
# Usage: ./validate_fixture.sh <task_id> [--deep]
# Preflight: task JSON parses; tag fixture looks like a git diff.
# With --deep: clone repo at base_commit and git apply --check (slow, needs network).

set -euo pipefail

TASK_ID="${1:?Usage: validate_fixture.sh <task_id> [--deep]}"
DEEP=false
if [ "${2:-}" = "--deep" ]; then
  DEEP=true
fi

EVAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TASK_FILE="$EVAL_DIR/tasks/${TASK_ID}.json"
TAG_PATCH="$EVAL_DIR/tasks/${TASK_ID}.tags.diff"

if [ ! -f "$TASK_FILE" ]; then
  echo "Error: missing task metadata: $TASK_FILE"
  exit 1
fi
if ! jq empty "$TASK_FILE" 2>/dev/null; then
  echo "Error: invalid JSON: $TASK_FILE"
  exit 1
fi
if [ ! -s "$TAG_PATCH" ]; then
  echo "Error: missing or empty tag fixture: $TAG_PATCH"
  exit 1
fi
if ! head -1 "$TAG_PATCH" | grep -q '^diff --git '; then
  echo "Error: $TAG_PATCH does not look like a unified diff (expected diff --git header)"
  exit 1
fi

if [ "$DEEP" = false ]; then
  echo "OK: $TASK_ID (quick checks passed)"
  exit 0
fi

REPO=$(jq -r '.repo' "$TASK_FILE")
BASE_COMMIT=$(jq -r '.base_commit' "$TASK_FILE")
WORK=$(mktemp -d "/tmp/agent-tags-validate-${TASK_ID}-XXXXXX")
cleanup() { rm -rf "$WORK"; }
trap cleanup EXIT

git clone --quiet "https://github.com/${REPO}.git" "$WORK/repo"
git -C "$WORK/repo" checkout --quiet "$BASE_COMMIT"
git -C "$WORK/repo" apply --check "$TAG_PATCH"
echo "OK: $TASK_ID (apply --check passed)"
