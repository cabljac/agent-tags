#!/usr/bin/env bash
set -euo pipefail

# Usage: ./compare.sh <task_id>
# Compares baseline vs with-tags results across all trials.

TASK_ID="${1:?Usage: compare.sh <task_id>}"
EVAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== Results for $TASK_ID ==="
echo ""

for condition in baseline with-tags; do
  echo "--- $condition ---"
  for trial_dir in "$EVAL_DIR/results/$TASK_ID/$condition"/trial-*; do
    [ -d "$trial_dir" ] || continue
    trial=$(basename "$trial_dir")
    result=$(cat "$trial_dir/result.txt" 2>/dev/null || echo "NOT_RUN")
    localization=$(cat "$trial_dir/localization.txt" 2>/dev/null || echo "?/?")
    files=$(wc -l < "$trial_dir/files_changed.txt" 2>/dev/null || echo "?")
    echo "  $trial: result=$result  localization=$localization  files_touched=$files"
  done
  echo ""
done
