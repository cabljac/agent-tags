#!/usr/bin/env bash
set -euo pipefail

# Usage: ./summary.sh
# Prints a summary table across all tasks and conditions.

EVAL_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Header
printf "%-40s | %-20s | %-20s\n" "Task" "Baseline" "With-tags"
printf "%-40s-+-%-20s-+-%-20s\n" "$(printf '%0.s-' {1..40})" "$(printf '%0.s-' {1..20})" "$(printf '%0.s-' {1..20})"

for json_file in "$EVAL_DIR"/tasks/*.json; do
  [ -f "$json_file" ] || continue
  task_id=$(basename "$json_file" .json)

  # Collect best result per condition
  for condition in baseline with-tags; do
    results=""
    best_loc="0/?"
    best_result="NOT_RUN"
    trial_count=0

    for trial_dir in "$EVAL_DIR/results/$task_id/$condition"/trial-*; do
      [ -d "$trial_dir" ] || continue
      trial_count=$((trial_count + 1))

      loc=$(cat "$trial_dir/localization.txt" 2>/dev/null || echo "?/?")
      result=$(cat "$trial_dir/result.txt" 2>/dev/null || echo "NOT_RUN")
      files=$(wc -l < "$trial_dir/files_changed.txt" 2>/dev/null | tr -d ' ' || echo "?")

      # Track best localization
      correct=$(echo "$loc" | cut -d/ -f1)
      if [ "$correct" != "?" ] && [ "$correct" -gt "$(echo "$best_loc" | cut -d/ -f1)" ] 2>/dev/null; then
        best_loc="$loc"
      fi

      if [ "$result" = "RESOLVED" ]; then
        best_result="RESOLVED"
      elif [ "$result" = "FAILED" ] && [ "$best_result" != "RESOLVED" ]; then
        best_result="FAILED"
      fi
    done

    if [ "$trial_count" -eq 0 ]; then
      eval "${condition//-/_}_summary=—"
    else
      loc_display="$best_loc files"
      if [ "$best_result" = "RESOLVED" ]; then
        loc_display="$loc_display ✓"
      elif [ "$best_result" = "FAILED" ]; then
        loc_display="$loc_display ✗"
      fi
      eval "${condition//-/_}_summary=\"$loc_display (n=$trial_count)\""
    fi
  done

  printf "%-40s | %-20s | %-20s\n" "$task_id" "$baseline_summary" "$with_tags_summary"
done

echo ""
echo "Legend: X/Y files = localization (correct/total), ✓ = resolved, ✗ = tests failed, n = trials run"
